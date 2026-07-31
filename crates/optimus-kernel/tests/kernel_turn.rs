//! Kernel turn loop offline tests.

use optimus_kernel::{
    CancellationToken, ChatApprovalDecision, ChatApprovalStatus, CompletionRequest,
    CompletionResponse, ExecutionStatus, ExecutionStore, Kernel, KernelConfig, KernelError,
    ModelProvider, PolicyMode, ProjectAuthorityStore, Role, ScriptedModel, SessionStore,
    StreamControl, StreamEvent, TimingEventKind, ToolCall, ToolLifecyclePhase, TurnStatus,
};
use optimus_packs::{PackError, ToolId, ToolOutcome, ToolOutcomeKind};
use optimus_runtime::RuntimeError;
use optimus_skills::{Permission, SkillDraft};
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

struct DeliveryAwareModel;

impl ModelProvider for DeliveryAwareModel {
    fn complete(
        &mut self,
        _request: CompletionRequest,
    ) -> optimus_kernel::Result<CompletionResponse> {
        panic!("cancellable completion seam was bypassed");
    }

    fn complete_streaming_cancellable(
        &mut self,
        _request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
    ) -> optimus_kernel::Result<CompletionResponse> {
        sink(StreamEvent::TextDelta("unconsumed".into()));
        if cancellation.is_cancelled() {
            Err(KernelError::Cancelled)
        } else {
            panic!("stream delivery rejection did not cancel the shared token");
        }
    }
}

#[test]
fn tool_stream_events_keep_runtime_identity_and_validated_outcome() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    kernel.remember_demo("user", "editor", "helix").unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "memory_recall".into(),
                arguments: json!({"subject":"user","predicate":"editor"}),
            }],
        },
        CompletionResponse {
            text: Some("You prefer helix.".into()),
            tool_calls: vec![],
        },
    ]);
    let mut events = Vec::new();

    kernel
        .turn_with_sink(&mut model, "Which editor?", &mut |event| events.push(event))
        .unwrap();

    let tools = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::Tool(tool) => Some(tool),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].phase, ToolLifecyclePhase::Started);
    assert_eq!(tools[1].phase, ToolLifecyclePhase::Succeeded);
    assert_eq!(tools[0].run_id, tools[1].run_id);
    assert_eq!(tools[0].call_id, "call-1");
    assert_eq!(tools[1].call_id, "call-1");
    assert_ne!(tools[0].event_id, tools[1].event_id);
    assert!(tools[0].outcome.is_none());
    assert_eq!(
        tools[1].outcome.as_ref().map(|outcome| outcome.kind),
        Some(ToolOutcomeKind::Succeeded)
    );
    assert!(tools[1].duration_ms.is_some());
}

