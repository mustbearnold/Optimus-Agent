//! P12 command capability envelope: workspace-only writable FS for Linux commands.

use std::fs;
use std::path::PathBuf;

use optimus_graph::{CommandFsEnvelope, Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_runtime::{linux_bwrap_args, ApprovalGrant, Runtime, RuntimeError};
use tempfile::tempdir;

fn grant_and_run(rt: &Runtime, job: optimus_graph::JobId) -> Result<(), RuntimeError> {
    match rt.run_next(job) {
        Err(RuntimeError::NeedsApproval { .. }) => {
            rt.grant_approval(ApprovalGrant::for_job(job))?;
            rt.run_next(job)?;
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn confined_command_cannot_write_outside_workspace() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    let external = root.path().join("external");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&external).unwrap();
    let marker = external.join("escaped.txt");
    let marker_str = marker.to_string_lossy().into_owned();

    let rt = Runtime::open_with_config(
        &root.path().join("optimus.db"),
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            command_fs_envelope: CommandFsEnvelope::Confined,
            ..Default::default()
        },
    )
    .expect("runtime");

    let job = rt
        .create_job(JobSpec {
            label: "escape-write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write-outside".into(),
                effect: Effect::RunCommand {
                    program: "sh".into(),
                    args: vec![
                        "-c".into(),
                        format!("echo pwned > '{marker_str}'; echo ok > inside.txt"),
                    ],
                },
            }],
        })
        .expect("job");

    grant_and_run(&rt, job).expect("command should complete (outer write fails soft)");
    assert!(
        workspace.join("inside.txt").is_file(),
        "workspace write must succeed"
    );
    assert!(
        !marker.exists(),
        "command must not create files outside workspace"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn confined_command_cannot_overwrite_system_path() {
    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let rt = Runtime::open_with_config(
        &root.path().join("optimus.db"),
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            command_fs_envelope: CommandFsEnvelope::Confined,
            ..Default::default()
        },
    )
    .unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "sys-write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "touch-etc".into(),
                effect: Effect::RunCommand {
                    program: "sh".into(),
                    args: vec![
                        "-c".into(),
                        "touch /etc/optimus-p12-should-fail 2>/tmp/err; echo done > status.txt"
                            .into(),
                    ],
                },
            }],
        })
        .unwrap();
    grant_and_run(&rt, job).expect("run");
    assert!(workspace.join("status.txt").is_file());
    assert!(!PathBuf::from("/etc/optimus-p12-should-fail").exists());
}

/// A confined command must be able to resolve hostnames.
///
/// `Confined` shares the network namespace, so it promises a working network.
/// The profile also mounts a tmpfs over `/run`, and on any systemd-resolved
/// host `/etc/resolv.conf` is a symlink into `/run` — so the promise was empty
/// and every lookup failed. Read through the sandbox rather than over the
/// internet: a reachable, non-empty resolver config is the precondition that
/// broke, and asserting it needs no network of its own.
#[cfg(target_os = "linux")]
#[test]
fn a_confined_command_can_still_resolve_names() {
    let Ok(resolver) = fs::canonicalize("/etc/resolv.conf") else {
        return; // No resolver on this host; nothing to protect.
    };
    if !resolver.starts_with("/run") && !resolver.starts_with("/var") {
        return; // A plain file survives the /etc ro-bind on its own.
    }

    let root = tempdir().expect("root");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let rt = Runtime::open_with_config(
        &root.path().join("optimus.db"),
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            command_fs_envelope: CommandFsEnvelope::Confined,
            ..Default::default()
        },
    )
    .expect("runtime");

    let job = rt
        .create_job(JobSpec {
            label: "resolver-visible".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "read-resolver".into(),
                effect: Effect::RunCommand {
                    program: "sh".into(),
                    args: vec!["-c".into(), "cat /etc/resolv.conf".into()],
                },
            }],
        })
        .expect("job");
    grant_and_run(&rt, job).expect("run");

    let capture = rt
        .latest_command_capture(job)
        .unwrap()
        .expect("a capture for the command");
    assert!(
        capture.stdout.contains("nameserver"),
        "the confined profile left the child with no resolver, so every \
         hostname fails while the effect still reports success; stdout={:?} \
         stderr={:?}",
        capture.stdout,
        capture.stderr
    );
}

#[test]
fn bwrap_confined_args_exclude_full_root_bind() {
    let args = linux_bwrap_args(
        &PathBuf::from("/tmp/ws-example"),
        CommandFsEnvelope::Confined,
    );
    let flat = args.join(" ");
    assert!(
        !flat.contains("--bind / /"),
        "confined profile must not bind entire host root rw: {flat}"
    );
}

#[test]
fn unrestricted_host_still_documents_full_bind() {
    let args = linux_bwrap_args(
        &PathBuf::from("/tmp/ws"),
        CommandFsEnvelope::UnrestrictedHost,
    );
    assert!(args.windows(3).any(|w| w == ["--bind", "/", "/"]));
}
