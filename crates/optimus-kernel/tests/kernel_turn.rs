//! Kernel turn loop offline tests.

use optimus_kernel::{
    CancellationToken, CompletionRequest, CompletionResponse, ExecutionStatus, ExecutionStore,
    Kernel, KernelConfig, KernelError, ModelProvider, ScriptedModel, SessionStore, StreamControl,
    StreamEvent, TimingEventKind, ToolCall, TurnStatus,
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
    let mut k = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
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