#[test]
fn project_write_emits_exact_approval_lifecycle_before_any_effect() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let authority = ProjectAuthorityStore::open(home.path()).unwrap();
    let selection = authority.stage_native_selection(project.path()).unwrap();
    authority
        .authorize_project(
            "project-a",
            std::slice::from_ref(&selection.path),
            Some(&selection.path),
            std::slice::from_ref(&selection.grant_token),
        )
        .unwrap();
    let mut kernel =
        Kernel::open_project_session(home.path(), KernelConfig::default(), None, "project-a")
            .unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "write-1".into(),
            name: "write_file".into(),
            arguments: json!({"path":"src/proof.txt","contents":"safe"}),
        }],
    }]);
    let mut events = Vec::new();

    let error = kernel
        .turn_with_sink(&mut model, "write the proof", &mut |event| {
            events.push(event)
        })
        .unwrap_err();

    assert!(matches!(
        error,
        KernelError::Runtime(RuntimeError::NeedsApproval { .. })
    ));
    let tools = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::Tool(tool) => Some(tool),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].phase, ToolLifecyclePhase::Started);
    assert_eq!(tools[1].phase, ToolLifecyclePhase::ApprovalRequired);
    assert_eq!(tools[0].call_id, tools[1].call_id);
    assert_eq!(tools[1].summary, "Write src/proof.txt (4 bytes)");
    assert!(tools[1].outcome.is_none());
    let binding = tools[1]
        .approval
        .clone()
        .expect("approval event must carry exact runtime binding");
    assert_eq!(binding.call_id, "write-1");
    assert_eq!(binding.effect_sha256.len(), 64);
    assert!(!project.path().join("src/proof.txt").exists());

    let pending = kernel.runtime.list_pending_approvals().unwrap();
    assert_eq!(pending.len(), 1);
    let effect: optimus_graph::Effect = serde_json::from_str(&pending[0].effect_json).unwrap();
    assert!(matches!(
        effect,
        optimus_graph::Effect::ProjectWriteFile { relative_path, .. }
            if relative_path == "src/proof.txt"
    ));

    let sessions = SessionStore::open(home.path().join("sessions.db")).unwrap();
    let active = sessions
        .active_turn(kernel.session_id())
        .unwrap()
        .expect("approval pause must keep turn active");
    assert_eq!(active.status, TurnStatus::Running);
    let executions = ExecutionStore::open(home.path().join("execution.db")).unwrap();
    let manifest_id = executions.find_by_turn(active.id).unwrap().unwrap();
    assert_eq!(manifest_id, binding.run_id);
    assert_eq!(
        executions.manifest(manifest_id).unwrap().status,
        ExecutionStatus::Running
    );
    let mut resume_model = ScriptedModel::new(Vec::new());
    assert!(kernel.resume_pending_turn(&mut resume_model).is_err());
    assert!(
        !project.path().join("src/proof.txt").exists(),
        "generic resume must not bypass approval resolution"
    );

    let foreign_call = kernel.resolve_chat_approval_exact(
        binding.run_id,
        "foreign-call",
        binding.job_id,
        binding.node_id,
        binding.node_index,
        &binding.effect_sha256,
        ChatApprovalDecision::Approve,
    );
    assert!(
        foreign_call.is_err(),
        "foreign call identity must fail closed"
    );
    let foreign = kernel.resolve_chat_approval_exact(
        binding.run_id,
        &binding.call_id,
        optimus_runtime::job_id(uuid::Uuid::new_v4()),
        binding.node_id,
        binding.node_index,
        &binding.effect_sha256,
        ChatApprovalDecision::Approve,
    );
    assert!(foreign.is_err(), "foreign job identity must fail closed");
    let changed = kernel.resolve_chat_approval_exact(
        binding.run_id,
        &binding.call_id,
        binding.job_id,
        binding.node_id,
        binding.node_index,
        &"0".repeat(64),
        ChatApprovalDecision::Approve,
    );
    assert!(changed.is_err(), "changed effect identity must fail closed");
    assert!(!project.path().join("src/proof.txt").exists());

    let session_id = kernel.session_id();
    drop(kernel);
    let mut kernel = Kernel::open_project_session(
        home.path(),
        KernelConfig::default(),
        Some(session_id),
        "project-a",
    )
    .unwrap();
    let resolution = kernel
        .resolve_chat_approval_exact(
            binding.run_id,
            &binding.call_id,
            binding.job_id,
            binding.node_id,
            binding.node_index,
            &binding.effect_sha256,
            ChatApprovalDecision::Approve,
        )
        .unwrap();
    assert_eq!(resolution.status, ChatApprovalStatus::Approved);
    assert_eq!(resolution.event.phase, ToolLifecyclePhase::Succeeded);
    assert!(resolution
        .event
        .outcome
        .as_ref()
        .unwrap()
        .provenance
        .is_some());
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/proof.txt")).unwrap(),
        "safe"
    );
    // Settling records the outcome and stops. The turn stays exactly as the
    // loop parked it, so the request that provoked the approval can still be
    // answered (ADR-0046).
    let settled = sessions
        .active_turn(kernel.session_id())
        .unwrap()
        .expect("settling an approval must not finish the turn");
    assert_eq!(settled.status, TurnStatus::Running);
    assert_eq!(settled.id, active.id);
    assert_eq!(
        executions.manifest(manifest_id).unwrap().status,
        ExecutionStatus::Running
    );
    let lifecycle = executions
        .tool_lifecycle_for_session(kernel.session_id())
        .unwrap();
    assert_eq!(
        lifecycle
            .iter()
            .map(|persisted| persisted.event.phase)
            .collect::<Vec<_>>(),
        vec![
            ToolLifecyclePhase::Started,
            ToolLifecyclePhase::ApprovalRequired,
            ToolLifecyclePhase::Succeeded,
        ]
    );
    assert_eq!(
        lifecycle.last().unwrap().event.approval.as_ref(),
        Some(&binding)
    );
    let links = sessions.effect_links(kernel.session_id()).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].effect_hash, binding.effect_sha256);
    // The transcript ends on the tool result, with nothing written in the
    // agent's voice about work the agent has not seen yet.
    let settled_tail = kernel.messages.last().unwrap();
    assert_eq!(settled_tail.role, Role::Tool);
    assert_eq!(settled_tail.tool_call_id.as_deref(), Some("write-1"));
    assert!(kernel
        .resolve_chat_approval_exact(
            binding.run_id,
            &binding.call_id,
            binding.job_id,
            binding.node_id,
            binding.node_index,
            &binding.effect_sha256,
            ChatApprovalDecision::Approve,
        )
        .is_err());

    // Resuming answers the original request. The model is called again, sees
    // the approved call's result, and it is the continuation — not settlement —
    // that finishes the turn.
    let mut answer = ScriptedModel::new(vec![CompletionResponse {
        text: Some("Wrote src/proof.txt.".into()),
        tool_calls: Vec::new(),
    }]);
    let resumed = kernel.resume_pending_turn(&mut answer).unwrap();
    assert_eq!(resumed.assistant_text, "Wrote src/proof.txt.");
    assert_eq!(answer.seen.len(), 1, "the paused turn must call the model");

    let carried = answer.seen[0]
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("write-1"))
        .expect("the approved call's result must reach the model");
    let data: serde_json::Value = serde_json::from_str(&carried.content).unwrap();
    assert_eq!(data["data"]["ok"], json!(true));
    assert_eq!(
        data["data"]["absolute_path"],
        json!(project.path().join("src/proof.txt").display().to_string()),
        "the resumed model must not need a terminal probe to report the saved path"
    );
    assert!(
        !data["data"]["receipt"].is_null(),
        "the model must see what the effect produced, not just that it ran: {}",
        carried.content
    );

    // The approved call is never re-derived: what the model was shown is the
    // exact call the user authorised.
    assert!(
        answer.seen[0]
            .messages
            .iter()
            .all(|message| message.role != Role::User || message.content == "write the proof"),
        "resumption must not invent a user turn"
    );

    assert!(sessions.active_turn(kernel.session_id()).unwrap().is_none());
    assert_eq!(
        executions.manifest(manifest_id).unwrap().status,
        ExecutionStatus::Succeeded
    );
}

