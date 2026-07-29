//! Running a command and keeping what it actually did.
//!
//! A phase advances on evidence, and evidence comes from here. The crate may
//! not depend on the runtime (see `scripts/check-crate-layers.py`), so the
//! execution surface is a trait: callers inside the control plane can inject a
//! runner that goes through the capability broker, and the crate's own default
//! is a plain child process with a working directory and a deadline it cannot
//! outlive.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Captured output is truncated past this. A verification log is worth keeping;
/// a runaway process printing into a durable record is not.
pub const MAX_CAPTURE_BYTES: usize = 256 * 1024;

const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// What a command did. There is no variant for "probably fine".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub program: String,
    pub args: Vec<String>,
    /// `None` when the process was killed by a signal or by the deadline.
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
}

impl CommandOutcome {
    /// Exit zero, and not killed on the way there.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }

    /// The status a phase records. A timeout has no exit code of its own, so it
    /// is reported as a distinct failure rather than as a missing value that
    /// something downstream might read as success.
    #[must_use]
    pub fn status_code(&self) -> i32 {
        match (self.timed_out, self.exit_code) {
            (true, _) => TIMEOUT_STATUS,
            (false, Some(code)) => code,
            (false, None) => SIGNAL_STATUS,
        }
    }

    /// The command as it would be typed, for the evidence record.
    #[must_use]
    pub fn command_line(&self) -> String {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// stdout and stderr as one blob, which is what a human reads and what the
    /// evidence digest covers.
    #[must_use]
    pub fn combined_output(&self) -> Vec<u8> {
        let mut out = self.stdout.clone();
        if !self.stderr.is_empty() {
            if !out.is_empty() && !out.ends_with(b"\n") {
                out.push(b'\n');
            }
            out.extend_from_slice(&self.stderr);
        }
        out
    }
}

/// Killed by the deadline.
pub const TIMEOUT_STATUS: i32 = -1;
/// Killed by a signal.
pub const SIGNAL_STATUS: i32 = -2;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("working directory {0} does not exist")]
    MissingCwd(PathBuf),
    #[error("spawning {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("waiting on {program}: {source}")]
    Wait {
        program: String,
        #[source]
        source: std::io::Error,
    },
}

/// How a run executes things.
///
/// Implement this to route commands through the capability broker, a sandbox,
/// or a recording harness. The contract every implementation owes: honour
/// `cwd`, honour `timeout`, and never report success for something that did not
/// exit zero.
pub trait CommandRunner {
    /// # Errors
    /// When the process could not be started or waited on. A command that ran
    /// and failed is an `Ok` outcome with a non-zero status, not an error —
    /// failing is information the run needs.
    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutcome, CommandError>;
}

/// A child process in a fixed directory with a deadline.
///
/// stdin is closed, so a command that decides to prompt fails instead of
/// hanging until the deadline.
///
/// The child gets its own process group, and the deadline kills the group
/// rather than the child. Killing only the child looks like it works — the
/// outcome is correctly marked `timed_out` — and then blocks anyway: a
/// surviving grandchild still holds the stdout pipe open, so draining it does
/// not reach EOF until the grandchild exits. `sh -c 'sleep 300'` with a
/// one-second deadline returns after 300 seconds, with a timeout flag on it.
/// A deadline that does not bound wall time is not a deadline.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutcome, CommandError> {
        if !cwd.is_dir() {
            return Err(CommandError::MissingCwd(cwd.to_path_buf()));
        }
        let started = Instant::now();
        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|source| CommandError::Spawn {
            program: program.to_string(),
            source,
        })?;

        // Drain both pipes on their own threads. Polling for the deadline while
        // a full pipe blocks the child is a deadlock, not a timeout.
        let out_pipe = child.stdout.take();
        let err_pipe = child.stderr.take();
        let out_thread = std::thread::spawn(move || drain_capped(out_pipe));
        let err_thread = std::thread::spawn(move || drain_capped(err_pipe));

        let deadline = started + timeout;
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        timed_out = true;
                        kill_group(&mut child);
                        let _ = child.wait();
                        break None;
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(source) => {
                    return Err(CommandError::Wait {
                        program: program.to_string(),
                        source,
                    })
                }
            }
        };

        let (stdout, stdout_truncated) = out_thread.join().unwrap_or_default();
        let (stderr, stderr_truncated) = err_thread.join().unwrap_or_default();

        Ok(CommandOutcome {
            program: program.to_string(),
            args: args.to_vec(),
            exit_code: status.and_then(|status| status.code()),
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
            timed_out,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }
}

/// Kill the child and everything it started.
///
/// On Unix the child leads its own process group (see [`ProcessRunner`]), so
/// one signal to the negated pid reaches the whole tree. If the group signal
/// fails — the group is already gone, or the platform is not Unix — fall back
/// to killing the child alone, which is strictly better than nothing.
fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = i32::try_from(child.id()).unwrap_or(-1);
        if pid > 0 {
            // SAFETY: `kill` with a negated pid signals a process group. The
            // group is the one this child leads, created at spawn, so nothing
            // outside this call's own subtree can receive it.
            let sent = unsafe { libc::kill(-pid, libc::SIGKILL) };
            if sent == 0 {
                return;
            }
        }
    }
    let _ = child.kill();
}

/// Read to EOF, keeping at most `MAX_CAPTURE_BYTES`.
///
/// Reading continues past the cap so the child is never blocked on a pipe it
/// cannot drain; the excess is discarded rather than held.
fn drain_capped(pipe: Option<impl Read>) -> (Vec<u8>, bool) {
    let Some(mut pipe) = pipe else {
        return (Vec::new(), false);
    };
    let mut kept: Vec<u8> = Vec::new();
    let mut truncated = false;
    let mut chunk = [0_u8; 8192];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = MAX_CAPTURE_BYTES.saturating_sub(kept.len());
                if room == 0 {
                    truncated = true;
                } else if n > room {
                    kept.extend_from_slice(&chunk[..room]);
                    truncated = true;
                } else {
                    kept.extend_from_slice(&chunk[..n]);
                }
            }
        }
    }
    (kept, truncated)
}
