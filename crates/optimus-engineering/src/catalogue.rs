//! The commands a phase actually runs (program P40, E40.9).
//!
//! [`DevPhase::contract`] says what evidence a phase owes. [`RunDriver`] can
//! run steps and record what they did. Between them was a hole a prompt was
//! filling: *which command*. A model deciding at run time that the gate is
//! probably `just test` is the same failure P41 removed from repository facts —
//! a confident sentence where there should be a resolved answer.
//!
//! So the steps come from the [`RepositoryPolicyProfile`]: the repository's own
//! focused and full verification commands, resolved once from what it declares
//! or from its task runner.
//!
//! Two distinctions this module refuses to collapse:
//!
//! 1. **"No command to run" is not "nothing needs running."** A phase whose
//!    evidence simply does not come from a command — a problem statement, a
//!    human approval — has no steps and is fine. A phase that *should* run the
//!    repository's gate, in a repository that never named one, has no steps and
//!    is **not** fine. Both would be an empty `Vec`, so they are separate
//!    fields, and [`PhasePlan::can_drive`] is false for the second.
//!
//! 2. **A differential proof is never a step.** One command at one commit
//!    cannot establish that a test fails at base and passes on the patch; that
//!    needs [`crate::differential::DifferentialProver`] and two runs at two
//!    commits. Emitting a plausible-looking single step for it would let a
//!    phase satisfy its hardest contract with its easiest command, so
//!    `FocusedVerify` reports `DifferentialProof` as owed to something other
//!    than a command and refuses to pretend otherwise.
//!
//! [`RunDriver`]: crate::controller::RunDriver

use std::time::Duration;

use crate::controller::PhaseStep;
use crate::phase::{DevPhase, EvidenceKind};
use crate::repository::RepositoryPolicyProfile;

use DevPhase as P;
use EvidenceKind as E;

/// Evidence a phase owes that no available command can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedStep {
    pub evidence: EvidenceKind,
    /// What the repository would have to provide, in words a human can act on.
    pub reason: String,
}

/// The steps one phase runs, and everything it cannot get from a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhasePlan {
    pub phase: DevPhase,
    /// Commands to run, in order.
    pub steps: Vec<PhaseStep>,
    /// Evidence this phase owes that never comes from a command — a problem
    /// statement, a review, an approval, a differential proof. Listed, not
    /// hidden, so a caller knows what it is still on the hook for.
    pub from_elsewhere: Vec<EvidenceKind>,
    /// Evidence that should have come from a command the repository never
    /// named. Non-empty means this phase cannot be driven at all.
    pub unresolved: Vec<UnresolvedStep>,
}

impl PhasePlan {
    /// Whether the driver can run this phase as planned.
    ///
    /// False when something is unresolved *or* when a phase that ought to run
    /// commands has none. A phase with no steps and nothing unresolved is a
    /// phase whose work is not command-shaped, and that is a legitimate
    /// `true` — see [`PhasePlan::is_command_driven`].
    #[must_use]
    pub fn can_drive(&self) -> bool {
        self.unresolved.is_empty()
    }