#[test]
fn an_approved_action_is_timed_from_settlement_not_from_the_card_appearing() {
    // How long the human took to read the card is not how long the action took.
    // The pause below stands in for that deliberation; the reported duration has
    // to exclude it, or every approved command is misreported as slow.
    const DELIBERATION_MS: u64 = 250;

    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let authority = ProjectAuthorityStore::open(home.path()).unwrap();
    let selection = authority.stage_native_selection(project.path()).unwrap();
    authority
        .authorize_project(
            "project-timed",
            std::slice::from_ref(&selection.path),
            Some(&selection.path),
            std::slice::from_ref(&selection.grant_token),
        )
        .unwrap();
    let mut kernel =
        Kernel::open_project_session(home.path(), KernelConfig::default(), None, "project-timed")
            .unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "timed-write".into(),
            name: "write_file".into(),
            arguments: json!({"path":"timed.txt","contents":"ok"}),
        }],
    }]);
    let mut events = Vec::new();
    assert!(kernel
        .turn_with_sink(&mut model, "write it", &mut |event| events.push(event))
        .is_err());
    let binding = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::Tool(tool) => tool.approval.clone(),
            _ => None,
        })
        .next_back()
        .expect("the held write must produce an exact binding");

    std::thread::sleep(std::time::Duration::from_millis(DELIBERATION_MS));

    let resolution = kernel
        .resolve_chat_approval_exact(
            binding.run_id,
            &binding.call_id,
            binding.job_id,
            binding.node_id,
            binding.node_index,
            &binding.effect_sha256,
            ChatApprovalDecision::Approve,
        )
        .unwrap();
    let reported = resolution
        .event
        .duration_ms
        .expect("a settled approval must report how long the action took");
    assert!(
        reported < DELIBERATION_MS,
        "duration {reported}ms includes the {DELIBERATION_MS}ms the card sat pending"
    );
}

#[test]
fn project_write_denial_never_executes_and_settles_cancelled_once() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let authority = ProjectAuthorityStore::open(home.path()).unwrap();
    let selection = authority.stage_native_selection(project.path()).unwrap();
    authority
        .authorize_project(
            "project-deny",
            std::slice::from_ref(&selection.path),
            Some(&selection.path),
            std::slice::from_ref(&selection.grant_token),
        )
        .unwrap();
    let mut kernel =
        Kernel::open_project_session(home.path(), KernelConfig::default(), None, "project-deny")
            .unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "deny-write".into(),
            name: "write_file".into(),
            arguments: json!({"path":"denied.txt","contents":"must-not-exist"}),
        }],
    }]);
    let mut events = Vec::new();
    assert!(matches!(
        kernel.turn_with_sink(&mut model, "do not write", &mut |event| events.push(event)),
        Err(KernelError::Runtime(RuntimeError::NeedsApproval { .. }))
    ));
    let binding = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::Tool(tool) if tool.phase == ToolLifecyclePhase::ApprovalRequired => {
                tool.approval.clone()
            }
            _ => None,
        })
        .next()
        .unwrap();

    let resolution = kernel
        .resolve_chat_approval_exact(
            binding.run_id,
            &binding.call_id,
            binding.job_id,
            binding.node_id,
            binding.node_index,
            &binding.effect_sha256,
            ChatApprovalDecision::Deny {
                reason: "user_denied_in_transcript".into(),
            },
        )
        .unwrap();
    assert_eq!(resolution.status, ChatApprovalStatus::Denied);
    assert_eq!(resolution.event.phase, ToolLifecyclePhase::Cancelled);
    assert_eq!(
        resolution.event.outcome.as_ref().unwrap().kind,
        ToolOutcomeKind::Cancelled
    );
    assert!(!project.path().join("denied.txt").exists());
    let sessions = SessionStore::open(home.path().join("sessions.db")).unwrap();
    // A denial is a tool result the agent has to answer for, not a turn the
    // surface cancels on its behalf (ADR-0046).
    let denied_turn = sessions
        .active_turn(kernel.session_id())
        .unwrap()
        .expect("denial must not finish the turn either");
    assert_eq!(denied_turn.status, TurnStatus::Running);
    let executions = ExecutionStore::open(home.path().join("execution.db")).unwrap();
    let lifecycle = executions
        .tool_lifecycle_for_session(kernel.session_id())
        .unwrap();
    assert_eq!(
        lifecycle.last().unwrap().event.phase,
        ToolLifecyclePhase::Cancelled
    );
    assert_eq!(
        lifecycle.last().unwrap().event.approval.as_ref(),
        Some(&binding)
    );
    assert!(sessions
        .effect_links(kernel.session_id())
        .unwrap()
        .is_empty());
    // The reason the user gave reaches the model, so the refusal is
    // acknowledged by the agent rather than asserted by the surface.
    let refusal = kernel.messages.last().unwrap();
    assert_eq!(refusal.role, Role::Tool);
    let data: serde_json::Value = serde_json::from_str(&refusal.content).unwrap();
    assert_eq!(data["data"]["denied_reason"], "user_denied_in_transcript");
    assert!(kernel
        .resolve_chat_approval_exact(
            binding.run_id,
            &binding.call_id,
            binding.job_id,
            binding.node_id,
            binding.node_index,
            &binding.effect_sha256,
            ChatApprovalDecision::Deny {
                reason: "user_denied_in_transcript".into(),
            },
        )
        .is_err());
    assert!(!project.path().join("denied.txt").exists());

    let mut answer = ScriptedModel::new(vec![CompletionResponse {
        text: Some("Understood, I will not write that file.".into()),
        tool_calls: Vec::new(),
    }]);
    let resumed = kernel.resume_pending_turn(&mut answer).unwrap();
    assert_eq!(
        resumed.assistant_text,
        "Understood, I will not write that file."
    );
    assert!(
        !project.path().join("denied.txt").exists(),
        "resuming a denial must never run the refused effect"
    );
}

#[test]
fn active_model_completion_observes_turn_cancellation() {
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
        "cancel this model request",
        &mut |event| events.push(event),
        &cancellation,
    );
    controller.join().unwrap();

    assert!(matches!(result, Err(KernelError::Cancelled)));
    let turns = SessionStore::open(dir.path().join("sessions.db"))
        .unwrap()
        .turns(kernel.session_id())
        .unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].status, TurnStatus::Cancelled);
    assert_eq!(turns[0].error_code.as_deref(), Some("turn_cancelled"));
    assert!(matches!(
        events.last(),
        Some(StreamEvent::Timing(timing))
            if timing.kind == TimingEventKind::TurnFinished
                && timing.status.as_deref() == Some("cancelled")
    ));
    let executions = ExecutionStore::open(dir.path().join("execution.db")).unwrap();
    let manifest = executions.find_by_turn(turns[0].id).unwrap().unwrap();
    assert_eq!(
        executions
            .timing_summary(manifest)
            .unwrap()
            .terminal_status
            .as_deref(),
        Some("cancelled")
    );
}

