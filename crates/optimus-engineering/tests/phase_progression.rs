//! Program P40 exit gate: the phase table is a gate, not a suggestion.

use optimus_engineering::{
    DevPhase, DevTaskRun, EvidenceItem, EvidenceKind, Role, RoleIdentity, RunError, StopKind,
    TaskOrigin, TransitionError,
};

/// A fixture author for each evidence kind, with one context per role so the
/// P43 separation rules see genuinely distinct producers.
fn author(kind: EvidenceKind) -> RoleIdentity {
    let role = Role::producing(kind).expect("every kind has a producing role");
    RoleIdentity::new(role, format!("fixture-{}", role.as_str()))
}

fn fresh() -> DevTaskRun {
    DevTaskRun::new(
        "t-progress",
        TaskOrigin::Issue {
            number: 7,
            title: "the fixture root leaks".into(),
        },
        "/repo",
    )
}

/// Walk a run to `target`, satisfying each contract with stated evidence.
fn drive_to(run: &mut DevTaskRun, target: DevPhase) {
    while run.phase != target {
        for kind in run.phase.contract().required_evidence {
            if !run.satisfied_evidence().contains(kind) {
                run.record(EvidenceItem::stated_by(author(*kind), *kind, "fixture"))
                    .unwrap();
            }
        }
        let next =
            *run.phase.allowed_next().first().unwrap_or_else(|| {
                panic!("{:?} has no successor on the way to {target:?}", run.phase)
            });
        run.advance_to(next).unwrap();
    }
}

#[test]
fn implement_cannot_jump_to_publication() {
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Implement);
    assert_eq!(run.phase, DevPhase::Implement);

    for forbidden in [
        DevPhase::ReadyToPublish,
        DevPhase::Published,
        DevPhase::ReadyToMerge,
        DevPhase::Complete,
        DevPhase::FullVerify,
        DevPhase::Review,
    ] {
        let err = run.advance_to(forbidden).unwrap_err();
        assert!(
            matches!(
                err,
                RunError::Transition(TransitionError::NotAllowed { .. })
            ),
            "{forbidden:?} should be unreachable from Implement, got {err:?}"
        );
    }
}

#[test]
fn a_green_unit_test_alone_does_not_finish_focused_verify() {
    let mut run = fresh();
    drive_to(&mut run, DevPhase::FocusedVerify);

    run.record(EvidenceItem::observed(
        EvidenceKind::FocusedTestRun,
        "just test-changed",
        0,
        "deadbeef",
        b"ok",
    ))
    .unwrap();

    // Tests passed. The differential proof did not happen, so the phase holds.
    assert!(!run.can_exit_phase());
    let err = run.advance_to(DevPhase::Review).unwrap_err();
    assert!(matches!(
        err,
        RunError::Transition(TransitionError::MissingEvidence { .. })
    ));

    // The proof is the regression test failing at the *base* commit. A run
    // that passed there would not be exercising the bug, so this is the one
    // place a non-zero exit is what corroborates.
    run.record(EvidenceItem::observed_failing(
        EvidenceKind::DifferentialProof,
        "just test-changed --at-base",
        1,
        "cafe1234",
        b"failed at base as expected",
    ))
    .unwrap();
    run.advance_to(DevPhase::Review).unwrap();
    assert_eq!(run.phase, DevPhase::Review);
}

#[test]
fn a_regression_test_that_passes_at_base_is_not_a_differential_proof() {
    let mut run = fresh();
    drive_to(&mut run, DevPhase::FocusedVerify);
    run.record(EvidenceItem::observed(
        EvidenceKind::FocusedTestRun,
        "just test-changed",
        0,
        "deadbeef",
        b"ok",
    ))
    .unwrap();

    // Green at base: the test does not catch the bug it was written for.
    run.record(EvidenceItem::observed_failing(
        EvidenceKind::DifferentialProof,
        "just test-changed --at-base",
        0,
        "cafe1234",
        b"passed at base, which proves nothing",
    ))
    .unwrap();

    assert!(
        !run.can_exit_phase(),
        "a proof that did not fail at base proves nothing"
    );
    let err = run.advance_to(DevPhase::Review).unwrap_err();
    assert!(matches!(
        err,
        RunError::Transition(TransitionError::MissingEvidence { .. })
    ));
    // Still recorded: the record shows the attempt and why it did not count.
    let item = run.evidence.last().unwrap();
    assert_eq!(item.exit_status, Some(0));
    assert!(!item.corroborates);
}

#[test]
fn a_repaired_patch_re_enters_verification_not_review() {
    let mut run = fresh();
    drive_to(&mut run, DevPhase::Review);
    run.record(EvidenceItem::stated_by(
        author(EvidenceKind::ReviewFindings),
        EvidenceKind::ReviewFindings,
        "1 major: the fix hides the failure",
    ))
    .unwrap();
    run.advance_to(DevPhase::Repair).unwrap();

    let err = run.advance_to(DevPhase::Review).unwrap_err();
    assert!(matches!(
        err,
        RunError::Transition(TransitionError::NotAllowed { .. })
    ));

    run.record(EvidenceItem::stated_by(
        author(EvidenceKind::Diff),
        EvidenceKind::Diff,
        "repaired",
    ))
    .unwrap();
    run.advance_to(DevPhase::FocusedVerify).unwrap();
    assert_eq!(run.phase, DevPhase::FocusedVerify);
    // The re-entered verification starts empty: the earlier green run belongs
    // to the earlier attempt.
    assert!(!run.can_exit_phase());
}

#[test]
fn publication_authority_belongs_to_exactly_one_phase() {
    for phase in DevPhase::all() {
        assert_eq!(
            phase.contract().authority.can_publish(),
            *phase == DevPhase::Published,
            "{phase:?}"
        );
    }
}

#[test]
fn a_rejected_issue_is_abandoned_with_a_reason() {
    let mut run = fresh();
    run.record(EvidenceItem::stated_by(
        author(EvidenceKind::ProblemStatement),
        EvidenceKind::ProblemStatement,
        "vague",
    ))
    .unwrap();
    run.advance_to(DevPhase::Triage).unwrap();
    run.halt(
        StopKind::NotActionable,
        "no reproduction and no acceptance criteria",
    )
    .unwrap();

    assert_eq!(run.phase, DevPhase::Abandoned);
    let stop = run.stop_reason.as_ref().expect("a halted run has a reason");
    assert_eq!(stop.phase, DevPhase::Triage);
    assert!(run.resume().is_err(), "abandoned runs do not resume");
}
