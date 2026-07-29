//! Program P41 exit gate, E41.4/E41.5: an issue earns its way into
//! `INVESTIGATE`, or it is refused with the reporter's own words.
//!
//! The unit tests in `triage.rs` cover each rule in isolation. These prove the
//! loop: a triage result is checked, an admissible contract becomes the exact
//! evidence `TRIAGE` owes and the run advances; an inadmissible one produces
//! nothing recordable and the run stays put with an attempt spent honestly; a
//! grounded refusal halts the run as `NotActionable` with the quote on the
//! record. Paths are grounded against *this* repository, so "the named
//! component exists" means what it says.

use std::path::{Path, PathBuf};

use optimus_engineering::{
    check, AcceptanceCriterion, BranchProtection, ChangeScope, DevPhase, DevTaskRun, EvidenceItem,
    EvidenceKind, RefusalReason, RepositoryPolicyProfile, RiskClass, Role, RoleIdentity, StopKind,
    TaskOrigin, TriageContext, TriageLimits, TriageOutput, TriageResult, VerificationCommands,
};

/// A real issue from this repository's tracker (#113, abridged): concrete
/// symptom, environment, and a reproduction.
const ISSUE_ACTIONABLE: &str = "TUI worker panics when the session list refreshes while a \
    thread is being created. Reproduced on Linux: start optimus-tui, create a new thread \
    immediately after launch, and the list refresh erases it. The thread exists on disk but \
    the panel shows nothing until restart.";

/// The kind of issue E41.5 exists for: a wish with no observable behind it.
const ISSUE_VAGUE: &str = "Optimus feels slow lately and the UI could be better. \
    Can this be improved?";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn profile() -> RepositoryPolicyProfile {
    RepositoryPolicyProfile {
        default_branch: Some("main".into()),
        protection: BranchProtection::Unprotected,
        pr_template: None,
        instruction_files: Vec::new(),
        sensitive_paths: vec![
            ".github/**".into(),
            "justfile".into(),
            "scripts/verify.sh".into(),
        ],
        verification: VerificationCommands::default(),
    }
}

fn navigator() -> RoleIdentity {
    RoleIdentity::new(Role::Navigator, "session-navigator-1")
}

fn run_in_triage(issue: &str) -> DevTaskRun {
    let mut run = DevTaskRun::new(
        "t-triage",
        TaskOrigin::Issue {
            number: 113,
            title: issue.chars().take(40).collect(),
        },
        "/repo",
    );
    run.record(EvidenceItem::stated_by(
        navigator(),
        EvidenceKind::ProblemStatement,
        "issue as reported",
    ))
    .expect("intake");
    run.advance_to(DevPhase::Triage).expect("into TRIAGE");
    run
}

fn actionable_contract() -> TriageOutput {
    TriageOutput {
        problem: "a session-list refresh races thread creation and erases the new thread \
            from the panel"
            .into(),
        evidence: vec![
            "the session list refreshes while a thread is being created".into(),
            "The thread exists on disk but the panel shows nothing until restart".into(),
        ],
        acceptance_criteria: vec![
            AcceptanceCriterion {
                statement: "a thread created during a session-list refresh stays visible".into(),
                checked_by: "apps/optimus-ui vitest, session store suite".into(),
            },
            AcceptanceCriterion {
                statement: "the worker does not panic when the refresh and the creation \
                    interleave"
                    .into(),
                checked_by: "cargo test -p optimus-tui".into(),
            },
        ],
        owning_components: vec![PathBuf::from("apps/optimus-ui"), PathBuf::from("crates")],
        relevant_tests: vec![PathBuf::from("apps/optimus-ui")],
        risk: RiskClass::Low,
        change_scope: ChangeScope { expected_files: 3 },
        stop_condition: "the race needs kernel-side ordering changes rather than a UI fix".into(),
    }
}