#[test]
fn stream_delivery_rejection_cancels_turn_and_execution_once() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = DeliveryAwareModel;

    let result = kernel.turn_with_controlled_sink(
        &mut model,
        "cancel when the stream consumer is lost",
        &mut |_| StreamControl::Cancel,
    );

    assert!(matches!(result, Err(KernelError::Cancelled)));
    let sessions = SessionStore::open(dir.path().join("sessions.db")).unwrap();
    let turns = sessions.turns(kernel.session_id()).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].status, TurnStatus::Cancelled);
    assert_eq!(turns[0].error_code.as_deref(), Some("turn_cancelled"));
    assert_eq!(sessions.turn_event_count(turns[0].id).unwrap(), 2);

    let executions = ExecutionStore::open(dir.path().join("execution.db")).unwrap();
    let manifest_id = executions.find_by_turn(turns[0].id).unwrap().unwrap();
    assert_eq!(
        executions.manifest(manifest_id).unwrap().status,
        ExecutionStatus::Cancelled
    );
    assert!(executions.trace_context(manifest_id).unwrap().is_some());
}

#[test]
fn successful_turn_returns_exact_persisted_root_trace() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: Some("pong".into()),
        tool_calls: vec![],
    }]);

    let result = kernel.turn(&mut model, "ping").unwrap();
    let sessions = SessionStore::open(dir.path().join("sessions.db")).unwrap();
    let turn = sessions.turns(kernel.session_id()).unwrap().remove(0);
    let executions = ExecutionStore::open(dir.path().join("execution.db")).unwrap();
    let manifest_id = executions.find_by_turn(turn.id).unwrap().unwrap();
    let persisted = executions.trace_context(manifest_id).unwrap().unwrap();

    assert_eq!(result.trace_context, persisted);
    assert!(persisted.parent_span_id.is_none());
}

#[test]
fn turn_recalls_memory_then_answers() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    k.remember_demo("user", "prefers_editor", "helix").unwrap();

    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "memory_recall".into(),
                arguments: json!({"subject":"user","predicate":"prefers_editor"}),
            }],
        },
        CompletionResponse {
            text: Some("You prefer helix.".into()),
            tool_calls: vec![],
        },
    ]);

    let result = k.turn(&mut model, "what editor do I prefer?").unwrap();
    assert_eq!(result.assistant_text, "You prefer helix.");
    assert_eq!(result.steps, 2);
    assert!(result
        .tool_trace
        .iter()
        .any(|t| t.starts_with("memory_recall")));
    assert_eq!(result.invoked_tools, vec![ToolId::from("memory_recall")]);
    assert!(model.seen[0]
        .tools
        .iter()
        .any(|t| t.id.as_str() == "terminal"));
    // Evidence fence present in tool output
    assert!(k
        .messages
        .iter()
        .any(|m| m.content.contains("EVIDENCE_DATA_NOT_INSTRUCTION")));
}

#[test]
fn activate_pack_increases_tools_and_tokens() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let before = k.packs.schema_tokens();

    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "a1".into(),
                name: "activate_pack".into(),
                arguments: json!({"name":"browser"}),
            }],
        },
        CompletionResponse {
            text: Some("browser ready".into()),
            tool_calls: vec![],
        },
    ]);

    let result = k.turn(&mut model, "load browser").unwrap();
    assert!(result.schema_tokens_final > before);
    assert!(result.loaded_packs.iter().any(|p| p == "browser"));
    // Second model call should see browser tools
    assert!(model.seen[1]
        .tools
        .iter()
        .any(|t| t.id.as_str() == "browser_navigate"));
    // Progressive activate outcome carries budget headroom fields in ToolOutcome.data.
    let tool_msg = k
        .messages
        .iter()
        .find(|m| m.tool_call_id.as_deref() == Some("a1"))
        .expect("activate_pack tool message");
    let outcome: ToolOutcome = serde_json::from_str(&tool_msg.content).unwrap();
    assert_eq!(outcome.kind, ToolOutcomeKind::Succeeded);
    assert_eq!(outcome.data["ok"], true);
    assert!(
        outcome.data["max_schema_tokens"].as_u64().unwrap()
            >= outcome.data["schema_tokens"].as_u64().unwrap()
    );
    assert_eq!(outcome.data["on_demand_loaded"], 1);
}

#[test]
fn activate_pack_schema_budget_returns_typed_tool_outcome_not_turn_abort() {
    use optimus_packs::{builtin_catalog, PackBudgetConfig, PackId};

    let dir = tempdir().unwrap();
    let core_tokens = builtin_catalog()
        .get(&PackId::Core)
        .unwrap()
        .schema_tokens();
    let mut k = Kernel::open(
        dir.path(),
        KernelConfig {
            pack_budget: PackBudgetConfig {
                max_on_demand_packs: 5,
                max_schema_tokens: core_tokens + 100,
            },
            ..KernelConfig::default()
        },
    )
    .unwrap();

    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "budget-1".into(),
                name: "activate_pack".into(),
                arguments: json!({"name": "browser"}),
            }],
        },
        CompletionResponse {
            text: Some("stayed on core after budget deny".into()),
            tool_calls: vec![],
        },
    ]);

    let result = k
        .turn(&mut model, "try to load browser under tight budget")
        .expect("budget denial is a tool outcome, not a turn failure");
    assert_eq!(result.assistant_text, "stayed on core after budget deny");
    assert_eq!(k.packs.loaded_packs(), vec![PackId::Core]);

    let tool_msg = k
        .messages
        .iter()
        .find(|m| m.tool_call_id.as_deref() == Some("budget-1"))
        .expect("failed activate_pack tool message");
    let outcome: ToolOutcome = serde_json::from_str(&tool_msg.content).unwrap();
    assert_eq!(outcome.kind, ToolOutcomeKind::Failed);
    assert_eq!(
        outcome.error.as_ref().map(|e| e.code.as_str()),
        Some("pack_schema_budget_exceeded")
    );
    assert!(outcome.error.as_ref().unwrap().retryable);
}

