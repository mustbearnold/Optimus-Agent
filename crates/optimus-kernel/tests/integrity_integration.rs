use std::collections::BTreeSet;

use optimus_graph::{Effect, JobId, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_kernel::{
    delivery_state, drain_one, enqueue, evaluate_integrity_observations, resolve_route,
    AgentBudget, AgentDescriptor, AgentFailure, AgentId, AgentInvocationStatus,
    AgentInvocationStore, AgentPermissions, AgentRegistry, AgentRequest, AgentResult,
    AgentResultKind, AgentVersion, CancellationToken, CompletionResponse, ExecutionStatus,
    ExecutionStore, IntegrityObservation, Kernel, KernelConfig, Message, PrivacyPolicy, Role,
    RouteRequest, RouteSurface, ScriptedModel, SessionStore, ToolCall, TurnStatus,
    WorkflowAdapterKind, WorkflowAgentRef, WorkflowNode, AGENT_REQUEST_SCHEMA_VERSION,
    AGENT_RESULT_SCHEMA_VERSION,
};
use optimus_memory::{ClaimDraft, Memory, Origin, Sensitivity, TrustDomain, WriteContext};
use optimus_packs::{builtin_catalog, DurableEffectProvenance, ToolId};
use optimus_runtime::{Runtime, RuntimeError};
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;

fn available_tools() -> BTreeSet<ToolId> {
    builtin_catalog()
        .into_values()
        .flat_map(|pack| pack.tools)
        .filter(|tool| tool.is_available())
        .map(|tool| tool.id)
        .collect()
}

fn permissions() -> AgentPermissions {
    AgentPermissions {
        filesystem_roots: BTreeSet::from(["workspace".into()]),
        network_hosts: BTreeSet::new(),
        effects: BTreeSet::new(),
    }
}

fn descriptor() -> AgentDescriptor {
    AgentDescriptor {
        id: AgentId::parse("workflow_agent").unwrap(),
        version: AgentVersion::parse("1.0.0").unwrap(),
        responsibility: "Perform one bounded workflow node".into(),
        request_schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        result_schema_version: AGENT_RESULT_SCHEMA_VERSION,
        required_tools: vec![ToolId::new("read_file")],
        permissions: permissions(),
    }
}

fn request() -> AgentRequest {
    AgentRequest {
        schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        agent_id: AgentId::parse("workflow_agent").unwrap(),
        agent_version: AgentVersion::parse("1.0.0").unwrap(),
        task: "Execute the typed workflow node".into(),
        context: vec![],
        constraints: vec!["Retain causal provenance".into()],
        tools: vec![ToolId::new("read_file")],
        permissions: permissions(),
        budget: AgentBudget {
            max_steps: 4,
            timeout_ms: 30_000,
            max_context_chars: 100_000,
            max_output_chars: 20_000,
        },
        cancellation_id: Uuid::new_v4(),
        trace_id: Uuid::new_v4(),
    }
}

fn result(invocation_id: Uuid, kind: AgentResultKind) -> AgentResult {
    AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id,
        kind,
        summary: format!("{kind:?}"),
        error: (kind == AgentResultKind::Failed).then(|| AgentFailure {
            code: "fixture_failure".into(),
            message: "offline failure".into(),
            retryable: true,
        }),
        cancellation_reason: (kind == AgentResultKind::Cancelled)
            .then(|| "operator_request".into()),
        evidence: vec![],
        artifacts: vec![],
        unresolved: if kind == AgentResultKind::Ambiguous {
            vec!["external settlement".into()]
        } else {
            Vec::new()
        },
    }
}

