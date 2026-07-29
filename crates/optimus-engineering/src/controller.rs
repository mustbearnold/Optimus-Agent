//! Driving a run through the phase table on the strength of what commands did.
//!
//! [`DevTaskRun`] knows which phases may follow which, and what evidence each
//! one owes. It has no way to *produce* evidence — every test so far handed it
//! `EvidenceItem`s that a test wrote by hand. That is the gap this closes: the
//! driver runs the command, keeps the output, and records the exit status it
//! actually got.
//!
//! The one rule that makes this worth having: the driver never decides whether
//! a phase is finished. It records; [`DevTaskRun::advance_to`] judges. Every
//! step gets an evidence row whatever it did — the log is honest about failure
//! — but only a step that came out the way it had to counts toward the phase
//! contract. There is no path from "the model believes the tests pass" to a
//! phase transition.
//!
//! "The way it had to" is usually exit zero, and deliberately not always: a
//! step marked [`PhaseStep::expect_failure`] proves its point by failing. That
//! is the differential proof — the new regression test run against the base
//! commit, which must fail there or it is not testing the bug.
//!
//! Named `RunDriver` rather than `RunController` because `optimus-workflow`
//! already has a `RunController` with a different phase vocabulary and no
//! durability.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::command::{CommandError, CommandOutcome, CommandRunner};
use crate::phase::{DevPhase, EvidenceKind, TransitionError};
use crate::run::{DevTaskRun, EvidenceItem, RunError};

/// Default ceiling for a single step. A phase contract may say less; nothing
/// may say "no limit".
pub const DEFAULT_STEP_TIMEOUT: Duration = Duration::from_secs(900);

/// One command whose outcome is what a phase needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseStep {
    /// What this step proves, if it comes out as expected.
    pub evidence: EvidenceKind,
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
    /// Whether *failure* is what this step proves.
    ///
    /// Almost always false. The exception is the differential proof: the new
    /// regression test run against the base commit has to fail there, or it is
    /// not exercising the bug.
    pub expect_failure: bool,
    /// Human-readable note stored alongside the evidence.
    pub summary: String,
}

impl PhaseStep {
    #[must_use]
    pub fn new(evidence: EvidenceKind, program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            evidence,
            program: program.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            timeout: DEFAULT_STEP_TIMEOUT,
            expect_failure: false,
            summary: String::new(),
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// This step proves its point by failing — see [`PhaseStep::expect_failure`].
    #[must_use]
    pub fn expecting_failure(mut self) -> Self {
        self.expect_failure = true;
        self
    }

    #[must_use]
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("writing evidence log: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot resolve the worktree head: {0}")]
    NoHead(String),
}

/// One step, and whether it came out the way the phase needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome {
    pub evidence: EvidenceKind,
    pub outcome: CommandOutcome,
    /// True when the step proved what it was there to prove. For an ordinary
    /// step that means exit zero; for one marked
    /// [`PhaseStep::expect_failure`] it means the opposite.
    pub corroborated: bool,
}

/// What driving a phase produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveOutcome {
    /// Every step that ran, in order. An uncorroborated step ends the
    /// sequence, so this is shorter than the step list when something went
    /// wrong.
    pub steps: Vec<StepOutcome>,
    /// Whether the run left the phase. False means the evidence was not earned;
    /// the phase and its attempt counter are unchanged.
    pub advanced: bool,
    /// Evidence the contract still wants. Empty when `advanced` is true.
    pub missing: Vec<EvidenceKind>,
}

impl DriveOutcome {
    /// The first step that did not prove its point, if any.
    #[must_use]
    pub fn first_failure(&self) -> Option<&StepOutcome> {
        self.steps.iter().find(|s| !s.corroborated)
    }
}

/// Runs the commands a phase needs and records what they did.
///
/// Borrows the run rather than owning it, so the caller keeps responsibility
/// for persisting it — a driver that could silently drop a run's state would
/// undo the durability the phase table exists for.
pub struct RunDriver<'a, R: CommandRunner> {
    run: &'a mut DevTaskRun,
    runner: R,
    worktree: PathBuf,
    evidence_dir: PathBuf,
}

