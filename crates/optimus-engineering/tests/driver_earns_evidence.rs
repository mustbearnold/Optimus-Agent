//! Program P40 exit gate (E40.8): a phase advances because a command exited
//! zero, and for no other reason.
//!
//! Every test here runs a real child process in a real git worktree. That is
//! the point — the phase table already refused bad transitions when a test
//! wrote the evidence by hand, and what was untested was whether evidence can
//! be *earned* honestly. The failure this guards against is a driver that
//! records "the tests passed" because the model said so.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use optimus_engineering::{
    DevPhase, DevTaskRun, EvidenceItem, EvidenceKind, PhaseStep, ProcessRunner, RunDriver,
    TaskOrigin, WorktreeManager,
};

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn seed_repo(root: &Path) {
    git(root, &["init", "--quiet"]);
    git(root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(root, &["config", "user.email", "runs@example.invalid"]);
    git(root, &["config", "user.name", "Optimus Test"]);
    fs::write(root.join("lib.rs"), "pub fn answer() -> u8 { 41 }\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "--quiet", "-m", "seed"]);
}

fn satisfy_and_advance(run: &mut DevTaskRun, next: DevPhase) {
    for kind in run.phase.contract().required_evidence {
        if !run.satisfied_evidence().contains(kind) {
            run.record(EvidenceItem::stated(*kind, "fixture")).unwrap();
        }
    }
    run.advance_to(next).unwrap();
}

/// A run parked in `Implement` with a live worktree, which is where the driver
/// does most of its work.
struct Fixture {
    _dir: tempfile::TempDir,
    run: DevTaskRun,
    worktree: std::path::PathBuf,
    evidence_dir: std::path::PathBuf,
}

fn implementing(task_id: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    seed_repo(&root);
    let manager = WorktreeManager::with_default_runs_dir(&root);
    let prepared = manager
        .prepare(task_id, &format!("wip/{task_id}"), "main")
        .unwrap();

    let mut run = DevTaskRun::new(
        task_id,
        TaskOrigin::Request {
            text: "earn some evidence".into(),
        },
        &root,
    );
    for next in [
        DevPhase::Triage,
        DevPhase::Investigate,
        DevPhase::Plan,
        DevPhase::PrepareWorktree,
    ] {
        satisfy_and_advance(&mut run, next);
    }
    run.worktree_path = Some(prepared.path.clone());
    run.base_sha = Some(prepared.base_sha.clone());
    run.branch = Some(prepared.branch.clone());
    satisfy_and_advance(&mut run, DevPhase::Implement);

    let evidence_dir = manager.runs_dir().join(task_id).join("evidence");
    Fixture {
        _dir: dir,
        run,
        worktree: prepared.path,
        evidence_dir,
    }
}

#[test]
fn a_command_that_exits_zero_earns_its_phase() {
    let mut f = implementing("t-green");
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    // `Implement` owes two facts, so two commands have to come back green.
    let steps = vec![
        PhaseStep::new(EvidenceKind::Diff, "git", &["status", "--short"]),
        PhaseStep::new(EvidenceKind::RegressionTest, "git", &["--version"]),
    ];
    let outcome = driver.drive(&steps, DevPhase::FocusedVerify).unwrap();

    assert!(outcome.advanced, "green steps should leave Implement");
    assert!(outcome.missing.is_empty());
    assert_eq!(f.run.phase, DevPhase::FocusedVerify);
}

#[test]
fn a_failing_command_is_recorded_but_satisfies_nothing() {
    let mut f = implementing("t-red");
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    // `git cat-file -e` on a bogus object exits non-zero without side effects.
    let step = PhaseStep::new(
        EvidenceKind::Diff,
        "git",
        &["cat-file", "-e", "0000000000000000000000000000000000000000"],
    );
    let outcome = driver.drive(&[step], DevPhase::FocusedVerify).unwrap();

    assert!(
        !outcome.advanced,
        "a red command must not advance the phase"
    );
    assert_eq!(
        outcome.missing,
        vec![EvidenceKind::Diff, EvidenceKind::RegressionTest],
        "the failed command left its own evidence unearned"
    );
    assert_eq!(f.run.phase, DevPhase::Implement);

    // Recorded anyway: the log is honest about what was tried.
    let item = f.run.evidence.last().unwrap();
    assert_eq!(item.kind, EvidenceKind::Diff);
    assert_ne!(item.exit_status, Some(0), "the real status is kept");
    assert!(!f.run.can_exit_phase());
}

#[test]
fn the_driver_stops_at_the_first_red_step() {
    let mut f = implementing("t-stop");
    let before = f.run.evidence.len();
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    let steps = vec![
        PhaseStep::new(EvidenceKind::Diff, "git", &["status", "--short"]),
        PhaseStep::new(EvidenceKind::Diff, "git", &["rev-parse", "nope"]),
        PhaseStep::new(EvidenceKind::Diff, "git", &["status", "--short"]),
    ];
    let outcome = driver.drive(&steps, DevPhase::FocusedVerify).unwrap();

    assert_eq!(outcome.steps.len(), 2, "the third step must not run");
    assert!(outcome.first_failure().is_some());
    assert!(!outcome.advanced);
    assert_eq!(f.run.evidence.len(), before + 2);
}

#[test]
fn a_focused_verify_needs_both_facts_not_one_green_run() {
    let mut f = implementing("t-both");
    satisfy_and_advance(&mut f.run, DevPhase::FocusedVerify);
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    // A genuinely passing test run, and nothing else.
    let step = PhaseStep::new(EvidenceKind::FocusedTestRun, "git", &["status", "--short"]);
    let outcome = driver.drive(&[step], DevPhase::Review).unwrap();

    assert!(outcome.steps[0].corroborated, "the step really did pass");
    assert!(
        !outcome.advanced,
        "green tests alone do not finish verification"
    );
    assert_eq!(outcome.missing, vec![EvidenceKind::DifferentialProof]);
}

#[test]
fn a_differential_proof_is_earned_by_failing_at_base() {
    let mut f = implementing("t-differential");
    satisfy_and_advance(&mut f.run, DevPhase::FocusedVerify);
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    let steps = vec![
        // Passes at head, as a fixed bug should.
        PhaseStep::new(EvidenceKind::FocusedTestRun, "git", &["status", "--short"]),
        // Fails at base, which is the whole proof.
        PhaseStep::new(
            EvidenceKind::DifferentialProof,
            "git",
            &["cat-file", "-e", "0000000000000000000000000000000000000000"],
        )
        .expecting_failure()
        .with_summary("regression test against the base commit"),
    ];
    let outcome = driver.drive(&steps, DevPhase::Review).unwrap();

    let proof = &outcome.steps[1];
    assert!(!proof.outcome.succeeded(), "the command really did fail");
    assert!(proof.corroborated, "and failing is what proved the point");
    assert!(outcome.advanced);
    assert_eq!(f.run.phase, DevPhase::Review);
}

#[test]
fn a_differential_proof_that_passes_at_base_earns_nothing() {
    let mut f = implementing("t-differential-green");
    satisfy_and_advance(&mut f.run, DevPhase::FocusedVerify);
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    let steps = vec![
        PhaseStep::new(EvidenceKind::FocusedTestRun, "git", &["status", "--short"]),
        // Green at base: the regression test does not exercise the bug.
        PhaseStep::new(EvidenceKind::DifferentialProof, "git", &["--version"]).expecting_failure(),
    ];
    let outcome = driver.drive(&steps, DevPhase::Review).unwrap();

    let proof = &outcome.steps[1];
    assert!(proof.outcome.succeeded(), "the command exited zero");
    assert!(
        !proof.corroborated,
        "which is exactly the wrong outcome here"
    );
    assert!(!outcome.advanced);
    assert_eq!(outcome.missing, vec![EvidenceKind::DifferentialProof]);
    assert_eq!(f.run.phase, DevPhase::FocusedVerify);
}

#[test]
fn captured_output_is_kept_where_the_digest_points() {
    let mut f = implementing("t-log");
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    let step = PhaseStep::new(EvidenceKind::Diff, "git", &["--version"]);
    driver.earn(&step).unwrap();

    let item = f.run.evidence.last().unwrap();
    let stored = f
        .evidence_dir
        .join(format!("{}.log", item.output_digest.as_ref().unwrap()));
    assert!(stored.is_file(), "the digest must resolve to real bytes");
    let text = fs::read_to_string(&stored).unwrap();
    assert!(text.contains("git version"), "got: {text:?}");
}

#[test]
fn a_command_runs_in_the_worktree_not_the_main_checkout() {
    let mut f = implementing("t-cwd");
    let worktree = f.worktree.clone();
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &worktree, &f.evidence_dir);

    let step = PhaseStep::new(EvidenceKind::Diff, "git", &["rev-parse", "--show-toplevel"]);
    let result = driver.earn(&step).unwrap();

    let reported = String::from_utf8_lossy(&result.outcome.stdout)
        .trim()
        .to_string();
    let reported = fs::canonicalize(&reported).unwrap();
    assert_eq!(reported, fs::canonicalize(&worktree).unwrap());
}

#[test]
fn a_command_that_outlives_its_deadline_is_killed_and_fails() {
    let mut f = implementing("t-timeout");
    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);

    let step = PhaseStep::new(EvidenceKind::FocusedTestRun, "sleep", &["30"])
        .with_timeout(Duration::from_millis(300));
    let result = driver.earn(&step).unwrap();

    assert!(result.outcome.timed_out);
    assert!(!result.corroborated, "a timeout is never a pass");
    assert!(
        result.outcome.duration_ms < 20_000,
        "the deadline should have ended it, took {}ms",
        result.outcome.duration_ms
    );
    assert!(!f.run.can_exit_phase());
}