#[test]
fn an_admissible_contract_is_exactly_what_triage_owes() {
    let root = repo_root();
    let p = profile();
    let ctx = TriageContext {
        repo_root: &root,
        issue_body: ISSUE_ACTIONABLE,
        profile: &p,
        limits: TriageLimits::CEILING,
    };
    let output = actionable_contract();
    let verdict = check(&TriageResult::Contract(output.clone()), &ctx);
    assert!(verdict.is_admissible(), "{}", verdict.explain());

    let mut run = run_in_triage(ISSUE_ACTIONABLE);
    for draft in output
        .evidence_drafts(navigator(), &verdict)
        .expect("admitted")
    {
        run.record(draft).expect("triage evidence lands");
    }
    run.advance_to(DevPhase::Investigate)
        .expect("the contract is what TRIAGE owed");
    assert_eq!(run.phase, DevPhase::Investigate);
}

#[test]
fn an_ungrounded_contract_produces_nothing_the_run_will_accept() {
    let root = repo_root();
    let p = profile();
    let ctx = TriageContext {
        repo_root: &root,
        issue_body: ISSUE_ACTIONABLE,
        profile: &p,
        limits: TriageLimits::CEILING,
    };
    // The failure E41.4 exists for: fluent, plausible, and attached to nothing.
    // The quote is invented, the component does not exist, and the criteria
    // restate the wish.
    let output = TriageOutput {
        problem: "the UI should not lose threads".into(),
        evidence: vec!["threads disappear constantly and users are frustrated".into()],
        acceptance_criteria: vec![AcceptanceCriterion {
            statement: "threads are never lost".into(),
            checked_by: "manual inspection".into(),
        }],
        owning_components: vec![PathBuf::from("crates/optimus-threads/src/panel.rs")],
        relevant_tests: Vec::new(),
        risk: RiskClass::Low,
        change_scope: ChangeScope { expected_files: 1 },
        stop_condition: "threads are never lost".into(),
    };
    let verdict = check(&TriageResult::Contract(output.clone()), &ctx);
    assert!(!verdict.is_admissible());

    // No admission, no drafts — there is no path from an ungrounded contract
    // to a recorded evidence item, which is the enforcement.
    let refused = output
        .evidence_drafts(navigator(), &verdict)
        .expect_err("an unadmitted contract yields no evidence");
    let explanation = refused.explain();
    assert!(
        explanation.contains("not in the issue"),
        "the retry needs the reason: {explanation}"
    );
    assert!(
        explanation.contains("does not exist"),
        "every grounding failure is named at once: {explanation}"
    );

    // The run is exactly where it was.
    let run = run_in_triage(ISSUE_ACTIONABLE);
    assert_eq!(run.phase, DevPhase::Triage);
    assert!(!run.can_exit_phase());
}

#[test]
fn a_vague_issue_is_refused_in_the_reporters_own_words() {
    let root = repo_root();
    let p = profile();
    let ctx = TriageContext {
        repo_root: &root,
        issue_body: ISSUE_VAGUE,
        profile: &p,
        limits: TriageLimits::CEILING,
    };
    let refusal = TriageResult::Refused {
        reason: RefusalReason::TooVague,
        detail: "no observable behaviour, no environment, no reproduction — there is \
            nothing to write a criterion against"
            .into(),
        evidence: vec!["feels slow lately and the UI could be better".into()],
        split_on: Vec::new(),
    };
    let verdict = check(&refusal, &ctx);
    assert!(verdict.is_admissible(), "{}", verdict.explain());

    // A grounded refusal is what halts the run — not the checker, which can
    // only ever blame the triage.
    let mut run = run_in_triage(ISSUE_VAGUE);
    let TriageResult::Refused { reason, detail, .. } = &refusal else {
        unreachable!()
    };
    run.halt(
        StopKind::NotActionable,
        format!("{}: {detail}", reason.as_str()),
    )
    .expect("a refusal halts the run");
    assert_eq!(run.phase, DevPhase::Abandoned);
    let stop = run.stop_reason.as_ref().expect("halted runs say why");
    assert!(
        stop.detail.starts_with("too_vague:"),
        "the issue comment can be written from the record: {}",
        stop.detail
    );
}

