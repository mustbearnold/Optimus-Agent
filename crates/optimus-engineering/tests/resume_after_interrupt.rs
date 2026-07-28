//! Program P40 exit gate: a run killed mid-`IMPLEMENT` comes back in
//! `IMPLEMENT`, against the same worktree and the same base SHA.
//!
//! The interruption is modelled the way it actually happens: the process
//! disappears without a chance to tidy up, and a *different* process loads the
//! record from disk. Nothing is carried in memory between the two halves.

use std::fs;
use std::path::Path;
use std::process::Command;

use optimus_engineering::{
    DevPhase, DevTaskRun, EvidenceItem, EvidenceKind, StopKind, TaskOrigin, WorktreeManager,
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

#[test]
fn an_interrupted_run_resumes_in_the_same_phase_and_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    seed_repo(root);
    let manager = WorktreeManager::with_default_runs_dir(root);
    let record = DevTaskRun::record_path(manager.runs_dir(), "t-resume");

    // ---- first process ----
    {
        let mut run = DevTaskRun::new(
            "t-resume",
            TaskOrigin::Issue {
                number: 11,
                title: "the answer is wrong".into(),
            },
            root,
        );
        satisfy_and_advance(&mut run, DevPhase::Triage);
        satisfy_and_advance(&mut run, DevPhase::Investigate);
        satisfy_and_advance(&mut run, DevPhase::Plan);
        satisfy_and_advance(&mut run, DevPhase::PrepareWorktree);

        let prepared = manager.prepare("t-resume", "wip/answer", "main").unwrap();
        run.worktree_path = Some(prepared.path.clone());
        run.base_sha = Some(prepared.base_sha.clone());
        run.branch = Some(prepared.branch.clone());
        run.record(EvidenceItem::stated(
            EvidenceKind::WorktreeReady,
            prepared.path.display().to_string(),
        ))
        .unwrap();
        run.record(EvidenceItem::observed(
            EvidenceKind::BaselineVerification,
            "just check",
            0,
            &prepared.base_sha,
            b"clean",
        ))
        .unwrap();
        run.advance_to(DevPhase::Implement).unwrap();

        run.record(EvidenceItem::stated(EvidenceKind::Diff, "half a patch"))
            .unwrap();
        run.save(&record).unwrap();

        // Half-written work in the worktree, and then the process is gone.
        fs::write(
            prepared.path.join("lib.rs"),
            "pub fn answer() -> u8 { 4 }\n",
        )
        .unwrap();
    }

    // ---- second process: nothing survives except the disk ----
    let resumed = DevTaskRun::load(&record).unwrap();

    assert_eq!(resumed.phase, DevPhase::Implement);
    assert_eq!(resumed.task_id, "t-resume");
    let worktree = resumed.worktree_path.clone().expect("worktree recorded");
    let base_sha = resumed.base_sha.clone().expect("base sha recorded");

    // The same checkout, with the interrupted work still in it.
    let prepared = manager.prepare("t-resume", "wip/answer", "main").unwrap();
    assert!(prepared.reused);
    assert_eq!(prepared.path, worktree);
    assert_eq!(
        fs::read_to_string(worktree.join("lib.rs")).unwrap(),
        "pub fn answer() -> u8 { 4 }\n"
    );

    // The base is the commit it branched from, not today's HEAD.
    assert_eq!(prepared.base_sha, base_sha);

    // The investigation is intact, not replayed from an empty record.
    assert!(resumed
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::ImpactMap));
    assert!(resumed
        .evidence
        .iter()
        .any(|e| e.kind == EvidenceKind::Diff && e.phase == DevPhase::Implement));
}

#[test]
fn a_blocked_run_returns_to_the_phase_it_was_blocked_in() {
    let dir = tempfile::tempdir().unwrap();
    let record = DevTaskRun::record_path(dir.path(), "t-block");

    let mut run = DevTaskRun::new(
        "t-block",
        TaskOrigin::Request {
            text: "fix it".into(),
        },
        dir.path(),
    );
    satisfy_and_advance(&mut run, DevPhase::Triage);
    satisfy_and_advance(&mut run, DevPhase::Investigate);
    run.halt(StopKind::ToolFailure, "provider returned 503")
        .unwrap();
    run.save(&record).unwrap();

    let mut resumed = DevTaskRun::load(&record).unwrap();
    assert_eq!(resumed.phase, DevPhase::Blocked);
    assert_eq!(
        resumed.stop_reason.as_ref().map(|s| s.phase),
        Some(DevPhase::Investigate)
    );

    assert_eq!(resumed.resume().unwrap(), DevPhase::Investigate);
    assert_eq!(resumed.phase, DevPhase::Investigate);
    assert!(resumed.stop_reason.is_none());
    // A resumed phase re-earns its exit.
    assert!(!resumed.can_exit_phase());
}

#[test]
fn a_partly_written_record_never_replaces_a_good_one() {
    let dir = tempfile::tempdir().unwrap();
    let record = DevTaskRun::record_path(dir.path(), "t-atomic");

    let mut run = DevTaskRun::new(
        "t-atomic",
        TaskOrigin::Request {
            text: "fix it".into(),
        },
        dir.path(),
    );
    run.save(&record).unwrap();
    satisfy_and_advance(&mut run, DevPhase::Triage);
    run.save(&record).unwrap();

    // The temp file the atomic write uses is never left behind on success.
    assert!(!record.with_extension("json.tmp").exists());
    assert_eq!(DevTaskRun::load(&record).unwrap().phase, DevPhase::Triage);
}
