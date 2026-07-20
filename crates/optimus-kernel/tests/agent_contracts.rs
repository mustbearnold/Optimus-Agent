use std::collections::BTreeSet;
use std::fs;

use optimus_graph::{Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_kernel::{
    AgentArtifactRef, AgentBudget, AgentContextRef, AgentDescriptor, AgentFailure, AgentId,
    AgentInvocationStatus, AgentInvocationStore, AgentPermissions, AgentRegistry, AgentRequest,
    AgentResult, AgentResultKind, AgentVersion, CancellationToken, AGENT_REQUEST_SCHEMA_VERSION,
    AGENT_RESULT_SCHEMA_VERSION,
};
use optimus_packs::{builtin_catalog, DurableEffectProvenance, ToolId};
use optimus_runtime::{Runtime, RuntimeError};
use rusqlite::Connection;
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
        network_hosts: BTreeSet::from(["example.com".into()]),
        effects: BTreeSet::from(["write_file".into()]),
    }
}

fn descriptor() -> AgentDescriptor {
    AgentDescriptor {
        id: AgentId::parse("research_agent").unwrap(),
        version: AgentVersion::parse("1.2.0").unwrap(),
        responsibility: "Collect source-backed evidence without authorizing effects".into(),
        request_schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        result_schema_version: AGENT_RESULT_SCHEMA_VERSION,
        required_tools: vec![ToolId::new("read_file")],
        permissions: AgentPermissions {
            filesystem_roots: BTreeSet::from(["workspace".into()]),
            network_hosts: BTreeSet::new(),
            effects: BTreeSet::new(),
        },
    }
}

fn request() -> AgentRequest {
    AgentRequest {
        schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        agent_id: AgentId::parse("research_agent").unwrap(),
        agent_version: AgentVersion::parse("1.2.0").unwrap(),
        task: "Inspect the bounded source set".into(),
        context: vec![AgentContextRef {
            source_id: "file:README.md".into(),
            sha256: "a".repeat(64),
        }],
        constraints: vec!["Do not perform effects".into()],
        tools: vec![ToolId::new("read_file")],
        permissions: AgentPermissions {
            filesystem_roots: BTreeSet::from(["workspace".into()]),
            network_hosts: BTreeSet::new(),
            effects: BTreeSet::new(),
        },
        budget: AgentBudget {
            max_steps: 8,
            timeout_ms: 30_000,
            max_context_chars: 100_000,
            max_output_chars: 20_000,
        },
        cancellation_id: Uuid::new_v4(),
        trace_id: Uuid::new_v4(),
    }
}

#[test]
fn canonical_agent_identity_and_version_reject_noncanonical_values() {
    assert_eq!(
        AgentId::parse("research_agent").unwrap().as_str(),
        "research_agent"
    );
    assert!(AgentId::parse("Research Agent").is_err());
    assert!(AgentId::parse("").is_err());
    assert_eq!(AgentVersion::parse("1.2.0").unwrap().as_str(), "1.2.0");
    assert!(AgentVersion::parse("1.02.0").is_err());
    assert!(AgentVersion::parse("1.2").is_err());
}

#[test]
fn request_is_versioned_bounded_catalog_aware_and_roundtrips() {
    let tools = available_tools();
    assert!(tools.contains(&ToolId::new("read_file")));
    let request = request();
    request.validate(&tools, &permissions()).unwrap();
    let encoded = serde_json::to_string(&request).unwrap();
    let decoded: AgentRequest = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, request);

    let mut invalid = request.clone();
    invalid.tools.push(ToolId::new("read_file"));
    assert!(invalid.validate(&tools, &permissions()).is_err());
    let mut invalid = request.clone();
    invalid.context[0].sha256 = "not-a-hash".into();
    assert!(invalid.validate(&tools, &permissions()).is_err());
    let mut invalid = request;
    invalid.permissions.effects.insert("run_command".into());
    assert!(invalid.validate(&tools, &permissions()).is_err());
}

#[test]
fn result_terminal_kind_and_evidence_are_consistent() {
    let result = AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id: Uuid::new_v4(),
        kind: AgentResultKind::Succeeded,
        summary: "bounded result".into(),
        error: None,
        cancellation_reason: None,
        evidence: vec![AgentContextRef {
            source_id: "file:README.md".into(),
            sha256: "b".repeat(64),
        }],
        artifacts: vec![AgentArtifactRef {
            uri: "artifact:report".into(),
            sha256: "c".repeat(64),
        }],
        unresolved: vec![],
    };
    result.validate().unwrap();
    let decoded: AgentResult =
        serde_json::from_str(&serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(decoded, result);

    let mut invalid = result.clone();
    invalid.error = Some(AgentFailure {
        code: "unexpected".into(),
        message: "success cannot contain failure".into(),
        retryable: false,
    });
    assert!(invalid.validate().is_err());
    let mut invalid = result.clone();
    invalid.kind = AgentResultKind::Cancelled;
    assert!(invalid.validate().is_err());
    let mut invalid = result;
    invalid.kind = AgentResultKind::Ambiguous;
    assert!(invalid.validate().is_err());
}

