//! Who is allowed to assert what, and why the reviewer cannot be the author
//! (program P43).
//!
//! [ADR-0052](../../../docs/decisions/0052-isolated-durable-engineering-runs.md)
//! says the implementation model does not approve its own patch. Until now
//! that was a sentence in a document — the phase table gave `Review` read-only
//! authority, but nothing stopped the same session that wrote the diff from
//! turning around and recording the review of it. A model asked to review its
//! own work agrees with itself, and the run gets a `ReviewFindings` row that
//! looks exactly like an independent one.
//!
//! So evidence that someone *asserts* carries who asserted it, and three rules
//! decide whether it counts:
//!
//! 1. **A role may only produce the evidence its role is for.** An implementer
//!    does not file review findings; a reviewer does not produce diffs.
//! 2. **A reviewer may not be a context that produced a diff in this run.**
//!    Not the same *role* — the same *context*. A model told to "now act as a
//!    reviewer" in the session where it wrote the code is the same reasoning
//!    wearing a different label, and the label is the only thing that changed.
//! 3. **Authority follows the role, not the phase alone.** A reviewer's
//!    ceiling is read-only wherever it runs, so a phase that permits project
//!    writes does not hand them to a reviewer that happens to be active in it.
//!
//! Evidence that came from *running a command* has no author to doubt: a
//! command either exited zero or it did not, and no amount of self-agreement
//! changes that. Those items are attributed to [`Role::Controller`], which is
//! deterministic code rather than a model. The distinction is the point — the
//! rules above exist for claims, and a claim is exactly what a command is not.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::phase::{DevPhase, EvidenceKind, PhaseAuthority};

use DevPhase as P;
use EvidenceKind as E;

/// A separated responsibility in a run.
///
/// Deliberately not "agent": a role is a *permission and evidence* boundary
/// that happens to be filled by a model. Two roles filled by the same context
/// are one role, which is what [`RoleIdentity`] exists to detect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Deterministic code: phase, budget, policy, and every command outcome.
    /// Writes no implementation and asserts no findings.
    Controller,
    /// Reads the repository and produces the impact map. Never writes.
    Navigator,
    /// Writes the patch. No push, no merge, no approval, no review.
    Implementer,
    /// Adversarial regression coverage; confirms a test catches the original
    /// failure.
    TestSpecialist,
    /// Independent findings, read-only. May not have authored the patch.
    Reviewer,
}

impl Role {
    /// The most this role may ever do, regardless of which phase it runs in.
    ///
    /// A phase ceiling and a role ceiling both apply; the effective authority
    /// is the lower of the two. A read-only role in a writing phase stays
    /// read-only.
    #[must_use]
    pub fn authority(self) -> PhaseAuthority {
        match self {
            Self::Navigator | Self::Reviewer => PhaseAuthority::ReadOnly,
            Self::Implementer | Self::TestSpecialist => PhaseAuthority::ProjectWrite,
            // The controller runs the repository's own gates, which write
            // build output inside the worktree. It never publishes: that is
            // ADR-0052 §7, and no role short-circuits it.
            Self::Controller => PhaseAuthority::ProjectWrite,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Controller => "controller",
            Self::Navigator => "navigator",
            Self::Implementer => "implementer",
            Self::TestSpecialist => "test_specialist",
            Self::Reviewer => "reviewer",
        }
    }