    /// Whether any of this phase's evidence comes from running something.
    #[must_use]
    pub fn is_command_driven(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Evidence the steps in this plan would produce if they all came out the
    /// way they must.
    #[must_use]
    pub fn evidence_from_commands(&self) -> Vec<EvidenceKind> {
        self.steps.iter().map(|step| step.evidence).collect()
    }

    /// One line explaining why this phase cannot run, or `None` if it can.
    #[must_use]
    pub fn blocking_reason(&self) -> Option<String> {
        if self.unresolved.is_empty() {
            return None;
        }
        let detail = self
            .unresolved
            .iter()
            .map(|item| item.reason.clone())
            .collect::<Vec<_>>()
            .join("; ");
        Some(format!("{} cannot run: {detail}", self.phase.as_str()))
    }
}

/// What a phase's steps should be, given what the repository says it runs.
///
/// Never invents a command. A repository that named no focused verification
/// gets an `unresolved` entry, not a guess at `cargo test`.
#[must_use]
pub fn plan_for(phase: DevPhase, profile: &RepositoryPolicyProfile) -> PhasePlan {
    let mut steps = Vec::new();
    let mut from_elsewhere = Vec::new();
    let mut unresolved = Vec::new();

    for evidence in phase.contract().required_evidence {
        match evidence {
            // The worktree was green before the patch. Without this a red
            // focused verify cannot be attributed to the change.
            E::BaselineVerification => match focused_step(
                profile,
                E::BaselineVerification,
                "baseline: the repository's focused gate, before any change",
            ) {
                Ok(step) => steps.push(step),
                Err(item) => unresolved.push(item),
            },
            E::FocusedTestRun => match focused_step(
                profile,
                E::FocusedTestRun,
                "focused verification of the patch",
            ) {
                Ok(step) => steps.push(step),
                Err(item) => unresolved.push(item),
            },
            E::FullVerification => match full_step(profile) {
                Ok(step) => steps.push(step),
                Err(item) => unresolved.push(item),
            },
            // Distinction 2. A proof needs two runs at two commits; there is no
            // single command that stands in for it.
            other => from_elsewhere.push(*other),
        }
    }

    PhasePlan {
        phase,
        steps,
        from_elsewhere,
        unresolved,
    }
}

/// Every phase's plan, for a caller that wants to know up front whether a run
/// is drivable at all in this repository.
#[must_use]
pub fn plans_for_run(profile: &RepositoryPolicyProfile) -> Vec<PhasePlan> {
    WORKING_PHASES
        .iter()
        .map(|phase| plan_for(*phase, profile))
        .collect()
}

/// Phases a normal run passes through. Excludes the halt states, which have no
/// contract worth planning.
const WORKING_PHASES: &[DevPhase] = &[
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
];

fn focused_step(
    profile: &RepositoryPolicyProfile,
    evidence: EvidenceKind,
    summary: &str,
) -> Result<PhaseStep, UnresolvedStep> {
    let command = profile
        .verification
        .focused
        .as_ref()
        .ok_or_else(|| UnresolvedStep {
            evidence,
            reason: "the repository named no focused verification command, and one \
                     will not be guessed"
                .to_string(),
        })?;
    step_from(evidence, command, summary).ok_or_else(|| UnresolvedStep {
        evidence,
        reason: "the repository's focused verification command is empty".to_string(),
    })
}

fn full_step(profile: &RepositoryPolicyProfile) -> Result<PhaseStep, UnresolvedStep> {
    let command = profile
        .verification
        .full
        .as_ref()
        .ok_or_else(|| UnresolvedStep {
            evidence: E::FullVerification,
            reason: "the repository named no full verification command; a run cannot \
                     report a repository verified by a gate it invented"
                .to_string(),
        })?;
    step_from(
        E::FullVerification,
        command,
        "the repository's complete gate",
    )
    .ok_or_else(|| UnresolvedStep {
        evidence: E::FullVerification,
        reason: "the repository's full verification command is empty".to_string(),
    })
}

/// Build a step from a resolved command, honouring the phase's own timeout.
fn step_from(evidence: EvidenceKind, command: &[String], summary: &str) -> Option<PhaseStep> {
    let (program, args) = command.split_first()?;
    let phase_timeout = timeout_for(evidence);
    Some(
        PhaseStep::new(
            evidence,
            program.clone(),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .with_timeout(phase_timeout)
        .with_summary(summary),
    )
}

/// A step may not outlive the phase that owns it.
fn timeout_for(evidence: EvidenceKind) -> Duration {
    let phase = match evidence {
        E::BaselineVerification => P::PrepareWorktree,
        E::FocusedTestRun => P::FocusedVerify,
        E::FullVerification => P::FullVerify,
        _ => P::Implement,
    };
    Duration::from_secs(phase.contract().timeout_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{BranchProtection, VerificationCommands};

    fn profile(focused: Option<&[&str]>, full: Option<&[&str]>) -> RepositoryPolicyProfile {
        RepositoryPolicyProfile {
            default_branch: Some("main".into()),
            protection: BranchProtection::Unprotected,
            pr_template: None,
            instruction_files: Vec::new(),
            sensitive_paths: Vec::new(),
            verification: VerificationCommands {
                focused: focused.map(|c| c.iter().map(|s| (*s).to_string()).collect()),
                full: full.map(|c| c.iter().map(|s| (*s).to_string()).collect()),
            },
        }
    }

    #[test]
    fn a_phase_with_no_command_shaped_evidence_has_no_steps_and_is_still_drivable() {
        // Intake owes a problem statement. Nothing runs; that is correct, not
        // a gap, and `can_drive` must not confuse the two.
        let plan = plan_for(P::Intake, &profile(Some(&["just", "gates"]), None));
        assert!(plan.steps.is_empty());
        assert!(!plan.is_command_driven());
        assert!(plan.can_drive());
        assert_eq!(plan.from_elsewhere, vec![E::ProblemStatement]);
    }

    #[test]
    fn a_missing_focused_command_blocks_the_phase_rather_than_emptying_it() {
        let plan = plan_for(P::FocusedVerify, &profile(None, Some(&["just", "verify"])));
        assert!(plan.steps.is_empty());
        assert!(!plan.can_drive());
        let reason = plan.blocking_reason().expect("blocked");
        assert!(reason.contains("focused"), "{reason}");
    }

    #[test]
    fn a_differential_proof_is_never_emitted_as_a_step() {
        let plan = plan_for(
            P::FocusedVerify,
            &profile(Some(&["just", "dev-check"]), Some(&["just", "verify"])),
        );
        assert!(plan.can_drive());
        assert_eq!(plan.evidence_from_commands(), vec![E::FocusedTestRun]);
        assert!(
            plan.from_elsewhere.contains(&E::DifferentialProof),
            "the proof must be declared as owed elsewhere, not silently dropped"
        );
    }

    #[test]
    fn the_baseline_runs_the_focused_gate_before_any_change() {
        let plan = plan_for(
            P::PrepareWorktree,
            &profile(Some(&["just", "dev-check"]), None),
        );
        let step = plan
            .steps
            .iter()
            .find(|s| s.evidence == E::BaselineVerification)
            .expect("baseline step");
        assert_eq!(step.program, "just");
        assert_eq!(step.args, vec!["dev-check".to_string()]);
        assert!(!step.expect_failure);
    }

    #[test]
    fn a_full_gate_the_repository_never_named_is_not_replaced_by_a_focused_one() {
        // The tempting fallback. A run that reported "fully verified" from the
        // focused gate would be reporting something nobody ran.
        let plan = plan_for(P::FullVerify, &profile(Some(&["just", "gates"]), None));
        assert!(plan.steps.is_empty());
        assert!(!plan.can_drive());
        assert_eq!(plan.unresolved.len(), 1);
        assert_eq!(plan.unresolved[0].evidence, E::FullVerification);
    }

    #[test]
    fn a_step_may_not_outlive_its_phase() {
        let plan = plan_for(P::FullVerify, &profile(None, Some(&["just", "verify"])));
        let step = &plan.steps[0];
        assert_eq!(
            step.timeout,
            Duration::from_secs(P::FullVerify.contract().timeout_secs)
        );
    }

    #[test]
    fn an_empty_command_is_unresolved_rather_than_a_step_with_no_program() {
        let plan = plan_for(P::FullVerify, &profile(None, Some(&[])));
        assert!(!plan.can_drive());
    }

    #[test]
    fn every_working_phase_plans_without_panicking() {
        let complete = profile(Some(&["just", "dev-check"]), Some(&["just", "verify"]));
        let plans = plans_for_run(&complete);
        assert_eq!(plans.len(), WORKING_PHASES.len());
        for plan in &plans {
            assert!(plan.can_drive(), "{:?}: {:?}", plan.phase, plan.unresolved);
        }
        // And every required evidence kind is accounted for somewhere: either a
        // command produces it or the plan says it comes from elsewhere.
        for plan in &plans {
            let mut accounted = plan.evidence_from_commands();
            accounted.extend(plan.from_elsewhere.iter().copied());
            for required in plan.phase.contract().required_evidence {
                assert!(
                    accounted.contains(required),
                    "{:?} loses {required:?}",
                    plan.phase
                );
            }
        }
    }

    #[test]
    fn a_repository_with_no_verification_at_all_blocks_exactly_the_verifying_phases() {
        let plans = plans_for_run(&profile(None, None));
        let blocked: Vec<DevPhase> = plans
            .iter()
            .filter(|plan| !plan.can_drive())
            .map(|plan| plan.phase)
            .collect();
        assert_eq!(
            blocked,
            vec![P::PrepareWorktree, P::FocusedVerify, P::FullVerify]
        );
    }
}