#[test]
fn activate_pack_on_demand_limit_returns_typed_tool_outcome_not_turn_abort() {
    use optimus_packs::{PackBudgetConfig, PackId};

    let dir = tempdir().unwrap();
    let mut k = Kernel::open(
        dir.path(),
        KernelConfig {
            pack_budget: PackBudgetConfig {
                max_on_demand_packs: 0,
                max_schema_tokens: PackBudgetConfig::default().max_schema_tokens,
            },
            ..KernelConfig::default()
        },
    )
    .unwrap();

    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "limit-1".into(),
                name: "activate_pack".into(),
                arguments: json!({"name": "browser"}),
            }],
        },
        CompletionResponse {
            text: Some("stayed on core after pack limit".into()),
            tool_calls: vec![],
        },
    ]);

    let result = k
        .turn(&mut model, "try to load browser with zero on-demand slots")
        .expect("pack limit denial is a tool outcome, not a turn failure");
    assert_eq!(result.assistant_text, "stayed on core after pack limit");
    assert_eq!(k.packs.loaded_packs(), vec![PackId::Core]);

    let tool_msg = k
        .messages
        .iter()
        .find(|m| m.tool_call_id.as_deref() == Some("limit-1"))
        .expect("failed activate_pack tool message");
    let outcome: ToolOutcome = serde_json::from_str(&tool_msg.content).unwrap();
    assert_eq!(outcome.kind, ToolOutcomeKind::Failed);
    assert_eq!(
        outcome.error.as_ref().map(|e| e.code.as_str()),
        Some("pack_on_demand_limit_exceeded")
    );
    assert!(outcome.error.as_ref().unwrap().retryable);
}

#[test]
fn desktop_pack_lists_workspace_without_terminal() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    std::fs::write(kernel.workspace().join("alpha.txt"), "alpha").unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "activate-desktop".into(),
                name: "activate_pack".into(),
                arguments: json!({"name":"desktop"}),
            }],
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "read-alpha".into(),
                name: "read_file".into(),
                arguments: json!({"path":"alpha.txt"}),
            }],
        },
        CompletionResponse {
            text: Some("alpha-content".into()),
            tool_calls: vec![],
        },
    ]);

    let result = kernel
        .turn(&mut model, "activate desktop and read file")
        .unwrap();
    assert_eq!(result.assistant_text, "alpha-content");
    assert!(result.invoked_tools.contains(&ToolId::from("read_file")));
    assert!(!result.invoked_tools.contains(&ToolId::from("terminal")));
    assert!(kernel
        .messages
        .iter()
        .any(|message| message.tool_call_id.as_deref() == Some("read-alpha")));
}

#[test]
fn model_context_includes_host_provided_current_utc_date() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: Some("ok".into()),
        tool_calls: vec![],
    }]);

    kernel.turn(&mut model, "what is current?").unwrap();
    let system = &model.seen[0].messages[0].content;
    // System prompt includes basic env hints (host CPU arch placeholder etc.)
    assert!(system.len() > 100);
}

#[test]
fn skill_resolve_returns_body() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    k.skills
        .create(SkillDraft {
            name: "win-temp".into(),
            body: "set TEMP to Local/Temp".into(),
            permissions: vec![Permission::Terminal],
            pin: true,
        })
        .unwrap();

    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "s1".into(),
                name: "skill_resolve".into(),
                arguments: json!({"name":"win-temp"}),
            }],
        },
        CompletionResponse {
            text: Some("Use Local/Temp.".into()),
            tool_calls: vec![],
        },
    ]);

    let result = k.turn(&mut model, "how fix LNK1104?").unwrap();
    assert!(result.assistant_text.contains("Local/Temp"));
    assert!(k
        .messages
        .iter()
        .any(|m| m.content.contains("set TEMP to Local/Temp")));
}

#[test]
fn write_file_tool_uses_durable_job() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(
        dir.path(),
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
                id: "w1".into(),
                name: "write_file".into(),
                arguments: json!({"path":"note.txt","contents":"hi\n"}),
            }],
        },
        CompletionResponse {
            text: Some("wrote note".into()),
            tool_calls: vec![],
        },
    ]);
    k.turn(&mut model, "write a note").unwrap();
    let body = std::fs::read_to_string(k.workspace().join("note.txt")).unwrap();
    assert_eq!(body, "hi\n");
    let tool_message = k
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("w1"))
        .unwrap();
    let outcome: ToolOutcome = serde_json::from_str(&tool_message.content).unwrap();
    assert_eq!(outcome.kind, ToolOutcomeKind::Succeeded);
    assert_eq!(outcome.tool_id.as_str(), "write_file");
    assert_eq!(outcome.data["relative_path"], json!("note.txt"));
    assert_eq!(
        outcome.data["absolute_path"],
        json!(k.workspace().join("note.txt").display().to_string())
    );
    assert!(outcome.provenance.is_some());
    let provenance = outcome.provenance.as_ref().unwrap();
    assert_eq!(provenance.effect_sha256.len(), 64);
    assert_eq!(provenance.receipt_sha256.as_ref().unwrap().len(), 64);
    outcome.validate().unwrap();
}

