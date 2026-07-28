//! The durable development-task record (ADR-0052 §1, §5).
//!
//! A development task is an object, not a conversation. It knows where it
//! branched from, which worktree is its own, which phase it is in, what it has
//! actually observed, and why it is not currently advancing. It survives a
//! process restart because it is written to disk after every mutation, not
//! because a session happened to stay open.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::phase::{DevPhase, EvidenceKind, TransitionError};

/// Schema version of the persisted record. A run pinned to an older version
/// still loads; the phase table it ran under is part of its history.
pub const RECORD_VERSION: u32 = 1;

/// Where the task came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TaskOrigin {
    /// A GitHub issue in the bound repository.
    Issue { number: u64, title: String },
    /// A human asked directly, with no issue behind it.
    Request { text: String },
}

/// One observed fact. Not a model claim: a command that ran, a file that
/// exists, a diff that was produced, a human who answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceItem {
    /// Monotonic within a run. The record is append-only, so this is also the
    /// insertion order and the replay order.
    pub seq: u64,
    pub kind: EvidenceKind,
    /// Phase that produced it. Evidence never satisfies a phase it did not
    /// come from.
    pub phase: DevPhase,
    /// Attempt within that phase. A re-entered phase does not inherit the
    /// previous attempt's evidence.
    pub attempt: u32,
    /// Worktree commit the observation was made against, when it had one.
    pub sha: Option<String>,
    /// Exact command, when the evidence came from running something.
    pub command: Option<String>,
    pub exit_status: Option<i32>,
    /// Digest of the captured output, so a large log can be referenced without
    /// being carried in the record.
    pub output_digest: Option<String>,
    /// Whether the observation came out the way it needed to.
    ///
    /// Not the same as "exited zero". A differential proof runs the new
    /// regression test against the *base* commit, where it must **fail** — a
    /// test that passes without the fix is not testing the bug. So the record
    /// keeps the raw exit status and, separately, whether that status was the
    /// one that proved the point. Only corroborating evidence satisfies a
    /// phase contract.
    pub corroborates: bool,
    pub summary: String,
}

impl EvidenceItem {
    /// A bare fact with no command behind it — a plan, a set of criteria, an
    /// approval.
    #[must_use]
    pub fn stated(kind: EvidenceKind, summary: impl Into<String>) -> EvidenceDraft {
        EvidenceDraft {
            kind,
            sha: None,
            command: None,
            exit_status: None,
            output_digest: None,
            corroborates: true,
            summary: summary.into(),
        }
    }

    /// A command that ran and had to pass, with the commit it ran against and
    /// the output it produced. This is the shape most phase contracts want.
    ///
    /// A non-zero status is still recorded — the log does not hide failures —
    /// but it corroborates nothing, so the phase stays where it is.
    #[must_use]
    pub fn observed(
        kind: EvidenceKind,
        command: impl Into<String>,
        exit_status: i32,
        sha: impl Into<String>,
        output: &[u8],
    ) -> EvidenceDraft {
        Self::draft(kind, command, exit_status, sha, output, exit_status == 0)
    }

    /// A command whose **failure** is the point.
    ///
    /// The differential proof: run the new regression test against the base
    /// commit, where it must fail. If it passes there, the test does not
    /// exercise the bug and the fix is unverified — so here a zero exit is
    /// what corroborates nothing.
    #[must_use]
    pub fn observed_failing(
        kind: EvidenceKind,
        command: impl Into<String>,
        exit_status: i32,
        sha: impl Into<String>,
        output: &[u8],
    ) -> EvidenceDraft {
        Self::draft(kind, command, exit_status, sha, output, exit_status != 0)
    }

    fn draft(
        kind: EvidenceKind,
        command: impl Into<String>,
        exit_status: i32,
        sha: impl Into<String>,
        output: &[u8],
        corroborates: bool,
    ) -> EvidenceDraft {
        EvidenceDraft {
            kind,
            sha: Some(sha.into()),
            command: Some(command.into()),
            exit_status: Some(exit_status),
            output_digest: Some(digest(output)),
            corroborates,
            summary: String::new(),
        }
    }
}

/// An evidence item before the run stamps it with a sequence, phase and
/// attempt. Callers cannot forge those three fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceDraft {
    kind: EvidenceKind,
    sha: Option<String>,
    command: Option<String>,
    exit_status: Option<i32>,
    output_digest: Option<String>,
    corroborates: bool,
    summary: String,
}

