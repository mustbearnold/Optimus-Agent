//! Program P43 exit gate: the implementation model does not approve its own
//! patch, and it is the run that stops it, not the prompt.
//!
//! [ADR-0052](../../../docs/decisions/0052-isolated-durable-engineering-runs.md)
//! said this in prose and the phase table did not enforce it. These tests
//! drive a real `DevTaskRun` through to `REVIEW` and then try, in every way a
//! confused or obliging model actually would, to get the author's review onto
//! the record. Each attempt is refused at `record()`, before the log says
//! anything untrue.

use std::fs;

use optimus_engineering::{
    routing_for, DevPhase, DevTaskRun, Effort, EvidenceItem, EvidenceKind, Role, RoleError,
    RoleIdentity, RunError, TaskOrigin,
};

/// The context that writes the code in every test below.
const AUTHOR: &str = "session-implementer-1";
/// A genuinely separate one.
const REVIEWER: &str = "session-reviewer-9";

fn author(kind: EvidenceKind) -> RoleIdentity {
    let role = Role::producing(kind).expect("every kind has a producing role");
    RoleIdentity::new(role, format!("fixture-{}", role.as_str()))
}

fn fresh() -> DevTaskRun {
    DevTaskRun::new(
        "t-roles",
        TaskOrigin::Issue {
            number: 43,
            title: "the reviewer agrees with itself".into(),
        },
        "/repo",
    )
}

/// Walk to `target`, letting the fixture roles supply whatever each contract
/// asks for — except the diff, which is recorded by [`AUTHOR`] so the tests
/// have a real patch author to catch.
fn drive_to(run: &mut DevTaskRun, target: DevPhase) {
    while run.phase != target {
        for kind in run.phase.contract().required_evidence {
            if run.satisfied_evidence().contains(kind) {
                continue;
            }
            let who = if *kind == EvidenceKind::Diff {
                RoleIdentity::new(Role::Implementer, AUTHOR)
            } else {
                author(*kind)
            };
            run.record(EvidenceItem::stated_by(who, *kind, "fixture"))
                .expect("fixture evidence is well-formed");
        }
        let next = *run
            .phase
            .allowed_next()
            .first()
            .unwrap_or_else(|| panic!("{:?} has no successor toward {target:?}", run.phase));
        run.advance_to(next).expect("fixture transition");
    }
}

#[test]
fn the_context_that_wrote_the_patch_cannot_review_it() {
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Review);

    let err = run
        .record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Reviewer, AUTHOR),
            EvidenceKind::ReviewFindings,
            "looks good to me",
        ))
        .expect_err("the author's review must be refused");

    assert!(
        matches!(
            err,
            RunError::Role(RoleError::ReviewingOwnWork { ref context, .. }) if context == AUTHOR
        ),
        "{err}"
    );
    // And the run is exactly where it was: nothing was logged and refused
    // afterwards.
    assert!(
        !run.can_exit_phase(),
        "a refused review must not satisfy REVIEW"
    );
    assert!(
        !run.evidence
            .iter()
            .any(|item| item.kind == EvidenceKind::ReviewFindings),
        "the refusal left a review row on the record"
    );
}

#[test]
fn an_independent_context_may_review_the_same_patch() {
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Review);

    run.record(EvidenceItem::stated_by(
        RoleIdentity::new(Role::Reviewer, REVIEWER),
        EvidenceKind::ReviewFindings,
        "1 major: the fix hides the failure",
    ))
    .expect("an independent review is exactly what REVIEW wants");

    assert!(run.can_exit_phase());
}

#[test]
fn changing_the_label_does_not_change_the_reasoning() {
    // The failure mode this whole program exists for: one session told "you
    // are now the reviewer". Same context, new hat, and the hat is the only
    // thing that changed.
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Review);

    for role in [Role::Reviewer, Role::TestSpecialist, Role::Navigator] {
        let err = run
            .record(EvidenceItem::stated_by(
                RoleIdentity::new(role, AUTHOR),
                EvidenceKind::ReviewFindings,
                "still me",
            ))
            .expect_err("{role:?} wearing the author's context must be refused");
        // Either rule may catch it — the point is that none of them let it
        // through.
        assert!(
            matches!(
                err,
                RunError::Role(RoleError::ReviewingOwnWork { .. } | RoleError::WrongRole { .. })
            ),
            "{role:?} slipped through: {err}"
        );
    }
}