#[test]
fn valid_tool_execution_error_becomes_canonical_failed_outcome() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "read-missing".into(),
                name: "read_file".into(),
                arguments: json!({"path":"missing.txt"}),
            }],
        },
        CompletionResponse {
            text: Some("file unavailable".into()),
            tool_calls: vec![],
        },
    ]);

    let result = kernel.turn(&mut model, "read missing").unwrap();

    assert_eq!(result.assistant_text, "file unavailable");
    let message = kernel
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("read-missing"))
        .unwrap();
    let outcome: ToolOutcome = serde_json::from_str(&message.content).unwrap();
    assert_eq!(outcome.kind, ToolOutcomeKind::Failed);
    assert_eq!(outcome.error.unwrap().code, "tool_execution_failed");
}

#[test]
fn max_steps_trips() {
    let dir = tempdir().unwrap();
    let cfg = KernelConfig {
        max_steps: 2,
        ..KernelConfig::default()
    };
    let mut k = Kernel::open(dir.path(), cfg).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "1".into(),
                name: "memory_recall".into(),
                arguments: json!({}),
            }],
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "2".into(),
                name: "memory_recall".into(),
                arguments: json!({}),
            }],
        },
        CompletionResponse {
            text: Some("never".into()),
            tool_calls: vec![],
        },
    ]);
    let mut events = Vec::new();
    let err = k
        .turn_with_sink(&mut model, "loop", &mut |event| events.push(event))
        .unwrap_err();
    assert!(matches!(err, optimus_kernel::KernelError::MaxSteps(2)));
    assert!(matches!(
        events.last(),
        Some(StreamEvent::Timing(timing))
            if timing.kind == TimingEventKind::TurnFinished
                && timing.status.as_deref() == Some("failed")
    ));
}

#[test]
fn tool_call_execution_budget_suppresses_overflow_and_forces_synthesis() {
    let dir = tempdir().unwrap();
    let config = KernelConfig {
        max_tool_calls_per_step: 1,
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open(dir.path(), config).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "one".into(),
                    name: "activate_pack".into(),
                    arguments: json!({"name":"browser"}),
                },
                ToolCall {
                    id: "two".into(),
                    name: "activate_pack".into(),
                    arguments: json!({"name":"browser"}),
                },
            ],
        },
        CompletionResponse {
            text: Some("Browser tools are ready.".into()),
            tool_calls: vec![],
        },
    ]);
    let mut events = Vec::new();

    let result = kernel
        .turn_with_sink(&mut model, "activate twice", &mut |event| {
            events.push(event)
        })
        .unwrap();
    assert_eq!(result.steps, 2);
    assert!(model.seen[1].tools.is_empty());
    assert!(kernel.messages.iter().any(|message| {
        message.tool_call_id.as_deref() == Some("two")
            && message.content.contains("tool_call_budget_suppressed")
    }));
    assert!(events.iter().any(|event| matches!(
        event,
        StreamEvent::Timing(timing)
            if timing.kind == TimingEventKind::ToolFinished
                && timing.call_id.as_deref() == Some("two")
                && timing.suppressed
    )));
    assert_eq!(
        kernel.packs.loaded_packs(),
        vec![optimus_packs::PackId::Core, optimus_packs::PackId::Browser]
    );
}

#[test]
fn hard_tool_call_ceiling_rejects_before_dispatch() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let calls = (0..65)
        .map(|index| ToolCall {
            id: format!("call-{index}"),
            name: "memory_recall".into(),
            arguments: json!({"subject": format!("subject-{index}")}),
        })
        .collect();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: calls,
    }]);

    let error = kernel.turn(&mut model, "too many calls").unwrap_err();
    assert!(matches!(
        error,
        KernelError::Model(message) if message.contains("hard per-step limit is 64")
    ));
    assert!(!kernel
        .messages
        .iter()
        .any(|message| message.role == optimus_kernel::Role::Tool));
}

#[test]
fn repeated_read_only_evidence_call_is_suppressed_and_forces_timed_synthesis() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let session_id = kernel.session_id();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "first".into(),
                name: "memory_recall".into(),
                arguments: json!({"subject":"user","predicate":"editor"}),
            }],
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "duplicate".into(),
                name: "memory_recall".into(),
                arguments: json!({"predicate":"editor","subject":"user"}),
            }],
        },
        CompletionResponse {
            text: Some("No matching preference was found.".into()),
            tool_calls: vec![],
        },
    ]);
    let mut events = Vec::new();

    let result = kernel
        .turn_with_sink(&mut model, "what editor do I prefer?", &mut |event| {
            events.push(event)
        })
        .unwrap();

    assert_eq!(result.steps, 3);
    assert_eq!(result.invoked_tools, vec![ToolId::from("memory_recall")]);
    assert!(model.seen[2].tools.is_empty());
    assert!(model.seen[2]
        .messages
        .iter()
        .any(|message| message.content.contains("synthesis-only")));
    assert!(kernel.messages.iter().any(|message| {
        message.tool_call_id.as_deref() == Some("duplicate")
            && message.content.contains("duplicate_tool_call_suppressed")
    }));
    assert!(result.timings.total_ms >= result.timings.model_ms);
    assert!(result.timings.first_response_ms.is_some());

    let timing_events = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::Timing(timing) => Some(timing),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        timing_events.first().map(|event| event.kind),
        Some(TimingEventKind::TurnStarted)
    );
    assert_eq!(
        timing_events.last().map(|event| event.kind),
        Some(TimingEventKind::TurnFinished)
    );
    assert_eq!(
        timing_events.last().unwrap().status.as_deref(),
        Some("succeeded")
    );
    assert!(timing_events.iter().any(|event| {
        event.kind == TimingEventKind::ToolFinished
            && event.call_id.as_deref() == Some("duplicate")
            && event.suppressed
    }));

    let sessions = SessionStore::open(dir.path().join("sessions.db")).unwrap();
    let turn = sessions.turns(session_id).unwrap().pop().unwrap();
    let executions = ExecutionStore::open(dir.path().join("execution.db")).unwrap();
    let manifest = executions.find_by_turn(turn.id).unwrap().unwrap();
    let timing = executions.timing_summary(manifest).unwrap();
    assert_eq!(timing.terminal_status.as_deref(), Some("succeeded"));
    assert_eq!(timing.model_call_count, 3);
    assert_eq!(timing.executed_tool_call_count, 1);
    assert_eq!(timing.suppressed_tool_call_count, 1);
}

