//! Deterministic trajectory eval harness — offline scripted turns with expected traces.

use std::{collections::BTreeSet, path::Path};

use optimus_graph::{Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_memory::{
    ClaimDraft, Memory, MemoryError, Origin, Sensitivity, TrustDomain, WriteContext,
};
use optimus_packs::{builtin_catalog, ToolId};
use optimus_runtime::{Runtime, RuntimeError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    delivery_state, drain_one, enqueue, resolve_route, AgentBudget, AgentDescriptor, AgentFailure,
    AgentId, AgentInvocationStore, AgentPermissions, AgentRegistry, AgentRequest, AgentResult,
    AgentResultKind, AgentVersion, CancellationToken, CompletionResponse, Kernel, KernelConfig,
    KernelError, PrivacyPolicy, RouteRequest, RouteSurface, ScriptedModel, ToolCall, TurnResult,
    AGENT_REQUEST_SCHEMA_VERSION, AGENT_RESULT_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub user: String,
    /// Scripted model steps (tool calls / final text).
    pub steps: Vec<CompletionResponse>,
    /// Canonical tool identities that must be invoked (any order).
    #[serde(default)]
    pub expect_tools: Vec<ToolId>,
    /// Substring that must appear in assistant_text.
    #[serde(default)]
    pub expect_text_contains: Option<String>,
    /// Disable stream chunking for offline scripted model.
    #[serde(default = "default_true")]
    pub stream_chunks: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCaseResult {
    pub id: String,
    pub ok: bool,
    pub detail: String,
    #[serde(default)]
    pub tool_trace: Vec<String>,
    #[serde(default)]
    pub assistant_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub passed: usize,
    pub failed: usize,
    pub cases: Vec<EvalCaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrityObservation {
    pub id: String,
    pub passed: bool,
    pub evidence: String,
}

pub const REQUIRED_INTEGRITY_EVALS: [&str; 6] = [
    "sensitivity_denial",
    "smartdeny_approval",
    "route_policy_denial",
    "cooperative_cancellation",
    "stale_completion_fence",
    "gateway_dead_letter",
];

pub fn evaluate_integrity_observations(
    observations: Vec<IntegrityObservation>,
) -> Result<EvalReport, KernelError> {
    let mut by_id = std::collections::BTreeMap::new();
    for observation in observations {
        if observation.evidence.trim().is_empty()
            || by_id.insert(observation.id.clone(), observation).is_some()
        {
            return Err(KernelError::Model(
                "integrity evaluation requires unique observations with evidence".into(),
            ));
        }
    }
    let expected: std::collections::BTreeSet<_> =
        REQUIRED_INTEGRITY_EVALS.iter().copied().collect();
    let actual: std::collections::BTreeSet<_> = by_id.keys().map(String::as_str).collect();
    if actual != expected {
        return Err(KernelError::Model(
            "integrity evaluation observations do not match required case set".into(),
        ));
    }
    let cases = REQUIRED_INTEGRITY_EVALS
        .iter()
        .map(|id| {
            let observation = &by_id[*id];
            EvalCaseResult {
                id: observation.id.clone(),
                ok: observation.passed,
                detail: observation.evidence.clone(),
                tool_trace: vec![],
                assistant_text: String::new(),
            }
        })
        .collect::<Vec<_>>();
    let passed = cases.iter().filter(|case| case.ok).count();
    Ok(EvalReport {
        passed,
        failed: cases.len() - passed,
        cases,
    })
}

fn integrity_observation(
    id: &str,
    success_evidence: &str,
    outcome: std::result::Result<(), &'static str>,
) -> IntegrityObservation {
    match outcome {
        Ok(()) => IntegrityObservation {
            id: id.into(),
            passed: true,
            evidence: success_evidence.into(),
        },
        Err(code) => IntegrityObservation {
            id: id.into(),
            passed: false,
            evidence: format!("integrity_case_failed:{code}"),
        },
    }
}

fn observe_sensitivity_denial(home: &Path) -> std::result::Result<(), &'static str> {
    let memory = Memory::open(home.join("memory.db")).map_err(|_| "memory_open_failed")?;
    let context = WriteContext {
        tenant: "local".into(),
        user: "alice".into(),
        agent: "optimus".into(),
        project: "eval".into(),
        principal: "user:alice".into(),
        max_trust: TrustDomain::User,
        max_sensitivity: Sensitivity::Personal,
    };
    match memory.remember(
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
    ) {
        Err(MemoryError::WriteDenied(reason))
            if reason == "claim sensitivity exceeds principal clearance" =>
        {
            Ok(())
        }
        Err(MemoryError::WriteDenied(_)) => Err("unexpected_memory_denial"),
        Err(_) => Err("memory_operation_failed"),
        Ok(_) => Err("restricted_memory_write_was_accepted"),
    }
}

fn observe_smartdeny_approval(home: &Path) -> std::result::Result<(), &'static str> {
    let workspace = home.join("policy-workspace");
    std::fs::create_dir_all(&workspace).map_err(|_| "policy_workspace_create_failed")?;
    let runtime = Runtime::open_with_config(
        &home.join("policy.db"),
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
        },
    )
    .map_err(|_| "policy_runtime_open_failed")?;
    let denied_job = runtime
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
        .map_err(|_| "policy_job_create_failed")?;
    match runtime.run_next(denied_job) {
        Err(RuntimeError::NeedsApproval { job_id, node_index })
            if job_id == denied_job
                && node_index == 0
                && !workspace.join("forbidden.txt").exists() =>
        {
            Ok(())
        }
        Err(RuntimeError::NeedsApproval { .. }) => Err("approval_identity_or_effect_mismatch"),
        Err(_) => Err("smartdeny_failed_without_approval_outcome"),
        Ok(_) => Err("smartdeny_command_was_executed"),
    }
}

