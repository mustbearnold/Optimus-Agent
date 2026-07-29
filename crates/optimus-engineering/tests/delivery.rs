//! Program P44 (E44.1/E44.2, ADR-0058): a run publishes the sentence a human
//! approved, and nothing else.
//!
//! Every push here is a real `git push` into a real bare repository, because
//! the properties worth proving are about what actually lands on a remote:
//! that nothing lands unapproved, that what lands is the approved commit even
//! when the branch has moved on, and that a receipt is only believed after
//! the remote itself reports the pushed commit. `gh` is the one command
//! stubbed — it reaches the forge — and the stub records its argument
//! vectors, so the tests can also prove what was *asked* of GitHub.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use optimus_engineering::{
    body_from_evidence, plan_publish, BranchProtection, CommandError, CommandOutcome,
    CommandRunner, DeliveryError, DevPhase, DevTaskRun, EvidenceItem, EvidenceKind, ProcessRunner,
    PublishPlan, Publisher, RepositoryPolicyProfile, Role, RoleIdentity, TaskOrigin,
    VerificationCommands,
};

const REPOSITORY: &str = "mustbearnold/fixture";

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
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn author(kind: EvidenceKind) -> RoleIdentity {
    let role = Role::producing(kind).expect("every kind has a producing role");
    RoleIdentity::new(role, format!("fixture-{}", role.as_str()))
}

fn satisfy_and_advance(run: &mut DevTaskRun, next: DevPhase) {
    for kind in run.phase.contract().required_evidence {
        if !run.satisfied_evidence().contains(kind) {
            run.record(EvidenceItem::stated_by(author(*kind), *kind, "fixture"))
                .unwrap();
        }
    }
    run.advance_to(next).unwrap();
}

/// A repository with one commit, a bare `origin` beside it, and a run parked
/// in `ReadyToPublish` — one human decision away from the phase under test.
struct Fixture {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    origin: PathBuf,
    run: DevTaskRun,
    evidence_dir: PathBuf,
}

