//! The development-task state machine (ADR-0052 §3, §4).
//!
//! The model chooses what to do inside a phase. It does not choose which phase
//! comes next: transitions are the fixed table in this module, and a phase
//! exits only when the evidence its contract demands is already recorded.

use serde::{Deserialize, Serialize};

/// One position in the delivery process for a single development task.
///
/// `Blocked` and `Abandoned` are reachable from every working phase. Neither
/// carries the phase it came from — the run record owns that, so the enum stays
/// flat and comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevPhase {
    Intake,
    Triage,
    Investigate,
    Plan,
    PrepareWorktree,
    Implement,
    FocusedVerify,
    Review,
    Repair,
    FullVerify,
    ReadyToPublish,
    Published,
    WaitingForCi,
    AddressingFeedback,
    ReadyToMerge,
    Complete,
    Blocked,
    Abandoned,
}

/// What a phase is allowed to reach, resolved through the broker at run time.
///
/// This is the phase's *ceiling*, not a grant: a phase that may write the
/// project still asks the broker, and the broker may still say no.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseAuthority {
    /// Bookkeeping only; touches neither the project nor the network.
    None,
    /// Reads and searches inside the run's worktree. No mutation.
    ReadOnly,
    /// Reads plus file mutation and command execution inside the worktree.
    ProjectWrite,
    /// Everything `ProjectWrite` has, plus outward GitHub writes. Never
    /// auto-authorized: publication always asks (ADR-0052 §7).
    Publish,
}

impl PhaseAuthority {
    #[must_use]
    pub fn can_write_project(self) -> bool {
        matches!(self, Self::ProjectWrite | Self::Publish)
    }

    #[must_use]
    pub fn can_publish(self) -> bool {
        matches!(self, Self::Publish)
    }
}

/// A recorded fact a phase must produce before it may exit.
///
/// Every variant names something that exists in the run record as an observed
/// result, never as a model claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// The issue or request, resolved to a concrete problem statement.
    ProblemStatement,
    /// A reproduction, or the observation standing in for one.
    Reproduction,
    /// Acceptance criteria a human would recognise as testable.
    AcceptanceCriteria,
    /// Owning components, relevant tests, and the blast radius.
    ImpactMap,
    /// The accepted plan, including expected change scope and stop condition.
    PlanAccepted,
    /// Branch, worktree and base SHA exist and are recorded.
    WorktreeReady,
    /// The worktree passed verification *before* any change was made.
    BaselineVerification,
    /// The patch, as a diff against the recorded base SHA.
    Diff,
    /// A regression test added for the reported failure.
    RegressionTest,
    /// Impact-selected tests, run and recorded.
    FocusedTestRun,
    /// The new test fails at base and passes on the patch.
    DifferentialProof,
    /// Structured findings from an independent reviewer.
    ReviewFindings,
    /// The complete repository gate, run and recorded.
    FullVerification,
    /// The branch reached the remote at a known head SHA.
    PushReceipt,
    /// A draft pull request exists for that head SHA.
    DraftPullRequest,
    /// Check and workflow results for the current head SHA.
    CiResult,
    /// A human said yes to something the run may not decide alone.
    HumanApproval,
}

/// Everything a phase declares about itself, as data rather than as prose in a
/// prompt (ADR-0052 §4).
#[derive(Debug, Clone, Copy)]
pub struct PhaseContract {
    pub phase: DevPhase,
    pub authority: PhaseAuthority,
    pub required_evidence: &'static [EvidenceKind],
    /// Attempts at this phase before the run is forced to `Blocked`.
    pub max_attempts: u32,
    /// Wall-clock ceiling for a single attempt.
    pub timeout_secs: u64,
}

/// Why a transition was refused. Every variant is a fact about the run, not an
/// opinion about the work.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    #[error("{from:?} does not transition to {to:?}")]
    NotAllowed { from: DevPhase, to: DevPhase },
    #[error("{phase:?} cannot exit without evidence: {missing:?}")]
    MissingEvidence {
        phase: DevPhase,
        missing: Vec<EvidenceKind>,
    },
    #[error("{phase:?} exhausted its {max_attempts} attempts")]
    AttemptsExhausted { phase: DevPhase, max_attempts: u32 },
    #[error("{phase:?} is terminal")]
    Terminal { phase: DevPhase },
}