#[test]
fn registry_is_immutable_catalog_checked_ordered_and_reopenable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("agents.db");
    let tools = available_tools();
    let registry = AgentRegistry::open(&path, tools.clone(), permissions()).unwrap();
    let first = descriptor();
    registry.register(&first).unwrap();
    assert!(registry.register(&first).is_err());

    let mut second = descriptor();
    second.id = AgentId::parse("analysis_agent").unwrap();
    registry.register(&second).unwrap();
    let listed = registry.list().unwrap();
    assert_eq!(listed[0].id.as_str(), "analysis_agent");
    assert_eq!(listed[1].id.as_str(), "research_agent");
    drop(registry);

    let reopened = AgentRegistry::open(&path, tools, permissions()).unwrap();
    assert_eq!(
        reopened.get(&first.id, &first.version).unwrap(),
        Some(first)
    );
}

#[test]
fn registry_rejects_unavailable_tools_broader_permissions_and_corruption() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("agents.db");
    let tools = available_tools();
    let registry = AgentRegistry::open(&path, tools.clone(), permissions()).unwrap();

    let mut unavailable = descriptor();
    unavailable.required_tools = vec![ToolId::new("not_a_canonical_tool")];
    assert!(registry.register(&unavailable).is_err());
    let mut broad = descriptor();
    broad.permissions.effects.insert("run_command".into());
    assert!(registry.register(&broad).is_err());
    assert!(registry.list().unwrap().is_empty());

    let valid = descriptor();
    registry.register(&valid).unwrap();
    drop(registry);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE agent_registry SET descriptor_json='{\"corrupt\":true}'",
            [],
        )
        .unwrap();
    drop(connection);
    let reopened = AgentRegistry::open(path, tools, permissions()).unwrap();
    assert!(reopened.get(&valid.id, &valid.version).is_err());
    assert!(reopened.list().is_err());
}

fn succeeded(invocation_id: Uuid) -> AgentResult {
    AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id,
        kind: AgentResultKind::Succeeded,
        summary: "completed".into(),
        error: None,
        cancellation_reason: None,
        evidence: vec![],
        artifacts: vec![],
        unresolved: vec![],
    }
}

#[test]
fn invocation_persists_ordered_events_and_exactly_one_terminal_result() {
    let dir = tempdir().unwrap();
    let registry_path = dir.path().join("registry.db");
    let invocation_path = dir.path().join("invocations.db");
    let registry = AgentRegistry::open(&registry_path, available_tools(), permissions()).unwrap();
    registry.register(&descriptor()).unwrap();
    let store = AgentInvocationStore::open(&invocation_path).unwrap();
    let id = store.begin(&registry, &request()).unwrap();
    assert_eq!(
        store.get(id).unwrap().status,
        AgentInvocationStatus::Running
    );
    assert_eq!(store.events(id).unwrap()[0].kind, "accepted");

    store.settle(&succeeded(id)).unwrap();
    assert!(store.settle(&succeeded(id)).is_err());
    let events = store.events(id).unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["accepted", "succeeded"]
    );
    assert!(events.windows(2).all(|pair| pair[0].seq < pair[1].seq));
    drop(store);

    let reopened = AgentInvocationStore::open(&invocation_path).unwrap();
    let invocation = reopened.get(id).unwrap();
    assert_eq!(invocation.status, AgentInvocationStatus::Succeeded);
    assert_eq!(invocation.result.unwrap().kind, AgentResultKind::Succeeded);
}

#[test]
fn cancellation_fences_late_completion_syncs_token_and_retry_has_new_identity() {
    let dir = tempdir().unwrap();
    let registry = AgentRegistry::open(
        dir.path().join("registry.db"),
        available_tools(),
        permissions(),
    )
    .unwrap();
    registry.register(&descriptor()).unwrap();
    let store = AgentInvocationStore::open(dir.path().join("invocations.db")).unwrap();
    let id = store.begin(&registry, &request()).unwrap();
    assert!(store.request_cancellation(id, "operator_request").unwrap());
    assert!(!store.request_cancellation(id, "duplicate").unwrap());
    let token = CancellationToken::new();
    assert!(store.sync_cancellation(id, &token).unwrap());
    assert!(token.is_cancelled());
    assert!(store.settle(&succeeded(id)).is_err());

    let cancelled = AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id: id,
        kind: AgentResultKind::Cancelled,
        summary: "cancelled cooperatively".into(),
        error: None,
        cancellation_reason: Some("operator_request".into()),
        evidence: vec![],
        artifacts: vec![],
        unresolved: vec![],
    };
    store.settle(&cancelled).unwrap();
    assert_eq!(
        store.get(id).unwrap().status,
        AgentInvocationStatus::Cancelled
    );

    let retry = store.begin_retry(&registry, id, &request()).unwrap();
    assert_ne!(retry, id);
    assert_eq!(store.get(retry).unwrap().retry_of, Some(id));
}

