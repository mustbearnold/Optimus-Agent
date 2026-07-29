//! Program P41 exit gate (E41.1–E41.3): a repository's policy is resolved from
//! the repository, not reconstructed inside a prompt.
//!
//! The forge cases use a scripted runner because the interesting ones —
//! "there is no ruleset" versus "nobody could ask" — are error paths that a
//! live `gh` will not reliably produce. The last test uses real `git` against a
//! real repository, so the filesystem and git halves are not proven only
//! against a mock that agrees with me.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use optimus_engineering::{
    instruction_files_for, resolve_profile, BranchProtection, CommandError, CommandOutcome,
    CommandRunner, DeclaredPolicy, ProcessRunner,
};

/// A runner that answers from a table and records nothing it was not asked.
#[derive(Default)]
struct ScriptedRunner {
    replies: HashMap<String, (i32, String, String)>,
}

impl ScriptedRunner {
    fn reply(mut self, command: &str, code: i32, stdout: &str, stderr: &str) -> Self {
        self.replies.insert(
            command.to_string(),
            (code, stdout.to_string(), stderr.to_string()),
        );
        self
    }
}

impl CommandRunner for ScriptedRunner {
    fn run(
        &self,
        _cwd: &Path,
        program: &str,
        args: &[String],
        _timeout: Duration,
    ) -> Result<CommandOutcome, CommandError> {
        let key = if args.is_empty() {
            program.to_string()
        } else {
            format!("{program} {}", args.join(" "))
        };
        // An unscripted command fails, the way an absent tool does.
        let (code, stdout, stderr) = self
            .replies
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (127, String::new(), format!("{program}: command not found")));
        Ok(CommandOutcome {
            program: program.to_string(),
            args: args.to_vec(),
            exit_code: Some(code),
            stdout: stdout.into_bytes(),
            stderr: stderr.into_bytes(),
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
            duration_ms: 0,
        })
    }
}

fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, body) in files {
        let full = dir.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, body).unwrap();
    }
    dir
}

fn with_default_branch(runner: ScriptedRunner, branch: &str) -> ScriptedRunner {
    runner.reply(
        "git symbolic-ref --short refs/remotes/origin/HEAD",
        0,
        &format!("origin/{branch}\n"),
        "",
    )
}

#[test]
fn a_branch_with_no_ruleset_resolves_to_unprotected_not_to_satisfied() {
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let runner = with_default_branch(ScriptedRunner::default(), "main").reply(
        "gh api repos/{owner}/{repo}/branches/main/protection",
        1,
        "",
        "gh: Branch not protected (HTTP 404)",
    );
    let profile = resolve_profile(&runner, repo.path(), &DeclaredPolicy::default()).unwrap();

    assert_eq!(profile.protection, BranchProtection::Unprotected);
    assert!(
        profile.protection.is_determined(),
        "absent is an answer; the run may proceed knowing it"
    );
    assert!(
        !profile.unresolved().contains(&"protection"),
        "a determined absence is not an unresolved field"
    );
}

#[test]
fn a_forge_that_cannot_be_asked_is_unknown_and_never_unprotected() {
    // The dangerous collapse: an expired token reading as "no protection here,
    // go ahead". Every non-404 failure has to land in Unknown.
    for stderr in [
        "gh: Bad credentials (HTTP 401)",
        "gh: Resource not accessible by integration (HTTP 403)",
        "dial tcp: lookup api.github.com: no such host",
    ] {
        let repo = repo_with(&[("AGENTS.md", "rules\n")]);
        let runner = with_default_branch(ScriptedRunner::default(), "main").reply(
            "gh api repos/{owner}/{repo}/branches/main/protection",
            1,
            "",
            stderr,
        );
        let profile = resolve_profile(&runner, repo.path(), &DeclaredPolicy::default()).unwrap();

        assert!(
            matches!(profile.protection, BranchProtection::Unknown { .. }),
            "{stderr:?} resolved to {:?}",
            profile.protection
        );
        assert!(profile.unresolved().contains(&"protection"));
        assert!(profile.protection.required_checks().is_empty());
    }
}

#[test]
fn a_missing_gh_leaves_protection_unknown_rather_than_absent() {
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let runner = with_default_branch(ScriptedRunner::default(), "main");
    let profile = resolve_profile(&runner, repo.path(), &DeclaredPolicy::default()).unwrap();
    assert!(matches!(
        profile.protection,
        BranchProtection::Unknown { .. }
    ));
}

