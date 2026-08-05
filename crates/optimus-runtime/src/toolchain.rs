//! Toolchain bind tier for the Developer Full Access command envelope
//! (spec-014 R1-R3, ADR-0080).
//!
//! The Confined envelope binds only the workspace read-write; on a rustup
//! machine the whole toolchain lives in `$HOME` and every approved build dies
//! instantly (`bwrap: execvp … No such file or directory`). This module
//! builds the classed (rw/ro) toolchain bind list for DFA grants:
//! non-secret caches rw (writes go through to the host; corruption is
//! contained and nothing credential-adjacent is bound), functional
//! toolchains ro, and credential/identity paths NEVER bound — ro-binds are
//! readable inside the sandbox, and under the shared-network Confined
//! envelope readable paths are exfiltratable.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use optimus_policy::{DeveloperAccessGrant, DeveloperScope};

use crate::{
    command_envelope, ensure_owned_command_containment, linux_contained_command, read_limited,
    sanitize_command_environment, CommandCapture, Effect, GraphError, JobId, KillOnDrop,
    RuntimeError,
};

/// Re-exported for integration tests; the argv builder lives in the private
/// envelope module.
pub use crate::command_envelope::linux_bwrap_args_classed;

/// How a host path is bound into the sandbox. rw binds write THROUGH to the
/// host; ro binds are readable but not writable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindMode {
    Rw,
    Ro,
}

/// The sandbox additions a spawn carries: classed toolchain binds, the
/// bind-derived PATH, and the home the bind set was derived from. Bundled so
/// spawn signatures stay under the too-many-arguments threshold.
#[derive(Debug, Default)]
pub struct CommandSandbox<'a> {
    pub toolchain: &'a [(PathBuf, BindMode)],
    pub bind_path: &'a str,
    pub home: &'a str,
}

impl<'a> CommandSandbox<'a> {
    pub fn new(toolchain: &'a [(PathBuf, BindMode)], bind_path: &'a str, home: &'a str) -> Self {
        Self {
            toolchain,
            bind_path,
            home,
        }
    }
}

/// Paths that must never enter the bind set: credentials and identity.
const NEVER_BOUND_SUFFIXES: &[&str] = &[
    ".cargo/credentials.toml",
    ".cargo/config.toml",
    ".gitconfig",
    ".config/git",
    ".config/gh",
    ".ssh",
];

/// Non-secret caches the toolchain writes into: rw, host-created when absent.
const RW_SUFFIXES: &[&str] = &[
    ".cargo/registry",
    ".cargo/git",
    ".bun",
    ".cache/cargo",
    ".cache/bun",
];

/// Functional toolchain paths the build reads: ro, skip-if-absent.
const RO_SUFFIXES: &[&str] = &[".cargo/bin", ".rustup", ".cache/ms-playwright"];

fn home_path(home: &Path, suffix: &str) -> PathBuf {
    home.join(suffix)
}

/// Build the classed toolchain bind list for a Developer Full Access grant.
///
/// Empty unless the grant is enabled, has terminal execution, and is not the
/// entire-local-machine scope (which binds `/` outright and needs no toolchain
/// tier). rw entries are host-created when absent and always bound; ro
/// entries are bound only when present. All rw binds precede ro binds so an
/// ro over-bind always wins on overlap.
pub fn toolchain_bind_list(home: &Path, grant: &DeveloperAccessGrant) -> Vec<(PathBuf, BindMode)> {
    if !grant.enabled
        || !grant.capabilities.terminal_execution
        || matches!(grant.scope, DeveloperScope::EntireLocalMachine)
    {
        return Vec::new();
    }
    let mut binds = Vec::new();
    for suffix in RW_SUFFIXES {
        let path = home_path(home, suffix);
        if std::fs::create_dir_all(&path).is_err() {
            continue;
        }
        binds.push((path, BindMode::Rw));
    }
    for suffix in RO_SUFFIXES {
        let path = home_path(home, suffix);
        if path.exists() {
            binds.push((path, BindMode::Ro));
        }
    }
    binds
}

