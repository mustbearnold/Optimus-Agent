use std::process::Command;

use tempfile::tempdir;

fn run_chat(extra_args: &[&str]) -> std::process::Output {
    let home = tempdir().expect("temporary Optimus home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_optimus"));
    command
        .args(["--home", home.path().to_str().unwrap(), "chat", "hello"])
        .args(extra_args)
        // Client mode (spec-015 B2 default) spawns `optimus serve --stdio`;
        // port 0 binds an ephemeral port so the parallel tests here never
        // collide on the production default.
        .env("OPTIMUS_SERVE_PORT", "0")
        .env_remove("OPTIMUS_API_KEY")
        .env_remove("OPTIMUS_OPENAI_BASE_URL")
        .env_remove("OPTIMUS_OPENAI_API_KEY")
        .env_remove("OPENAI_API_KEY");
    command.output().expect("run optimus chat")
}

#[test]
fn chat_defaults_to_auto_and_fresh_auto_resolves_offline() {
    let output = run_chat(&[]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("offline echo: hello"), "{stdout}");
    assert!(stdout.contains("[provider=offline "), "{stdout}");
}

#[test]
fn model_auto_is_not_sent_as_a_literal_offline_model() {
    let output = run_chat(&["--provider", "offline", "--model", "Auto"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("offline echo: hello"));
}

#[test]
fn explicit_model_intent_reaches_canonical_routing_unchanged() {
    let output = run_chat(&["--provider", "codex", "--model", "sol"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("model_not_owned_by_provider"), "{stderr}");
}
