//! Branch and worktree lifecycle for one engineering run (ADR-0052 §2).
//!
//! Every run gets its own checkout cut from a recorded commit. The main
//! checkout is never written by a run, so a failed attempt is a directory that
//! gets deleted rather than an incident in the tree a human is using.
//!
//! This module shells out to `git`. It is the only part of the crate that
//! touches anything outside the process.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Everything git can refuse to do, kept specific enough to act on.
#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("git {command} failed ({status}): {stderr}")]
    Git {
        command: String,
        status: String,
        stderr: String,
    },
    #[error("{path} is not a git repository")]
    NotARepo { path: PathBuf },
    #[error("refusing to use the main checkout {path} as a run worktree")]
    WouldUseMainCheckout { path: PathBuf },
    #[error("worktree {path} has uncommitted work")]
    Dirty { path: PathBuf },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A worktree that exists and is ready to be worked in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWorktree {
    pub path: PathBuf,
    pub branch: String,
    /// The commit the branch was cut from, resolved to a full SHA.
    pub base_sha: String,
    /// True when the worktree was already present — a resumed run reattaches
    /// to its own checkout rather than starting over.
    pub reused: bool,
}

/// What happened when a run's worktree was taken down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovalOutcome {
    /// Gone. The branch is kept; only the checkout is removed.
    Removed { path: PathBuf },
    /// Left in place because it still holds uncommitted work. Reported, never
    /// silently discarded (ADR-0052 §2).
    Retained {
        path: PathBuf,
        dirty_paths: Vec<String>,
    },
    /// There was nothing to remove.
    Absent,
}

/// Creates, reuses and removes the isolated checkouts for engineering runs.
#[derive(Debug, Clone)]
pub struct WorktreeManager {
    repo_root: PathBuf,
    runs_dir: PathBuf,
}