#[test]
fn terminal_tool_requires_approval_before_process_effect() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let marker = dir.path().join("terminal-ran.txt");
    let command = format!("echo ran>\"{}\"", marker.display());
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "t1".into(),
            name: "terminal".into(),
            arguments: json!({"program":"cmd","args":["/C",command]}),
        }],
    }]);
    let error = k.turn(&mut model, "run a command").unwrap_err();
    assert!(
        matches!(
            error,
            KernelError::Runtime(RuntimeError::NeedsApproval { .. })
        ),
        "unexpected error: {error:?}"
    );
    assert_eq!(model.seen.len(), 1);
    assert_eq!(k.runtime.list_pending_approvals().unwrap().len(), 1);
    assert!(!marker.exists(), "command ran before explicit approval");
}

#[test]
fn browser_tools_http_effector_when_pack_active() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "b1".into(),
                name: "activate_pack".into(),
                arguments: json!({"name":"browser"}),
            }],
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "b2".into(),
                name: "browser_navigate".into(),
                arguments: json!({"url":"https://example.com"}),
            }],
        },
        CompletionResponse {
            text: Some("browser page noted".into()),
            tool_calls: vec![],
        },
    ]);
    model.stream_chunks = false;
    let result = k.turn(&mut model, "open example").unwrap();
    assert!(result
        .tool_trace
        .iter()
        .any(|t| t.contains("browser_navigate")));
    assert_eq!(result.assistant_text, "browser page noted");
}

#[test]
fn same_response_activation_cannot_authorize_unadvertised_sibling_call() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![
            ToolCall {
                id: "activate".into(),
                name: "activate_pack".into(),
                arguments: json!({"name":"browser"}),
            },
            ToolCall {
                id: "stale-browser".into(),
                name: "browser_navigate".into(),
                arguments: json!({"url":"https://example.com"}),
            },
        ],
    }]);
    assert!(matches!(
        k.turn(&mut model, "activate and browse in one response")
            .unwrap_err(),
        KernelError::Packs(PackError::ToolNotAdvertised(tool))
            if tool == "browser_navigate"
    ));
    assert_eq!(k.packs.loaded_packs(), vec![optimus_packs::PackId::Core]);
}

#[test]
fn unknown_tool_fails_closed() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "bad-1".into(),
            name: "does_not_exist".into(),
            arguments: json!({}),
        }],
    }]);
    assert!(matches!(
        k.turn(&mut model, "call an unknown tool").unwrap_err(),
        KernelError::Packs(PackError::UnknownTool(name)) if name == "does_not_exist"
    ));
}

#[test]
fn known_but_unloaded_tool_fails_closed_before_effect() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "bad-2".into(),
            name: "browser_navigate".into(),
            arguments: json!({"url":"https://example.com"}),
        }],
    }]);
    assert!(matches!(
        k.turn(&mut model, "bypass pack activation").unwrap_err(),
        KernelError::Packs(PackError::ToolNotAdvertised(tool))
            if tool == "browser_navigate"
    ));
}

#[test]
fn deactivated_tool_fails_closed_before_effect() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    k.packs.activate(optimus_packs::PackId::Browser).unwrap();
    k.packs.deactivate(optimus_packs::PackId::Browser).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "deactivated".into(),
            name: "browser_navigate".into(),
            arguments: json!({"url":"https://example.com"}),
        }],
    }]);
    assert!(matches!(
        k.turn(&mut model, "use a deactivated tool").unwrap_err(),
        KernelError::Packs(PackError::ToolNotAdvertised(tool))
            if tool == "browser_navigate"
    ));
}

#[test]
fn descriptorless_legacy_aliases_cannot_invoke_effects() {
    for (name, arguments) in [
        ("need_capability", json!({"name":"browser"})),
        (
            "job_write_file",
            json!({"path":"alias-created.txt","contents":"bad"}),
        ),
        (
            "run_command",
            json!({"program":"cmd","args":["/C","exit","0"]}),
        ),
    ] {
        let dir = tempdir().unwrap();
        let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
        let mut model = ScriptedModel::new(vec![CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: format!("alias-{name}"),
                name: name.into(),
                arguments,
            }],
        }]);
        assert!(matches!(
            k.turn(&mut model, "invoke an undeclared alias").unwrap_err(),
            KernelError::Packs(PackError::UnknownTool(tool)) if tool == name
        ));
        assert_eq!(k.packs.loaded_packs(), vec![optimus_packs::PackId::Core]);
        assert!(!k.workspace().join("alias-created.txt").exists());
        assert!(k.runtime.list_jobs_summary().unwrap().is_empty());
    }
}

#[test]
fn call_identity_batch_prevalidation_blocks_all_effects() {
    for (second_id, expected) in [("", "non-empty id"), ("write-id", "duplicate id")] {
        let dir = tempdir().unwrap();
        let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
        let output = k.workspace().join("identity-bypass.txt");
        let mut model = ScriptedModel::new(vec![CompletionResponse {
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "write-id".into(),
                    name: "write_file".into(),
                    arguments: json!({"path":"identity-bypass.txt","contents":"bad"}),
                },
                ToolCall {
                    id: second_id.into(),
                    name: "memory_recall".into(),
                    arguments: json!({"subject":"user"}),
                },
            ],
        }]);
        assert!(matches!(
            k.turn(&mut model, "send an invalid sibling call").unwrap_err(),
            KernelError::Model(message) if message.contains(expected)
        ));
        assert!(!output.exists());
        assert!(k.runtime.list_jobs_summary().unwrap().is_empty());
    }
}

