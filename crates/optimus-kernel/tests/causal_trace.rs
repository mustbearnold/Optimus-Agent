//! Phase-5 causal reconstruction from durable execution evidence (+ P14 export).

use optimus_kernel::{
    classify_security_denial, export_causal_document, load_causal_turn, parse_causal_query,
    write_causal_export, CancellationToken, CausalQuery, CausalQueryKind, CompletionRequest,
    CompletionResponse, ExecutionStatus, ExecutionStore, Kernel, KernelConfig, KernelError,
    ModelProvider, PolicyMode, ScriptedModel, SecurityDenialCode, SessionStore, StreamEvent,
    ToolCall, CAUSAL_EXPORT_VERSION,
};
use serde_json::json;
use tempfile::tempdir;

struct BlockingCancellableModel {
    started: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ModelProvider for BlockingCancellableModel {
    fn complete(
        &mut self,
        _request: CompletionRequest,
    ) -> optimus_kernel::Result<CompletionResponse> {
        panic!("cancellable completion seam was bypassed");
    }

    fn complete_streaming_cancellable(
        &mut self,
        _request: CompletionRequest,
        _sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
    ) -> optimus_kernel::Result<CompletionResponse> {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        while !cancellation.is_cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err(KernelError::Cancelled)
    }
}

#[test]
fn successful_turn_is_reconstructible_by_trace_and_manifest_id() {
    let directory = tempdir().unwrap();
    let home = directory.path();
    let mut kernel = Kernel::open(
        home,
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
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

#[test]
fn causal_export_is_versioned_store_backed_and_redacts_home() {
    let directory = tempdir().unwrap();
    let home = directory.path();
    let mut kernel = Kernel::open(
        home,
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "write_file".into(),
                arguments: json!({"path":"export.txt","contents":"p14"}),
            }],
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
        },
    ]);
    let result = kernel.turn(&mut model, "export me").unwrap();
    let query = CausalQuery {
        kind: CausalQueryKind::TraceId,
        id: result.trace_context.trace_id.to_string(),
    };
    let doc = export_causal_document(home, query.clone()).unwrap();
    assert_eq!(doc.export_version, CAUSAL_EXPORT_VERSION);
    assert_eq!(doc.format, "optimus.causal.v1");
    assert!(doc.store_backed);
    assert!(!doc.live_provider_replay);
    assert_eq!(doc.report.home, "$OPTIMUS_HOME");
    assert!(!doc.report.home.contains(home.display().to_string().as_str()));
    assert_eq!(doc.report.manifest.status, ExecutionStatus::Succeeded);

    let out = home.join("export").join("turn.json");
    write_causal_export(home, query, &out).unwrap();
    let raw = std::fs::read_to_string(&out).unwrap();
    assert!(raw.contains("optimus.causal.v1"));
    assert!(raw.contains("$OPTIMUS_HOME"));
    assert!(!raw.contains(&home.display().to_string()));
}

#[test]
fn cancelled_turn_is_reconstructible_from_execution_store_without_logs() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut model = BlockingCancellableModel {
        started: started.clone(),
    };
    let cancellation = CancellationToken::new();
    let controller_token = cancellation.clone();
    let controller = std::thread::spawn(move || {
        while !started.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::yield_now();
        }
        controller_token.cancel();
    });
    let mut events = Vec::new();
    let result = kernel.turn_with_sink_cancellable(
        &mut model,
        "cancel",
        &mut |event| events.push(event),
        &cancellation,
    );
    controller.join().unwrap();
    assert!(matches!(result, Err(KernelError::Cancelled)));

    let turns = SessionStore::open(dir.path().join("sessions.db"))
        .unwrap()
        .turns(kernel.session_id())
        .unwrap();
    assert_eq!(turns[0].status, optimus_kernel::TurnStatus::Cancelled);
    let manifest_id = ExecutionStore::open(dir.path().join("execution.db"))
        .unwrap()
        .find_by_turn(turns[0].id)
        .unwrap()
        .unwrap();
    let report = load_causal_turn(
        dir.path(),
        CausalQuery {
            kind: CausalQueryKind::ManifestId,
            id: manifest_id.to_string(),
        },
    )
    .unwrap();
    assert_eq!(report.manifest.status, ExecutionStatus::Cancelled);
    assert!(report.trace_context.is_some());
    // Store-backed only — no log files consulted.
    assert!(dir.path().join("execution.db").is_file());
}
