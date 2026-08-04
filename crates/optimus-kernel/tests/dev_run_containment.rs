//! A session bound to a checkout reaches that checkout and nothing else.
//!
//! Originally the kernel half of ADR-0052 §2's containment pair, where a
//! separate crate proved the worktree was created correctly. That crate was
//! archived unintegrated (ADR-0073), and this half was retained on its own
//! merit: binding a session to a directory must actually narrow what the tools
//! can touch, including the main checkout the human is using. The property is
//! the kernel's job, not git's, and it holds for any caller that scopes a
//! session — no development-run machinery required.

use std::fs;
use std::path::{Path, PathBuf};

use optimus_kernel::{
    CompletionResponse, Kernel, KernelConfig, PolicyMode, ProjectAuthorityStore, ScriptedModel,
    ToolCall,
};
use serde_json::{json, Value};

const PROJECT: &str = "repo-under-test";

/// A project the user authorized, standing in for the main checkout, with a
/// run's worktree already present inside it.
struct Fixture {
    home: PathBuf,
    repo_root: PathBuf,
    worktree: PathBuf,
    _home_dir: tempfile::TempDir,
    _repo_dir: tempfile::TempDir,
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
    let repo_root = scope.primary_root.clone();

    // What the human is working on, in the main checkout.
    fs::write(repo_root.join("main-only.txt"), "the human's work\n").unwrap();

    // Where the run works, laid out the way WorktreeManager lays it out.
    let worktree = repo_root
        .join("local")
        .join("runs")
        .join("t-1")
        .join("tree");
    fs::create_dir_all(&worktree).unwrap();
    fs::write(worktree.join("run-only.txt"), "the run's work\n").unwrap();

    Fixture {
        home,
        repo_root,
        worktree,
        _home_dir: home_dir,
        _repo_dir: repo_dir,
    }
}

fn config() -> KernelConfig {
    KernelConfig {
        effect_policy: PolicyMode::Unrestricted,
        ..KernelConfig::default()
    }
}

fn tool_step(name: &str, arguments: Value) -> CompletionResponse {
    CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "t1".into(),
            name: name.into(),
            arguments,
        }],
        reasoning_content: None,
    }
}

fn done(text: &str) -> CompletionResponse {
    CompletionResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        reasoning_content: None,
    }
}

fn evidence(model: &ScriptedModel) -> String {
    model
        .seen
        .iter()
        .flat_map(|request| request.messages.iter())
        .filter(|message| message.tool_call_id.is_some())
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The refusal message from a session that must not open. `Kernel` is not
/// `Debug`, so `expect_err` is unavailable.
fn refusal(opened: Result<Kernel, optimus_kernel::KernelError>) -> String {
    match opened {
        Ok(_) => panic!("the session opened when it should have been refused"),
        Err(error) => error.to_string(),
    }
}

/// Read `path` from inside a session bound to `worktree`.
///
/// `Ok` carries what the model was shown; `Err` carries the refusal. A read
/// outside the allowlist is refused at the sandbox, which fails the turn — so
/// the two outcomes are genuinely different, not "empty result versus full".
fn read_in_dev_run(home: &Path, worktree: &Path, path: &str) -> Result<String, String> {
    let mut kernel =
        Kernel::open_dev_run_session(home, config(), None, PROJECT, worktree).expect("dev session");
    let mut model = ScriptedModel::new(vec![
        tool_step("read_file", json!({ "path": path })),
        done("read attempted"),
    ]);
    match kernel.turn(&mut model, "read a file") {
        Ok(_) => Ok(evidence(&model)),
        Err(error) => Err(error.to_string()),
    }
}

#[test]
fn a_run_reads_its_own_worktree() {
    let f = fixture();
    let seen = read_in_dev_run(&f.home, &f.worktree, "run-only.txt")
        .expect("the run's own checkout is readable");
    assert!(
        seen.contains("the run's work"),
        "the run must reach its own checkout, got: {seen}"
    );
}

#[test]
fn a_run_cannot_read_the_main_checkout() {
    let f = fixture();
    let absolute = f.repo_root.join("main-only.txt");
    match read_in_dev_run(&f.home, &f.worktree, &absolute.display().to_string()) {
        Ok(seen) => panic!("the run reached the main checkout it must not see: {seen}"),
        Err(error) => assert!(
            error.contains("path not allowed"),
            "expected a containment refusal, got: {error}"
        ),
    }
}

#[test]
fn a_run_cannot_climb_out_with_a_relative_path() {
    let f = fixture();
    match read_in_dev_run(&f.home, &f.worktree, "../../../../main-only.txt") {
        Ok(seen) => panic!("a relative escape reached the main checkout: {seen}"),
        Err(error) => assert!(
            error.contains("path not allowed"),
            "expected a containment refusal, got: {error}"
        ),
    }
}

#[test]
fn the_same_project_without_a_run_still_sees_the_whole_checkout() {
    // Control: the narrowing is the dev-run binding, not a broken fixture.
    let f = fixture();
    let mut kernel =
        Kernel::open_project_session(&f.home, config(), None, PROJECT).expect("project session");
    let mut model = ScriptedModel::new(vec![
        tool_step("read_file", json!({ "path": "main-only.txt" })),
        done("read attempted"),
    ]);
    kernel.turn(&mut model, "read a file").unwrap();
    assert!(
        evidence(&model).contains("the human's work"),
        "an ordinary project session must still reach the project root"
    );
}

#[test]
fn a_worktree_outside_the_authorized_root_is_refused() {
    let f = fixture();
    let elsewhere = tempfile::tempdir().unwrap();
    let error = refusal(Kernel::open_dev_run_session(
        &f.home,
        config(),
        None,
        PROJECT,
        elsewhere.path(),
    ));
    assert!(
        error.contains("not inside an authorized root"),
        "unexpected refusal: {error}"
    );
}

#[test]
fn the_project_root_itself_is_not_a_valid_worktree() {
    let f = fixture();
    let error = refusal(Kernel::open_dev_run_session(
        &f.home,
        config(),
        None,
        PROJECT,
        &f.repo_root,
    ));
    assert!(
        error.contains("not inside an authorized root"),
        "unexpected refusal: {error}"
    );
}
