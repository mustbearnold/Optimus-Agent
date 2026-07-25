//! Phase-5 causal reconstruction from durable execution evidence.

use optimus_kernel::{
    classify_security_denial, load_causal_turn, parse_causal_query, CausalQuery, CausalQueryKind,
    CompletionResponse, ExecutionStatus, Kernel, KernelConfig, KernelError, PolicyMode,
    ScriptedModel, SecurityDenialCode, ToolCall,
};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn successful_turn_is_reconstructible_by_trace_and_manifest_id() {
    let directory = tempdir().unwrap();
    let home = directory.path();
    let mut kernel = Kernel::open(
        home,
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write_file".into(),
                arguments: json!({"path":"note.txt","contents":"causal"}),
            }],
        },
        CompletionResponse {
            text: Some("wrote note".into()),
            tool_calls: vec![],
        },
    ]);
    let result = kernel.turn(&mut model, "write a note").unwrap();
    assert!(!result.assistant_text.is_empty());
    assert_eq!(
        result.trace_context.trace_id.to_string().len(),
        36,
        "turn always binds a root trace id"
    );

    let by_trace = load_causal_turn(
        home,
        CausalQuery {
            kind: CausalQueryKind::TraceId,
            id: result.trace_context.trace_id.to_string(),
        },
    )
    .unwrap();
    assert_eq!(
        by_trace.trace_context.as_ref().unwrap().trace_id,
        result.trace_context.trace_id
    );
    assert_eq!(by_trace.manifest.status, ExecutionStatus::Succeeded);
    assert!(!by_trace.model_calls.is_empty());
    assert!(by_trace
        .tool_calls
        .iter()
        .any(|call| call.tool_id == "write_file"));
    assert!(by_trace.effect_transcript_consistent);

    let by_manifest = load_causal_turn(
        home,
        CausalQuery {
            kind: CausalQueryKind::ManifestId,
            id: by_trace.manifest.id.to_string(),
        },
    )
    .unwrap();
    assert_eq!(by_manifest.manifest.id, by_trace.manifest.id);

    let by_turn = load_causal_turn(
        home,
        CausalQuery {
            kind: CausalQueryKind::TurnId,
            id: by_trace.manifest.turn_id.to_string(),
        },
    )
    .unwrap();
    assert_eq!(by_turn.manifest.turn_id, by_trace.manifest.turn_id);

    let parsed = parse_causal_query(&result.trace_context.trace_id.to_string()).unwrap();
    assert_eq!(parsed.kind, CausalQueryKind::TraceId);
}

#[test]
fn security_denial_codes_are_stable_for_known_fences() {
    assert_eq!(
        classify_security_denial(&KernelError::Tool("path not allowed under root".into())),
        Some(SecurityDenialCode::FsSandboxDeny)
    );
    assert_eq!(
        classify_security_denial(&KernelError::Tool("secret path denied: .env".into())),
        Some(SecurityDenialCode::SecretBasenameDeny)
    );
    assert_eq!(
        classify_security_denial(&KernelError::Tool(
            "write requires SmartDeny approval".into()
        )),
        Some(SecurityDenialCode::ApprovalRequired)
    );
    assert_eq!(
        classify_security_denial(&KernelError::Browser(
            optimus_kernel::BrowserError::Ssrf("loopback".into())
        )),
        Some(SecurityDenialCode::NetworkSsrfDeny)
    );
}
