//! Live web_search tool (network).

use optimus_kernel::{
    web_search_json, CompletionResponse, Kernel, KernelConfig, ScriptedModel, ToolCall,
};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn web_search_json_returns_results_or_wiki_fallback() {
    let raw = web_search_json("OpenAI GPT-5.4", 3).expect("search");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["ok"], true);
    assert!(v["count"].as_u64().unwrap_or(0) >= 1, "{raw}");
}

#[test]
fn kernel_web_search_tool_via_scripted_turn() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "s1".into(),
                name: "web_search".into(),
                arguments: json!({"query": "Rust programming language", "limit": 2}),
            }],
        },
        CompletionResponse {
            text: Some("search complete".into()),
            tool_calls: vec![],
        },
    ]);
    model.stream_chunks = false;
    let r = k.turn(&mut model, "search rust").unwrap();
    assert!(r.tool_trace.iter().any(|t| t.contains("web_search")));
    assert_eq!(r.assistant_text, "search complete");
}
