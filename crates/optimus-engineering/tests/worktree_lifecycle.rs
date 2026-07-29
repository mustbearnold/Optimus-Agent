//! Program P40 exit gate: a run works in its own checkout, and never in the
//! main one.
//!
//! These tests drive real `git`. They build a throwaway repository per test,
//! so nothing here depends on the repository they are running inside.

use std::fs;
use std::path::Path;
use std::process::Command;

use optimus_engineering::{DevTaskRun, RemovalOutcome, TaskOrigin, WorktreeManager};

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

/// A one-commit repository with a deterministic identity and no reliance on
/// the host's git config.
fn seed_repo(root: &Path) {
    git(root, &["init", "--quiet"]);
    git(root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(root, &["config", "user.email", "runs@example.invalid"]);
    git(root, &["config", "user.name", "Optimus Test"]);
    fs::write(root.join("lib.rs"), "pub fn answer() -> u8 { 41 }\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "--quiet", "-m", "seed"]);
}

#[test]
fn a_run_gets_its_own_checkout_at_a_recorded_base() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);

    let manager = WorktreeManager::with_default_runs_dir(root);
    let prepared = manager.prepare("t-1", "wip/fix-answer", "main").unwrap();

    assert!(!prepared.reused);
    assert!(prepared.path.join("lib.rs").exists());
    assert_eq!(prepared.base_sha, git(root, &["rev-parse", "HEAD"]).trim());
    assert_eq!(prepared.base_sha.len(), 40, "base is a full SHA");
    assert_eq!(manager.head_sha(&prepared.path).unwrap(), prepared.base_sha);
}

#[test]
fn work_in_a_run_does_not_touch_the_main_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);
    let before = fs::read_to_string(root.join("lib.rs")).unwrap();

    let manager = WorktreeManager::with_default_runs_dir(root);
    let prepared = manager.prepare("t-1", "wip/fix-answer", "main").unwrap();
    fs::write(
        prepared.path.join("lib.rs"),
        "pub fn answer() -> u8 { 42 }\n",
    )
    .unwrap();

    // The main checkout's content is untouched, and git agrees it is clean.
    assert_eq!(fs::read_to_string(root.join("lib.rs")).unwrap(), before);
    assert!(
        manager.dirty_paths(root).unwrap().is_empty(),
        "the main checkout must stay clean while a run works"
    );
    assert!(!manager.dirty_paths(&prepared.path).unwrap().is_empty());

    let diff = manager
        .diff_against_base(&prepared.path, &prepared.base_sha)
        .unwrap();
    assert!(diff.contains("42"), "the diff sees the run's change");
}

#[test]
fn two_runs_do_not_collide() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);

    let manager = WorktreeManager::with_default_runs_dir(root);
    let a = manager.prepare("t-a", "wip/a", "main").unwrap();
    let b = manager.prepare("t-b", "wip/b", "main").unwrap();

    assert_ne!(a.path, b.path);
    fs::write(a.path.join("lib.rs"), "// a\n").unwrap();
    assert_eq!(
        fs::read_to_string(b.path.join("lib.rs")).unwrap(),
        "pub fn answer() -> u8 { 41 }\n"
    );
}

#[test]
fn preparing_twice_reattaches_instead_of_starting_over() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);

    let manager = WorktreeManager::with_default_runs_dir(root);
    let first = manager.prepare("t-1", "wip/fix", "main").unwrap();
    fs::write(first.path.join("scratch.txt"), "half-finished\n").unwrap();

    let second = manager.prepare("t-1", "wip/fix", "main").unwrap();
    assert!(second.reused);
    assert_eq!(second.path, first.path);
    assert!(
        second.path.join("scratch.txt").exists(),
        "a resumed run keeps its work in progress"
    );
}

#[test]
fn a_dirty_worktree_is_retained_and_reported_not_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);

    let manager = WorktreeManager::with_default_runs_dir(root);
    let prepared = manager.prepare("t-1", "wip/fix", "main").unwrap();
    fs::write(prepared.path.join("lib.rs"), "// unfinished\n").unwrap();

    match manager.remove("t-1", false).unwrap() {
        RemovalOutcome::Retained { path, dirty_paths } => {
            assert_eq!(path, prepared.path);
            assert!(dirty_paths.iter().any(|p| p.contains("lib.rs")));
            assert!(path.exists(), "the work is still there");
        }
        other => panic!("expected the dirty worktree to be retained, got {other:?}"),
    }

    // Discarding it is a separate, explicit decision.
    assert!(matches!(
        manager.remove("t-1", true).unwrap(),
        RemovalOutcome::Removed { .. }
    ));
    assert!(!prepared.path.exists());
}

#[test]
fn a_clean_worktree_is_removed_but_the_branch_survives() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);

    let manager = WorktreeManager::with_default_runs_dir(root);
    let prepared = manager.prepare("t-1", "wip/fix", "main").unwrap();
    fs::write(
        prepared.path.join("lib.rs"),
        "pub fn answer() -> u8 { 42 }\n",
    )
    .unwrap();
    git(&prepared.path, &["add", "-A"]);
    git(
        &prepared.path,
        &["commit", "--quiet", "-m", "fix the answer"],
    );

    assert!(matches!(
        manager.remove("t-1", false).unwrap(),
        RemovalOutcome::Removed { .. }
    ));
    assert!(!prepared.path.exists());
    // The committed work is still reachable: only the checkout went away.
    assert!(
        git(root, &["rev-parse", "--verify", "wip/fix"])
            .trim()
            .len()
            == 40
    );
    assert!(matches!(
        manager.remove("t-1", false).unwrap(),
        RemovalOutcome::Absent
    ));
}

#[test]
fn the_run_record_binds_containment_to_the_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);

    let manager = WorktreeManager::with_default_runs_dir(root);
    let prepared = manager.prepare("t-1", "wip/fix", "main").unwrap();

    let mut run = DevTaskRun::new(
        "t-1",
        TaskOrigin::Request {
            text: "fix the answer".into(),
        },
        root,
    );
    run.worktree_path = Some(prepared.path.clone());
    run.base_sha = Some(prepared.base_sha.clone());
    run.branch = Some(prepared.branch.clone());

    assert!(run
        .assert_within_worktree(&prepared.path.join("lib.rs"))
        .is_ok());
    // The main checkout is outside the run's boundary, like any other tree.
    assert!(run.assert_within_worktree(&root.join("lib.rs")).is_err());
}