    /// The role a piece of evidence is *meant* to come from.
    ///
    /// The inverse of [`Role::may_produce`], for a caller deciding whom to
    /// dispatch. It is a well-formedness convenience, not a way in: naming the
    /// right role does not make a context independent, and
    /// [`check_assertion`]'s separation rule still refuses a reviewer that
    /// wrote the patch.
    #[must_use]
    pub fn producing(kind: EvidenceKind) -> Option<Self> {
        match kind {
            E::ReviewFindings => Some(Self::Reviewer),
            E::ImpactMap | E::AcceptanceCriteria => Some(Self::Navigator),
            E::DifferentialProof => Some(Self::TestSpecialist),
            E::Reproduction | E::RegressionTest => Some(Self::TestSpecialist),
            E::PlanAccepted => Some(Self::Implementer),
            E::Diff => Some(Self::Implementer),
            E::ProblemStatement
            | E::WorktreeReady
            | E::BaselineVerification
            | E::FocusedTestRun
            | E::FullVerification
            | E::PushReceipt
            | E::DraftPullRequest
            | E::CiResult
            | E::HumanApproval => Some(Self::Controller),
        }
    }

    /// Evidence this role is permitted to assert.
    #[must_use]
    pub fn may_produce(self, kind: EvidenceKind) -> bool {
        match self {
            // Anything that came from running something, plus the bookkeeping
            // a run does about itself.
            Self::Controller => matches!(
                kind,
                E::WorktreeReady
                    | E::BaselineVerification
                    | E::FocusedTestRun
                    | E::FullVerification
                    | E::Diff
                    | E::PushReceipt
                    | E::DraftPullRequest
                    | E::CiResult
                    | E::HumanApproval
                    | E::ProblemStatement
            ),
            Self::Navigator => matches!(
                kind,
                E::ProblemStatement | E::Reproduction | E::AcceptanceCriteria | E::ImpactMap
            ),
            Self::Implementer => matches!(kind, E::PlanAccepted | E::Diff | E::RegressionTest),
            Self::TestSpecialist => {
                matches!(
                    kind,
                    E::RegressionTest | E::DifferentialProof | E::Reproduction
                )
            }
            // One kind, and only this role produces it.
            Self::Reviewer => matches!(kind, E::ReviewFindings),
        }
    }
}

/// A role *and* the context filling it.
///
/// `context` is opaque here — a session id, an agent invocation id, whatever
/// the caller uses to mean "one continuous piece of reasoning". Its only job
/// is comparison: two evidence items with the same context came from the same
/// place, whatever roles they claimed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RoleIdentity {
    pub role: Role,
    pub context: String,
}

impl RoleIdentity {
    #[must_use]
    pub fn new(role: Role, context: impl Into<String>) -> Self {
        Self {
            role,
            context: context.into(),
        }
    }

    /// Deterministic code. Every controller identity is the same context,
    /// because there is only one and it does not reason.
    #[must_use]
    pub fn controller() -> Self {
        Self::new(Role::Controller, "controller")
    }

    #[must_use]
    pub fn is_controller(&self) -> bool {
        self.role == Role::Controller
    }
}

/// Why a piece of evidence was refused before it reached the log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoleError {
    #[error("a {role} may not produce {kind:?}")]
    WrongRole {
        role: &'static str,
        kind: EvidenceKind,
    },
    #[error(
        "context {context} produced the patch, so its review is not independent \
         — {kind:?} must come from a context that did not write the diff"
    )]
    ReviewingOwnWork { context: String, kind: EvidenceKind },
    #[error("{role} may not act in {phase:?}, which needs {needed:?}")]
    InsufficientAuthority {
        role: &'static str,
        phase: DevPhase,
        needed: PhaseAuthority,
    },
}

/// Check an assertion against the three rules, given what the run already
/// holds.
///
/// `authored_diff` is every context that has recorded a [`EvidenceKind::Diff`]
/// in this run. Passing the *set* rather than a flag is what makes rule 2 hold
/// across phases: a repair three phases later is still the same author.
///
/// # Errors
/// One variant per rule, each naming what would have to change.
pub fn check_assertion(
    author: &RoleIdentity,
    kind: EvidenceKind,
    phase: DevPhase,
    authored_diff: &BTreeSet<String>,
) -> Result<(), RoleError> {
    if !author.role.may_produce(kind) {
        return Err(RoleError::WrongRole {
            role: author.role.as_str(),
            kind,
        });
    }
    if kind == E::ReviewFindings && authored_diff.contains(&author.context) {
        return Err(RoleError::ReviewingOwnWork {
            context: author.context.clone(),
            kind,
        });
    }
    let needed = phase.contract().authority;
    if !authority_permits(author.role.authority(), needed) {
        return Err(RoleError::InsufficientAuthority {
            role: author.role.as_str(),
            phase,
            needed,
        });
    }
    Ok(())
}

