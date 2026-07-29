//! Program P42 exit gate: a fix is not proven by a green suite.
//!
//! These drive real `git`, real worktrees and real child processes against a
//! throwaway repository per test. The "bug" is a line in `src.txt` and the
//! "regression test" is a shell script that reads it — small enough to reason
//! about completely, real enough that nothing here is mocked into agreeing.
//!
//! The case that matters most is
//! [`a_test_that_passes_without_the_fix_is_refused`]: that patch has a real
//! fix, a real test, and a green suite, and the proof refuses it anyway
//! because the test would not have caught the bug.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use optimus_engineering::{
    DifferentialError, DifferentialProver, DifferentialRequest, DifferentialVerdict, ProcessRunner,
    WorktreeManager,
};

const TIMEOUT: Duration = Duration::from_secs(60);

fn git(cwd: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository whose one commit contains the bug: `src.txt` says `broken`.
fn seed_repo(root: &Path) -> String {
    git(root, &["init", "--quiet"]);
    git(root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(root, &["config", "user.email", "runs@example.invalid"]);
    git(root, &["config", "user.name", "Optimus Test"]);
    fs::write(root.join("src.txt"), "broken\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "--quiet", "-m", "seed with the bug"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

/// Cut a run worktree and put the fix and the test in it.
///
/// `fix` is written to `src.txt`; `test_body` becomes `check.sh`, which is the
/// file the proof carries back to base.
fn patch_worktree(root: &Path, base_sha: &str, fix: &str, test_body: &str) -> std::path::PathBuf {
    let manager = WorktreeManager::with_default_runs_dir(root);
    let prepared = manager
        .prepare("task-1", "wip/task-1", base_sha)
        .expect("worktree");
    fs::write(prepared.path.join("src.txt"), fix).unwrap();
    fs::write(prepared.path.join("check.sh"), test_body).unwrap();
    prepared.path
}

fn request(base_sha: &str, worktree: &Path) -> DifferentialRequest {
    DifferentialRequest {
        task_id: "task-1".into(),
        base_sha: base_sha.to_string(),
        patch_worktree: worktree.to_path_buf(),
        test_paths: vec!["check.sh".into()],
        program: "sh".into(),
        args: vec!["check.sh".into()],
        timeout: TIMEOUT,
    }
}

fn prover(root: &Path) -> DifferentialProver {
    DifferentialProver::new(root, root.join("local").join("runs"))
}

// --- the four verdicts -------------------------------------------------------

#[test]
fn a_test_that_fails_at_base_and_passes_on_the_patch_is_proven() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    // The test reads the thing the fix changes, so it can only pass with it.
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");

    let proof = prover(root)
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    assert_eq!(proof.verdict, DifferentialVerdict::Proven);
    assert!(proof.proves_the_fix());
    // The test is new, so base had never seen it.
    assert_eq!(
        proof.newly_added,
        vec![std::path::PathBuf::from("check.sh")]
    );
    assert!(proof.base.is_some() && proof.patch.is_some());
}

#[test]
fn a_test_that_passes_without_the_fix_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    // A real fix and a real, green test — that tests nothing about the bug.
    // This patch would sail through any suite-is-green check.
    let tree = patch_worktree(root, &base, "fixed\n", "exit 0\n");

    let proof = prover(root)
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    assert_eq!(proof.verdict, DifferentialVerdict::TestPassesWithoutTheFix);
    assert!(!proof.proves_the_fix());
    assert!(
        proof.verdict.explain().contains("not testing the bug"),
        "{}",
        proof.verdict.explain()
    );
    // Both runs happened and both were green. Exit status alone would have
    // called this a pass.
    assert!(proof.base.as_ref().unwrap().succeeded());
    assert!(proof.patch.as_ref().unwrap().succeeded());
}

#[test]
fn a_fix_that_does_not_fix_it_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "still broken\n", "grep -q fixed src.txt\n");

    let proof = prover(root)
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    assert_eq!(proof.verdict, DifferentialVerdict::NotFixed);
    assert!(!proof.proves_the_fix());
}

#[test]
fn a_patch_that_breaks_a_passing_test_is_named_as_such() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    // The test asserts the old behaviour; the patch changes it.
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q broken src.txt\n");

    let proof = prover(root)
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    assert_eq!(proof.verdict, DifferentialVerdict::PatchBrokeIt);
    assert!(!proof.proves_the_fix());
}

// --- the third state ---------------------------------------------------------

#[test]
fn a_base_run_that_never_reached_the_test_is_inconclusive_not_proven() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    // The base checkout cannot build. Exit status is non-zero either way, and
    // reading that as "the test failed at base" would manufacture a proof out
    // of a broken build.
    let tree = patch_worktree(
        root,
        &base,
        "fixed\n",
        "echo 'error: could not compile `thing`' >&2; exit 101\n",
    );

    let proof = prover(root)
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    assert!(
        matches!(proof.verdict, DifferentialVerdict::Inconclusive { .. }),
        "{:?}",
        proof.verdict
    );
    assert!(!proof.proves_the_fix());
    // The patch run is skipped: nothing it could return would rescue the proof.
    assert!(proof.patch.is_none());
}

