//! Keeping library noise off the screen the TUI owns.
//!
//! The terminal face links the kernel in-process rather than talking to it over
//! a transport (ADR-0045). That means anything the kernel or a pack writes to
//! stderr lands in the middle of the frame ratatui is drawing: a browser
//! fallback notice appears halfway down the transcript, at whatever column the
//! cursor happened to be, and the pane is corrupt until the next full redraw.
//!
//! Pointing fd 2 at a file catches those writes wherever they come from —
//! including dependencies this crate does not control — and keeps them readable
//! afterwards instead of throwing them away.
//!
//! Only stderr is redirected. Stdout is where ratatui draws, so it has to stay
//! exactly where it is; surfaces that own the screen keep prompts off stdout by
//! taking a callback instead (`codex_device_login::device_code_login_with`).

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

/// Where library noise goes while the terminal face is up.
pub fn log_path(home: &Path) -> PathBuf {
    home.join("logs").join("tui.log")
}

/// Holds fd 2 pointed at a file, and puts the original back when dropped.
///
/// Restoration runs on drop rather than at a call site so that a panic
/// unwinding out of the event loop still leaves the real stderr attached, and
/// its message reaches the terminal instead of the log.
pub struct StderrLog {
    #[cfg(unix)]
    saved: Option<std::os::fd::RawFd>,
}

#[cfg(unix)]
impl StderrLog {
    /// Redirect stderr to `path`, creating the directory if it is missing.
    ///
    /// Appends: a run that crashes should not erase the evidence from the run
    /// before it.
    pub fn to_file(path: &Path) -> io::Result<Self> {
        use std::os::fd::AsRawFd;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::options().create(true).append(true).open(path)?;

        // SAFETY: both calls act on descriptors that are open for the duration.
        // `saved` is owned by this value and closed exactly once, on drop.
        let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
        if saved < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            let failure = io::Error::last_os_error();
            unsafe { libc::close(saved) };
            return Err(failure);
        }
        // `file` may drop here: dup2 already duplicated the description onto
        // fd 2, which keeps it open.
        Ok(Self { saved: Some(saved) })
    }
}

#[cfg(unix)]
impl Drop for StderrLog {
    fn drop(&mut self) {
        if let Some(saved) = self.saved.take() {
            // SAFETY: `saved` came from `dup` above and has not been closed.
            unsafe {
                libc::dup2(saved, libc::STDERR_FILENO);
                libc::close(saved);
            }
        }
    }
}

#[cfg(not(unix))]
impl StderrLog {
    /// No redirection off unix. The screen can still be corrupted by a library
    /// write; saying so beats pretending the guard did something.
    pub fn to_file(_path: &Path) -> io::Result<Self> {
        Ok(Self {})
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::tempdir;

    /// Serialises every test in this module.
    ///
    /// Redirection swaps file descriptor 2, which belongs to the whole process
    /// and not to the thread doing the swapping. Run in parallel, these tests
    /// capture each other's writes and restore each other's descriptors — the
    /// failures land on whichever test lost the race, which is why they looked
    /// unrelated to any change that merely altered test ordering.
    fn exclusive() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        // A poisoned lock means some other test in this module already failed;
        // that failure is the report, so carry on rather than masking it.
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Write the way a linked-in library does: straight at the descriptor.
    ///
    /// `eprintln!` cannot be used here. The test harness swaps the Rust-level
    /// stderr for a capture buffer, so the macro never reaches fd 2 and would
    /// prove nothing about the redirection. In the real binary no capture is
    /// installed and `eprintln!` lands on the same descriptor this writes to.
    fn write_stderr(line: &str) {
        let mut stderr = io::stderr();
        stderr.write_all(line.as_bytes()).unwrap();
        stderr.flush().unwrap();
    }

    #[test]
    fn stderr_writes_land_in_the_log_instead_of_the_screen() {
        let _exclusive = exclusive();
        let dir = tempdir().unwrap();
        let path = log_path(dir.path());
        {
            let _guard = StderrLog::to_file(&path).expect("redirect");
            // Exactly what `optimus_kernel::browser` says on a CDP fallback.
            write_stderr("[browser] CDP effector failed, falling back to HTTP: boom\n");
        }
        let logged = fs::read_to_string(&path).unwrap();
        assert!(logged.contains("CDP effector failed"), "{logged}");
    }

    #[test]
    fn the_log_survives_a_second_run() {
        let _exclusive = exclusive();
        let dir = tempdir().unwrap();
        let path = log_path(dir.path());
        for line in ["first run\n", "second run\n"] {
            let _guard = StderrLog::to_file(&path).expect("redirect");
            write_stderr(line);
        }
        let logged = fs::read_to_string(&path).unwrap();
        assert!(
            logged.contains("first run") && logged.contains("second run"),
            "a crash must not erase the run before it: {logged}"
        );
    }

    #[test]
    fn dropping_the_guard_gives_the_real_stderr_back() {
        let _exclusive = exclusive();
        let dir = tempdir().unwrap();
        let (first, second) = (log_path(dir.path()), dir.path().join("after.log"));
        drop(StderrLog::to_file(&first).expect("redirect"));
        // Prove the write went elsewhere without letting it loose on the
        // terminal that is running the suite.
        let _guard = StderrLog::to_file(&second).expect("redirect");
        write_stderr("after the guard was dropped\n");
        assert_eq!(
            fs::read_to_string(&first).unwrap(),
            "",
            "a dropped guard must stop capturing"
        );
        assert!(fs::read_to_string(&second).unwrap().contains("after the"));
    }

    #[test]
    fn a_missing_log_directory_is_created_rather_than_failing_the_run() {
        let _exclusive = exclusive();
        let dir = tempdir().unwrap();
        let path = log_path(&dir.path().join("home-that-does-not-exist-yet"));
        let guard = StderrLog::to_file(&path);
        assert!(guard.is_ok(), "{:?}", guard.err());
        assert!(path.exists());
    }
}