use DevPhase as P;
use EvidenceKind as E;

impl DevPhase {
    /// Phases a run can never leave.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, P::Complete | P::Abandoned)
    }

    /// Phases that hold a run without ending it. A `Blocked` run resumes at the
    /// phase the record remembers it was blocked from.
    #[must_use]
    pub fn is_halted(self) -> bool {
        matches!(self, P::Blocked | P::Abandoned)
    }

    /// The successor set. `Blocked` and `Abandoned` are reachable from every
    /// working phase and are therefore not listed here; see [`Self::can_halt`].
    #[must_use]
    pub fn allowed_next(self) -> &'static [DevPhase] {
        match self {
            P::Intake => &[P::Triage],
            P::Triage => &[P::Investigate],
            P::Investigate => &[P::Plan],
            P::Plan => &[P::PrepareWorktree],
            P::PrepareWorktree => &[P::Implement],
            P::Implement => &[P::FocusedVerify],
            // A failing focused verify goes back to the implementer; it does
            // not get to call a red suite "reviewed".
            P::FocusedVerify => &[P::Review, P::Implement],
            P::Review => &[P::Repair, P::FullVerify],
            // Repair re-enters verification. It can never rejoin at Review,
            // which is what would let a repaired patch skip its own tests.
            P::Repair => &[P::FocusedVerify],
            P::FullVerify => &[P::ReadyToPublish, P::Repair],
            P::ReadyToPublish => &[P::Published],
            P::Published => &[P::WaitingForCi],
            P::WaitingForCi => &[P::AddressingFeedback, P::ReadyToMerge],
            P::AddressingFeedback => &[P::FocusedVerify],
            P::ReadyToMerge => &[P::Complete],
            P::Complete | P::Blocked | P::Abandoned => &[],
        }
    }

    /// Whether a working phase may be halted into `Blocked` or `Abandoned`.
    #[must_use]
    pub fn can_halt(self) -> bool {
        !self.is_terminal()
    }

    /// The phase's declared contract.
    #[must_use]
    pub fn contract(self) -> PhaseContract {
        let (authority, required_evidence, max_attempts, timeout_secs): (
            PhaseAuthority,
            &'static [EvidenceKind],
            u32,
            u64,
        ) = match self {
            P::Intake => (PhaseAuthority::None, &[E::ProblemStatement], 1, 60),
            P::Triage => (
                PhaseAuthority::ReadOnly,
                &[E::AcceptanceCriteria, E::Reproduction],
                2,
                600,
            ),
            P::Investigate => (PhaseAuthority::ReadOnly, &[E::ImpactMap], 3, 1_800),
            P::Plan => (PhaseAuthority::ReadOnly, &[E::PlanAccepted], 2, 600),
            P::PrepareWorktree => (
                PhaseAuthority::ProjectWrite,
                &[E::WorktreeReady, E::BaselineVerification],
                2,
                1_800,
            ),
            P::Implement => (
                PhaseAuthority::ProjectWrite,
                &[E::Diff, E::RegressionTest],
                4,
                3_600,
            ),
            // A green unit test alone is never completion (ADR-0052 §5): the
            // differential proof is part of the contract, not a nicety.
            P::FocusedVerify => (
                PhaseAuthority::ProjectWrite,
                &[E::FocusedTestRun, E::DifferentialProof],
                3,
                1_800,
            ),
            P::Review => (PhaseAuthority::ReadOnly, &[E::ReviewFindings], 2, 1_800),
            P::Repair => (PhaseAuthority::ProjectWrite, &[E::Diff], 3, 1_800),
            P::FullVerify => (
                PhaseAuthority::ProjectWrite,
                &[E::FullVerification],
                2,
                5_400,
            ),
            // Where an autonomous run stops by default.
            P::ReadyToPublish => (PhaseAuthority::None, &[E::HumanApproval], 1, 60),
            P::Published => (
                PhaseAuthority::Publish,
                &[E::PushReceipt, E::DraftPullRequest],
                2,
                900,
            ),
            P::WaitingForCi => (PhaseAuthority::ReadOnly, &[E::CiResult], 6, 3_600),
            P::AddressingFeedback => (PhaseAuthority::ProjectWrite, &[E::Diff], 3, 1_800),
            P::ReadyToMerge => (PhaseAuthority::None, &[E::HumanApproval], 1, 60),
            P::Complete | P::Blocked | P::Abandoned => (PhaseAuthority::None, &[], 1, 60),
        };
        PhaseContract {
            phase: self,
            authority,
            required_evidence,
            max_attempts,
            timeout_secs,
        }
    }

    /// Stable snake_case name, for logs and records.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            P::Intake => "intake",
            P::Triage => "triage",
            P::Investigate => "investigate",
            P::Plan => "plan",
            P::PrepareWorktree => "prepare_worktree",
            P::Implement => "implement",
            P::FocusedVerify => "focused_verify",
            P::Review => "review",
            P::Repair => "repair",
            P::FullVerify => "full_verify",
            P::ReadyToPublish => "ready_to_publish",
            P::Published => "published",
            P::WaitingForCi => "waiting_for_ci",
            P::AddressingFeedback => "addressing_feedback",
            P::ReadyToMerge => "ready_to_merge",
            P::Complete => "complete",
            P::Blocked => "blocked",
            P::Abandoned => "abandoned",
        }
    }

    /// Every phase, in declaration order. Used by tests and by table dumps.
    #[must_use]
    pub fn all() -> &'static [DevPhase] {
        &[
            P::Intake,
            P::Triage,
            P::Investigate,
            P::Plan,
            P::PrepareWorktree,
            P::Implement,
            P::FocusedVerify,
            P::Review,
            P::Repair,
            P::FullVerify,
            P::ReadyToPublish,
            P::Published,
            P::WaitingForCi,
            P::AddressingFeedback,
            P::ReadyToMerge,
            P::Complete,
            P::Blocked,
            P::Abandoned,
        ]
    }
}

