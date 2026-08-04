//! Kernel-level compression integration.

use optimus_kernel::{
    estimate_chars, CompletionResponse, CompressionConfig, Kernel, KernelConfig, ScriptedModel,
    COMPRESSED_MARKER,
};
use tempfile::tempdir;

#[test]
fn turn_compresses_bloated_history() {
    let dir = tempdir().unwrap();
    let cfg = KernelConfig {
        compression: CompressionConfig {
            enabled: true,
            max_message_chars: 800,
            keep_tail_messages: 2,
            snippet_chars: 40,
            max_tool_result_chars: 20_000,
        },
        ..KernelConfig::default()
    };
    let mut k = Kernel::open(dir.path(), cfg).unwrap();

    // Inflate history with prior turns via direct message push (simulates long session).
    for i in 0..30 {
        k.messages.push(optimus_kernel::Message {
            role: optimus_kernel::Role::User,
            content: format!("padding user {i} {}", "p".repeat(80)),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        });
        k.messages.push(optimus_kernel::Message {
            role: optimus_kernel::Role::Assistant,
            content: format!("padding assistant {i} {}", "q".repeat(80)),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        });
    }
    let before = estimate_chars(&k.messages);
    assert!(before > 800);

    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: Some("ok after compress".into()),
        tool_calls: vec![],
        reasoning_content: None,
    }]);
    let result = k.turn(&mut model, "final question").unwrap();
    assert!(result.compressed);
    assert_eq!(result.assistant_text, "ok after compress");
    assert!(estimate_chars(&k.messages) < before);
    assert!(k
        .messages
        .iter()
        .any(|m| m.content.contains(COMPRESSED_MARKER)));
    // System preserved
    assert_eq!(k.messages[0].role, optimus_kernel::Role::System);
    // Model saw compressed transcript
    assert!(model.seen[0]
        .messages
        .iter()
        .any(|m| m.content.contains(COMPRESSED_MARKER)));
}
