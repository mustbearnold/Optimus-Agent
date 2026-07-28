//! R30.5 (ADR-0044): a durable project trust grant is what makes routine
//! engineering work stop asking — and it is the *only* thing that does.
//!
//! `project_trust.rs` proves the store keeps grants correctly. This proves the
//! consequence: the same write, in the same worktree, under the same policy
//! mode, pauses without a grant and lands with one. Anything less than a
//! written file is not evidence that autonomy was actually granted.
//!
//! The scoping claim gets its own test. A grant covers runs, not chat: a
//! session opened the ordinary way on a trusted project still asks, because
//! "the user authorized this project for engineering runs" is not the same
//! statement as "the user stopped wanting to see edits".

use std::fs;
use std::path::PathBuf;

use optimus_graph::AutonomyProfile;
use optimus_kernel::{
    CompletionResponse, Kernel, KernelConfig, PolicyMode, ProjectAuthorityStore, ProjectTrustStore,
    ScriptedModel, ToolCall,
};
use serde_json::json;

const PROJECT: &str = "repo-under-test";
const PROOF: &str = "granted.txt";

struct Fixture {
    home: PathBuf,
    worktree: PathBuf,
    _home_dir: tempfile::TempDir,
    _repo_dir: tempfile::TempDir,
}

impl Fixture {
    fn trust(&self) -> ProjectTrustStore {
        ProjectTrustStore::open(&self.home).expect("trust store")
    }

    fn proof_exists(&self) -> bool {
        self.worktree.join(PROOF).exists()
    }
}

fn fixture() -> Fixture {
    let home_dir = tempfile::tempdir().unwrap();
    let repo_dir = tempfile::tempdir().unwrap();
    let home = home_dir.path().to_path_buf();

    let authority = ProjectAuthorityStore::open(&home).unwrap();
    let selection = authority.stage_native_selection(repo_dir.path()).unwrap();
    let scope = authority
        .authorize_project(
            PROJECT,
            std::slice::from_ref(&selection.path),
            Some(&selection.path),
            std::slice::from_ref(&selection.grant_token),
        )
        .unwrap()
        .expect("authorization must produce a scope");

    let worktree = scope
        .primary_root
        .join("local")
        .join("runs")
        .join("t-1")
        .join("tree");
    fs::create_dir_all(&worktree).unwrap();

    Fixture {
        home,
        worktree,
        _home_dir: home_dir,
        _repo_dir: repo_dir,
    }
}

/// Deliberately *not* `Unrestricted`: the point is what the profile decides,
/// and an unrestricted policy mode would answer before the profile was asked.
fn config() -> KernelConfig {
    KernelConfig {
        effect_policy: PolicyMode::SmartDeny,
        ..KernelConfig::default()
    }
}

fn write_proof(f: &Fixture) -> Result<(), String> {
    let mut kernel = Kernel::open_dev_run_session(&f.home, config(), None, PROJECT, &f.worktree)
        .expect("dev session");
    run_write(&mut kernel)
}

fn run_write(kernel: &mut Kernel) -> Result<(), String> {
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "w1".into(),
                name: "write_file".into(),
                arguments: json!({ "path": PROOF, "contents": "granted\n" }),
            }],
        },
        CompletionResponse {
            text: Some("written".into()),
            tool_calls: vec![],
        },
    ]);
    kernel
        .turn(&mut model, "write the proof file")
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// A refusal that is specifically "ask the user", not any old failure.
///
/// Without this the negative tests would pass on a broken fixture, a missing
/// tool, or a panic in the sandbox — every one of which also produces `Err`.
fn asked_for_approval(result: &Result<(), String>) -> bool {
    matches!(result, Err(message) if message.contains("needs approval"))
}

#[test]
fn without_a_grant_a_run_asks_before_writing() {
    let f = fixture();
    let result = write_proof(&f);
    assert!(
        asked_for_approval(&result),
        "an ungranted run wrote without asking: {result:?}"
    );
    assert!(
        !f.proof_exists(),
        "the file landed despite the turn failing — the pause is not real"
    );
}

#[test]
fn a_standard_grant_lets_routine_work_through() {
    let f = fixture();
    f.trust()
        .grant(PROJECT, AutonomyProfile::Standard, None, "engineering runs")
        .expect("grant");

    write_proof(&f).expect("a granted run must not pause on an ordinary project write");
    assert!(
        f.proof_exists(),
        "the turn succeeded but nothing was written — succeeding is not doing"
    );
}

#[test]
fn revoking_the_grant_puts_the_asking_back() {
    let f = fixture();
    let trust = f.trust();
    trust
        .grant(PROJECT, AutonomyProfile::Standard, None, "temporary")
        .expect("grant");
    assert!(trust.revoke(PROJECT).expect("revoke"), "nothing to revoke");

    let result = write_proof(&f);
    assert!(
        asked_for_approval(&result),
        "a revoked grant still allowed the write: {result:?}"
    );
    assert!(!f.proof_exists(), "a revoked grant still wrote the file");
}

#[test]
fn a_grant_for_one_project_does_not_travel_to_another() {
    let f = fixture();
    f.trust()
        .grant(
            "some-other-repo",
            AutonomyProfile::Standard,
            None,
            "unrelated",
        )
        .expect("grant");

    let result = write_proof(&f);
    assert!(
        asked_for_approval(&result),
        "another project's grant covered this run: {result:?}"
    );
}

#[test]
fn a_chat_session_on_a_trusted_project_still_asks() {
    // The scoping line R30.5 draws. A standing grant is for runs inside a
    // worktree, not for every session that happens to name the project.
    let f = fixture();
    f.trust()
        .grant(PROJECT, AutonomyProfile::Standard, None, "engineering runs")
        .expect("grant");

    let mut kernel =
        Kernel::open_project_session(&f.home, config(), None, PROJECT).expect("project session");
    let result = run_write(&mut kernel);
    assert!(
        asked_for_approval(&result),
        "a chat session inherited the run's autonomy: {result:?}"
    );
}

#[test]
fn the_config_profile_survives_when_no_grant_speaks() {
    // A caller that already chose a profile is not overridden by the absence
    // of a grant — only by the presence of one.
    let f = fixture();
    let config = KernelConfig {
        effect_policy: PolicyMode::SmartDeny,
        autonomy_profile: AutonomyProfile::Standard,
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open_dev_run_session(&f.home, config, None, PROJECT, &f.worktree)
        .expect("dev session");
    run_write(&mut kernel).expect("an explicitly Standard caller must still be Standard");
    assert!(f.proof_exists());
}