#[test]
fn the_head_sha_is_read_from_git_rather_than_remembered() {
    let mut f = implementing("t-head");
    let worktree = f.worktree.clone();
    let base = f.run.base_sha.clone().unwrap();

    fs::write(worktree.join("lib.rs"), "pub fn answer() -> u8 { 42 }\n").unwrap();
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "--quiet", "-m", "fix the answer"]);

    let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &worktree, &f.evidence_dir);
    let head = driver.head_sha().unwrap();
    assert_ne!(head, base, "the driver must see the new commit");

    let step = PhaseStep::new(EvidenceKind::Diff, "git", &["--version"]);
    driver.earn(&step).unwrap();
    assert_eq!(f.run.evidence.last().unwrap().sha.as_ref(), Some(&head));
}

#[test]
fn a_re_entered_phase_does_not_inherit_the_previous_attempt_s_green_run() {
    let mut f = implementing("t-attempt");
    satisfy_and_advance(&mut f.run, DevPhase::FocusedVerify);

    // Earn FocusedVerify honestly, then bounce back to Implement.
    for kind in [
        EvidenceKind::FocusedTestRun,
        EvidenceKind::DifferentialProof,
    ] {
        let mut driver = RunDriver::new(&mut f.run, ProcessRunner, &f.worktree, &f.evidence_dir);
        driver
            .earn(&PhaseStep::new(kind, "git", &["status", "--short"]))
            .unwrap();
    }
    assert!(f.run.can_exit_phase());
    f.run.advance_to(DevPhase::Implement).unwrap();
    satisfy_and_advance(&mut f.run, DevPhase::FocusedVerify);

    assert!(
        !f.run.can_exit_phase(),
        "the second attempt must run its own commands"
    );
    assert_eq!(
        f.run
            .phase
            .contract()
            .missing_evidence(&f.run.satisfied_evidence()),
        vec![
            EvidenceKind::FocusedTestRun,
            EvidenceKind::DifferentialProof
        ]
    );
}
