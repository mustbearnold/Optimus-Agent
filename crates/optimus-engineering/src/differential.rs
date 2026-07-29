//! Proving a fix by running its test at the commit the bug still lives in
//! (program P42, E42.4).
//!
//! A green test suite on a patch says almost nothing. It says the tests pass —
//! not that any of them would have caught the bug. The claim "this fixes issue
//! N" is only supported when the *new* test is run against the code from
//! *before* the fix and fails there. A regression test that passes without the
//! fix is decoration.
//!
//! So the proof is differential, and it has a shape that is easy to get subtly
//! wrong:
//!
//! 1. Check out the base commit — the code as it was, with none of the fix.
//! 2. Carry **only the test files** across from the patch. Not the fix. The
//!    test does not exist at base, and a harness asked to run a test that is
//!    not there exits non-zero, which is indistinguishable from the test
//!    failing. That false red is the most convincing wrong answer available
//!    here, and carrying the test is what removes it.
//! 3. Verify the base checkout is dirty in *exactly* the carried paths. If
//!    anything else moved, the fix came along for the ride and the whole
//!    exercise proves nothing.
//! 4. Run the test at base. It must fail.
//! 5. Run the same test on the patch. It must pass.
//!
//! Only fail-then-pass is [`DifferentialVerdict::Proven`]. Every other
//! combination is named and refused, and a base run that never got as far as
//! executing the test is [`DifferentialVerdict::Inconclusive`] — a third state,
//! for the same reason
//! [`crate::repository::BranchProtection`] has one: "could not tell" is not
//! "answered no".

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::command::{CommandError, CommandOutcome, CommandRunner};

/// Where base checkouts are cut, relative to the runs directory.
const BASE_CHECKOUT_DIR: &str = "differential";

/// Output fragments that mean the harness never got to the test.
///
/// These can only move a verdict *toward* [`DifferentialVerdict::Inconclusive`],
/// never toward `Proven` — the same asymmetry the impact selector uses. A build
/// break at base that none of these match is read as a genuine failure, which
/// is the residual risk and is documented rather than papered over.
const NEVER_RAN_MARKERS: &[&str] = &[
    "error: could not compile",
    "error[E0",
    "error: linking with",
    "No such file or directory",
    "command not found",
    "ModuleNotFoundError",
    "SyntaxError",
    "cannot find module",
    "Cannot find module",
    "unresolved import",
];

#[derive(Debug, thiserror::Error)]
pub enum DifferentialError {
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("git {command} failed ({status}): {stderr}")]
    Git {
        command: String,
        status: String,
        stderr: String,
    },
    #[error("no test paths were named; a differential proof needs a test to carry")]
    NoTestPaths,
    #[error("test path {0} does not exist in the patch worktree")]
    MissingTestPath(PathBuf),
    #[error("test path {0} escapes the patch worktree")]
    EscapingTestPath(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// What running the same test at two commits established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DifferentialVerdict {
    /// Failed at base, passed on the patch. The only verdict that supports a
    /// claim that the patch fixes the bug.
    Proven,
    /// Passed at base. The test does not exercise the bug, so it would not
    /// have caught it and will not catch its return.
    TestPassesWithoutTheFix,
    /// Failed at both. Whatever the test measures, the patch has not changed
    /// it.
    NotFixed,
    /// Passed at base, failed on the patch — the patch broke a test that
    /// previously worked. Recorded distinctly because it is the one outcome
    /// that is worse than no proof at all.
    PatchBrokeIt,
    /// The base run cannot be read as a result: it timed out, it did not
    /// build, or the base checkout was dirty beyond the carried tests. A red
    /// exit status and "never got as far as the test" look identical from
    /// outside, so they are not merged.
    Inconclusive { reason: String },
}