fn observe_route_policy_denial(home: &Path) -> std::result::Result<(), &'static str> {
    let mut request = RouteRequest::standard(RouteSurface::Gateway, "codex", None);
    request.privacy = PrivacyPolicy::LocalOnly;
    match resolve_route(home.join("route"), &request) {
        Err(KernelError::Model(reason)) if reason.contains("codex:privacy_requires_local") => {
            Ok(())
        }
        Err(_) => Err("route_failed_without_privacy_policy_reason"),
        Ok(_) => Err("remote_route_allowed_under_local_only"),
    }
}

fn integrity_available_tools() -> BTreeSet<ToolId> {
    builtin_catalog()
        .into_values()
        .flat_map(|pack| pack.tools)
        .filter(|tool| tool.is_available())
        .map(|tool| tool.id)
        .collect()
}

fn integrity_permissions() -> AgentPermissions {
    AgentPermissions {
        filesystem_roots: BTreeSet::from(["workspace".into()]),
        network_hosts: BTreeSet::new(),
        effects: BTreeSet::new(),
    }
}

fn integrity_descriptor() -> AgentDescriptor {
    AgentDescriptor {
        id: AgentId::parse("integrity_eval_agent").expect("static agent id is valid"),
        version: AgentVersion::parse("1.0.0").expect("static agent version is valid"),
        responsibility: "Exercise cancellation and stale completion fencing".into(),
        request_schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        result_schema_version: AGENT_RESULT_SCHEMA_VERSION,
        required_tools: vec![ToolId::new("read_file")],
        permissions: integrity_permissions(),
    }
}

fn integrity_request() -> AgentRequest {
    AgentRequest {
        schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        agent_id: AgentId::parse("integrity_eval_agent").expect("static agent id is valid"),
        agent_version: AgentVersion::parse("1.0.0").expect("static agent version is valid"),
        task: "Exercise cancellation and stale completion fencing".into(),
        context: vec![],
        constraints: vec!["retain exact cancellation identity".into()],
        tools: vec![ToolId::new("read_file")],
        permissions: integrity_permissions(),
        budget: AgentBudget {
            max_steps: 1,
            timeout_ms: 30_000,
            max_context_chars: 10_000,
            max_output_chars: 2_000,
        },
        cancellation_id: Uuid::new_v4(),
        trace_id: Uuid::new_v4(),
    }
}

fn integrity_result(invocation_id: Uuid, kind: AgentResultKind) -> AgentResult {
    AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id,
        kind,
        summary: format!("{kind:?}"),
        error: (kind == AgentResultKind::Failed).then(|| AgentFailure {
            code: "integrity_eval_failure".into(),
            message: "offline integrity fixture failure".into(),
            retryable: false,
        }),
        cancellation_reason: (kind == AgentResultKind::Cancelled)
            .then(|| "operator_request".into()),
        evidence: vec![],
        artifacts: vec![],
        unresolved: vec![],
    }
}

