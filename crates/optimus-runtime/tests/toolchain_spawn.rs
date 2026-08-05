//! Spawn-path wiring for the toolchain bind tier (spec-014 R1-R3, ADR-0080):
//! the runtime builds classed binds from the DFA grant, normalizes bare
//! programs to visible absolute paths, sets a bind-derived PATH, and probes
//! before carding a doomed effect.

use std::path::PathBuf;
use std::sync::Mutex;

use optimus_graph::{CommandFsEnvelope, Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_policy::{
    DeveloperAccessGrant, DeveloperCapabilities, DeveloperScope,
    DEVELOPER_ACCESS_CONFIRMATION_VERSION,
};
use optimus_runtime::{Runtime, RuntimeError};
use tempfile::tempdir;

/// The runtime reads the process-global `$HOME` at open; tests that redirect
/// it must not run concurrently (parallel test threads share the env).
static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Redirect `$HOME` for the duration of the closure, serialized against other
/// HOME-mutating tests in this binary. Poison-tolerant: a test that panics
/// while holding the lock must not take down its siblings.
fn with_home<R>(home: &std::path::Path, body: impl FnOnce() -> R) -> R {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var_os("HOME");
    std::env::set_var("HOME", home);
    let result = body();
    match previous {
        Some(home) => std::env::set_var("HOME", home),
        None => std::env::remove_var("HOME"),
    }
    result
}

fn valid_grant(root: &str, terminal: bool) -> DeveloperAccessGrant {
    let mut capabilities = DeveloperCapabilities::default();
    if !terminal {
        capabilities.terminal_execution = false;
    }
    DeveloperAccessGrant {
        enabled: true,
        confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
        issued_unix: 1,
        // The envelope tests exercise the toolchain tier, not the destructive
        // pause policy; the real self-build flow pairs this with session
        // consent (spec-014 B3).
        pause_before_destructive: false,
        scope: DeveloperScope::SelectedRepository {
            root: root.into(),
            root_hash: None,
        },
        capabilities,
        ..Default::default()
    }
}

fn confined_config() -> RuntimeConfig {
    RuntimeConfig {
        policy: PolicyMode::SmartDeny,
        command_fs_envelope: CommandFsEnvelope::Confined,
        autonomy_profile: optimus_graph::AutonomyProfile::DeveloperFullAccess,
        ..Default::default()
    }
}

/// The toolchain tier is real: a fake rustup-style `~/.cargo/bin/cargo` shim
/// placed OUTSIDE the workspace is unreachable without the bind; the derived
/// PATH leads with the toolchain bin dir. The shim writes its $PATH into the
/// workspace so the test asserts both the bind and the PATH property.
#[cfg(target_os = "linux")]
#[test]
fn spawn_uses_toolchain_binds_and_derived_path() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let home = root.path().join("home");
    let bin = home.join(".cargo/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        bin.join("cargo"),
        "#!/bin/sh\nprintf '%s' \"$PATH\" > \"$PWD/path.txt\"\necho shim-ran > \"$PWD/ran.txt\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(bin.join("cargo"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }

    // The runtime reads $HOME at open to build the toolchain tier.
    with_home(&home, || {
        let rt = Runtime::open_with_developer_access(
            &root.path().join("optimus.db"),
            &workspace,
            confined_config(),
            Some(valid_grant(workspace.to_str().unwrap(), true)),
            vec![workspace.clone()],
        )
        .expect("runtime");

        let job = rt
            .create_job(JobSpec {
                label: "toolchain-cargo".into(),
                budget: Default::default(),
                nodes: vec![NodeSpec {
                    label: "run".into(),
                    effect: Effect::RunCommand {
                        program: "cargo".into(),
                        args: vec!["--version".into()],
                    },
                }],
            })
            .unwrap();
        let status = rt.run_all(job).expect("job runs under the DFA grant");
        if status != optimus_runtime::JobStatus::Succeeded {
            let capture = rt.latest_command_capture(job).ok().flatten();
            panic!(
                "expected Succeeded, got {status:?}; capture={capture:?}; ws files: {:?}",
                std::fs::read_dir(&workspace).map(|d| d
                    .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                    .collect::<Vec<_>>())
            );
        }
        let path = std::fs::read_to_string(workspace.join("path.txt")).unwrap();
        assert!(
            path.starts_with(&bin.to_string_lossy().into_owned()),
            "derived PATH must lead with the toolchain bin dir: {path}"
        );
        assert!(
            std::fs::read_to_string(workspace.join("ran.txt"))
                .unwrap()
                .contains("shim-ran"),
            "the shim ran — the bind + normalization are real"
        );
    });
}

/// The pre-card probe: an invisible program is DENIED with a recovery reason,
/// never carded (a doomed approval would fail at spawn anyway).
#[cfg(target_os = "linux")]
#[test]
fn invisible_program_is_denied_not_carded() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    // No grant -> strict Confined envelope -> toolchain absent -> the program
    // cannot exist anywhere visible.
    let rt = Runtime::open_with_config(
        &root.path().join("optimus.db"),
        &workspace,
        confined_config(),
    )
    .expect("runtime");
    let job = rt
        .create_job(JobSpec {
            label: "missing-tool".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "run".into(),
                effect: Effect::RunCommand {
                    program: "optimus-definitely-missing-xyz".into(),
                    args: Vec::new(),
                },
            }],
        })
        .unwrap();
    match rt.run_all(job) {
        Err(RuntimeError::PolicyDenied { code, reason }) => {
            assert!(code.contains("envelope"), "code: {code}");
            assert!(!reason.is_empty(), "recovery reason required");
        }
        Err(RuntimeError::NeedsApproval { .. }) => {
            panic!("invisible program must be denied, not carded")
        }
        other => panic!("expected PolicyDenied, got {other:?}"),
    }
}