#[test]
fn required_checks_come_from_the_forge_not_from_a_guess() {
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let body = serde_json::json!({
        "required_status_checks": { "strict": true, "contexts": ["verify"] },
        "required_pull_request_reviews": { "required_approving_review_count": 1 }
    })
    .to_string();
    let runner = with_default_branch(ScriptedRunner::default(), "main").reply(
        "gh api repos/{owner}/{repo}/branches/main/protection",
        0,
        &body,
        "",
    );
    let profile = resolve_profile(&runner, repo.path(), &DeclaredPolicy::default()).unwrap();
    assert_eq!(profile.protection.required_checks(), ["verify"]);
}

#[test]
fn an_unknown_default_branch_does_not_become_main() {
    // Guessing `main` produces a diff against the wrong history, silently.
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let profile = resolve_profile(
        &ScriptedRunner::default(),
        repo.path(),
        &DeclaredPolicy::default(),
    )
    .unwrap();
    assert_eq!(profile.default_branch, None);
    assert!(profile.unresolved().contains(&"default_branch"));
    assert!(
        matches!(profile.protection, BranchProtection::Unknown { .. }),
        "with no branch to ask about, protection cannot be known"
    );
}

#[test]
fn verification_commands_are_detected_from_the_task_runner() {
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let runner = with_default_branch(ScriptedRunner::default(), "main").reply(
        "just --summary",
        0,
        "build check gates test verify ui\n",
        "",
    );
    let profile = resolve_profile(&runner, repo.path(), &DeclaredPolicy::default()).unwrap();
    assert_eq!(
        profile.verification.focused.as_deref(),
        Some(["just".to_string(), "gates".to_string()].as_slice()),
        "focused prefers the cheapest recipe the repository actually has"
    );
    assert_eq!(
        profile.verification.full.as_deref(),
        Some(["just".to_string(), "verify".to_string()].as_slice())
    );
    assert!(!profile.unresolved().contains(&"verification"));
}

#[test]
fn a_repository_with_no_recipes_reports_no_verification_rather_than_inventing_one() {
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let runner = with_default_branch(ScriptedRunner::default(), "main");
    let profile = resolve_profile(&runner, repo.path(), &DeclaredPolicy::default()).unwrap();
    assert_eq!(profile.verification.full, None);
    assert!(!profile.verification.can_verify());
    assert!(profile.unresolved().contains(&"verification"));
}

#[test]
fn a_declared_command_wins_over_a_detected_one() {
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let runner = with_default_branch(ScriptedRunner::default(), "main").reply(
        "just --summary",
        0,
        "gates verify\n",
        "",
    );
    let declared = DeclaredPolicy {
        full_verification: Some(vec!["make".into(), "ci".into()]),
        ..DeclaredPolicy::default()
    };
    let profile = resolve_profile(&runner, repo.path(), &declared).unwrap();
    assert_eq!(
        profile.verification.full.as_deref(),
        Some(["make".to_string(), "ci".to_string()].as_slice())
    );
    assert_eq!(
        profile.verification.focused.as_deref(),
        Some(["just".to_string(), "gates".to_string()].as_slice()),
        "declaring one depth must not blank the other"
    );
}

#[test]
fn a_repository_can_add_sensitive_paths_but_not_remove_the_floor() {
    let repo = repo_with(&[("AGENTS.md", "rules\n")]);
    let declared = DeclaredPolicy {
        extra_sensitive_paths: vec!["config/production.yml".into()],
        ..DeclaredPolicy::default()
    };
    let profile = resolve_profile(&ScriptedRunner::default(), repo.path(), &declared).unwrap();

    assert!(profile.is_sensitive("config/production.yml"));
    // The floor holds regardless of what the repository declared.
    assert!(profile.is_sensitive(".github/workflows/ci.yml"));
    assert!(profile.is_sensitive("scripts/verify.sh"));
    assert!(profile.is_sensitive("crates/optimus-kernel/AGENTS.md"));
    assert!(profile.is_sensitive(".env.production"));
    // And ordinary code is still ordinary.
    assert!(!profile.is_sensitive("crates/optimus-kernel/src/lib.rs"));
    assert!(!profile.is_sensitive("README.md"));
}