impl DifferentialVerdict {
    /// Whether this verdict supports advancing on the strength of the fix.
    ///
    /// Exactly one variant does. Written as a match rather than as
    /// `!= Refused` so that a new variant is a compile error here instead of
    /// silently defaulting to "good enough".
    #[must_use]
    pub fn proves_the_fix(&self) -> bool {
        matches!(self, Self::Proven)
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proven => "proven",
            Self::TestPassesWithoutTheFix => "test-passes-without-the-fix",
            Self::NotFixed => "not-fixed",
            Self::PatchBrokeIt => "patch-broke-it",
            Self::Inconclusive { .. } => "inconclusive",
        }
    }

    /// Why a run should not claim this fix, in words a human can act on.
    #[must_use]
    pub fn explain(&self) -> String {
        match self {
            Self::Proven => "the test fails at the base commit and passes on the patch".to_string(),
            Self::TestPassesWithoutTheFix => {
                "the test passes without the fix, so it is not testing the bug".to_string()
            }
            Self::NotFixed => "the test fails on the patch too; the bug is still there".to_string(),
            Self::PatchBrokeIt => {
                "the test passed at the base commit and fails on the patch".to_string()
            }
            Self::Inconclusive { reason } => format!("the base run proved nothing: {reason}"),
        }
    }
}

/// What to prove, and with what.
#[derive(Debug, Clone)]
pub struct DifferentialRequest {
    /// Names the base checkout so concurrent runs do not share one.
    pub task_id: String,
    /// The commit the bug still lives in.
    pub base_sha: String,
    /// The worktree holding the fix and the new test.
    pub patch_worktree: PathBuf,
    /// Test files to carry back to base, relative to the patch worktree.
    ///
    /// Only tests. Carrying a source file carries the fix, and the base run
    /// stops being a base run.
    pub test_paths: Vec<PathBuf>,
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

/// A differential proof and everything it rests on.
#[derive(Debug, Clone)]
pub struct DifferentialProof {
    pub verdict: DifferentialVerdict,
    pub base_sha: String,
    /// `None` when the base run never happened — the checkout or the carry
    /// failed first.
    pub base: Option<CommandOutcome>,
    /// `None` when the base verdict made the patch run pointless.
    pub patch: Option<CommandOutcome>,
    /// Test files carried back, in the order they were named.
    pub carried: Vec<PathBuf>,
    /// Carried paths that did not exist at base at all — the genuinely new
    /// tests. A proof where this is empty is proving something about an
    /// existing test, which is legitimate but different.
    pub newly_added: Vec<PathBuf>,
}

impl DifferentialProof {
    #[must_use]
    pub fn proves_the_fix(&self) -> bool {
        self.verdict.proves_the_fix()
    }

    /// One line for an evidence record.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "differential against {}: {} — {}",
            short_sha(&self.base_sha),
            self.verdict.as_str(),
            self.verdict.explain()
        )
    }
}

/// Cuts base checkouts and cleans them up.
#[derive(Debug, Clone)]
pub struct DifferentialProver {
    repo_root: PathBuf,
    runs_dir: PathBuf,
}

