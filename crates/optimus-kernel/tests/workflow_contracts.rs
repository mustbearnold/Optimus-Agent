use std::collections::{BTreeMap, BTreeSet};

use optimus_kernel::{
    adapt_campaign_status, adapt_cron_attempt_status, adapt_gateway_status, adapt_job_status,
    builtin_workflow_adapters, AdapterCapability, AdapterLifecycleStatus, ApprovalPolicy,
    CancellationPolicy, CapabilitySupport, RetryPolicy, RollbackPolicy, WorkflowAdapterKind,
    WorkflowDefinition, WorkflowId, WorkflowNode, WorkflowObservability, WorkflowPort,
    WorkflowRegistry, WorkflowTerminalKind, WorkflowTerminalPolicy, WorkflowTrigger,
    WorkflowVersion, WORKFLOW_SCHEMA_VERSION,
};
use optimus_runtime::{CampaignStatus, JobStatus};
use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;

fn terminals() -> BTreeSet<WorkflowTerminalKind> {
    BTreeSet::from([
        WorkflowTerminalKind::Succeeded,
        WorkflowTerminalKind::Failed,
        WorkflowTerminalKind::Cancelled,
        WorkflowTerminalKind::Ambiguous,
    ])
}

fn node(id: &str, dependencies: &[&str]) -> WorkflowNode {
    WorkflowNode {
        id: id.into(),
        adapter: WorkflowAdapterKind::Job,
        agent: None,
        dependencies: dependencies.iter().map(|value| (*value).into()).collect(),
        retry: RetryPolicy {
            max_attempts: 3,
            backoff_ms: 100,
            retryable: BTreeSet::from([
                WorkflowTerminalKind::Failed,
                WorkflowTerminalKind::Ambiguous,
            ]),
        },
        timeout_ms: 30_000,
        cancellation: CancellationPolicy::Cooperative,
        approval: ApprovalPolicy::None,
        rollback: RollbackPolicy::Unsupported,
    }
}

fn definition() -> WorkflowDefinition {
    WorkflowDefinition {
        schema_version: WORKFLOW_SCHEMA_VERSION,
        id: WorkflowId::parse("evidence_pipeline").unwrap(),
        version: WorkflowVersion::parse("1.0.0").unwrap(),
        description: "Collect evidence then write one bounded report".into(),
        triggers: vec![WorkflowTrigger::Manual],
        inputs: vec![WorkflowPort {
            name: "question".into(),
            schema: json!({"type":"string"}),
        }],
        outputs: vec![WorkflowPort {
            name: "report".into(),
            schema: json!({"type":"object"}),
        }],
        nodes: vec![node("collect", &[]), node("report", &["collect"])],
        terminal: WorkflowTerminalPolicy {
            handled: terminals(),
        },
        observability: WorkflowObservability {
            trace_required: true,
            event_classes: BTreeSet::from(["accepted".into(), "running".into(), "terminal".into()]),
        },
    }
}

#[test]
fn workflow_identity_version_and_definition_roundtrip() {
    assert!(WorkflowId::parse("Evidence Pipeline").is_err());
    assert!(WorkflowVersion::parse("1.00.0").is_err());
    let definition = definition();
    definition.validate().unwrap();
    let decoded: WorkflowDefinition =
        serde_json::from_str(&serde_json::to_string(&definition).unwrap()).unwrap();
    assert_eq!(decoded, definition);
}

#[test]
fn workflow_rejects_invalid_ports_triggers_and_schema_version() {
    let mut invalid = definition();
    invalid.schema_version += 1;
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.triggers.clear();
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.inputs.push(invalid.inputs[0].clone());
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.outputs[0].schema = json!({"description":"missing type"});
    assert!(invalid.validate().is_err());
}

#[test]
fn workflow_rejects_missing_self_duplicate_and_cyclic_dependencies() {
    let mut invalid = definition();
    invalid.nodes[1].dependencies = vec!["missing".into()];
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.nodes[1].dependencies = vec!["report".into()];
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.nodes[1].dependencies = vec!["collect".into(), "collect".into()];
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.nodes[0].dependencies = vec!["report".into()];
    assert!(invalid.validate().is_err());
}