impl PhaseContract {
    /// Which required evidence kinds are absent from `recorded`.
    #[must_use]
    pub fn missing_evidence(&self, recorded: &[EvidenceKind]) -> Vec<EvidenceKind> {
        self.required_evidence
            .iter()
            .copied()
            .filter(|kind| !recorded.contains(kind))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implement_cannot_reach_publication() {
        assert!(!P::Implement.allowed_next().contains(&P::ReadyToPublish));
        assert!(!P::Implement.allowed_next().contains(&P::Published));
    }

    #[test]
    fn repair_must_re_verify_before_review() {
        assert_eq!(P::Repair.allowed_next(), &[P::FocusedVerify]);
    }

    #[test]
    fn only_published_may_publish() {
        for phase in DevPhase::all() {
            let can = phase.contract().authority.can_publish();
            assert_eq!(can, *phase == P::Published, "{phase:?}");
        }
    }

    #[test]
    fn read_only_phases_cannot_write() {
        for phase in [
            P::Triage,
            P::Investigate,
            P::Plan,
            P::Review,
            P::WaitingForCi,
        ] {
            assert!(
                !phase.contract().authority.can_write_project(),
                "{phase:?} must not write the project"
            );
        }
    }

    #[test]
    fn terminal_phases_have_no_successors() {
        for phase in DevPhase::all() {
            if phase.is_terminal() {
                assert!(phase.allowed_next().is_empty(), "{phase:?}");
            }
        }
    }

    #[test]
    fn every_phase_is_reachable_from_intake() {
        let mut seen = vec![P::Intake];
        let mut frontier = vec![P::Intake];
        while let Some(phase) = frontier.pop() {
            for next in phase.allowed_next() {
                if !seen.contains(next) {
                    seen.push(*next);
                    frontier.push(*next);
                }
            }
        }
        for phase in DevPhase::all() {
            if phase.is_halted() {
                continue;
            }
            assert!(seen.contains(phase), "{phase:?} is unreachable");
        }
    }
}