fn observe_agent_cancellation(
    home: &Path,
) -> (
    std::result::Result<(), &'static str>,
    std::result::Result<(), &'static str>,
) {
    let setup = || -> std::result::Result<(AgentInvocationStore, Uuid), &'static str> {
        let registry = AgentRegistry::open(
            home.join("eval-agents.db"),
            integrity_available_tools(),
            integrity_permissions(),
        )
        .map_err(|_| "agent_registry_open_failed")?;
        registry
            .register(&integrity_descriptor())
            .map_err(|_| "agent_register_failed")?;
        let invocations = AgentInvocationStore::open(home.join("eval-invocations.db"))
            .map_err(|_| "invocation_store_open_failed")?;
        let invocation = invocations
            .begin(&registry, &integrity_request())
            .map_err(|_| "invocation_begin_failed")?;
        Ok((invocations, invocation))
    };
    let (invocations, invocation) = match setup() {
        Ok(value) => value,
        Err(code) => return (Err(code), Err(code)),
    };
    if !matches!(
        invocations.request_cancellation(invocation, "operator_request"),
        Ok(true)
    ) {
        return (
            Err("cancellation_request_failed"),
            Err("cancellation_request_failed"),
        );
    }
    let token = CancellationToken::new();
    let cooperative = match invocations.sync_cancellation(invocation, &token) {
        Ok(true) if token.is_cancelled() => Ok(()),
        Ok(_) => Err("cancellation_did_not_reach_token"),
        Err(_) => Err("cancellation_sync_failed"),
    };
    let stale = match invocations.settle(&integrity_result(invocation, AgentResultKind::Succeeded))
    {
        Err(KernelError::Tool(reason))
            if reason == "cancelled agent invocation rejects late non-cancel outcome" =>
        {
            match invocations.settle(&integrity_result(invocation, AgentResultKind::Cancelled)) {
                Ok(()) => Ok(()),
                Err(_) => Err("cancelled_terminal_settlement_failed"),
            }
        }
        Err(_) => Err("late_success_failed_without_stale_fence_reason"),
        Ok(()) => Err("late_success_was_accepted"),
    };
    (cooperative, stale)
}

fn observe_gateway_dead_letter(home: &Path) -> std::result::Result<(), &'static str> {
    let gateway_home = home.join("gateway-eval");
    let message = enqueue(&gateway_home, "local", "fail", "offline", None)
        .map_err(|_| "gateway_enqueue_failed")?;
    for expected in ["retry_scheduled:", "retry_scheduled:", "dead_lettered:"] {
        let outcome = drain_one(&gateway_home, |_| Err("provider_unavailable".into()))
            .map_err(|_| "gateway_drain_failed")?
            .ok_or("gateway_message_missing")?;
        if !outcome.status.starts_with(expected) {
            return Err("gateway_retry_sequence_mismatch");
        }
    }
    match delivery_state(&gateway_home, &message.id) {
        Ok(Some((Some(reason), None))) if reason == "dead_lettered" => Ok(()),
        Ok(_) => Err("gateway_terminal_state_mismatch"),
        Err(_) => Err("gateway_state_read_failed"),
    }
}

/// Execute the six required offline integrity cases against isolated local state.
pub fn run_offline_integrity_suite(home: impl AsRef<Path>) -> Result<EvalReport, KernelError> {
    let run_home = home
        .as_ref()
        .join("integrity-runs")
        .join(Uuid::new_v4().to_string());
    if std::fs::create_dir_all(&run_home).is_err() {
        return evaluate_integrity_observations(
            REQUIRED_INTEGRITY_EVALS
                .iter()
                .map(|id| integrity_observation(id, "", Err("run_home_create_failed")))
                .collect(),
        );
    }
    let cancellation = observe_agent_cancellation(&run_home);
    evaluate_integrity_observations(vec![
        integrity_observation(
            "sensitivity_denial",
            "restricted write rejected under personal clearance",
            observe_sensitivity_denial(&run_home),
        ),
        integrity_observation(
            "smartdeny_approval",
            "RunCommand awaited approval and created no file",
            observe_smartdeny_approval(&run_home),
        ),
        integrity_observation(
            "route_policy_denial",
            "remote Codex route rejected under local-only privacy",
            observe_route_policy_denial(&run_home),
        ),
        integrity_observation(
            "cooperative_cancellation",
            "durable cancellation synchronized to kernel token",
            cancellation.0,
        ),
        integrity_observation(
            "stale_completion_fence",
            "late success rejected after cancellation request",
            cancellation.1,
        ),
        integrity_observation(
            "gateway_dead_letter",
            "third bounded failure produced dead-letter state",
            observe_gateway_dead_letter(&run_home),
        ),
    ])
}

impl EvalReport {
    pub fn all_ok(&self) -> bool {
        self.failed == 0
    }
}