#[test]
fn tool_manifest_runtime_agent_and_workflow_share_exact_causal_identity() {
    let dir = tempdir().unwrap();
    let session_id = {
        let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
        let session_id = kernel.session_id();
        let mut model = ScriptedModel::new(vec![
            CompletionResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "causal-write".into(),
                    name: "write_file".into(),
                    arguments: json!({"path":"causal.txt","contents":"linked"}),
                }],
            },
            CompletionResponse {
                text: Some("done".into()),
                tool_calls: vec![],
            },
        ]);
        kernel
            .turn(&mut model, "write with exact provenance")
            .unwrap();
        session_id
    };

    let sessions = SessionStore::open(dir.path().join("sessions.db")).unwrap();
    let turn = sessions.turns(session_id).unwrap().pop().unwrap();
    assert_eq!(turn.status, TurnStatus::Succeeded);
    let link = sessions.effect_links(session_id).unwrap().pop().unwrap();
    let executions = ExecutionStore::open(dir.path().join("execution.db")).unwrap();
    let manifest_id = executions.find_by_turn(turn.id).unwrap().unwrap();
    assert_eq!(
        executions.manifest(manifest_id).unwrap().status,
        ExecutionStatus::Succeeded
    );

    let runtime = Runtime::open(
        &dir.path().join("optimus.db"),
        &dir.path().join("workspace"),
    )
    .unwrap();
    let effect = runtime
        .latest_effect_outcome(JobId(link.job_id))
        .unwrap()
        .unwrap();
    assert_eq!(effect.attempt_id, link.effect_attempt_id);
    assert_eq!(effect.node_id, link.node_id);
    assert_eq!(effect.effect_hash, link.effect_hash);
    assert_eq!(effect.receipt_hash, link.receipt_hash);

    let registry = AgentRegistry::open(
        dir.path().join("agents.db"),
        available_tools(),
        permissions(),
    )
    .unwrap();
    let descriptor = descriptor();
    registry.register(&descriptor).unwrap();
    let invocations = AgentInvocationStore::open(dir.path().join("invocations.db")).unwrap();
    let invocation_id = invocations.begin(&registry, &request()).unwrap();
    invocations
        .link_effect(
            &runtime,
            invocation_id,
            &DurableEffectProvenance {
                job_id: link.job_id,
                node_id: link.node_id,
                effect_attempt_id: link.effect_attempt_id,
                effect_sha256: link.effect_hash,
                receipt_sha256: link.receipt_hash,
            },
        )
        .unwrap();
    invocations
        .settle(&result(invocation_id, AgentResultKind::Succeeded))
        .unwrap();

    let workflow_node = WorkflowNode {
        id: "execute".into(),
        adapter: WorkflowAdapterKind::Job,
        agent: Some(WorkflowAgentRef {
            id: descriptor.id,
            version: descriptor.version,
        }),
        dependencies: vec![],
        retry: optimus_kernel::RetryPolicy {
            max_attempts: 1,
            backoff_ms: 0,
            retryable: BTreeSet::new(),
        },
        timeout_ms: 30_000,
        cancellation: optimus_kernel::CancellationPolicy::Cooperative,
        approval: optimus_kernel::ApprovalPolicy::None,
        rollback: optimus_kernel::RollbackPolicy::Unsupported,
    };
    let agent = workflow_node.agent.unwrap();
    let invocation = invocations.get(invocation_id).unwrap();
    assert_eq!(agent.id, invocation.request.agent_id);
    assert_eq!(agent.version, invocation.request.agent_version);
}

#[test]
fn session_and_agent_terminal_outcomes_agree_after_independent_reopen() {
    let dir = tempdir().unwrap();
    let sessions_path = dir.path().join("sessions.db");
    let agents_path = dir.path().join("agents.db");
    let invocations_path = dir.path().join("invocations.db");
    let registry = AgentRegistry::open(&agents_path, available_tools(), permissions()).unwrap();
    registry.register(&descriptor()).unwrap();
    let invocations = AgentInvocationStore::open(&invocations_path).unwrap();
    let sessions = SessionStore::open(&sessions_path).unwrap();

    for (turn_status, agent_kind, error_code) in [
        (TurnStatus::Succeeded, AgentResultKind::Succeeded, None),
        (
            TurnStatus::Failed,
            AgentResultKind::Failed,
            Some("fixture_failure"),
        ),
        (
            TurnStatus::Cancelled,
            AgentResultKind::Cancelled,
            Some("operator_request"),
        ),
    ] {
        let session_id = sessions.create("contract trajectory").unwrap();
        let messages = vec![Message {
            role: Role::User,
            content: "accepted".into(),
            tool_call_id: None,
            name: None,
        }];
        let turn_id = sessions
            .begin_turn(session_id, "contract trajectory", &[], &messages, 0)
            .unwrap();
        let invocation_id = invocations.begin(&registry, &request()).unwrap();
        if agent_kind == AgentResultKind::Cancelled {
            invocations
                .request_cancellation(invocation_id, "operator_request")
                .unwrap();
        }
        sessions
            .finish_turn(
                turn_id,
                session_id,
                "contract trajectory",
                &[],
                &messages,
                turn_status,
                error_code,
            )
            .unwrap();
        invocations
            .settle(&result(invocation_id, agent_kind))
            .unwrap();

        drop(SessionStore::open(&sessions_path).unwrap());
        let reopened_sessions = SessionStore::open(&sessions_path).unwrap();
        let reopened_invocations = AgentInvocationStore::open(&invocations_path).unwrap();
        assert_eq!(
            reopened_sessions.turns(session_id).unwrap()[0].status,
            turn_status
        );
        let expected = match agent_kind {
            AgentResultKind::Succeeded => AgentInvocationStatus::Succeeded,
            AgentResultKind::Failed => AgentInvocationStatus::Failed,
            AgentResultKind::Cancelled => AgentInvocationStatus::Cancelled,
            AgentResultKind::Ambiguous => unreachable!(),
        };
        assert_eq!(
            reopened_invocations.get(invocation_id).unwrap().status,
            expected
        );
    }
}