#[test]
fn instruction_files_are_ordered_root_first_so_the_nearest_one_wins() {
    let repo = repo_with(&[
        ("AGENTS.md", "root rules\n"),
        ("crates/AGENTS.md", "crate rules\n"),
        ("crates/thing/CLAUDE.md", "thing rules\n"),
        ("crates/thing/src/lib.rs", "fn main() {}\n"),
    ]);
    let found = instruction_files_for(repo.path(), Path::new("crates/thing/src/lib.rs"));
    assert_eq!(
        found,
        vec![
            Path::new("AGENTS.md"),
            Path::new("crates/AGENTS.md"),
            Path::new("crates/thing/CLAUDE.md"),
        ],
        "a reader applying these in order ends on the most specific"
    );
}

#[test]
fn an_instruction_path_cannot_climb_out_of_the_repository() {
    let repo = repo_with(&[("AGENTS.md", "root rules\n")]);
    let outside = repo.path().parent().unwrap().join("OUTSIDE-AGENTS.md");
    std::fs::write(&outside, "not yours\n").ok();
    let found = instruction_files_for(repo.path(), Path::new("../../AGENTS.md"));
    assert_eq!(
        found,
        vec![Path::new("AGENTS.md")],
        "only the repository's own instructions govern work inside it"
    );
}

#[test]
fn a_real_repository_resolves_its_branch_template_and_instructions() {
    // The mock-free half: real git, real files, the real ProcessRunner.
    let repo = repo_with(&[
        ("AGENTS.md", "root rules\n"),
        (".github/pull_request_template.md", "## What\n"),
        ("src/lib.rs", "fn main() {}\n"),
    ]);
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .status()
            .expect("git must be installed");
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "--initial-branch=trunk"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "test"]);
    git(&["add", "-A"]);
    git(&["commit", "-m", "first"]);

    let profile = resolve_profile(&ProcessRunner, repo.path(), &DeclaredPolicy::default()).unwrap();

    assert_eq!(
        profile.default_branch.as_deref(),
        Some("trunk"),
        "a repository with no remote still knows what it is on"
    );
    assert_eq!(
        profile.pr_template.as_deref(),
        Some(Path::new(".github/pull_request_template.md"))
    );
    assert_eq!(profile.instruction_files, vec![Path::new("AGENTS.md")]);
    assert!(
        matches!(profile.protection, BranchProtection::Unknown { .. }),
        "a local repository has no forge to answer for it"
    );
}

#[test]
fn a_root_that_does_not_exist_is_refused_before_anything_is_queried() {
    let missing = std::env::temp_dir().join("optimus-no-such-repo-p41");
    let error = resolve_profile(&ProcessRunner, &missing, &DeclaredPolicy::default())
        .expect_err("a missing root must not resolve to an empty profile");
    assert!(error.to_string().contains("does not exist"), "{error}");
}

/// The P41 exit gate, run against the repository this crate lives in.
///
/// Ignored by default because it asks the forge about branch protection, which
/// needs a network and a token. Run it explicitly:
/// `cargo test -p optimus-engineering --test repository_profile -- --ignored`
#[test]
#[ignore = "network: queries the forge for branch protection"]
fn this_repository_resolves_with_no_reconstructed_fields() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives two levels below the repository root");
    let profile = resolve_profile(&ProcessRunner, root, &DeclaredPolicy::default()).unwrap();
    println!("default branch : {:?}", profile.default_branch);
    println!("protection     : {:?}", profile.protection);
    println!("pr template    : {:?}", profile.pr_template);
    println!("instructions   : {:?}", profile.instruction_files);
    println!("focused        : {:?}", profile.verification.focused);
    println!("full           : {:?}", profile.verification.full);
    println!("unresolved     : {:?}", profile.unresolved());

    assert_eq!(profile.default_branch.as_deref(), Some("main"));
    assert_eq!(
        profile.verification.full.as_deref(),
        Some(["just".to_string(), "verify".to_string()].as_slice())
    );
    assert_eq!(
        profile.instruction_files,
        vec![Path::new("AGENTS.md"), Path::new("CLAUDE.md")],
        "this repository keeps both, and both govern work in it"
    );
    assert!(
        profile.protection.is_determined(),
        "the forge was reachable, so protection must be a determined answer"
    );
}