impl EvidenceDraft {
    #[must_use]
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }
}

/// Why a run is not advancing. A halted run without one of these is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    /// Waiting for a human decision the run may not make alone.
    AwaitingApproval,
    /// The phase used its attempts.
    AttemptsExhausted,
    /// The phase exceeded its declared timeout.
    Timeout,
    /// A tool or provider failed in a way the run cannot repair.
    ToolFailure,
    /// The issue was rejected at triage as too vague or too large.
    NotActionable,
    /// A human stopped it.
    HumanInterrupt,
}

/// The stop, with the phase it happened in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopReason {
    pub kind: StopKind,
    pub phase: DevPhase,
    pub detail: String,
}

/// Consumption against the run's ceilings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunBudget {
    pub wall_secs: u64,
    pub cost_micros: u64,
    pub model_calls: u64,
    pub tool_calls: u64,
}

/// The accepted plan for the task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPlan {
    pub acceptance_criteria: Vec<String>,
    pub expected_files: Vec<String>,
    pub relevant_tests: Vec<String>,
    pub risk_class: String,
    pub stop_condition: String,
}

/// Things that can go wrong that are not transition refusals.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("transition refused: {0}")]
    Transition(#[from] TransitionError),
    #[error("run is halted in {phase:?}; resume it before recording")]
    Halted { phase: DevPhase },
    #[error("path {path} is outside the run's worktree")]
    OutsideWorktree { path: PathBuf },
    #[error("run has no worktree yet")]
    NoWorktree,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("record is not valid json: {0}")]
    Decode(#[from] serde_json::Error),
}

/// One development task, whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevTaskRun {
    pub version: u32,
    pub task_id: String,
    pub origin: TaskOrigin,
    /// Canonical project root the task belongs to.
    pub repo_root: PathBuf,
    /// Commit the run branched from. `None` until `PrepareWorktree`.
    pub base_sha: Option<String>,
    pub branch: Option<String>,
    /// The run's isolated checkout, and its containment boundary once set.
    pub worktree_path: Option<PathBuf>,
    pub phase: DevPhase,
    /// Phase a halted run should return to.
    pub halted_from: Option<DevPhase>,
    pub plan: Option<TaskPlan>,
    /// Append-only. Nothing removes an item once recorded.
    pub evidence: Vec<EvidenceItem>,
    /// Entries per phase, so a re-entered phase does not reuse stale evidence.
    pub phase_attempts: BTreeMap<DevPhase, u32>,
    pub budget: RunBudget,
    pub stop_reason: Option<StopReason>,
}

impl DevTaskRun {
    /// A new run at `Intake`, bound to a project root and nothing else yet.
    #[must_use]
    pub fn new(
        task_id: impl Into<String>,
        origin: TaskOrigin,
        repo_root: impl Into<PathBuf>,
    ) -> Self {
        let mut phase_attempts = BTreeMap::new();
        phase_attempts.insert(DevPhase::Intake, 1);
        Self {
            version: RECORD_VERSION,
            task_id: task_id.into(),
            origin,
            repo_root: repo_root.into(),
            base_sha: None,
            branch: None,
            worktree_path: None,
            phase: DevPhase::Intake,
            halted_from: None,
            plan: None,
            evidence: Vec::new(),
            phase_attempts,
            budget: RunBudget::default(),
            stop_reason: None,
        }
    }

    /// Current attempt number at the current phase, 1-based.
    #[must_use]
    pub fn attempt(&self) -> u32 {
        self.phase_attempts.get(&self.phase).copied().unwrap_or(1)
    }

    /// Append an observation, stamped with the phase and attempt that produced
    /// it. A halted run records nothing: resume it first.
    pub fn record(&mut self, draft: EvidenceDraft) -> Result<&EvidenceItem, RunError> {
        if self.phase.is_halted() {
            return Err(RunError::Halted { phase: self.phase });
        }
        let seq = self.evidence.len() as u64 + 1;
        self.evidence.push(EvidenceItem {
            seq,
            kind: draft.kind,
            phase: self.phase,
            attempt: self.attempt(),
            sha: draft.sha,
            command: draft.command,
            exit_status: draft.exit_status,
            output_digest: draft.output_digest,
            corroborates: draft.corroborates,
            summary: draft.summary,
        });
        Ok(self.evidence.last().expect("just pushed"))
    }