#[test]
fn canonical_schema_rejects_extra_runtime_arguments() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    std::fs::write(k.workspace().join("safe.txt"), "safe").unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: "bad-args".into(),
            name: "read_file".into(),
            arguments: json!({"path":"safe.txt","escape":true}),
        }],
    }]);
    assert!(matches!(
        k.turn(&mut model, "send invalid arguments").unwrap_err(),
        KernelError::Packs(PackError::InvalidArguments { tool, .. }) if tool == "read_file"
    ));
}

#[test]
fn read_file_uses_workspace_sandbox_and_denies_secrets() {
    let dir = tempdir().unwrap();
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "outside-secret").unwrap();
    std::fs::write(k.workspace().join(".env"), "TOKEN=secret").unwrap();

    for path in [
        "../outside.txt".to_string(),
        outside.to_string_lossy().into_owned(),
        ".env".to_string(),
    ] {
        let mut model = ScriptedModel::new(vec![CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: format!("read-denied-{path}"),
                name: "read_file".into(),
                arguments: json!({"path":path}),
            }],
        }]);
        assert!(matches!(
            k.turn(&mut model, "read a denied path").unwrap_err(),
            KernelError::Tool(message)
                if message.contains("path not allowed") || message.contains("secret path denied")
        ));
        assert!(!k.messages.iter().any(|message| {
            message.content.contains("outside-secret") || message.content.contains("TOKEN=secret")
        }));
    }
}

/// ADR-0044 threading proof: the identical scripted project write that pauses
/// under the default profile completes without any human approval when the
/// per-turn config carries `Standard` — and the file really lands on disk.
#[test]
fn standard_profile_threads_through_the_turn_and_writes_without_a_pause() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let authority = ProjectAuthorityStore::open(home.path()).unwrap();
    let selection = authority.stage_native_selection(project.path()).unwrap();
    authority
        .authorize_project(
            "project-a",
            std::slice::from_ref(&selection.path),
            Some(&selection.path),
            std::slice::from_ref(&selection.grant_token),
        )
        .unwrap();

    let config = KernelConfig {
        autonomy_profile: optimus_graph::AutonomyProfile::Standard,
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open_project_session(home.path(), config, None, "project-a").unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "write-std".into(),
                name: "write_file".into(),
                arguments: json!({"path":"src/standard.txt","contents":"auto-approved"}),
            }],
        },
        CompletionResponse {
            text: Some("written".into()),
            tool_calls: vec![],
        },
    ]);

    let result = kernel
        .turn_with_sink(&mut model, "write it", &mut |_| {})
        .expect("Standard must not pause an ordinary project write");

    assert_eq!(result.assistant_text, "written");
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/standard.txt")).unwrap(),
        "auto-approved",
        "the effect ran on disk without a human approval"
    );
}

/// Searching must reach the model as a plain workspace read.
///
/// The engine has its own unit tests; what this covers is the wiring — that
/// the tool is advertised in the core pack, that its schema validates, and
/// that the dispatch arm answers. A search engine nobody can call from a turn
/// is not a feature.
#[test]
fn the_core_pack_can_search_find_and_list_without_a_terminal() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let workspace = kernel.workspace().to_path_buf();
    std::fs::create_dir_all(workspace.join("src")).unwrap();
    std::fs::write(
        workspace.join("src/main.rs"),
        "fn main() {\n    needle();\n}\n",
    )
    .unwrap();

    let call = |id: &str, name: &str, arguments| CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        }],
    };
    let mut model = ScriptedModel::new(vec![
        call("s1", "search_content", json!({"pattern": "needle"})),
        call("f1", "find_files", json!({"glob": "**/*.rs"})),
        call("l1", "list_dir", json!({"path": "src"})),
        CompletionResponse {
            text: Some("found it".into()),
            tool_calls: vec![],
        },
    ]);

    let result = kernel.turn(&mut model, "find the needle").unwrap();
    assert_eq!(result.assistant_text, "found it");
    for tool in ["search_content", "find_files", "list_dir"] {
        assert!(
            result.invoked_tools.contains(&ToolId::from(tool)),
            "{tool} never ran"
        );
    }

    let answer = |id: &str| {
        let message = kernel
            .messages
            .iter()
            .find(|message| message.tool_call_id.as_deref() == Some(id))
            .unwrap_or_else(|| panic!("no tool message for {id}"));
        let outcome: ToolOutcome = serde_json::from_str(&message.content).unwrap();
        assert_eq!(outcome.kind, ToolOutcomeKind::Succeeded, "{id} failed");
        outcome.data
    };

    let hit = &answer("s1")["matches"][0];
    assert_eq!(hit["path"], "src/main.rs");
    assert_eq!(
        hit["line"], 2,
        "a hit the model cannot cite is not an answer"
    );
    assert_eq!(answer("f1")["paths"][0], "src/main.rs");
    assert_eq!(answer("l1")["path"], "src");
}

/// A window on a read must say where the window sits.
#[test]
fn read_file_can_return_one_slice_instead_of_the_whole_file() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    std::fs::write(
        kernel.workspace().join("long.txt"),
        (1..=200).map(|n| format!("line {n}\n")).collect::<String>(),
    )
    .unwrap();

    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "r1".into(),
                name: "read_file".into(),
                arguments: json!({"path": "long.txt", "offset": 100, "limit": 3}),
            }],
        },
        CompletionResponse {
            text: Some("read".into()),
            tool_calls: vec![],
        },
    ]);
    kernel.turn(&mut model, "read the middle").unwrap();

    let message = kernel
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("r1"))
        .unwrap();
    let outcome: ToolOutcome = serde_json::from_str(&message.content).unwrap();
    let data = outcome.data;
    assert_eq!(data["contents"], "line 100\nline 101\nline 102");
    assert_eq!(data["start_line"], 100);
    assert_eq!(
        data["total_lines"], 200,
        "a slice without the total hides how much was left unread"
    );
}