#[test]
fn offline_integrity_evaluation_executes_all_required_contract_cases() {
    let dir = tempdir().unwrap();

    let memory = Memory::open(dir.path().join("memory.db")).unwrap();
    let context = WriteContext {
        tenant: "local".into(),
        user: "alice".into(),
        agent: "optimus".into(),
        project: "eval".into(),
        principal: "user:alice".into(),
        max_trust: TrustDomain::User,
        max_sensitivity: Sensitivity::Personal,
    };
    let sensitivity_denied = memory
        .remember(
            &context,
            ClaimDraft {
                subject: "secret".into(),
                predicate: "value".into(),
                object: "must-not-persist".into(),
                valid_from: "2026-01-01T00:00:00Z".into(),
                valid_to: None,
                confidence: 1.0,
                origin: Origin::UserStatement,
                learned_at: Some("2026-01-01T00:00:00Z".into()),
                sensitivity: Sensitivity::Restricted,
                retention_until: None,
            },
        )
        .is_err();

    let workspace = dir.path().join("policy-workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let policy_runtime = Runtime::open_with_config(
        &dir.path().join("policy.db"),
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    let denied_job = policy_runtime
        .create_job(JobSpec {
            label: "eval-smartdeny".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "command".into(),
                effect: Effect::RunCommand {
                    program: "cmd".into(),
                    args: vec!["/C".into(), "echo forbidden>forbidden.txt".into()],
                },
            }],
        })
        .unwrap();
    let smartdeny = matches!(
        policy_runtime.run_next(denied_job),
        Err(RuntimeError::NeedsApproval { .. })
    ) && !workspace.join("forbidden.txt").exists();

    let mut route = RouteRequest::standard(RouteSurface::Gateway, "codex", None);
    route.privacy = PrivacyPolicy::LocalOnly;
    let route_denied = resolve_route(dir.path().join("route"), &route).is_err();

    let registry = AgentRegistry::open(
        dir.path().join("eval-agents.db"),
        available_tools(),
        permissions(),
    )
    .unwrap();
    registry.register(&descriptor()).unwrap();
    let invocations = AgentInvocationStore::open(dir.path().join("eval-invocations.db")).unwrap();
    let invocation = invocations.begin(&registry, &request()).unwrap();
    invocations
        .request_cancellation(invocation, "operator_request")
        .unwrap();
    let token = CancellationToken::new();
    let cooperative_cancellation =
        invocations.sync_cancellation(invocation, &token).unwrap() && token.is_cancelled();
    let stale_completion_fenced = invocations
        .settle(&result(invocation, AgentResultKind::Succeeded))
        .is_err();
    invocations
        .settle(&result(invocation, AgentResultKind::Cancelled))
        .unwrap();

    let gateway_home = dir.path().join("gateway-eval");
    let message = enqueue(&gateway_home, "local", "fail", "offline", None).unwrap();
    for _ in 0..3 {
        drain_one(&gateway_home, |_| Err("provider_unavailable".into()))
            .unwrap()
            .unwrap();
    }
    let gateway_dead_letter = delivery_state(&gateway_home, &message.id)
        .unwrap()
        .unwrap()
        .0
        .as_deref()
        == Some("dead_lettered");

    let report = evaluate_integrity_observations(vec![
        IntegrityObservation {
            id: "sensitivity_denial".into(),
            passed: sensitivity_denied,
            evidence: "restricted write rejected under personal clearance".into(),
        },
        IntegrityObservation {
            id: "smartdeny_approval".into(),
            passed: smartdeny,
            evidence: "RunCommand awaited approval and created no file".into(),
        },
        IntegrityObservation {
            id: "route_policy_denial".into(),
            passed: route_denied,
            evidence: "remote Codex route rejected under local-only privacy".into(),
        },
        IntegrityObservation {
            id: "cooperative_cancellation".into(),
            passed: cooperative_cancellation,
            evidence: "durable cancellation synchronized to kernel token".into(),
        },
        IntegrityObservation {
            id: "stale_completion_fence".into(),
            passed: stale_completion_fenced,
            evidence: "late success rejected after cancellation request".into(),
        },
        IntegrityObservation {
            id: "gateway_dead_letter".into(),
            passed: gateway_dead_letter,
            evidence: "third bounded failure produced dead-letter state".into(),
        },
    ])
    .unwrap();
    assert!(report.all_ok(), "{:#?}", report.cases);
    assert_eq!(report.passed, 6);
}