impl<'a, R: CommandRunner> RunDriver<'a, R> {
    /// `evidence_dir` is where captured output lands, so that the digests in
    /// the run record point at bytes that still exist.
    pub fn new(
        run: &'a mut DevTaskRun,
        runner: R,
        worktree: impl Into<PathBuf>,
        evidence_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            run,
            runner,
            worktree: worktree.into(),
            evidence_dir: evidence_dir.into(),
        }
    }

    #[must_use]
    pub fn run(&self) -> &DevTaskRun {
        self.run
    }

    #[must_use]
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    /// The worktree's current commit, read from git rather than remembered.
    ///
    /// # Errors
    /// When git cannot be run or the worktree has no commit yet.
    pub fn head_sha(&self) -> Result<String, ControllerError> {
        let outcome = self.runner.run(
            &self.worktree,
            "git",
            &["rev-parse".to_string(), "HEAD".to_string()],
            Duration::from_secs(30),
        )?;
        if !outcome.succeeded() {
            return Err(ControllerError::NoHead(
                String::from_utf8_lossy(&outcome.stderr).trim().to_string(),
            ));
        }
        let sha = String::from_utf8_lossy(&outcome.stdout).trim().to_string();
        if sha.is_empty() {
            return Err(ControllerError::NoHead("git printed nothing".into()));
        }
        Ok(sha)
    }

    /// Run one step and record it, whatever it did.
    ///
    /// Recording is unconditional on purpose. A red test run is a fact about
    /// this attempt and belongs in the log; it simply does not satisfy
    /// anything.
    ///
    /// # Errors
    /// When the command could not be started, the output could not be stored,
    /// or the run refused the evidence.
    pub fn earn(&mut self, step: &PhaseStep) -> Result<StepOutcome, ControllerError> {
        let sha = self.head_sha()?;
        let outcome = self
            .runner
            .run(&self.worktree, &step.program, &step.args, step.timeout)?;

        let output = outcome.combined_output();
        let digest = self.store_output(&output)?;
        let summary = if step.summary.is_empty() {
            describe(&outcome)
        } else {
            format!("{} — {}", step.summary, describe(&outcome))
        };

        let record = if step.expect_failure {
            EvidenceItem::observed_failing
        } else {
            EvidenceItem::observed
        };
        let draft = record(
            step.evidence,
            outcome.command_line(),
            outcome.status_code(),
            sha,
            &output,
        )
        .with_summary(summary);
        debug_assert_eq!(
            crate::run::digest(&output),
            digest,
            "stored log and recorded digest must agree"
        );
        let corroborated = self.run.record(draft)?.corroborates;
        Ok(StepOutcome {
            evidence: step.evidence,
            outcome,
            corroborated,
        })
    }

    /// Run every step, then try to leave the phase for `next`.
    ///
    /// Stops at the first step that does not exit zero: later steps in a phase
    /// generally assume the earlier ones held, and a cascade of failures caused
    /// by one root cause is noise in the record. The phase is not advanced, so
    /// the caller can repair and re-enter — which bumps the attempt counter and
    /// makes this attempt's evidence stop counting.
    ///
    /// # Errors
    /// When a command could not be started or its output could not be stored.
    /// A refused transition is **not** an error: it is `advanced: false` plus
    /// the list of what is missing, because failing to earn a phase is an
    /// expected outcome rather than a malfunction.
    pub fn drive(
        &mut self,
        steps: &[PhaseStep],
        next: DevPhase,
    ) -> Result<DriveOutcome, ControllerError> {
        let mut done = Vec::with_capacity(steps.len());
        for step in steps {
            let result = self.earn(step)?;
            let stop = !result.corroborated;
            done.push(result);
            if stop {
                break;
            }
        }

        let missing = self
            .run
            .phase
            .contract()
            .missing_evidence(&self.run.satisfied_evidence());
        if !missing.is_empty() {
            return Ok(DriveOutcome {
                steps: done,
                advanced: false,
                missing,
            });
        }

        match self.run.advance_to(next) {
            Ok(()) => Ok(DriveOutcome {
                steps: done,
                advanced: true,
                missing: Vec::new(),
            }),
            Err(RunError::Transition(TransitionError::MissingEvidence { missing, .. })) => {
                Ok(DriveOutcome {
                    steps: done,
                    advanced: false,
                    missing,
                })
            }
            Err(other) => Err(other.into()),
        }
    }

    /// Write captured output where the run record's digest can find it.
    fn store_output(&self, output: &[u8]) -> Result<String, ControllerError> {
        Ok(store_output(&self.evidence_dir, output)?)
    }
}

/// Write captured output under `evidence_dir`, named by its digest, so the
/// digests in the run record point at bytes that still exist. Shared with
/// [`crate::delivery`], whose push and PR receipts keep their logs the same
/// way a phase step does.
pub(crate) fn store_output(evidence_dir: &Path, output: &[u8]) -> Result<String, std::io::Error> {
    let digest = crate::run::digest(output);
    std::fs::create_dir_all(evidence_dir)?;
    let path = evidence_dir.join(format!("{digest}.log"));
    // Content-addressed, so an identical capture is already correct on
    // disk and rewriting it would only risk truncating a good file.
    if !path.exists() {
        std::fs::write(&path, output)?;
    }
    Ok(digest)
}

/// A one-line account of what happened, for a human reading the run record.
pub(crate) fn describe(outcome: &CommandOutcome) -> String {
    let what = if outcome.timed_out {
        format!("timed out after {}ms", outcome.duration_ms)
    } else {
        match outcome.exit_code {
            Some(0) => format!("exit 0 in {}ms", outcome.duration_ms),
            Some(code) => format!("exit {code} in {}ms", outcome.duration_ms),
            None => format!("killed by signal after {}ms", outcome.duration_ms),
        }
    };
    if outcome.stdout_truncated || outcome.stderr_truncated {
        format!("{what} (output truncated)")
    } else {
        what
    }
}