/// Whether a role's ceiling covers what a phase needs.
///
/// Ordering is `None < ReadOnly < ProjectWrite < Publish`, written out rather
/// than derived so that adding a variant is a compile error here.
fn authority_permits(held: PhaseAuthority, needed: PhaseAuthority) -> bool {
    rank(held) >= rank(needed)
}

fn rank(authority: PhaseAuthority) -> u8 {
    match authority {
        PhaseAuthority::None => 0,
        PhaseAuthority::ReadOnly => 1,
        PhaseAuthority::ProjectWrite => 2,
        PhaseAuthority::Publish => 3,
    }
}

/// How much thinking a phase is worth (E43.6).
///
/// A task-class signal, not a model choice: this crate cannot reach the
/// router, and should not — picking a provider is the kernel's decision, made
/// with telemetry this crate never sees. What the phase table *does* know is
/// which work is hard, and that knowledge is what a router is missing today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effort {
    /// Classification and summarisation. Cheap and frequent.
    Cheap,
    /// Ordinary implementation.
    Standard,
    /// Root cause, architecture, and final review — where being wrong is
    /// expensive and being slow is not.
    High,
}

impl Effort {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cheap => "cheap",
            Self::Standard => "standard",
            Self::High => "high",
        }
    }
}

/// The role a phase's reasoning belongs to, and what it is worth.
///
/// `None` for phases that are pure bookkeeping or pure command execution —
/// there is nothing to route because there is nothing to think about.
#[must_use]
pub fn routing_for(phase: DevPhase) -> Option<(Role, Effort)> {
    match phase {
        P::Intake => Some((Role::Navigator, Effort::Cheap)),
        P::Triage => Some((Role::Navigator, Effort::Cheap)),
        // Root cause. The expensive one to get wrong.
        P::Investigate => Some((Role::Navigator, Effort::High)),
        P::Plan => Some((Role::Implementer, Effort::High)),
        P::Implement | P::Repair | P::AddressingFeedback => {
            Some((Role::Implementer, Effort::Standard))
        }
        // The proof, not the test run: deciding whether a regression test
        // actually exercises the bug is the judgement call.
        P::FocusedVerify => Some((Role::TestSpecialist, Effort::Standard)),
        // Final review. Being wrong here is what the whole run was for.
        P::Review => Some((Role::Reviewer, Effort::High)),
        P::WaitingForCi => Some((Role::Navigator, Effort::Cheap)),
        // Worktree preparation, the full gate, publication and the terminal
        // states are commands and bookkeeping.
        P::PrepareWorktree
        | P::FullVerify
        | P::ReadyToPublish
        | P::Published
        | P::ReadyToMerge
        | P::Complete
        | P::Blocked
        | P::Abandoned => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_authors() -> BTreeSet<String> {
        BTreeSet::new()
    }

    #[test]
    fn only_a_reviewer_files_review_findings() {
        for role in [
            Role::Controller,
            Role::Navigator,
            Role::Implementer,
            Role::TestSpecialist,
        ] {
            assert!(
                !role.may_produce(E::ReviewFindings),
                "{} may file findings",
                role.as_str()
            );
        }
        assert!(Role::Reviewer.may_produce(E::ReviewFindings));
    }

    #[test]
    fn a_reviewer_produces_nothing_else() {
        // The narrowest role in the run. Anything it could also produce is
        // something it could be asked to do instead of reviewing.
        for kind in [
            E::Diff,
            E::RegressionTest,
            E::ImpactMap,
            E::PlanAccepted,
            E::FullVerification,
            E::DifferentialProof,
        ] {
            assert!(!Role::Reviewer.may_produce(kind), "{kind:?}");
        }
    }

    #[test]
    fn the_context_that_wrote_the_patch_cannot_review_it() {
        let author = RoleIdentity::new(Role::Implementer, "session-7");
        let mut wrote_diff = BTreeSet::new();
        wrote_diff.insert(author.context.clone());

        // Same context, now claiming to be the reviewer. This is the exact
        // move the rule exists to stop: the label changed, the reasoning did
        // not.
        let wearing_a_hat = RoleIdentity::new(Role::Reviewer, "session-7");
        let error = check_assertion(&wearing_a_hat, E::ReviewFindings, P::Review, &wrote_diff)
            .expect_err("self-review must be refused");
        assert!(
            matches!(error, RoleError::ReviewingOwnWork { .. }),
            "{error}"
        );
    }

    #[test]
    fn a_different_context_may_review_the_same_patch() {
        let mut wrote_diff = BTreeSet::new();
        wrote_diff.insert("session-7".to_string());
        let reviewer = RoleIdentity::new(Role::Reviewer, "session-9");
        check_assertion(&reviewer, E::ReviewFindings, P::Review, &wrote_diff)
            .expect("an independent reviewer is exactly what is wanted");
    }

    #[test]
    fn separation_survives_a_repair_in_a_later_phase() {
        // The implementer wrote the diff in IMPLEMENT and again in REPAIR.
        // Both contexts are barred from reviewing, however many phases later.
        let mut wrote_diff = BTreeSet::new();
        wrote_diff.insert("session-7".to_string());
        wrote_diff.insert("session-8".to_string());
        for context in ["session-7", "session-8"] {
            let error = check_assertion(
                &RoleIdentity::new(Role::Reviewer, context),
                E::ReviewFindings,
                P::Review,
                &wrote_diff,
            )
            .expect_err(context);
            assert!(matches!(error, RoleError::ReviewingOwnWork { .. }));
        }
    }

    #[test]
    fn an_implementer_cannot_file_its_own_findings_even_from_a_fresh_context() {
        // Rule 1 catches this before rule 2 has to: a fresh session claiming
        // the implementer role still may not produce review findings.
        let error = check_assertion(
            &RoleIdentity::new(Role::Implementer, "session-fresh"),
            E::ReviewFindings,
            P::Review,
            &no_authors(),
        )
        .expect_err("wrong role");
        assert!(matches!(error, RoleError::WrongRole { .. }), "{error}");
    }

    #[test]
    fn a_read_only_role_cannot_act_in_a_writing_phase() {
        let error = check_assertion(
            &RoleIdentity::new(Role::Navigator, "session-1"),
            E::Diff,
            P::Implement,
            &no_authors(),
        )
        .expect_err("a navigator does not write");
        // Rule 1 fires first — a navigator cannot produce a diff at all.
        assert!(matches!(error, RoleError::WrongRole { .. }), "{error}");

        // And where the kind is allowed, the authority check still holds.
        let reviewer = RoleIdentity::new(Role::Reviewer, "session-2");
        assert_eq!(reviewer.role.authority(), PhaseAuthority::ReadOnly);
        assert!(!authority_permits(
            PhaseAuthority::ReadOnly,
            PhaseAuthority::ProjectWrite
        ));
    }

    #[test]
    fn no_role_carries_publish_authority() {
        // Publication asks a human. A role that could publish would be a way
        // around that (ADR-0052 §7).
        for role in [
            Role::Controller,
            Role::Navigator,
            Role::Implementer,
            Role::TestSpecialist,
            Role::Reviewer,
        ] {
            assert_ne!(
                role.authority(),
                PhaseAuthority::Publish,
                "{}",
                role.as_str()
            );
        }
    }

    #[test]
    fn the_controller_asserts_nothing_a_model_would_have_to_judge() {
        // Everything the controller may produce is either a command outcome or
        // a fact about the run itself. It never files findings, an impact map,
        // a differential proof, or acceptance criteria.
        for kind in [
            E::ReviewFindings,
            E::ImpactMap,
            E::DifferentialProof,
            E::AcceptanceCriteria,
            E::PlanAccepted,
        ] {
            assert!(!Role::Controller.may_produce(kind), "{kind:?}");
        }
    }

    #[test]
    fn every_evidence_kind_has_at_least_one_role_that_can_produce_it() {
        // A kind no role may assert is a phase that can never exit.
        let roles = [
            Role::Controller,
            Role::Navigator,
            Role::Implementer,
            Role::TestSpecialist,
            Role::Reviewer,
        ];
        for phase in [
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
        ] {
            for kind in phase.contract().required_evidence {
                assert!(
                    roles.iter().any(|role| role.may_produce(*kind)),
                    "{phase:?} needs {kind:?}, which no role may produce"
                );
            }
        }
    }

    #[test]
    fn the_canonical_producer_of_a_kind_is_allowed_to_produce_it() {
        for kind in [
            E::ProblemStatement,
            E::Reproduction,
            E::AcceptanceCriteria,
            E::ImpactMap,
            E::PlanAccepted,
            E::WorktreeReady,
            E::BaselineVerification,
            E::Diff,
            E::RegressionTest,
            E::FocusedTestRun,
            E::DifferentialProof,
            E::ReviewFindings,
            E::FullVerification,
            E::PushReceipt,
            E::DraftPullRequest,
            E::CiResult,
            E::HumanApproval,
        ] {
            let role = Role::producing(kind).unwrap_or_else(|| panic!("{kind:?} has no producer"));
            assert!(role.may_produce(kind), "{role:?} cannot produce {kind:?}");
        }
    }

    #[test]
    fn naming_the_right_role_does_not_buy_independence() {
        // The convenience must not become the loophole: producing() says
        // "a reviewer files findings", and the separation rule still refuses
        // this one because it wrote the patch.
        let role = Role::producing(E::ReviewFindings).expect("reviewer");
        let mut wrote_diff = BTreeSet::new();
        wrote_diff.insert("session-7".to_string());
        let error = check_assertion(
            &RoleIdentity::new(role, "session-7"),
            E::ReviewFindings,
            P::Review,
            &wrote_diff,
        )
        .expect_err("still refused");
        assert!(
            matches!(error, RoleError::ReviewingOwnWork { .. }),
            "{error}"
        );
    }

    #[test]
    fn the_routing_table_spends_where_being_wrong_is_expensive() {
        assert_eq!(routing_for(P::Investigate).map(|r| r.1), Some(Effort::High));
        assert_eq!(routing_for(P::Review), Some((Role::Reviewer, Effort::High)));
        assert_eq!(
            routing_for(P::Implement),
            Some((Role::Implementer, Effort::Standard))
        );
        assert_eq!(routing_for(P::Triage).map(|r| r.1), Some(Effort::Cheap));
    }

    #[test]
    fn phases_that_only_run_commands_route_nothing() {
        for phase in [P::PrepareWorktree, P::FullVerify, P::Published, P::Complete] {
            assert_eq!(routing_for(phase), None, "{phase:?}");
        }
    }

    #[test]
    fn a_routed_role_may_produce_what_its_phase_requires() {
        // The routing table and the evidence table have to agree, or a run is
        // routed to a role that is then refused the evidence it was sent for.
        for phase in [P::Triage, P::Investigate, P::Implement, P::Review] {
            let (role, _) = routing_for(phase).expect("routed");
            let required = phase.contract().required_evidence;
            assert!(
                required.iter().any(|kind| role.may_produce(*kind)),
                "{phase:?} routes to {} which produces none of {required:?}",
                role.as_str()
            );
        }
    }
}