#[test]
fn an_issue_that_is_four_tasks_is_split_not_attempted() {
    let root = repo_root();
    let p = profile();
    let issue = "Please add retry to the runner, fix the panel scroll reset, migrate the \
        config format, and document the new phases.";
    let ctx = TriageContext {
        repo_root: &root,
        issue_body: issue,
        profile: &p,
        limits: TriageLimits::CEILING,
    };

    // Attempting it as one contract trips the size rule…
    let mut oversized = actionable_contract();
    oversized.evidence = vec!["fix the panel scroll reset".into()];
    oversized.owning_components = vec![
        PathBuf::from("crates"),
        PathBuf::from("apps"),
        PathBuf::from("docs"),
        PathBuf::from("scripts"),
    ];
    let verdict = check(&TriageResult::Contract(oversized), &ctx);
    assert!(!verdict.is_admissible());
    assert!(
        verdict.explain().contains("too_large refusal"),
        "the verdict points at the right remedy: {}",
        verdict.explain()
    );

    // …and the remedy it points at is admissible.
    let split = TriageResult::Refused {
        reason: RefusalReason::TooLarge,
        detail: "four deliverables under one number".into(),
        evidence: vec![
            "add retry to the runner".into(),
            "migrate the config format".into(),
        ],
        split_on: vec![
            "retry in the runner".into(),
            "panel scroll reset".into(),
            "config migration".into(),
            "phase documentation".into(),
        ],
    };
    let verdict = check(&split, &ctx);
    assert!(verdict.is_admissible(), "{}", verdict.explain());
}

#[test]
fn the_checker_holds_a_refusal_to_the_same_standard_as_a_contract() {
    // E41.5's failure mode is not only working vague issues — it is *refusing
    // workable ones*. An ungrounded refusal of the actionable issue is caught
    // by the same quote check that grounds contracts.
    let root = repo_root();
    let p = profile();
    let ctx = TriageContext {
        repo_root: &root,
        issue_body: ISSUE_ACTIONABLE,
        profile: &p,
        limits: TriageLimits::CEILING,
    };
    let lazy = TriageResult::Refused {
        reason: RefusalReason::TooVague,
        detail: "cannot reproduce".into(),
        evidence: vec!["no reproduction steps were provided by the reporter".into()],
        split_on: Vec::new(),
    };
    let verdict = check(&lazy, &ctx);
    assert!(
        !verdict.is_admissible(),
        "a refusal quoting words the reporter never wrote must not close their issue"
    );
    assert!(
        verdict.explain().contains("not in the issue"),
        "{}",
        verdict.explain()
    );
}

#[test]
fn triage_evidence_is_the_navigators_and_the_role_rules_agree() {
    // The P41 contract and the P43 boundary meet here: the drafts an admitted
    // contract produces are asserted by the navigator, and the run's role
    // rules accept them in TRIAGE — cheap to state, but it pins the two
    // subsystems together so neither can drift without this failing.
    let root = repo_root();
    let p = profile();
    let ctx = TriageContext {
        repo_root: &root,
        issue_body: ISSUE_ACTIONABLE,
        profile: &p,
        limits: TriageLimits::CEILING,
    };
    let output = actionable_contract();
    let verdict = check(&TriageResult::Contract(output.clone()), &ctx);
    let drafts = output
        .evidence_drafts(navigator(), &verdict)
        .expect("admitted");

    let mut run = run_in_triage(ISSUE_ACTIONABLE);
    for draft in drafts {
        let item = run.record(draft).expect("navigator evidence in TRIAGE");
        assert_eq!(item.author, navigator());
    }
    assert!(run.can_exit_phase());
}
