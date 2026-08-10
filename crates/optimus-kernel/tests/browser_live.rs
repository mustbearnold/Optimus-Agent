//! Integration: real browser navigate against example.com (network).

use optimus_kernel::{CompletionResponse, Kernel, KernelConfig, ScriptedModel, ToolCall};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn browser_navigate_example_com() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "1".into(),
                name: "activate_pack".into(),
                arguments: json!({"name": "browser"}),
            }],
            reasoning_content: None,
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "2".into(),
                name: "browser_navigate".into(),
                arguments: json!({"url": "https://example.com/"}),
            }],
            reasoning_content: None,
        },
        CompletionResponse {
            text: Some("saw example".into()),
            tool_calls: vec![],
            reasoning_content: None,
        },
    ]);
    // Chunk streaming not needed
    model.stream_chunks = false;
    let r = k.turn(&mut model, "open example.com").expect("turn");
    let joined = r.tool_trace.join("\n");
    assert!(
        joined.contains("browser_navigate")
            && (joined.contains("Example") || joined.contains("example") || joined.contains("200")),
        "trace={joined}"
    );
    // Parity (optimus-agent #84): the navigate trace must headline the page's
    // readable text — `body_text=` on the CDP effector (it is only included
    // when non-empty), `text=` on the HTTP fallback. Text-only models browse
    // blind without it.
    assert!(
        joined.contains("body_text=") || joined.contains("text=Example Domain"),
        "trace must headline page body text: {joined}"
    );
    assert_eq!(r.assistant_text, "saw example");
}