fn ready_to_publish(task_id: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let origin = dir.path().join("origin.git");
    fs::create_dir_all(&repo).unwrap();
    git(
        origin.parent().unwrap(),
        &["init", "--quiet", "--bare", "origin.git"],
    );
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(&repo, &["config", "user.email", "runs@example.invalid"]);
    git(&repo, &["config", "user.name", "Optimus Test"]);
    fs::write(repo.join("lib.rs"), "pub fn answer() -> u8 { 42 }\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "seed"]);
    git(
        &repo,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );

    let mut run = DevTaskRun::new(
        task_id,
        TaskOrigin::Issue {
            number: 86,
            title: "the divider drifts on resize".into(),
        },
        &repo,
    );
    run.branch = Some("wip/fix-86".into());
    for next in [
        DevPhase::Triage,
        DevPhase::Investigate,
        DevPhase::Plan,
        DevPhase::PrepareWorktree,
        DevPhase::Implement,
        DevPhase::FocusedVerify,
        DevPhase::Review,
        DevPhase::FullVerify,
        DevPhase::ReadyToPublish,
    ] {
        satisfy_and_advance(&mut run, next);
    }

    let evidence_dir = dir.path().join("evidence");
    Fixture {
        _dir: dir,
        repo,
        origin,
        run,
        evidence_dir,
    }
}

impl Fixture {
    fn head(&self) -> String {
        git(&self.repo, &["rev-parse", "HEAD"])
    }

    fn plan(&self) -> PublishPlan {
        PublishPlan::new(
            "origin",
            Some(REPOSITORY.into()),
            "wip/fix-86",
            self.head(),
            "main",
        )
        .expect("a well-formed plan")
    }

    /// The human says yes to exactly this plan's sentence; the run enters the
    /// only phase that may publish.
    fn approve_and_enter_published(&mut self, plan: &PublishPlan) {
        self.run.record(plan.approval_draft()).unwrap();
        self.run.advance_to(DevPhase::Published).unwrap();
    }

    fn commit_something_new(&self) {
        fs::write(self.repo.join("lib.rs"), "pub fn answer() -> u8 { 43 }\n").unwrap();
        git(&self.repo, &["add", "-A"]);
        git(&self.repo, &["commit", "--quiet", "-m", "one more"]);
    }

    /// What the bare remote holds at the run's branch, asked directly.
    fn remote_tip(&self) -> Option<String> {
        let out = Command::new("git")
            .current_dir(&self.repo)
            .args([
                "ls-remote",
                self.origin.to_str().unwrap(),
                "refs/heads/wip/fix-86",
            ])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout
            .split_whitespace()
            .next()
            .filter(|sha| !sha.is_empty())
            .map(str::to_string)
    }
}

/// Real commands, except `gh`: canned success outcomes, with every argument
/// vector kept so a test can prove what the forge was asked.
#[derive(Clone)]
struct ForgeStub {
    create_stdout: String,
    view_stdout: String,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl ForgeStub {
    fn new(create_stdout: &str, view_stdout: &str) -> Self {
        Self {
            create_stdout: create_stdout.to_string(),
            view_stdout: view_stdout.to_string(),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn call_args(&self, subcommand: &str) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .find(|args| args.get(1).map(String::as_str) == Some(subcommand))
            .cloned()
            .unwrap_or_else(|| panic!("gh pr {subcommand} was never called"))
    }
}

fn view_json(number: u64, head: &str, base: &str) -> String {
    format!(
        "{{\"number\": {number}, \"headRefOid\": \"{head}\", \"isDraft\": true, \
         \"baseRefName\": \"{base}\", \"url\": \"https://github.com/{REPOSITORY}/pull/{number}\"}}"
    )
}

impl CommandRunner for ForgeStub {
    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutcome, CommandError> {
        if program != "gh" {
            return ProcessRunner.run(cwd, program, args, timeout);
        }
        self.calls.lock().unwrap().push(args.to_vec());
        let stdout = if args.get(1).map(String::as_str) == Some("create") {
            self.create_stdout.clone()
        } else {
            self.view_stdout.clone()
        };
        Ok(CommandOutcome {
            program: program.to_string(),
            args: args.to_vec(),
            exit_code: Some(0),
            stdout: stdout.into_bytes(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
            duration_ms: 1,
        })
    }
}

#[test]
fn an_unapproved_push_is_refused_and_nothing_reaches_the_remote() {
    let mut f = ready_to_publish("t-unapproved");
    let plan = f.plan();
    // A human approval exists — the generic fixture one — but it does not say
    // what this push would do, and "an approval" is not "the approval".
    f.run
        .record(EvidenceItem::stated_by(
            author(EvidenceKind::HumanApproval),
            EvidenceKind::HumanApproval,
            "fixture",
        ))
        .unwrap();
    f.run.advance_to(DevPhase::Published).unwrap();

    let mut publisher = Publisher::new(&mut f.run, ProcessRunner, &f.repo, &f.evidence_dir);
    let refused = publisher.push(&plan).expect_err("no approval says this");
    assert!(
        matches!(refused, DeliveryError::Unapproved { .. }),
        "{refused}"
    );
    assert_eq!(f.remote_tip(), None, "nothing may have reached the remote");
}

#[test]
fn approving_one_commit_does_not_approve_the_commit_after_it() {
    let mut f = ready_to_publish("t-stale");
    let approved_plan = f.plan();
    f.approve_and_enter_published(&approved_plan);

    // The worktree moves on after the human said yes.
    f.commit_something_new();
    let newer_plan = f.plan();

    let mut publisher = Publisher::new(&mut f.run, ProcessRunner, &f.repo, &f.evidence_dir);
    let refused = publisher
        .push(&newer_plan)
        .expect_err("the recorded sentence names the older commit");
    assert!(
        matches!(refused, DeliveryError::Unapproved { .. }),
        "{refused}"
    );
    assert_eq!(f.remote_tip(), None);
}

#[test]
fn an_approved_push_lands_and_the_remote_is_read_back_before_it_is_believed() {
    let mut f = ready_to_publish("t-lands");
    let plan = f.plan();
    let head = f.head();
    f.approve_and_enter_published(&plan);

    let mut publisher = Publisher::new(&mut f.run, ProcessRunner, &f.repo, &f.evidence_dir);
    let receipt = publisher.push(&plan).expect("an approved push lands");
    assert_eq!(receipt.head_sha, head);
    assert_eq!(f.remote_tip().as_ref(), Some(&head));

    let item = f.run.evidence.last().unwrap();
    assert_eq!(item.kind, EvidenceKind::PushReceipt);
    assert!(item.corroborates, "confirmed push must corroborate");
    assert_eq!(item.sha.as_ref(), Some(&head));
    let log = f
        .evidence_dir
        .join(format!("{}.log", item.output_digest.as_ref().unwrap()));
    assert!(
        log.is_file(),
        "the receipt's output is kept where the digest points"
    );
}

#[test]
fn the_push_publishes_the_approved_commit_not_what_came_after() {
    let mut f = ready_to_publish("t-exact");
    let plan = f.plan();
    let approved = f.head();
    f.approve_and_enter_published(&plan);

    // A commit lands *after* approval. A branch-name push would publish it.
    f.commit_something_new();
    let newest = f.head();
    assert_ne!(approved, newest);

    let mut publisher = Publisher::new(&mut f.run, ProcessRunner, &f.repo, &f.evidence_dir);
    publisher
        .push(&plan)
        .expect("the approved commit is still pushable");
    assert_eq!(
        f.remote_tip().as_ref(),
        Some(&approved),
        "the remote must hold the approved commit, not the branch tip"
    );
}

#[test]
fn a_phase_without_publish_authority_cannot_push_approval_or_not() {
    let f = ready_to_publish("t-authority");
    let plan = f.plan();
    // Rewind conceptually: the run is *not* in Published. Even with the exact
    // sentence on the record, FullVerify may not touch the remote.
    let mut run = DevTaskRun::new(
        "t-authority-inner",
        TaskOrigin::Request { text: "x".into() },
        &f.repo,
    );
    run.branch = Some("wip/fix-86".into());
    for next in [
        DevPhase::Triage,
        DevPhase::Investigate,
        DevPhase::Plan,
        DevPhase::PrepareWorktree,
        DevPhase::Implement,
        DevPhase::FocusedVerify,
        DevPhase::Review,
    ] {
        satisfy_and_advance(&mut run, next);
    }
    satisfy_and_advance(&mut run, DevPhase::FullVerify);
    run.record(plan.approval_draft()).unwrap();

    let mut publisher = Publisher::new(&mut run, ProcessRunner, &f.repo, &f.evidence_dir);
    let refused = publisher
        .push(&plan)
        .expect_err("FullVerify may not publish");
    assert!(
        matches!(
            refused,
            DeliveryError::CannotPublishFrom {
                phase: DevPhase::FullVerify
            }
        ),
        "{refused}"
    );
    assert_eq!(f.remote_tip(), None);
}

#[test]
fn the_pr_number_is_githubs_and_the_request_is_pinned_to_the_repository() {
    let mut f = ready_to_publish("t-number");
    let plan = f.plan();
    let head = f.head();
    f.approve_and_enter_published(&plan);

    let stub = ForgeStub::new(
        &format!("https://github.com/{REPOSITORY}/pull/118\n"),
        &view_json(118, &head, "main"),
    );
    let mut publisher = Publisher::new(&mut f.run, stub.clone(), &f.repo, &f.evidence_dir);
    publisher.push(&plan).expect("push first");
    let receipt = publisher
        .create_draft_pr(&plan)
        .expect("a confirmed draft PR");

    assert_eq!(
        receipt.number, 118,
        "the number GitHub printed, nothing else"
    );
    let create = stub.call_args("create");
    for expected in [
        "--draft",
        "--repo",
        REPOSITORY,
        "--head",
        "wip/fix-86",
        "--base",
        "main",
    ] {
        assert!(
            create.iter().any(|arg| arg == expected),
            "gh pr create was missing {expected:?}: {create:?}"
        );
    }
    let item = f.run.evidence.last().unwrap();
    assert_eq!(item.kind, EvidenceKind::DraftPullRequest);
    assert!(item.corroborates);
}

#[test]
fn forge_output_without_a_number_is_refused_not_guessed() {
    let mut f = ready_to_publish("t-no-number");
    let plan = f.plan();
    let head = f.head();
    f.approve_and_enter_published(&plan);

    let stub = ForgeStub::new("Creating pull request...\n", &view_json(1, &head, "main"));
    let mut publisher = Publisher::new(&mut f.run, stub, &f.repo, &f.evidence_dir);
    publisher.push(&plan).expect("push first");
    let refused = publisher
        .create_draft_pr(&plan)
        .expect_err("no number, no receipt");
    assert!(
        matches!(refused, DeliveryError::NumberUnparsed { .. }),
        "{refused}"
    );

    let item = f.run.evidence.last().unwrap();
    assert_eq!(item.kind, EvidenceKind::DraftPullRequest);
    assert!(
        !item.corroborates,
        "an unconfirmed creation satisfies nothing"
    );
    assert!(!f.run.can_exit_phase());
}

#[test]
fn a_pr_whose_confirmed_head_is_not_the_approved_commit_is_refused() {
    let mut f = ready_to_publish("t-wrong-head");
    let plan = f.plan();
    f.approve_and_enter_published(&plan);

    let stub = ForgeStub::new(
        &format!("https://github.com/{REPOSITORY}/pull/119\n"),
        &view_json(119, "beef00000000000000000000000000000000beef", "main"),
    );
    let mut publisher = Publisher::new(&mut f.run, stub, &f.repo, &f.evidence_dir);
    publisher.push(&plan).expect("push first");
    let refused = publisher.create_draft_pr(&plan).expect_err("head mismatch");
    assert!(
        matches!(
            refused,
            DeliveryError::PrUnconfirmed {
                field: "headRefOid",
                ..
            }
        ),
        "{refused}"
    );
    assert!(!f.run.evidence.last().unwrap().corroborates);
}

#[test]
fn a_pr_is_not_opened_for_a_branch_the_remote_does_not_hold() {
    let mut f = ready_to_publish("t-not-pushed");
    let plan = f.plan();
    f.approve_and_enter_published(&plan);

    // No push happened. gh must never be reached, so the real ProcessRunner
    // is safe here: the refusal comes from git's own answer.
    let mut publisher = Publisher::new(&mut f.run, ProcessRunner, &f.repo, &f.evidence_dir);
    let refused = publisher
        .create_draft_pr(&plan)
        .expect_err("nothing to open a PR for");
    assert!(
        matches!(refused, DeliveryError::RemoteHeadMismatch { .. }),
        "{refused}"
    );
}

#[test]
fn a_remote_without_a_forge_repository_cannot_be_asked_for_a_pr() {
    let mut f = ready_to_publish("t-no-forge");
    // Resolve the plan the honest way: from the run, the profile and git. The
    // bare directory's URL names no owner/name, and that is a recorded fact,
    // not a fallback.
    let profile = RepositoryPolicyProfile {
        default_branch: Some("main".into()),
        protection: BranchProtection::Unprotected,
        pr_template: None,
        instruction_files: Vec::new(),
        sensitive_paths: Vec::new(),
        verification: VerificationCommands::default(),
    };
    let plan = plan_publish(&ProcessRunner, &f.repo, &f.run, &profile, "origin")
        .expect("the plan resolves from what git says");
    assert_eq!(
        plan.head_sha(),
        f.head(),
        "the head is read, not remembered"
    );
    assert_eq!(plan.repository(), None);
    f.approve_and_enter_published(&plan);

    let mut publisher = Publisher::new(&mut f.run, ProcessRunner, &f.repo, &f.evidence_dir);
    publisher
        .push(&plan)
        .expect("pushing to a local remote is fine");
    let refused = publisher
        .create_draft_pr(&plan)
        .expect_err("no name to address");
    assert!(
        matches!(refused, DeliveryError::NoForgeRepository { .. }),
        "{refused}"
    );
}

#[test]
fn the_body_handed_to_the_forge_is_the_rendered_record_byte_for_byte() {
    let mut f = ready_to_publish("t-body");
    let plan = f.plan();
    let head = f.head();
    f.approve_and_enter_published(&plan);

    let stub = ForgeStub::new(
        &format!("https://github.com/{REPOSITORY}/pull/120\n"),
        &view_json(120, &head, "main"),
    );
    let mut publisher = Publisher::new(&mut f.run, stub.clone(), &f.repo, &f.evidence_dir);
    publisher.push(&plan).expect("push first");
    publisher.create_draft_pr(&plan).expect("created");

    let create = stub.call_args("create");
    let body_flag = create
        .iter()
        .position(|arg| arg == "--body")
        .expect("--body");
    let sent = &create[body_flag + 1];
    assert_eq!(
        sent,
        &body_from_evidence(&f.run),
        "the body must be the record's rendering, with no prose slipped in"
    );
    assert!(sent.contains("- Issue #86: the divider drifts on resize"));
    assert!(
        sent.contains(", evidence "),
        "claims must cite their rows: {sent}"
    );

    let title_flag = create
        .iter()
        .position(|arg| arg == "--title")
        .expect("--title");
    assert_eq!(
        create[title_flag + 1],
        "the divider drifts on resize",
        "the title comes from the task origin"
    );
}

#[test]
fn published_exits_only_when_both_receipts_are_confirmed() {
    let mut f = ready_to_publish("t-exit");
    let plan = f.plan();
    let head = f.head();
    f.approve_and_enter_published(&plan);
    assert!(!f.run.can_exit_phase(), "nothing is earned yet");

    let stub = ForgeStub::new(
        &format!("https://github.com/{REPOSITORY}/pull/121\n"),
        &view_json(121, &head, "main"),
    );
    let mut publisher = Publisher::new(&mut f.run, stub, &f.repo, &f.evidence_dir);
    publisher.push(&plan).expect("push");
    assert!(
        !publisher.run().can_exit_phase(),
        "a push alone is half a publication"
    );
    publisher.create_draft_pr(&plan).expect("draft PR");

    assert!(f.run.can_exit_phase());
    f.run
        .advance_to(DevPhase::WaitingForCi)
        .expect("both receipts are on the record");
    assert_eq!(f.run.phase, DevPhase::WaitingForCi);
}