/// Live acceptance (spec-014 A1): with the REAL rustup toolchain in $HOME,
/// bare `cargo` runs through the exact product chain (systemd-run → bwrap →
/// cargo) under the DFA grant and the toolchain tier. Skips gracefully on
/// hosts without ~/.cargo/bin/cargo (mirrors the resolver-test pattern).
#[cfg(target_os = "linux")]
#[test]
fn live_bare_cargo_runs_through_the_product_chain() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return;
    };
    let cargo_shim = home.join(".cargo/bin/cargo");
    if !cargo_shim.is_file() {
        return; // No rustup toolchain on this host; nothing to prove here.
    }
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    // The whole body holds the HOME lock: the sandbox HOME is derived from
    // the open-time home (pinned in the runtime), and the sibling tests
    // redirect the process HOME.
    with_home(
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .as_deref()
            .unwrap_or(std::path::Path::new("/")),
        || {
            let rt = Runtime::open_with_developer_access(
                &root.path().join("optimus.db"),
                &workspace,
                confined_config(),
                Some(valid_grant(workspace.to_str().unwrap(), true)),
                vec![workspace.clone()],
            )
            .expect("runtime");
            let job = rt
                .create_job(JobSpec {
                    label: "live-cargo".into(),
                    budget: Default::default(),
                    nodes: vec![NodeSpec {
                        label: "run".into(),
                        effect: Effect::RunCommand {
                            program: "cargo".into(),
                            args: vec!["--version".into()],
                        },
                    }],
                })
                .unwrap();
            match rt.run_all(job) {
                Ok(optimus_runtime::JobStatus::Succeeded) => {
                    let capture = rt
                        .latest_command_capture(job)
                        .expect("capture query")
                        .expect("capture after success");
                    assert!(
                        capture.stdout.contains("cargo"),
                        "cargo must report its version: {}",
                        capture.stdout
                    );
                    assert_eq!(capture.exit_code, Some(0));
                }
                Ok(status) => {
                    let capture = rt.latest_command_capture(job).ok().flatten();
                    panic!("expected Succeeded, got {status:?}; capture={capture:?}")
                }
                Err(error) => panic!(
                    "bare cargo must run under the toolchain tier: {error}; env HOME={:?}",
                    std::env::var_os("HOME")
                ),
            }
        },
    );
}

/// HostInstall into a ro-bound toolchain dir is denied with a read-only
/// reason instead of being carded (the install would fail at spawn).
#[cfg(target_os = "linux")]
#[test]
fn host_install_into_ro_toolchain_bin_is_denied() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let home = root.path().join("home");
    let bin = home.join(".cargo/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("cargo"), "#!/bin/sh\n").unwrap();

    with_home(&home, || {
        let rt = Runtime::open_with_developer_access(
            &root.path().join("optimus.db"),
            &workspace,
            confined_config(),
            Some(valid_grant(workspace.to_str().unwrap(), true)),
            vec![workspace.clone()],
        )
        .expect("runtime");

        let job = rt
            .create_job(JobSpec {
                label: "cargo-install".into(),
                budget: Default::default(),
                nodes: vec![NodeSpec {
                    label: "run".into(),
                    effect: Effect::RunCommand {
                        program: "cargo".into(),
                        args: vec!["install".into(), "ripgrep".into()],
                    },
                }],
            })
            .unwrap();
        match rt.run_all(job) {
            Err(RuntimeError::PolicyDenied { code, reason }) => {
                assert!(
                    reason.contains("read-only"),
                    "reason must name the read-only target: {reason}"
                );
                assert!(
                    reason.contains("cargo/bin"),
                    "reason must name the target: {reason}"
                );
                let _ = code;
            }
            other => panic!("expected PolicyDenied for ro-target install, got {other:?}"),
        }
    });
}