#[test]
fn workflow_rejects_unbounded_retry_timeout_approval_and_terminal_contracts() {
    let mut invalid = definition();
    invalid.nodes[0].retry.max_attempts = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.nodes[0].timeout_ms = 0;
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.nodes[0].approval = ApprovalPolicy::Required {
        effect_kinds: BTreeSet::new(),
    };
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid
        .terminal
        .handled
        .remove(&WorkflowTerminalKind::Ambiguous);
    assert!(invalid.validate().is_err());
    let mut invalid = definition();
    invalid.observability.trace_required = false;
    assert!(invalid.validate().is_err());
}

#[test]
fn workflow_registry_is_immutable_ordered_reopenable_and_corruption_safe() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("workflows.db");
    let registry = WorkflowRegistry::open(&path).unwrap();
    let later = definition();
    registry.register(&later).unwrap();
    assert!(registry.register(&later).is_err());
    let mut earlier = definition();
    earlier.id = WorkflowId::parse("analysis_pipeline").unwrap();
    registry.register(&earlier).unwrap();
    assert_eq!(registry.list().unwrap()[0].id, earlier.id);
    drop(registry);

    let reopened = WorkflowRegistry::open(&path).unwrap();
    assert_eq!(
        reopened.get(&later.id, &later.version).unwrap(),
        Some(later.clone())
    );
    drop(reopened);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE workflow_registry SET definition_json='{\"corrupt\":true}'
             WHERE workflow_id=?1",
            [later.id.as_str()],
        )
        .unwrap();
    drop(connection);
    let reopened = WorkflowRegistry::open(path).unwrap();
    assert!(reopened.get(&later.id, &later.version).is_err());
    assert!(reopened.list().is_err());
}

#[test]
fn builtin_adapters_are_complete_and_preserve_unsupported_capabilities() {
    let adapters = builtin_workflow_adapters();
    assert_eq!(adapters.len(), 4);
    for adapter in &adapters {
        adapter.validate().unwrap();
        assert_eq!(adapter.terminal_outcomes, terminals());
        assert_eq!(
            adapter.capabilities[&AdapterCapability::Observability],
            CapabilitySupport::Supported
        );
        assert_eq!(
            adapter.capabilities[&AdapterCapability::Cancellation],
            CapabilitySupport::Supported
        );
    }
    let by_kind: BTreeMap<_, _> = adapters
        .into_iter()
        .map(|adapter| (format!("{:?}", adapter.kind), adapter))
        .collect();
    assert_eq!(
        by_kind["Job"].capabilities[&AdapterCapability::Retry],
        CapabilitySupport::Unsupported
    );
    assert_eq!(
        by_kind["Campaign"].capabilities[&AdapterCapability::Approval],
        CapabilitySupport::Supported
    );
    assert_eq!(
        by_kind["Cron"].capabilities[&AdapterCapability::Approval],
        CapabilitySupport::Unsupported
    );
    assert_eq!(
        by_kind["Gateway"].capabilities[&AdapterCapability::DeadLetter],
        CapabilitySupport::Supported
    );
}

#[test]
fn adapter_status_mappings_are_exact_and_unknown_strings_fail_closed() {
    assert_eq!(
        adapt_job_status(JobStatus::Pending),
        AdapterLifecycleStatus::Pending
    );
    assert_eq!(
        adapt_job_status(JobStatus::AwaitingApproval),
        AdapterLifecycleStatus::AwaitingApproval
    );
    assert_eq!(
        adapt_job_status(JobStatus::Interrupted),
        AdapterLifecycleStatus::Ambiguous
    );
    assert_eq!(
        adapt_campaign_status(CampaignStatus::Cancelled),
        AdapterLifecycleStatus::Cancelled
    );
    assert_eq!(
        adapt_cron_attempt_status("released").unwrap(),
        AdapterLifecycleStatus::Ambiguous
    );
    assert!(adapt_cron_attempt_status("invented").is_err());
    assert_eq!(
        adapt_gateway_status("claimed", None).unwrap(),
        AdapterLifecycleStatus::Running
    );
    assert_eq!(
        adapt_gateway_status("failed", Some("cancelled")).unwrap(),
        AdapterLifecycleStatus::Cancelled
    );
    assert!(adapt_gateway_status("invented", None).is_err());
}