#[test]
fn ambiguous_is_a_distinct_terminal_outcome() {
    let dir = tempdir().unwrap();
    let registry = AgentRegistry::open(
        dir.path().join("registry.db"),
        available_tools(),
        permissions(),
    )
    .unwrap();
    registry.register(&descriptor()).unwrap();
    let store = AgentInvocationStore::open(dir.path().join("invocations.db")).unwrap();
    let id = store.begin(&registry, &request()).unwrap();
    let result = AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id: id,
        kind: AgentResultKind::Ambiguous,
        summary: "external settlement unknown".into(),
        error: None,
        cancellation_reason: None,
        evidence: vec![],
        artifacts: vec![],
        unresolved: vec!["external effect status".into()],
    };
    store.settle(&result).unwrap();
    assert_eq!(
        store.get(id).unwrap().status,
        AgentInvocationStatus::Ambiguous
    );
}

#[test]
fn registry_membership_does_not_broaden_request_permissions() {
    let dir = tempdir().unwrap();
    let registry = AgentRegistry::open(
        dir.path().join("registry.db"),
        available_tools(),
        permissions(),
    )
    .unwrap();
    registry.register(&descriptor()).unwrap();
    let invocation_path = dir.path().join("invocations.db");
    let store = AgentInvocationStore::open(&invocation_path).unwrap();
    let mut broad = request();
    broad.permissions.effects.insert("write_file".into());
    assert!(store.begin(&registry, &broad).is_err());
    let connection = Connection::open(invocation_path).unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM agent_invocations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn effect_link_requires_exact_terminal_runtime_provenance() {
    let dir = tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let runtime = Runtime::open(&dir.path().join("runtime.db"), &workspace).unwrap();
    let job = runtime
        .create_job(JobSpec {
            label: "agent-effect".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write".into(),
                effect: Effect::WriteFile {
                    relative_path: "agent.txt".into(),
                    contents: "verified\n".into(),
                },
            }],
        })
        .unwrap();
    runtime.run_next(job).unwrap();
    let outcome = runtime.latest_effect_outcome(job).unwrap().unwrap();
    let provenance = DurableEffectProvenance {
        job_id: outcome.job_id.0,
        node_id: outcome.node_id,
        effect_attempt_id: outcome.attempt_id,
        effect_sha256: outcome.effect_hash,
        receipt_sha256: outcome.receipt_hash,
    };

    let registry = AgentRegistry::open(
        dir.path().join("registry.db"),
        available_tools(),
        permissions(),
    )
    .unwrap();
    registry.register(&descriptor()).unwrap();
    let store = AgentInvocationStore::open(dir.path().join("invocations.db")).unwrap();
    let invocation = store.begin(&registry, &request()).unwrap();
    store
        .link_effect(&runtime, invocation, &provenance)
        .unwrap();
    assert_eq!(
        store.events(invocation).unwrap().last().unwrap().kind,
        "effect_linked"
    );

    let mut false_provenance = provenance;
    false_provenance.effect_sha256 = "f".repeat(64);
    assert!(store
        .link_effect(&runtime, invocation, &false_provenance)
        .is_err());
}

#[test]
fn registered_agent_cannot_bypass_smart_deny() {
    let dir = tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let runtime = Runtime::open_with_config(
        &dir.path().join("runtime.db"),
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    let job = runtime
        .create_job(JobSpec {
            label: "agent-denied-effect".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "command".into(),
                effect: Effect::RunCommand {
                    program: "cmd".into(),
                    args: vec!["/C".into(), "echo bypass>denied.txt".into()],
                },
            }],
        })
        .unwrap();
    assert!(matches!(
        runtime.run_next(job),
        Err(RuntimeError::NeedsApproval { .. })
    ));
    assert!(!workspace.join("denied.txt").exists());
    assert!(runtime.latest_effect_outcome(job).unwrap().is_none());

    let registry = AgentRegistry::open(
        dir.path().join("registry.db"),
        available_tools(),
        permissions(),
    )
    .unwrap();
    registry.register(&descriptor()).unwrap();
    let store = AgentInvocationStore::open(dir.path().join("invocations.db")).unwrap();
    let invocation = store.begin(&registry, &request()).unwrap();
    let fabricated = DurableEffectProvenance {
        job_id: job.0,
        node_id: Uuid::new_v4(),
        effect_attempt_id: Uuid::new_v4(),
        effect_sha256: "a".repeat(64),
        receipt_sha256: None,
    };
    assert!(store
        .link_effect(&runtime, invocation, &fabricated)
        .is_err());
}