#[test]
fn a_base_run_that_times_out_is_inconclusive() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    // `sleep` is a child of the shell, not the shell itself. Killing only the
    // shell leaves it holding the stdout pipe, so the drain never reaches EOF
    // and the "timeout" returns after the full 30 seconds. The deadline has to
    // take the whole process group with it.
    let tree = patch_worktree(root, &base, "fixed\n", "sleep 30\n");

    let mut req = request(&base, &tree);
    req.timeout = Duration::from_millis(300);

    let started = std::time::Instant::now();
    let proof = prover(root).prove(&ProcessRunner, &req).expect("proof");
    let elapsed = started.elapsed();

    match &proof.verdict {
        DifferentialVerdict::Inconclusive { reason } => {
            assert!(reason.contains("timed out"), "{reason}");
        }
        other => panic!("expected inconclusive, got {other:?}"),
    }
    assert!(
        elapsed < Duration::from_secs(10),
        "the deadline did not bound wall time: {elapsed:?} for a 300ms timeout"
    );
}

// --- what the base checkout is allowed to contain ----------------------------

#[test]
fn the_base_checkout_gets_the_test_but_never_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");

    let prover = prover(root);
    prover
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    // The base checkout survives the proof, so its contents can be inspected:
    // the carried test is there, and the source is still the buggy one.
    let checkout = prover.base_checkout_path("task-1");
    assert!(checkout.join("check.sh").is_file());
    assert_eq!(
        fs::read_to_string(checkout.join("src.txt")).unwrap(),
        "broken\n",
        "the fix leaked into the base checkout; every verdict from it is void"
    );
}

#[test]
fn the_base_checkout_is_detached_so_a_run_cannot_push_from_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");

    let prover = prover(root);
    prover
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    let checkout = prover.base_checkout_path("task-1");
    // Detached HEAD reports the literal name "HEAD" rather than a branch.
    let head = git(&checkout, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(head.trim(), "HEAD", "base checkout is on a branch: {head}");
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]).trim(), base);
}

#[test]
fn a_stale_base_checkout_is_not_reused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");
    let prover = prover(root);

    prover
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("first proof");

    // Something an interrupted proof might have left behind. If the second
    // proof reused this checkout, the leftover would be in it and the verdict
    // would be about a tree nobody described.
    let checkout = prover.base_checkout_path("task-1");
    fs::write(checkout.join("leftover.txt"), "from a previous attempt\n").unwrap();

    let proof = prover
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("second proof");

    assert_eq!(proof.verdict, DifferentialVerdict::Proven);
    assert!(!prover
        .base_checkout_path("task-1")
        .join("leftover.txt")
        .exists());
}

#[test]
fn discarding_a_base_checkout_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");
    let prover = prover(root);

    prover
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");
    prover.discard_base_checkout("task-1").expect("first");
    prover.discard_base_checkout("task-1").expect("second");
    assert!(!prover.base_checkout_path("task-1").exists());
}

// --- refusing a malformed request --------------------------------------------

#[test]
fn a_proof_with_no_test_to_carry_is_refused_before_anything_is_checked_out() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");

    let mut req = request(&base, &tree);
    req.test_paths.clear();

    let prover = prover(root);
    let error = prover.prove(&ProcessRunner, &req).unwrap_err();
    assert!(matches!(error, DifferentialError::NoTestPaths), "{error}");
    // Nothing was cut, so nothing needs cleaning up.
    assert!(!prover.base_checkout_path("task-1").exists());
}

#[test]
fn a_named_test_that_does_not_exist_is_an_error_not_a_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");

    let mut req = request(&base, &tree);
    req.test_paths = vec!["tests/never_written.rs".into()];

    let error = prover(root).prove(&ProcessRunner, &req).unwrap_err();
    assert!(
        matches!(error, DifferentialError::MissingTestPath(_)),
        "{error}"
    );
}

#[test]
fn a_test_path_cannot_climb_out_of_the_patch_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");

    for escape in ["../src.txt", "/etc/passwd"] {
        let mut req = request(&base, &tree);
        req.test_paths = vec![escape.into()];
        let error = prover(root).prove(&ProcessRunner, &req).unwrap_err();
        assert!(
            matches!(error, DifferentialError::EscapingTestPath(_)),
            "{escape}: {error}"
        );
    }
}

// --- what the proof records ---------------------------------------------------

#[test]
fn the_summary_names_the_base_commit_and_the_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let base = seed_repo(root);
    let tree = patch_worktree(root, &base, "fixed\n", "grep -q fixed src.txt\n");

    let proof = prover(root)
        .prove(&ProcessRunner, &request(&base, &tree))
        .expect("proof");

    let summary = proof.summary();
    assert!(summary.contains(&base[..12]), "{summary}");
    assert!(summary.contains("proven"), "{summary}");
}