#[test]
fn a_repair_author_is_still_an_author_when_review_comes_round_again() {
    // The interesting version: the first patch is reviewed independently, the
    // reviewer's own findings send it back to REPAIR, and a *second* context
    // writes the repair. That second context must not review either — the rule
    // holds across phases, which is why the run keeps the set of diff authors
    // rather than a flag.
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Review);

    run.record(EvidenceItem::stated_by(
        RoleIdentity::new(Role::Reviewer, REVIEWER),
        EvidenceKind::ReviewFindings,
        "1 major",
    ))
    .expect("independent review");
    run.advance_to(DevPhase::Repair)
        .expect("findings send it back");

    const REPAIRER: &str = "session-implementer-2";
    run.record(EvidenceItem::stated_by(
        RoleIdentity::new(Role::Implementer, REPAIRER),
        EvidenceKind::Diff,
        "repaired",
    ))
    .expect("a repair is a diff");
    run.advance_to(DevPhase::FocusedVerify)
        .expect("repair verifies");
    drive_to(&mut run, DevPhase::Review);

    let err = run
        .record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Reviewer, REPAIRER),
            EvidenceKind::ReviewFindings,
            "my repair is fine",
        ))
        .expect_err("the repair's author is an author");
    assert!(
        matches!(
            err,
            RunError::Role(RoleError::ReviewingOwnWork { ref context, .. }) if context == REPAIRER
        ),
        "{err}"
    );

    // The original author is still blocked too — the set accumulates, it does
    // not get replaced by the most recent one.
    let err = run
        .record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Reviewer, AUTHOR),
            EvidenceKind::ReviewFindings,
            "and so is mine",
        ))
        .expect_err("the first author is still an author");
    assert!(
        matches!(err, RunError::Role(RoleError::ReviewingOwnWork { .. })),
        "{err}"
    );
}

#[test]
fn a_role_cannot_assert_evidence_that_is_not_its_job() {
    let mut run = fresh();

    // A navigator does not write patches.
    let err = run
        .record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Navigator, "session-nav"),
            EvidenceKind::Diff,
            "I had a look and fixed it",
        ))
        .expect_err("a navigator producing a diff must be refused");
    assert!(
        matches!(err, RunError::Role(RoleError::WrongRole { .. })),
        "{err}"
    );

    // A reviewer produces findings and nothing else — not even the impact map
    // it would find useful.
    let err = run
        .record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Reviewer, REVIEWER),
            EvidenceKind::ImpactMap,
            "here is what it touches",
        ))
        .expect_err("a reviewer producing an impact map must be refused");
    assert!(
        matches!(err, RunError::Role(RoleError::WrongRole { .. })),
        "{err}"
    );
}

#[test]
fn a_reviewer_never_gets_write_authority_from_the_phase_it_is_in() {
    // Rule 3: a phase that permits project writes does not hand them to a
    // read-only role that happens to be active in it.
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Implement);
    assert_eq!(
        run.phase.contract().authority,
        optimus_engineering::PhaseAuthority::ProjectWrite
    );

    let err = run
        .record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Reviewer, REVIEWER),
            EvidenceKind::ReviewFindings,
            "reviewing early",
        ))
        .expect_err("a reviewer in a writing phase is still read-only");
    assert!(
        matches!(err, RunError::Role(RoleError::InsufficientAuthority { .. })),
        "{err}"
    );
}

#[test]
fn a_command_outcome_needs_no_role_because_it_makes_no_claim() {
    // `just verify` exiting zero is a fact about a process, not an assertion
    // by a reasoner. The controller records it in REVIEW — a read-only phase
    // whose contract no *role* could satisfy — and it lands.
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Review);

    run.record(
        EvidenceItem::observed(
            EvidenceKind::FullVerification,
            "just verify",
            0,
            "deadbeef",
            b"39/39 ok",
        )
        .with_summary("full gate green"),
    )
    .expect("a command outcome is not an assertion");

    let item = run.evidence.last().expect("recorded");
    assert!(item.author.is_controller());
    assert!(item.corroborates);
}