    /// Evidence kinds recorded by the current phase in its current attempt that
    /// came out the way they had to.
    ///
    /// Deliberately narrow on three axes. A diff produced during `Implement`
    /// does not let `Repair` exit. An earlier attempt's green test run does not
    /// let a re-entered phase exit without running anything. And an observation
    /// that did not corroborate — a test run that went red, or a differential
    /// proof that passed at base when it should have failed — is kept in the
    /// record but satisfies nothing. It is evidence the phase is *not*
    /// finished.
    #[must_use]
    pub fn satisfied_evidence(&self) -> Vec<EvidenceKind> {
        let attempt = self.attempt();
        self.evidence
            .iter()
            .filter(|item| item.phase == self.phase && item.attempt == attempt)
            .filter(|item| item.corroborates)
            .map(|item| item.kind)
            .collect()
    }

    /// Whether the current phase has everything its contract demands.
    #[must_use]
    pub fn can_exit_phase(&self) -> bool {
        self.phase
            .contract()
            .missing_evidence(&self.satisfied_evidence())
            .is_empty()
    }

    /// Move to `next`, if the table allows it and the evidence exists.
    ///
    /// This is the only way forward. Nothing else in the crate writes `phase`.
    pub fn advance_to(&mut self, next: DevPhase) -> Result<(), RunError> {
        if self.phase.is_terminal() {
            return Err(TransitionError::Terminal { phase: self.phase }.into());
        }
        if self.phase.is_halted() {
            return Err(RunError::Halted { phase: self.phase });
        }
        if !self.phase.allowed_next().contains(&next) {
            return Err(TransitionError::NotAllowed {
                from: self.phase,
                to: next,
            }
            .into());
        }
        let contract = self.phase.contract();
        let missing = contract.missing_evidence(&self.satisfied_evidence());
        if !missing.is_empty() {
            return Err(TransitionError::MissingEvidence {
                phase: self.phase,
                missing,
            }
            .into());
        }
        self.enter(next)
    }

    /// Halt the run, remembering where to come back to.
    pub fn halt(&mut self, kind: StopKind, detail: impl Into<String>) -> Result<(), RunError> {
        if self.phase.is_terminal() {
            return Err(TransitionError::Terminal { phase: self.phase }.into());
        }
        let target = match kind {
            StopKind::NotActionable => DevPhase::Abandoned,
            _ => DevPhase::Blocked,
        };
        self.stop_reason = Some(StopReason {
            kind,
            phase: self.phase,
            detail: detail.into(),
        });
        self.halted_from = Some(self.phase);
        self.phase = target;
        Ok(())
    }

    /// Return a blocked run to the phase it was blocked from, as a fresh
    /// attempt. An abandoned run does not resume.
    pub fn resume(&mut self) -> Result<DevPhase, RunError> {
        if self.phase != DevPhase::Blocked {
            return Err(TransitionError::Terminal { phase: self.phase }.into());
        }
        let target = self.halted_from.unwrap_or(DevPhase::Intake);
        self.halted_from = None;
        self.stop_reason = None;
        self.enter(target)?;
        Ok(target)
    }

    /// Assert a path lies inside the run's own checkout (ADR-0052 §2).
    ///
    /// The comparison is against the recorded worktree path; callers that can
    /// see the filesystem should canonicalize before calling.
    pub fn assert_within_worktree(&self, path: &Path) -> Result<(), RunError> {
        let root = self.worktree_path.as_ref().ok_or(RunError::NoWorktree)?;
        if path.starts_with(root) {
            Ok(())
        } else {
            Err(RunError::OutsideWorktree {
                path: path.to_path_buf(),
            })
        }
    }