impl DifferentialProver {
    #[must_use]
    pub fn new(repo_root: impl Into<PathBuf>, runs_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            runs_dir: runs_dir.into(),
        }
    }

    #[must_use]
    pub fn base_checkout_path(&self, task_id: &str) -> PathBuf {
        self.runs_dir.join(BASE_CHECKOUT_DIR).join(task_id)
    }

    /// Run the test at base and on the patch, and say what that established.
    ///
    /// # Errors
    /// When the request is malformed — no tests named, a named test missing or
    /// pointing outside the worktree — or when git or the runner fails outright.
    /// A test that *ran and failed* is never an error; it is the point.
    pub fn prove<R: CommandRunner>(
        &self,
        runner: &R,
        request: &DifferentialRequest,
    ) -> Result<DifferentialProof, DifferentialError> {
        let carried = self.validate_paths(request)?;
        let checkout = self.cut_base_checkout(&request.task_id, &request.base_sha)?;

        let mut newly_added = Vec::new();
        for relative in &carried {
            let destination = checkout.join(relative);
            if !destination.exists() {
                newly_added.push(relative.clone());
            }
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(request.patch_worktree.join(relative), &destination)?;
        }

        // Step 3. If anything but the carried tests moved, the fix is in the
        // base checkout and every result below is meaningless.
        if let Some(reason) = self.stowaway_reason(&checkout, &carried)? {
            return Ok(DifferentialProof {
                verdict: DifferentialVerdict::Inconclusive { reason },
                base_sha: request.base_sha.clone(),
                base: None,
                patch: None,
                carried,
                newly_added,
            });
        }

        let base = runner.run(&checkout, &request.program, &request.args, request.timeout)?;
        if let Some(reason) = never_ran_reason(&base) {
            return Ok(DifferentialProof {
                verdict: DifferentialVerdict::Inconclusive { reason },
                base_sha: request.base_sha.clone(),
                base: Some(base),
                patch: None,
                carried,
                newly_added,
            });
        }

        let patch = runner.run(
            &request.patch_worktree,
            &request.program,
            &request.args,
            request.timeout,
        )?;

        let verdict = match (base.succeeded(), patch.succeeded()) {
            (false, true) => DifferentialVerdict::Proven,
            (true, true) => DifferentialVerdict::TestPassesWithoutTheFix,
            (false, false) => DifferentialVerdict::NotFixed,
            (true, false) => DifferentialVerdict::PatchBrokeIt,
        };

        Ok(DifferentialProof {
            verdict,
            base_sha: request.base_sha.clone(),
            base: Some(base),
            patch: Some(patch),
            carried,
            newly_added,
        })
    }

    /// Take down a base checkout. Always safe to discard: nothing is authored
    /// here, only copied in.
    ///
    /// # Errors
    /// When git refuses to remove the worktree.
    pub fn discard_base_checkout(&self, task_id: &str) -> Result<(), DifferentialError> {
        let path = self.base_checkout_path(task_id);
        if !path.exists() {
            return Ok(());
        }
        self.git(
            &self.repo_root,
            [
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                path.as_os_str(),
            ],
        )?;
        Ok(())
    }

    fn validate_paths(
        &self,
        request: &DifferentialRequest,
    ) -> Result<Vec<PathBuf>, DifferentialError> {
        if request.test_paths.is_empty() {
            return Err(DifferentialError::NoTestPaths);
        }
        let mut seen = BTreeSet::new();
        let mut carried = Vec::new();
        for path in &request.test_paths {
            if path.is_absolute() || path.components().any(|c| c.as_os_str() == "..") {
                return Err(DifferentialError::EscapingTestPath(path.clone()));
            }
            if !request.patch_worktree.join(path).is_file() {
                return Err(DifferentialError::MissingTestPath(path.clone()));
            }
            if seen.insert(path.clone()) {
                carried.push(path.clone());
            }
        }
        Ok(carried)
    }

    fn cut_base_checkout(
        &self,
        task_id: &str,
        base_sha: &str,
    ) -> Result<PathBuf, DifferentialError> {
        let path = self.base_checkout_path(task_id);
        if path.exists() {
            // A stale checkout from an interrupted proof would carry whatever
            // the last attempt copied in. Start clean every time.
            self.discard_base_checkout(task_id)?;
            if path.exists() {
                std::fs::remove_dir_all(&path)?;
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Detached: the base checkout is read-only in spirit and must never
        // hold a branch a run might later push.
        self.git(
            &self.repo_root,
            [
                OsStr::new("worktree"),
                OsStr::new("add"),
                OsStr::new("--detach"),
                path.as_os_str(),
                OsStr::new(base_sha),
            ],
        )?;
        Ok(path)
    }

    /// Anything dirty in the base checkout that is not a carried test.
    fn stowaway_reason(
        &self,
        checkout: &Path,
        carried: &[PathBuf],
    ) -> Result<Option<String>, DifferentialError> {
        let expected: BTreeSet<String> = carried
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        let status = self.git(checkout, [OsStr::new("status"), OsStr::new("--porcelain")])?;
        let mut stowaways: Vec<String> = Vec::new();
        for line in status.lines() {
            let entry = line.get(3..).unwrap_or("").trim().trim_matches('"');
            if entry.is_empty() || expected.contains(entry) {
                continue;
            }
            stowaways.push(entry.to_string());
        }
        if stowaways.is_empty() {
            return Ok(None);
        }
        stowaways.sort();
        Ok(Some(format!(
            "the base checkout also carries {} — the fix may be in it",
            stowaways.join(", ")
        )))
    }

    fn git<I, S>(&self, cwd: &Path, args: I) -> Result<String, DifferentialError>
    where
        I: IntoIterator<Item = S> + Clone,
        S: AsRef<OsStr>,
    {
        let rendered = args
            .clone()
            .into_iter()
            .map(|a| a.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let output = Command::new("git").current_dir(cwd).args(args).output()?;
        if !output.status.success() {
            return Err(DifferentialError::Git {
                command: rendered,
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Whether a base run failed for a reason that is not "the test failed".
fn never_ran_reason(outcome: &CommandOutcome) -> Option<String> {
    if outcome.timed_out {
        return Some("the base run timed out before finishing".to_string());
    }
    if outcome.succeeded() {
        return None;
    }
    let haystack = format!(
        "{}{}",
        String::from_utf8_lossy(&outcome.stdout),
        String::from_utf8_lossy(&outcome.stderr)
    );
    NEVER_RAN_MARKERS
        .iter()
        .find(|marker| haystack.contains(**marker))
        .map(|marker| format!("the base checkout did not build or load ({marker})"))
}

fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(code: i32, stdout: &str, stderr: &str) -> CommandOutcome {
        CommandOutcome {
            program: "cargo".into(),
            args: vec!["test".into()],
            exit_code: Some(code),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
            duration_ms: 1,
        }
    }

    #[test]
    fn a_green_base_run_is_never_inconclusive() {
        // Success is unambiguous: the test ran and passed. Only failure needs
        // to be disambiguated from "never ran".
        assert!(never_ran_reason(&outcome(0, "test result: ok", "")).is_none());
    }

    #[test]
    fn a_failing_test_at_base_reads_as_a_failing_test() {
        let reason = never_ran_reason(&outcome(101, "test result: FAILED. 1 failed", ""));
        assert!(reason.is_none(), "{reason:?}");
    }

    #[test]
    fn a_build_break_at_base_is_not_a_failing_test() {
        let reason = never_ran_reason(&outcome(101, "", "error[E0599]: no method named `fix`"));
        assert!(reason.is_some());
    }

    #[test]
    fn a_timeout_at_base_proves_nothing() {
        let mut timed_out = outcome(0, "", "");
        timed_out.timed_out = true;
        assert!(never_ran_reason(&timed_out).is_some());
    }

    #[test]
    fn only_proven_supports_the_fix() {
        assert!(DifferentialVerdict::Proven.proves_the_fix());
        for verdict in [
            DifferentialVerdict::TestPassesWithoutTheFix,
            DifferentialVerdict::NotFixed,
            DifferentialVerdict::PatchBrokeIt,
            DifferentialVerdict::Inconclusive { reason: "x".into() },
        ] {
            assert!(!verdict.proves_the_fix(), "{verdict:?}");
        }
    }

    #[test]
    fn every_verdict_explains_itself_without_repeating_its_own_name() {
        for verdict in [
            DifferentialVerdict::Proven,
            DifferentialVerdict::TestPassesWithoutTheFix,
            DifferentialVerdict::NotFixed,
            DifferentialVerdict::PatchBrokeIt,
            DifferentialVerdict::Inconclusive {
                reason: "the base run timed out".into(),
            },
        ] {
            let explanation = verdict.explain();
            assert!(explanation.len() > 20, "{verdict:?}: {explanation}");
            assert!(!explanation.contains(verdict.as_str()), "{explanation}");
        }
    }

    #[test]
    fn a_short_sha_survives_a_sha_that_is_already_short() {
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha("0123456789abcdef"), "0123456789ab");
    }
}
