//! A failing tool's real cause reaches the model, not a generic placeholder.
//!
//! Regression coverage for issue #83: every dispatch error that was not a
//! pack error, a control-plane denial, or a pending approval used to collapse
//! to `ToolErrorDetail { code: "tool_execution_failed", message: "tool
//! execution failed" }` — an SSRF refusal, a dead Chrome, and a bad click
//! index all looked identical to the model, so it could not self-correct.
//! These turns must still complete: a failed tool outcome is evidence fed
//! back to the model, not a turn abort.

use optimus_kernel::{
    CompletionResponse, Kernel, KernelConfig, PolicyMode, ScriptedModel, ToolCall,
};
use serde_json::{json, Value};
use tempfile::tempdir;

fn open_kernel(home: &std::path::Path) -> Kernel {
    Kernel::open(
        home,
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
        },
    )
    .expect("kernel must open on a fresh home")
}

fn tool_step(id: &str, name: &str, arguments: Value) -> CompletionResponse {
    CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: id.into(),
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

fn scripted(script: Vec<CompletionResponse>) -> ScriptedModel {
    let mut model = ScriptedModel::new(script);
    model.stream_chunks = false;
    model
}

/// All tool-result content the model was shown, concatenated in order.
fn evidence_shown_to_model(model: &ScriptedModel) -> String {
    model
        .seen
        .iter()
        .flat_map(|request| request.messages.iter())
        .filter(|message| message.tool_call_id.is_some())
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_loopback_navigate_shows_the_model_the_real_ssrf_cause_not_a_placeholder() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step("t1", "activate_pack", json!({"name": "browser"})),
        // Port 9 is the discard service; the SSRF guard must refuse before
        // any connection is attempted, so nothing needs to listen there —
        // deterministic, no network required.
        tool_step(
            "t2",
            "browser_navigate",
            json!({"url": "http://127.0.0.1:9/"}),
        ),
        done("navigate refusal observed"),
    ]);

    let result = kernel
        .turn(&mut model, "navigate to a loopback address")
        .expect("a failed tool outcome is evidence, not a turn abort");
    assert_eq!(
        result.assistant_text, "navigate refusal observed",
        "the turn must run to completion past the failed tool call"
    );

    // Which effector runs is environment-dependent (CDP when Chrome is
    // available, plain HTTP otherwise) and each phrases the refusal
    // differently — the assertion accepts either real cause, never the
    // placeholder.
    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("ssrf")
            || evidence.contains("127.0.0.1")
            || evidence.contains("non-public address")
            || evidence.contains("unsafe browser URL"),
        "the model must see the real SSRF cause, got: {evidence}"
    );
    assert!(
        !evidence.contains("tool execution failed"),
        "the generic catch-all message must not survive the fix, got: {evidence}"
    );
    assert!(
        !evidence.contains("\"browser_navigate failed\""),
        "the old content-free summary must not survive the fix, got: {evidence}"
    );
}

#[test]
fn a_missing_file_read_shows_the_model_the_real_not_found_cause() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step("t1", "read_file", json!({"path": "does-not-exist.txt"})),
        done("read failure observed"),
    ]);

    let result = kernel
        .turn(&mut model, "read a file that was never created")
        .expect("a failed tool outcome is evidence, not a turn abort");
    assert_eq!(result.assistant_text, "read failure observed");

    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("not found") && evidence.contains("does-not-exist.txt"),
        "the model must see the real missing-path cause, got: {evidence}"
    );
    assert!(
        !evidence.contains("tool execution failed"),
        "the generic catch-all message must not survive the fix, got: {evidence}"
    );
}