    /// Persist the record. Written to a sibling temp file and renamed, so a
    /// crash mid-write leaves the previous record intact rather than a
    /// truncated one.
    pub fn save(&self, path: &Path) -> Result<(), RunError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Read a record back. The phase, worktree and evidence come back exactly
    /// as they were; nothing is recomputed.
    pub fn load(path: &Path) -> Result<Self, RunError> {
        let bytes = fs::read(path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Conventional location of a run's record under a runs directory.
    #[must_use]
    pub fn record_path(runs_dir: &Path, task_id: &str) -> PathBuf {
        runs_dir.join(task_id).join("run.json")
    }

    fn enter(&mut self, phase: DevPhase) -> Result<(), RunError> {
        let contract = phase.contract();
        let attempts = self.phase_attempts.entry(phase).or_insert(0);
        *attempts += 1;
        if *attempts > contract.max_attempts {
            let max_attempts = contract.max_attempts;
            self.stop_reason = Some(StopReason {
                kind: StopKind::AttemptsExhausted,
                phase,
                detail: format!("{} attempts at {}", max_attempts, phase.as_str()),
            });
            self.halted_from = Some(phase);
            self.phase = DevPhase::Blocked;
            return Err(TransitionError::AttemptsExhausted {
                phase,
                max_attempts,
            }
            .into());
        }
        self.phase = phase;
        Ok(())
    }
}

/// Hex SHA-256 of a byte slice. Used for output digests so a record can point
/// at a large log without carrying it.
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> DevTaskRun {
        DevTaskRun::new(
            "t-1",
            TaskOrigin::Issue {
                number: 42,
                title: "boom".into(),
            },
            "/repo",
        )
    }

    #[test]
    fn a_phase_cannot_exit_without_its_evidence() {
        let mut run = run();
        let err = run.advance_to(DevPhase::Triage).unwrap_err();
        assert!(matches!(
            err,
            RunError::Transition(TransitionError::MissingEvidence { .. })
        ));
    }

    #[test]
    fn evidence_from_another_phase_does_not_count() {
        let mut run = run();
        run.record(EvidenceItem::stated(EvidenceKind::ProblemStatement, "p"))
            .unwrap();
        run.advance_to(DevPhase::Triage).unwrap();
        // Triage wants acceptance criteria and a reproduction; the intake
        // statement is recorded but belongs to Intake.
        assert!(!run.can_exit_phase());
    }

    #[test]
    fn a_re_entered_phase_starts_with_no_evidence() {
        let mut run = run();
        run.record(EvidenceItem::stated(EvidenceKind::ProblemStatement, "p"))
            .unwrap();
        run.advance_to(DevPhase::Triage).unwrap();
        run.record(EvidenceItem::stated(EvidenceKind::AcceptanceCriteria, "a"))
            .unwrap();
        run.record(EvidenceItem::stated(EvidenceKind::Reproduction, "r"))
            .unwrap();
        assert!(run.can_exit_phase());

        run.halt(StopKind::HumanInterrupt, "coffee").unwrap();
        assert_eq!(run.resume().unwrap(), DevPhase::Triage);
        // Same phase, new attempt: the old evidence is still on the record but
        // no longer satisfies the contract.
        assert_eq!(run.attempt(), 2);
        assert!(!run.can_exit_phase());
        assert_eq!(run.evidence.len(), 3);
    }

    #[test]
    fn attempts_exhausted_blocks_rather_than_looping() {
        let mut run = run();
        // Intake allows one attempt; a second entry must block.
        run.record(EvidenceItem::stated(EvidenceKind::ProblemStatement, "p"))
            .unwrap();
        run.advance_to(DevPhase::Triage).unwrap();
        run.halt(StopKind::HumanInterrupt, "stop").unwrap();
        run.resume().unwrap();
        run.halt(StopKind::HumanInterrupt, "stop").unwrap();
        let err = run.resume().unwrap_err();
        assert!(matches!(
            err,
            RunError::Transition(TransitionError::AttemptsExhausted { .. })
        ));
        assert_eq!(run.phase, DevPhase::Blocked);
        assert_eq!(
            run.stop_reason.as_ref().map(|s| s.kind),
            Some(StopKind::AttemptsExhausted)
        );
    }

    #[test]
    fn containment_rejects_paths_outside_the_worktree() {
        let mut run = run();
        run.worktree_path = Some(PathBuf::from("/repo/local/runs/t-1/tree"));
        assert!(run
            .assert_within_worktree(Path::new("/repo/local/runs/t-1/tree/src/lib.rs"))
            .is_ok());
        // The main checkout is outside the run's boundary, like anything else.
        assert!(run
            .assert_within_worktree(Path::new("/repo/src/lib.rs"))
            .is_err());
    }

    #[test]
    fn an_abandoned_run_does_not_resume() {
        let mut run = run();
        run.halt(StopKind::NotActionable, "too vague").unwrap();
        assert_eq!(run.phase, DevPhase::Abandoned);
        assert!(run.resume().is_err());
    }
}