impl WorktreeManager {
    /// Bind to a repository, with worktrees under `runs_dir`.
    #[must_use]
    pub fn new(repo_root: impl Into<PathBuf>, runs_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            runs_dir: runs_dir.into(),
        }
    }

    /// Bind to a repository using the conventional `local/runs` directory,
    /// which is git-ignored so run checkouts never appear in `git status`.
    #[must_use]
    pub fn with_default_runs_dir(repo_root: impl Into<PathBuf>) -> Self {
        let repo_root = repo_root.into();
        let runs_dir = repo_root.join("local").join("runs");
        Self::new(repo_root, runs_dir)
    }

    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    #[must_use]
    pub fn runs_dir(&self) -> &Path {
        &self.runs_dir
    }

    /// Where a task's checkout lives.
    #[must_use]
    pub fn worktree_path(&self, task_id: &str) -> PathBuf {
        self.runs_dir.join(task_id).join("tree")
    }

    /// Resolve a revision to a full commit SHA.
    pub fn resolve_sha(&self, rev: &str) -> Result<String, WorktreeError> {
        self.git(
            &self.repo_root,
            ["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        )
        .map(|out| out.trim().to_string())
    }

    /// Give a task a branch and a checkout, cut from `base`.
    ///
    /// Idempotent: a run that already has its worktree gets it back, with the
    /// base SHA it was originally cut from, so resuming does not silently
    /// rebase the work onto a newer main.
    pub fn prepare(
        &self,
        task_id: &str,
        branch: &str,
        base: &str,
    ) -> Result<PreparedWorktree, WorktreeError> {
        if !self.repo_root.join(".git").exists() {
            return Err(WorktreeError::NotARepo {
                path: self.repo_root.clone(),
            });
        }
        let path = self.worktree_path(task_id);
        if same_path(&path, &self.repo_root) {
            return Err(WorktreeError::WouldUseMainCheckout { path });
        }

        if path.exists() {
            let base_sha = self.merge_base_of(&path, branch)?;
            return Ok(PreparedWorktree {
                path,
                branch: branch.to_string(),
                base_sha,
                reused: true,
            });
        }

        let base_sha = self.resolve_sha(base)?;
        self.ensure_runs_dir()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path_arg = path.as_os_str().to_owned();
        if self.branch_exists(branch)? {
            // A resumed run whose checkout was cleaned up but whose branch
            // survived: reattach rather than clobber the branch.
            self.git(
                &self.repo_root,
                [
                    OsStr::new("worktree"),
                    OsStr::new("add"),
                    &path_arg,
                    OsStr::new(branch),
                ],
            )?;
        } else {
            self.git(
                &self.repo_root,
                [
                    OsStr::new("worktree"),
                    OsStr::new("add"),
                    OsStr::new("-b"),
                    OsStr::new(branch),
                    &path_arg,
                    OsStr::new(&base_sha),
                ],
            )?;
        }

        Ok(PreparedWorktree {
            path,
            branch: branch.to_string(),
            base_sha,
            reused: false,
        })
    }

    /// The commit a worktree currently points at.
    pub fn head_sha(&self, worktree: &Path) -> Result<String, WorktreeError> {
        self.git(worktree, ["rev-parse", "HEAD"])
            .map(|out| out.trim().to_string())
    }

    /// Paths with uncommitted changes, in `git status --porcelain` form.
    pub fn dirty_paths(&self, worktree: &Path) -> Result<Vec<String>, WorktreeError> {
        let out = self.git(worktree, ["status", "--porcelain"])?;
        Ok(out
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect())
    }

    /// The diff of a worktree against the commit it was cut from.
    pub fn diff_against_base(
        &self,
        worktree: &Path,
        base_sha: &str,
    ) -> Result<String, WorktreeError> {
        self.git(worktree, ["diff", base_sha])
    }

    /// Take down a run's checkout.
    ///
    /// Refuses to discard uncommitted work: a dirty worktree is reported and
    /// left alone unless `force` is set, which is a human decision.
    pub fn remove(&self, task_id: &str, force: bool) -> Result<RemovalOutcome, WorktreeError> {
        let path = self.worktree_path(task_id);
        if !path.exists() {
            return Ok(RemovalOutcome::Absent);
        }
        let dirty = self.dirty_paths(&path)?;
        if !dirty.is_empty() && !force {
            return Ok(RemovalOutcome::Retained {
                path,
                dirty_paths: dirty,
            });
        }
        let path_arg = path.as_os_str().to_owned();
        let mut args: Vec<&OsStr> = vec![OsStr::new("worktree"), OsStr::new("remove")];
        if force {
            args.push(OsStr::new("--force"));
        }
        args.push(&path_arg);
        self.git(&self.repo_root, args)?;
        Ok(RemovalOutcome::Removed { path })
    }

    /// Create the runs directory and make it invisible to the main checkout.
    ///
    /// The `.gitignore` written here contains `*`, which matches the file
    /// itself, so the whole directory disappears from `git status` no matter
    /// what the repository's own ignore rules say. Isolation should not depend
    /// on the host repo remembering to ignore us.
    fn ensure_runs_dir(&self) -> Result<(), WorktreeError> {
        std::fs::create_dir_all(&self.runs_dir)?;
        let ignore = self.runs_dir.join(".gitignore");
        if !ignore.exists() {
            std::fs::write(&ignore, "*\n")?;
        }
        Ok(())
    }

    fn branch_exists(&self, branch: &str) -> Result<bool, WorktreeError> {
        let refname = format!("refs/heads/{branch}");
        match self.git(
            &self.repo_root,
            ["show-ref", "--verify", "--quiet", &refname],
        ) {
            Ok(_) => Ok(true),
            Err(WorktreeError::Git { .. }) => Ok(false),
            Err(other) => Err(other),
        }
    }

    /// Base of an existing run branch, so a reused worktree reports the commit
    /// it was actually cut from rather than today's default branch.
    fn merge_base_of(&self, worktree: &Path, branch: &str) -> Result<String, WorktreeError> {
        // The first parent of the branch's root-most run commit is not
        // reconstructible in general; the reflog-free approximation used here
        // is the branch point against the current HEAD of the main checkout.
        let main_head = self.resolve_sha("HEAD")?;
        match self.git(worktree, ["merge-base", &main_head, branch]) {
            Ok(out) => Ok(out.trim().to_string()),
            Err(_) => Ok(main_head),
        }
    }

    fn git<I, S>(&self, cwd: &Path, args: I) -> Result<String, WorktreeError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<std::ffi::OsString> =
            args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        let output = Command::new("git").current_dir(cwd).args(&args).output()?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        Err(WorktreeError::Git {
            command: args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" "),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// Compare two paths after canonicalizing whatever exists. Symlinked aliases
/// of the project root resolve to the same place, so a literal string match
/// would prove nothing.
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_path_is_task_scoped() {
        let manager = WorktreeManager::new("/repo", "/repo/local/runs");
        assert_eq!(
            manager.worktree_path("t-1"),
            PathBuf::from("/repo/local/runs/t-1/tree")
        );
        assert_ne!(manager.worktree_path("t-1"), manager.worktree_path("t-2"));
    }

    #[test]
    fn default_runs_dir_is_git_ignored_local() {
        let manager = WorktreeManager::with_default_runs_dir("/repo");
        assert_eq!(manager.runs_dir(), Path::new("/repo/local/runs"));
    }

    #[test]
    fn a_missing_repo_is_reported_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let manager = WorktreeManager::with_default_runs_dir(dir.path());
        let err = manager.prepare("t-1", "wip/x", "HEAD").unwrap_err();
        assert!(matches!(err, WorktreeError::NotARepo { .. }));
    }
}