/// Whether a bind-list entry is a credential or identity path. Defensive
/// guard for future callers; the builder never emits these.
pub fn is_never_bound(path: &Path) -> bool {
    NEVER_BOUND_SUFFIXES
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

/// Resolve a bare program name deterministically (spec-014 R2, ADR-0080).
///
/// Walks `path_entries` in order and returns the FIRST candidate that exists
/// on the host AND satisfies `visible` (i.e. resolves inside the active
/// bind set). No fallback to an invisible candidate: a binary the sandbox
/// cannot see would fail at spawn, so resolving it would be a lie. Returns
/// `None` when no entry qualifies.
pub fn resolve_program_in_path(
    program: &str,
    path_entries: &[PathBuf],
    visible: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    for entry in path_entries {
        let candidate = entry.join(program);
        if candidate.is_file() && visible(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// The sandbox PATH derived from the bind set (spec-014 R2, ADR-0080).
///
/// Toolchain bin dirs (a bound path ending in `.cargo/bin`) lead the PATH so
/// the bwrap child resolves bare `cargo`/`rustc` the same way the host-side
/// normalization did; the system dirs stay reachable. The string is
/// runtime-constructed from the bind list, never from the model.
pub fn bind_derived_path(binds: &[(PathBuf, BindMode)]) -> String {
    let mut entries: Vec<String> = binds
        .iter()
        .filter(|(path, _)| path.ends_with(".cargo/bin"))
        .map(|(path, _)| path.display().to_string())
        .collect();
    entries.extend(["/usr/local/bin".into(), "/usr/bin".into(), "/bin".into()]);
    entries.join(":")
}

/// System trees the confined profile ro-binds; a binary under one of these is
/// visible inside the sandbox.
const SYSTEM_RO_CANDIDATES: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt", "/nix",
];

/// Whether `candidate` resolves inside the active envelope: under the
/// workspace, under any classed bind, or under a system tree the confined
/// profile ro-binds.
pub fn is_visible_in_envelope(
    candidate: &Path,
    workspace: &Path,
    toolchain_binds: &[(PathBuf, BindMode)],
) -> bool {
    if candidate.starts_with(workspace) {
        return true;
    }
    if toolchain_binds
        .iter()
        .any(|(path, _)| candidate.starts_with(path))
    {
        return true;
    }
    SYSTEM_RO_CANDIDATES
        .iter()
        .any(|tree| candidate.starts_with(tree))
}

/// PATH entries the spawn resolution walks: the host PATH first (a GUI-launched
/// app may carry a minimal one), then the toolchain bin dirs from the binds.
pub fn resolution_path_entries(toolchain_binds: &[(PathBuf, BindMode)]) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    for (path, _) in toolchain_binds {
        if path.ends_with(".cargo/bin") && !entries.contains(path) {
            entries.push(path.clone());
        }
    }
    entries
}

/// Pre-card feasibility probe for a command effect (spec-014 R3, ADR-0080).
///
/// Returns `Ok(Some(resolved_program))` when the program resolves to a
/// visible absolute path and the effect's write targets are not inside a
/// read-only bind; `Ok(None)` when the envelope is not confined (nothing to
/// probe); `Err(reason)` when the effect is doomed — the caller must deny
/// with `reason` instead of carding it.
pub fn probe_command_runnable(
    program: &str,
    args: &[String],
    workspace: &Path,
    envelope: optimus_graph::CommandFsEnvelope,
    toolchain_binds: &[(PathBuf, BindMode)],
) -> Result<Option<PathBuf>, String> {
    if !envelope.linux_workspace_only_writable() {
        return Ok(None);
    }
    let path_entries = resolution_path_entries(toolchain_binds);
    let resolved = resolve_program_in_path(program, &path_entries, |candidate| {
        is_visible_in_envelope(candidate, workspace, toolchain_binds)
    });
    let Some(resolved) = resolved else {
        return Err(format!(
            "{program} is not visible inside the confined command envelope — \
             toolchains and caches outside the workspace are not bound. Enable \
             Developer Full Access (terminal execution) to bind the toolchain."
        ));
    };
    // HostInstall-class effects write into the toolchain bin dir, which is
    // ro-bound; the install would fail at spawn. Deny with the reason.
    use optimus_policy::{classify_command, CommandClass};
    let class = classify_command(program, args);
    if class == CommandClass::HostInstall {
        let parent = resolved.parent().unwrap_or(Path::new("/"));
        let in_ro_bind = toolchain_binds
            .iter()
            .any(|(path, mode)| *mode == BindMode::Ro && parent.starts_with(path));
        if in_ro_bind {
            return Err(format!(
                "{} installs into {}, which is read-only inside the sandbox — \
                 installs are not supported under the confined envelope.",
                resolved.display(),
                parent.display()
            ));
        }
    }
    Ok(Some(resolved))
}

impl crate::Runtime {
    pub(crate) fn run_command_bounded(
        &self,
        program: &str,
        args: &[String],
        timeout: Duration,
        job_id: JobId,
    ) -> crate::Result<CommandCapture> {
        ensure_owned_command_containment(std::env::consts::OS)
            .map_err(|error| RuntimeError::Effector(error.to_string()))?;
        command_envelope::command_envelope_supported(
            std::env::consts::OS,
            self.config.command_fs_envelope,
        )
        .map_err(RuntimeError::Effector)?;
        // Spawn-time re-verification (spec-014 R2, ADR-0080): the same
        // resolution the pre-card probe used, re-executed here because the
        // bind set is rebuilt per turn. Fail fast with the recovery reason
        // instead of letting the sandbox fail the approved command.
        let toolchain = &self.developer.toolchain;
        let resolved_program = crate::toolchain::probe_command_runnable(
            program,
            args,
            &self.workspace,
            self.config.command_fs_envelope,
            toolchain,
        )
        .map_err(|reason| RuntimeError::PolicyDenied {
            code: "effect_not_runnable_in_envelope".into(),
            reason,
        })?
        .unwrap_or_else(|| std::path::PathBuf::from(program));
        let spawn_program = resolved_program.to_string_lossy().into_owned();
        let bind_path = crate::toolchain::bind_derived_path(toolchain);
        let sandbox_home = self.developer.home.to_string_lossy().into_owned();
        #[cfg(target_os = "linux")]
        let (mut command, linux_unit) = linux_contained_command(
            &spawn_program,
            args,
            &self.workspace,
            self.config.command_fs_envelope,
            &self.developer.roots,
            &CommandSandbox::new(toolchain, &bind_path, &sandbox_home),
        );
        #[cfg(not(target_os = "linux"))]
        let mut command = Command::new(&spawn_program);
        #[cfg(not(target_os = "linux"))]
        command
            .args(args)
            .current_dir(&self.workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        sanitize_command_environment(&mut command);
        #[cfg(target_os = "linux")]
        let mut guard = KillOnDrop::spawn(&mut command, linux_unit)
            .map_err(|e| RuntimeError::Effector(format!("spawn {program}: {e}")))?;
        #[cfg(not(target_os = "linux"))]
        let mut guard = KillOnDrop::spawn(&mut command, false)
            .map_err(|e| RuntimeError::Effector(format!("spawn {program}: {e}")))?;

        let child = guard.child.as_mut().expect("contained child is present");
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let out_h = thread::spawn(move || read_limited(stdout_pipe));
        let err_h = thread::spawn(move || read_limited(stderr_pipe));

        let start = Instant::now();
        let mut timed_out = false;
        let mut cancelled = false;
        let cleanup_error;
        let wait_status = loop {
            match guard.try_wait() {
                Ok(Some(status)) => {
                    #[cfg(windows)]
                    {
                        cleanup_error = guard.terminate_job_and_confirm_empty().err();
                    }
                    #[cfg(target_os = "linux")]
                    {
                        cleanup_error = guard.terminate_linux_unit_and_confirm_empty().err();
                    }
                    #[cfg(not(any(windows, target_os = "linux")))]
                    {
                        cleanup_error = None;
                    }
                    guard.child = None;
                    break Some(status);
                }
                Ok(None) => {
                    if self
                        .store
                        .job_cancellation_requested(job_id.0)
                        .map_err(GraphError::from)?
                    {
                        cleanup_error = guard.kill_and_reap().err();
                        cancelled = true;
                        break None;
                    }
                    if start.elapsed() >= timeout {
                        cleanup_error = guard.kill_and_reap().err();
                        timed_out = true;
                        break None;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    let _ = guard.kill_and_reap();
                    return Err(RuntimeError::Effector(format!("wait {program}: {e}")));
                }
            }
        };

        let (stdout, trunc_out) = out_h.join().unwrap_or_else(|_| (String::new(), false));
        let (stderr, trunc_err) = err_h.join().unwrap_or_else(|_| (String::new(), false));

        if let Some(error) = cleanup_error {
            return Err(RuntimeError::Effector(format!(
                "process-tree cleanup for {program} was not confirmed: {error}"
            )));
        }
        if cancelled {
            return Err(RuntimeError::Cancelled { job_id });
        }

        let exit_code = wait_status.and_then(|s| s.code());
        let capture = CommandCapture {
            stdout,
            stderr,
            exit_code,
            truncated_stdout: trunc_out,
            truncated_stderr: trunc_err,
            timed_out,
        };

        if timed_out {
            return Err(RuntimeError::CommandFailed {
                exit_code: None,
                capture,
            });
        }
        if wait_status.map(|s| s.success()).unwrap_or(false) {
            return Ok(capture);
        }
        Err(RuntimeError::CommandFailed { exit_code, capture })
    }

    /// Pre-card feasibility probe for command effects (spec-014 R3,
    /// ADR-0080). File effects are always runnable; command effects whose
    /// program is invisible in the envelope (or whose HostInstall target is a
    /// ro-bound toolchain dir) return the denial reason.
    pub(crate) fn probe_effect_runnable(&self, effect: &Effect) -> std::result::Result<(), String> {
        let (program, args) = match effect {
            Effect::RunCommand { program, args }
            | Effect::ProjectRunCommand { program, args, .. }
            | Effect::ProjectServe { program, args, .. } => (program, args),
            _ => return Ok(()),
        };
        crate::toolchain::probe_command_runnable(
            program,
            args,
            &self.workspace,
            self.config.command_fs_envelope,
            &self.developer.toolchain,
        )
        .map(|_| ())
    }

    /// Probe-then-settle for the SmartDeny branch (spec-014 R3, ADR-0080).
    /// A command effect whose program is invisible in the envelope is doomed;
    /// deny it with the recovery reason instead of carding it.
    pub(crate) fn settle_with_probe(
        &self,
        job_id: JobId,
        node_id: uuid::Uuid,
        node_index: u32,
        effect: &Effect,
        effect_hash: &str,
    ) -> crate::Result<()> {
        if let Err(reason) = self.probe_effect_runnable(effect) {
            return Err(RuntimeError::PolicyDenied {
                code: "effect_not_runnable_in_envelope".into(),
                reason,
            });
        }
        self.settle_high_risk_authority(job_id, node_id, node_index, effect, effect_hash)
    }
}