#[test]
fn every_asserted_item_says_who_asserted_it_across_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = DevTaskRun::record_path(dir.path(), "t-roles");

    let mut run = fresh();
    drive_to(&mut run, DevPhase::Review);
    run.record(EvidenceItem::stated_by(
        RoleIdentity::new(Role::Reviewer, REVIEWER),
        EvidenceKind::ReviewFindings,
        "1 major",
    ))
    .expect("independent review");
    run.save(&path).expect("save");

    let reloaded = DevTaskRun::load(&path).expect("load");
    let diff = reloaded
        .evidence
        .iter()
        .find(|item| item.kind == EvidenceKind::Diff)
        .expect("a diff on the record");
    let review = reloaded
        .evidence
        .iter()
        .find(|item| item.kind == EvidenceKind::ReviewFindings)
        .expect("a review on the record");

    assert_eq!(diff.author, RoleIdentity::new(Role::Implementer, AUTHOR));
    assert_eq!(review.author, RoleIdentity::new(Role::Reviewer, REVIEWER));
    assert_ne!(
        diff.author.context, review.author.context,
        "the record must be able to prove these came from different places"
    );

    // The separation survives the restart, not just the attribution: a run
    // resumed from disk still refuses the author's review.
    let mut reloaded = reloaded;
    let err = reloaded
        .record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Reviewer, AUTHOR),
            EvidenceKind::ReviewFindings,
            "second opinion, same brain",
        ))
        .expect_err("refused after a restart too");
    assert!(
        matches!(err, RunError::Role(RoleError::ReviewingOwnWork { .. })),
        "{err}"
    );
}

#[test]
fn a_record_written_before_roles_existed_still_loads() {
    // P43 added `author` to every evidence item. Runs on disk from before it
    // do not have the field, and a run that cannot be resumed after an upgrade
    // is a run whose durability was decorative.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = DevTaskRun::record_path(dir.path(), "t-roles");

    let mut run = fresh();
    drive_to(&mut run, DevPhase::Implement);
    run.save(&path).expect("save");

    let text = fs::read_to_string(&path).expect("read");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("parse");
    let items = value["evidence"].as_array_mut().expect("evidence array");
    assert!(!items.is_empty());
    for item in items.iter_mut() {
        item.as_object_mut().expect("object").remove("author");
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(&value).expect("serialise"),
    )
    .expect("write");

    let reloaded = DevTaskRun::load(&path).expect("a pre-P43 record still loads");
    assert!(
        reloaded
            .evidence
            .iter()
            .all(|item| item.author.is_controller()),
        "unattributed history reads as the controller's, not as some role's"
    );
}

#[test]
fn the_phases_worth_thinking_hardest_about_are_the_ones_that_get_it_wrong_expensively() {
    // E43.6: the routing signal is a property of the phase table, not a
    // sentence in a prompt. Root cause, planning and review are where a cheap
    // answer costs the most.
    for phase in [DevPhase::Investigate, DevPhase::Plan, DevPhase::Review] {
        let (_, effort) = routing_for(phase).unwrap_or_else(|| panic!("{phase:?} routes nowhere"));
        assert_eq!(effort, Effort::High, "{phase:?}");
    }
    // And the role a phase routes to is one that may produce what the phase
    // requires — otherwise the router would dispatch a model that cannot
    // satisfy the contract it was sent to satisfy.
    for phase in DevPhase::all() {
        let Some((role, _)) = routing_for(*phase) else {
            continue;
        };
        let required = phase.contract().required_evidence;
        let asserted: Vec<_> = required
            .iter()
            .filter(|kind| Role::producing(**kind) != Some(Role::Controller))
            .collect();
        for kind in asserted {
            assert!(
                role.may_produce(*kind),
                "{phase:?} routes to {role:?}, which cannot produce the {kind:?} it needs"
            );
        }
    }
}