/// Built-in offline suite (no network). Extensible later via JSON files.
pub fn builtin_suite() -> Vec<EvalCase> {
    vec![
        EvalCase {
            id: "offline-echo".into(),
            user: "ping".into(),
            steps: vec![CompletionResponse {
                text: Some("pong".into()),
                tool_calls: vec![],
            }],
            expect_tools: vec![],
            expect_text_contains: Some("pong".into()),
            stream_chunks: false,
        },
        EvalCase {
            id: "memory-then-answer".into(),
            user: "what editor?".into(),
            steps: vec![
                CompletionResponse {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "t1".into(),
                        name: "memory_recall".into(),
                        arguments: json!({
                            "subject": "user",
                            "predicate": "prefers_editor"
                        }),
                    }],
                },
                CompletionResponse {
                    text: Some("You prefer helix.".into()),
                    tool_calls: vec![],
                },
            ],
            expect_tools: vec!["memory_recall".into()],
            expect_text_contains: Some("helix".into()),
            stream_chunks: false,
        },
        EvalCase {
            id: "pack-activate-browser".into(),
            user: "need browser".into(),
            steps: vec![
                CompletionResponse {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "a1".into(),
                        name: "activate_pack".into(),
                        arguments: json!({"name": "browser"}),
                    }],
                },
                CompletionResponse {
                    text: Some("browser pack ready".into()),
                    tool_calls: vec![],
                },
            ],
            expect_tools: vec!["activate_pack".into()],
            expect_text_contains: Some("browser".into()),
            stream_chunks: false,
        },
        EvalCase {
            id: "write-file-job".into(),
            user: "write note".into(),
            steps: vec![
                CompletionResponse {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "w1".into(),
                        name: "write_file".into(),
                        arguments: json!({
                            "path": "notes/eval.txt",
                            "contents": "deterministic-write"
                        }),
                    }],
                },
                CompletionResponse {
                    text: Some("wrote notes/eval.txt".into()),
                    tool_calls: vec![],
                },
            ],
            expect_tools: vec!["write_file".into()],
            expect_text_contains: Some("wrote".into()),
            stream_chunks: false,
        },
    ]
}

pub fn run_case(home: impl AsRef<Path>, case: &EvalCase) -> Result<EvalCaseResult, KernelError> {
    let mut k = Kernel::open(home.as_ref(), KernelConfig::default())?;
    // Seed memory for the recall case (deterministic fixture).
    if case.id == "memory-then-answer" {
        k.remember_demo("user", "prefers_editor", "helix")?;
    }
    let mut model = ScriptedModel::new(case.steps.clone());
    model.stream_chunks = case.stream_chunks;
    let result: TurnResult = k.turn(&mut model, &case.user)?;

    let mut problems = Vec::new();
    for tool in &case.expect_tools {
        if !result.invoked_tools.contains(tool) {
            problems.push(format!("missing canonical tool invocation {tool:?}"));
        }
    }
    if let Some(sub) = &case.expect_text_contains {
        if !result.assistant_text.contains(sub) {
            problems.push(format!(
                "assistant_text missing {sub:?}: got {:?}",
                result.assistant_text
            ));
        }
    }

    Ok(EvalCaseResult {
        id: case.id.clone(),
        ok: problems.is_empty(),
        detail: if problems.is_empty() {
            "ok".into()
        } else {
            problems.join("; ")
        },
        tool_trace: result.tool_trace,
        assistant_text: result.assistant_text,
    })
}

pub fn run_suite(home: impl AsRef<Path>, cases: &[EvalCase]) -> EvalReport {
    let mut cases_out = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    for (i, case) in cases.iter().enumerate() {
        // Isolate each case under a subdir for determinism / no cross-talk.
        let case_home = home.as_ref().join(format!("case-{i}-{}", case.id));
        let _ = std::fs::create_dir_all(&case_home);
        match run_case(&case_home, case) {
            Ok(r) => {
                if r.ok {
                    passed += 1;
                } else {
                    failed += 1;
                }
                cases_out.push(r);
            }
            Err(e) => {
                failed += 1;
                cases_out.push(EvalCaseResult {
                    id: case.id.clone(),
                    ok: false,
                    detail: format!("kernel error: {e}"),
                    tool_trace: vec![],
                    assistant_text: String::new(),
                });
            }
        }
    }
    EvalReport {
        passed,
        failed,
        cases: cases_out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn builtin_suite_passes_offline() {
        let d = tempdir().unwrap();
        let report = run_suite(d.path(), &builtin_suite());
        assert!(
            report.all_ok(),
            "eval failed: {}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        assert_eq!(report.passed, 4);
    }
}
