//! Built-in multi-agent verticals and registered DAG execution (P10 / ADR-0033).
//!
//! Specialists:
//! - `workspace_writer@1.0.0` — durable SmartDeny `WriteFile`
//! - `workspace_reader@1.0.0` — bounded workspace read + handoff artifact
//!
//! Workflows:
//! - `write_file_handoff@1.0.0` — single write node
//! - `read_file_handoff@1.0.0` — single read node
//! - `write_then_read_handoff@1.0.0` — write → read DAG
//!
//! Execution uses `WorkflowRunStore` (lease, node projections, child links,
//! exactly-one terminal) and never bypasses Work Graph SmartDeny for writes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use optimus_graph::{Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_packs::{DurableEffectProvenance, ToolId};
use optimus_runtime::{JobId, Runtime, RuntimeError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    adapt_job_status, is_denied_name, AgentArtifactRef, AgentBudget, AgentDescriptor, AgentFailure,
    AgentId, AgentInvocationStore, AgentPermissions, AgentRegistry, AgentRequest, AgentResult,
    AgentResultKind, AgentVersion, ArtifactRecord, ArtifactStore, CancellationToken, FsRoots,
    KernelError, Result, WorkflowAdapterKind, WorkflowAgentRef, WorkflowDefinition, WorkflowId,
    WorkflowNode, WorkflowNodeRun, WorkflowObservability, WorkflowPort, WorkflowRegistry,
    WorkflowRun, WorkflowRunChild, WorkflowRunLease, WorkflowRunStatus, WorkflowRunStore,
    WorkflowTerminalKind, WorkflowTerminalPolicy, WorkflowTrigger, WorkflowVersion,
    AGENT_REQUEST_SCHEMA_VERSION, AGENT_RESULT_SCHEMA_VERSION, WORKFLOW_SCHEMA_VERSION,
    ApprovalPolicy, CancellationPolicy, RetryPolicy, RollbackPolicy,
};
pub const WORKSPACE_WRITER_ID: &str = "workspace_writer";
pub const WORKSPACE_WRITER_VERSION: &str = "1.0.0";
pub const WORKSPACE_READER_ID: &str = "workspace_reader";
pub const WORKSPACE_READER_VERSION: &str = "1.0.0";
pub const WRITE_FILE_HANDOFF_WORKFLOW_ID: &str = "write_file_handoff";
pub const WRITE_FILE_HANDOFF_WORKFLOW_VERSION: &str = "1.0.0";
pub const READ_FILE_HANDOFF_WORKFLOW_ID: &str = "read_file_handoff";
pub const READ_FILE_HANDOFF_WORKFLOW_VERSION: &str = "1.0.0";
pub const WRITE_THEN_READ_HANDOFF_WORKFLOW_ID: &str = "write_then_read_handoff";
pub const WRITE_THEN_READ_HANDOFF_WORKFLOW_VERSION: &str = "1.0.0";

const MAX_WRITE_BYTES: usize = 256 * 1024;
const MAX_READ_BYTES: usize = 256 * 1024;
const MAX_RELATIVE_PATH: usize = 512;
const LEASE_OWNER: &str = "optimus-dag-executor";

/// Built-in specialist: write one bounded workspace file under runtime policy.
pub fn workspace_writer_descriptor() -> Result<AgentDescriptor> {
    Ok(AgentDescriptor {
        id: AgentId::parse(WORKSPACE_WRITER_ID)?,
        version: AgentVersion::parse(WORKSPACE_WRITER_VERSION)?,
        responsibility:
            "Write a single relative-path workspace file through durable SmartDeny effects; no shell, no network."
                .into(),
        request_schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        result_schema_version: AGENT_RESULT_SCHEMA_VERSION,
        required_tools: vec![ToolId::new("write_file")],
        permissions: AgentPermissions {
            filesystem_roots: BTreeSet::from(["workspace".into()]),
            network_hosts: BTreeSet::new(),
            effects: BTreeSet::from(["write_file".into()]),
        },
    })
}

/// Built-in specialist: read one bounded workspace file and publish a handoff artifact.
pub fn workspace_reader_descriptor() -> Result<AgentDescriptor> {
    Ok(AgentDescriptor {
        id: AgentId::parse(WORKSPACE_READER_ID)?,
        version: AgentVersion::parse(WORKSPACE_READER_VERSION)?,
        responsibility:
            "Read a single relative-path workspace file and publish a content-addressed handoff artifact; no write, shell, or network."
                .into(),
        request_schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        result_schema_version: AGENT_RESULT_SCHEMA_VERSION,
        required_tools: vec![ToolId::new("read_file")],
        permissions: AgentPermissions {
            filesystem_roots: BTreeSet::from(["workspace".into()]),
            network_hosts: BTreeSet::new(),
            effects: BTreeSet::from(["read_file".into()]),
        },
    })
}

fn standard_node(
    id: &str,
    agent_id: &str,
    agent_version: &str,
    dependencies: Vec<String>,
    effect_kinds: BTreeSet<String>,
) -> Result<WorkflowNode> {
    Ok(WorkflowNode {
        id: id.into(),
        adapter: WorkflowAdapterKind::Job,
        agent: Some(WorkflowAgentRef {
            id: AgentId::parse(agent_id)?,
            version: AgentVersion::parse(agent_version)?,
        }),
        dependencies,
        retry: RetryPolicy {
            max_attempts: 1,
            backoff_ms: 0,
            retryable: BTreeSet::new(),
        },
        timeout_ms: 60_000,
        cancellation: CancellationPolicy::Cooperative,
        approval: if effect_kinds.is_empty() {
            ApprovalPolicy::None
        } else {
            ApprovalPolicy::Required {
                effect_kinds,
            }
        },
        rollback: RollbackPolicy::Unsupported,
    })
}

fn standard_observability() -> WorkflowObservability {
    WorkflowObservability {
        trace_required: true,
        event_classes: BTreeSet::from([
            "accepted".into(),
            "running".into(),
            "terminal".into(),
        ]),
    }
}

fn all_terminals() -> WorkflowTerminalPolicy {
    WorkflowTerminalPolicy {
        handled: BTreeSet::from([
            WorkflowTerminalKind::Succeeded,
            WorkflowTerminalKind::Failed,
            WorkflowTerminalKind::Cancelled,
            WorkflowTerminalKind::Ambiguous,
        ]),
    }
}

/// Built-in workflow: one Job node that binds the workspace_writer specialist.
pub fn write_file_handoff_workflow() -> Result<WorkflowDefinition> {
    Ok(WorkflowDefinition {
        schema_version: WORKFLOW_SCHEMA_VERSION,
        id: WorkflowId::parse(WRITE_FILE_HANDOFF_WORKFLOW_ID)?,
        version: WorkflowVersion::parse(WRITE_FILE_HANDOFF_WORKFLOW_VERSION)?,
        description:
            "Execute workspace_writer to durable-write a file, link effect provenance, and publish a handoff artifact."
                .into(),
        triggers: vec![WorkflowTrigger::Manual],
        inputs: vec![
            WorkflowPort {
                name: "relative_path".into(),
                schema: serde_json::json!({"type": "string", "maxLength": MAX_RELATIVE_PATH}),
            },
            WorkflowPort {
                name: "contents".into(),
                schema: serde_json::json!({"type": "string", "maxLength": MAX_WRITE_BYTES}),
            },
        ],
        outputs: vec![
            WorkflowPort {
                name: "relative_path".into(),
                schema: serde_json::json!({"type": "string"}),
            },
            WorkflowPort {
                name: "artifact_sha256".into(),
                schema: serde_json::json!({"type": "string"}),
            },
        ],
        nodes: vec![standard_node(
            "write",
            WORKSPACE_WRITER_ID,
            WORKSPACE_WRITER_VERSION,
            vec![],
            BTreeSet::from(["write_file".into()]),
        )?],
        terminal: all_terminals(),
        observability: standard_observability(),
    })
}

/// Built-in workflow: one read node that binds workspace_reader.
pub fn read_file_handoff_workflow() -> Result<WorkflowDefinition> {
    Ok(WorkflowDefinition {
        schema_version: WORKFLOW_SCHEMA_VERSION,
        id: WorkflowId::parse(READ_FILE_HANDOFF_WORKFLOW_ID)?,
        version: WorkflowVersion::parse(READ_FILE_HANDOFF_WORKFLOW_VERSION)?,
        description:
            "Execute workspace_reader to read a workspace file and publish a handoff artifact."
                .into(),
        triggers: vec![WorkflowTrigger::Manual],
        inputs: vec![WorkflowPort {
            name: "relative_path".into(),
            schema: serde_json::json!({"type": "string", "maxLength": MAX_RELATIVE_PATH}),
        }],
        outputs: vec![
            WorkflowPort {
                name: "relative_path".into(),
                schema: serde_json::json!({"type": "string"}),
            },
            WorkflowPort {
                name: "artifact_sha256".into(),
                schema: serde_json::json!({"type": "string"}),
            },
        ],
        nodes: vec![standard_node(
            "read",
            WORKSPACE_READER_ID,
            WORKSPACE_READER_VERSION,
            vec![],
            BTreeSet::new(),
        )?],
        terminal: all_terminals(),
        observability: standard_observability(),
    })
}

/// Built-in DAG: write then read handoff (dependency order proof).
pub fn write_then_read_handoff_workflow() -> Result<WorkflowDefinition> {
    Ok(WorkflowDefinition {
        schema_version: WORKFLOW_SCHEMA_VERSION,
        id: WorkflowId::parse(WRITE_THEN_READ_HANDOFF_WORKFLOW_ID)?,
        version: WorkflowVersion::parse(WRITE_THEN_READ_HANDOFF_WORKFLOW_VERSION)?,
        description:
            "Write via workspace_writer then read via workspace_reader; publish handoff artifacts; proves registered DAG order."
                .into(),
        triggers: vec![WorkflowTrigger::Manual],
        inputs: vec![
            WorkflowPort {
                name: "relative_path".into(),
                schema: serde_json::json!({"type": "string", "maxLength": MAX_RELATIVE_PATH}),
            },
            WorkflowPort {
                name: "contents".into(),
                schema: serde_json::json!({"type": "string", "maxLength": MAX_WRITE_BYTES}),
            },
        ],
        outputs: vec![
            WorkflowPort {
                name: "relative_path".into(),
                schema: serde_json::json!({"type": "string"}),
            },
            WorkflowPort {
                name: "write_artifact_sha256".into(),
                schema: serde_json::json!({"type": "string"}),
            },
            WorkflowPort {
                name: "read_artifact_sha256".into(),
                schema: serde_json::json!({"type": "string"}),
            },
        ],
        nodes: vec![
            standard_node(
                "write",
                WORKSPACE_WRITER_ID,
                WORKSPACE_WRITER_VERSION,
                vec![],
                BTreeSet::from(["write_file".into()]),
            )?,
            standard_node(
                "read",
                WORKSPACE_READER_ID,
                WORKSPACE_READER_VERSION,
                vec!["write".into()],
                BTreeSet::new(),
            )?,
        ],
        terminal: all_terminals(),
        observability: standard_observability(),
    })
}

/// Host permission ceiling used when seeding the built-in agent registry.
pub fn builtin_agent_permission_ceiling() -> AgentPermissions {
    AgentPermissions {
        filesystem_roots: BTreeSet::from(["workspace".into()]),
        network_hosts: BTreeSet::new(),
        effects: BTreeSet::from(["write_file".into(), "read_file".into()]),
    }
}

fn builtin_available_tools() -> BTreeSet<ToolId> {
    BTreeSet::from([ToolId::new("write_file"), ToolId::new("read_file")])
}

/// Open (or create) agent registry and ensure built-in specialists are registered.
pub fn open_seeded_agent_registry(path: impl AsRef<Path>) -> Result<AgentRegistry> {
    let registry = AgentRegistry::open(path, builtin_available_tools(), builtin_agent_permission_ceiling())?;
    for descriptor in [workspace_writer_descriptor()?, workspace_reader_descriptor()?] {
        if registry
            .get(&descriptor.id, &descriptor.version)?
            .is_none()
        {
            registry.register(&descriptor)?;
        }
    }
    Ok(registry)
}

/// Open (or create) workflow registry and ensure built-in workflows are registered.
pub fn open_seeded_workflow_registry(path: impl AsRef<Path>) -> Result<WorkflowRegistry> {
    let registry = WorkflowRegistry::open(path)?;
    for definition in [
        write_file_handoff_workflow()?,
        read_file_handoff_workflow()?,
        write_then_read_handoff_workflow()?,
    ] {
        if registry.get(&definition.id, &definition.version)?.is_none() {
            registry.register(&definition)?;
        }
    }
    Ok(registry)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteFileHandoffRequest {
    pub relative_path: String,
    pub contents: String,
    /// When true and SmartDeny blocks, grant the exact write and resume (tests/operator).
    #[serde(default)]
    pub auto_grant: bool,
    /// Runtime policy; default SmartDeny.
    #[serde(default)]
    pub policy: PolicyMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteFileHandoffReport {
    pub workflow_id: String,
    pub workflow_version: String,
    pub agent_id: String,
    pub agent_version: String,
    pub invocation_id: Uuid,
    pub job_id: Option<Uuid>,
    pub workflow_terminal: WorkflowTerminalKind,
    pub agent_result: AgentResult,
    pub relative_path: String,
    pub artifact: Option<ArtifactRecord>,
    pub adapter_status: String,
    /// Present when executed through the durable DAG runner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadFileHandoffRequest {
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDagRequest {
    pub workflow_id: String,
    pub workflow_version: String,
    pub inputs: serde_json::Value,
    #[serde(default)]
    pub auto_grant: bool,
    #[serde(default)]
    pub policy: PolicyMode,
    /// When set, cancellation is requested on the run immediately after begin
    /// (tests parent/child fence). Production callers use `cancel_workflow_run`.
    #[serde(default)]
    pub cancel_after_begin: bool,
    #[serde(default)]
    pub cancel_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDagReport {
    pub run_id: Uuid,
    pub workflow_id: String,
    pub workflow_version: String,
    pub status: WorkflowRunStatus,
    pub workflow_terminal: Option<WorkflowTerminalKind>,
    pub nodes: Vec<WorkflowNodeRun>,
    pub children: Vec<WorkflowRunChild>,
    pub relative_path: Option<String>,
    pub artifacts: Vec<ArtifactRecord>,
    pub summary: String,
}

/// Execute the built-in write-file handoff vertical end-to-end (DAG wrapper).
pub fn run_write_file_handoff(
    home: impl AsRef<Path>,
    request: WriteFileHandoffRequest,
) -> Result<WriteFileHandoffReport> {
    validate_write_inputs(&request.relative_path, &request.contents)?;
    let dag = run_registered_workflow(
        home,
        WorkflowDagRequest {
            workflow_id: WRITE_FILE_HANDOFF_WORKFLOW_ID.into(),
            workflow_version: WRITE_FILE_HANDOFF_WORKFLOW_VERSION.into(),
            inputs: serde_json::json!({
                "relative_path": request.relative_path,
                "contents": request.contents,
            }),
            auto_grant: request.auto_grant,
            policy: request.policy,
            cancel_after_begin: false,
            cancel_reason: None,
        },
    )?;
    compact_write_report(dag)
}

/// Execute the built-in read-file handoff vertical.
pub fn run_read_file_handoff(
    home: impl AsRef<Path>,
    request: ReadFileHandoffRequest,
) -> Result<WorkflowDagReport> {
    validate_relative_path(&request.relative_path)?;
    run_registered_workflow(
        home,
        WorkflowDagRequest {
            workflow_id: READ_FILE_HANDOFF_WORKFLOW_ID.into(),
            workflow_version: READ_FILE_HANDOFF_WORKFLOW_VERSION.into(),
            inputs: serde_json::json!({"relative_path": request.relative_path}),
            auto_grant: false,
            policy: PolicyMode::SmartDeny,
            cancel_after_begin: false,
            cancel_reason: None,
        },
    )
}

/// Execute write→read DAG vertical.
pub fn run_write_then_read_handoff(
    home: impl AsRef<Path>,
    request: WriteFileHandoffRequest,
) -> Result<WorkflowDagReport> {
    validate_write_inputs(&request.relative_path, &request.contents)?;
    run_registered_workflow(
        home,
        WorkflowDagRequest {
            workflow_id: WRITE_THEN_READ_HANDOFF_WORKFLOW_ID.into(),
            workflow_version: WRITE_THEN_READ_HANDOFF_WORKFLOW_VERSION.into(),
            inputs: serde_json::json!({
                "relative_path": request.relative_path,
                "contents": request.contents,
            }),
            auto_grant: request.auto_grant,
            policy: request.policy,
            cancel_after_begin: false,
            cancel_reason: None,
        },
    )
}

/// Run any seeded/registered built-in workflow definition through the DAG store.
pub fn run_registered_workflow(
    home: impl AsRef<Path>,
    request: WorkflowDagRequest,
) -> Result<WorkflowDagReport> {
    let home = home.as_ref();
    std::fs::create_dir_all(home)?;

    let workflow_id = WorkflowId::parse(request.workflow_id.clone())?;
    let workflow_version = WorkflowVersion::parse(request.workflow_version.clone())?;
    let workflow_registry = open_seeded_workflow_registry(home.join("workflow-registry.db"))?;
    let definition = workflow_registry
        .get(&workflow_id, &workflow_version)?
        .ok_or_else(|| KernelError::Model("workflow missing after seed".into()))?;
    definition.validate()?;

    let runs = WorkflowRunStore::open(home.join("workflow-runs.db"))?;
    let run_id = runs.begin(&definition, request.inputs.clone())?;
    if request.cancel_after_begin {
        let reason = request
            .cancel_reason
            .clone()
            .unwrap_or_else(|| "cancel_after_begin".into());
        let _ = runs.request_cancellation(run_id, &reason)?;
    }
    let mut lease = runs.claim_lease(run_id, LEASE_OWNER, None)?;

    let agent_registry = open_seeded_agent_registry(home.join("agent-registry.db"))?;
    let invocations = AgentInvocationStore::open(home.join("agent-invocations.db"))?;
    let workspace = home.join("workspace");
    std::fs::create_dir_all(&workspace)?;
    let runtime = Runtime::open_with_config(
        &home.join("optimus.db"),
        &workspace,
        RuntimeConfig {
            policy: request.policy,
        },
    )?;
    let artifacts = ArtifactStore::open(home)?;

    // If cancel already requested, settle immediately without children.
    if let Some(reason) = runs.cancellation_requested(run_id)? {
        return finalize_cancelled_run(
            home,
            &runs,
            &invocations,
            &runtime,
            run_id,
            &lease,
            &reason,
            &request.inputs,
        );
    }

    loop {
        lease = runs.renew_lease(run_id, &lease)?;
        if let Some(reason) = runs.cancellation_requested(run_id)? {
            return finalize_cancelled_run(
                home,
                &runs,
                &invocations,
                &runtime,
                run_id,
                &lease,
                &reason,
                &request.inputs,
            );
        }

        if runs.any_node_failed(run_id)? {
            let _ = fanout_cancel_children(&runs, &invocations, &runtime, run_id, "node failed")?;
            runs.mark_remaining_cancelled(run_id, &lease)?;
            runs.settle_terminal(
                run_id,
                &lease,
                WorkflowRunStatus::Failed,
                Some("node failed"),
            )?;
            break;
        }

        if runs.all_nodes_succeeded(run_id)? {
            runs.settle_terminal(run_id, &lease, WorkflowRunStatus::Succeeded, None)?;
            break;
        }

        let ready = runs.ready_nodes(run_id, &definition)?;
        if ready.is_empty() {
            // No ready nodes and not all succeeded → blocked or incomplete graph.
            runs.settle_terminal(
                run_id,
                &lease,
                WorkflowRunStatus::Failed,
                Some("no ready nodes before terminal"),
            )?;
            break;
        }

        // Sequential schedule: lexicographically first ready node.
        let mut ready = ready;
        ready.sort();
        let node_id = ready.remove(0);
        let node = definition
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or_else(|| KernelError::Model("ready node missing from definition".into()))?;

        match execute_node(
            home,
            &runs,
            &lease,
            run_id,
            node,
            &request,
            &agent_registry,
            &invocations,
            &runtime,
            &artifacts,
            &workspace,
        ) {
            Ok(NodeOutcome::Succeeded) => continue,
            Ok(NodeOutcome::AwaitingApproval { job_id: _ }) => {
                runs.mark_awaiting_approval(run_id, &lease, &node_id)?;
                // Map awaiting approval to failed terminal for run projection
                // when auto_grant is false (operator must re-run with grant).
                // Node already marked failed with approval_required.
                runs.settle_terminal(
                    run_id,
                    &lease,
                    WorkflowRunStatus::Failed,
                    Some("approval_required"),
                )?;
                break;
            }
            Ok(NodeOutcome::Failed) => {
                let _ = fanout_cancel_children(
                    &runs,
                    &invocations,
                    &runtime,
                    run_id,
                    "node failed",
                )?;
                let _ = runs.mark_remaining_cancelled(run_id, &lease);
                runs.settle_terminal(
                    run_id,
                    &lease,
                    WorkflowRunStatus::Failed,
                    Some("node failed"),
                )?;
                break;
            }
            Ok(NodeOutcome::Cancelled { reason }) => {
                return finalize_cancelled_run(
                    home,
                    &runs,
                    &invocations,
                    &runtime,
                    run_id,
                    &lease,
                    &reason,
                    &request.inputs,
                );
            }
            Err(error) => {
                let _ = runs.mark_node_failed(
                    run_id,
                    &lease,
                    &node_id,
                    "node_error",
                    &error.to_string(),
                );
                let _ = fanout_cancel_children(
                    &runs,
                    &invocations,
                    &runtime,
                    run_id,
                    "node error",
                )?;
                let _ = runs.mark_remaining_cancelled(run_id, &lease);
                runs.settle_terminal(
                    run_id,
                    &lease,
                    WorkflowRunStatus::Failed,
                    Some("node error"),
                )?;
                break;
            }
        }
    }

    build_dag_report(&runs, &artifacts, run_id, &request.inputs)
}

/// Request cancellation on a workflow run and fan out to child invocations/jobs.
pub fn cancel_workflow_run(
    home: impl AsRef<Path>,
    run_id: Uuid,
    reason: &str,
) -> Result<bool> {
    let home = home.as_ref();
    let runs = WorkflowRunStore::open(home.join("workflow-runs.db"))?;
    let requested = runs.request_cancellation(run_id, reason)?;
    if !requested {
        return Ok(false);
    }
    let invocations = AgentInvocationStore::open(home.join("agent-invocations.db"))?;
    // Always cancel child invocations first (even if Runtime/workspace unavailable).
    for child in runs.list_children(run_id)? {
        let _ = invocations.request_cancellation(child.invocation_id, reason)?;
    }
    let workspace = home.join("workspace");
    if workspace.is_dir() {
        if let Ok(runtime) = Runtime::open(&home.join("optimus.db"), &workspace) {
            for child in runs.list_children(run_id)? {
                if let Some(job) = child.job_id {
                    let _ = runtime.cancel_job(JobId(job));
                }
            }
        }
    }
    Ok(true)
}

/// Request cancellation on a running vertical invocation and cancel its job if known.
pub fn cancel_write_file_handoff(
    home: impl AsRef<Path>,
    invocation_id: Uuid,
    job_id: Option<Uuid>,
    reason: &str,
) -> Result<bool> {
    let home = home.as_ref();
    let invocations = AgentInvocationStore::open(home.join("agent-invocations.db"))?;
    let requested = invocations.request_cancellation(invocation_id, reason)?;
    if let Some(job) = job_id {
        let workspace = home.join("workspace");
        if workspace.is_dir() {
            if let Ok(runtime) = Runtime::open(&home.join("optimus.db"), &workspace) {
                let _ = runtime.cancel_job(JobId(job));
            }
        }
    }
    Ok(requested)
}

enum NodeOutcome {
    Succeeded,
    Failed,
    AwaitingApproval { #[allow(dead_code)] job_id: Uuid },
    Cancelled { reason: String },
}

#[allow(clippy::too_many_arguments)]
fn execute_node(
    home: &Path,
    runs: &WorkflowRunStore,
    lease: &WorkflowRunLease,
    run_id: Uuid,
    node: &WorkflowNode,
    request: &WorkflowDagRequest,
    agent_registry: &AgentRegistry,
    invocations: &AgentInvocationStore,
    runtime: &Runtime,
    artifacts: &ArtifactStore,
    workspace: &Path,
) -> Result<NodeOutcome> {
    runs.assert_can_begin_child(run_id)?;
    let agent_ref = node
        .agent
        .as_ref()
        .ok_or_else(|| KernelError::Model(format!("node {} requires an agent binding", node.id)))?;
    let agent_id = agent_ref.id.as_str();
    let agent_version = agent_ref.version.as_str();

    let relative_path = input_string(&request.inputs, "relative_path")?;
    validate_relative_path(&relative_path)?;

    let (tools, permissions, task) = match agent_id {
        WORKSPACE_WRITER_ID => {
            let contents = input_string(&request.inputs, "contents")?;
            if contents.len() > MAX_WRITE_BYTES {
                return Err(KernelError::Model(
                    "contents exceed workspace_writer bound".into(),
                ));
            }
            (
                vec![ToolId::new("write_file")],
                AgentPermissions {
                    filesystem_roots: BTreeSet::from(["workspace".into()]),
                    network_hosts: BTreeSet::new(),
                    effects: BTreeSet::from(["write_file".into()]),
                },
                format!(
                    "Write {} bytes to relative path {}",
                    contents.len(),
                    relative_path
                ),
            )
        }
        WORKSPACE_READER_ID => (
            vec![ToolId::new("read_file")],
            AgentPermissions {
                filesystem_roots: BTreeSet::from(["workspace".into()]),
                network_hosts: BTreeSet::new(),
                effects: BTreeSet::from(["read_file".into()]),
            },
            format!("Read relative path {relative_path} and publish handoff artifact"),
        ),
        other => {
            return Err(KernelError::Model(format!(
                "no built-in dispatch for specialist {other}"
            )))
        }
    };

    if agent_version
        != match agent_id {
            WORKSPACE_WRITER_ID => WORKSPACE_WRITER_VERSION,
            WORKSPACE_READER_ID => WORKSPACE_READER_VERSION,
            _ => "",
        }
    {
        return Err(KernelError::Model("agent version binding mismatch".into()));
    }

    let agent_request = AgentRequest {
        schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        agent_id: agent_ref.id.clone(),
        agent_version: agent_ref.version.clone(),
        task,
        context: vec![],
        constraints: vec![
            "Only registered tools".into(),
            "No shell or network".into(),
            format!("workflow_run:{run_id}"),
        ],
        tools,
        permissions,
        budget: AgentBudget {
            max_steps: 1,
            timeout_ms: node.timeout_ms,
            max_context_chars: 8_192,
            max_output_chars: 8_192,
        },
        cancellation_id: Uuid::new_v4(),
        trace_id: Uuid::new_v4(),
    };

    let invocation_id = invocations.begin(agent_registry, &agent_request)?;
    runs.link_child(run_id, &node.id, invocation_id, None)?;
    let token = CancellationToken::new();

    if invocations.sync_cancellation(invocation_id, &token)?
        || runs.cancellation_requested(run_id)?.is_some()
    {
        let reason = runs
            .cancellation_requested(run_id)?
            .unwrap_or_else(|| "cancellation observed before effect".into());
        settle_agent_cancelled(invocations, invocation_id, &reason)?;
        let _ = runs.mark_node_cancelled(run_id, lease, &node.id, &reason);
        return Ok(NodeOutcome::Cancelled { reason });
    }

    match agent_id {
        WORKSPACE_WRITER_ID => {
            let contents = input_string(&request.inputs, "contents")?;
            let job = runtime.create_job(JobSpec {
                label: format!("agent:{agent_id}:write"),
                budget: Default::default(),
                nodes: vec![NodeSpec {
                    label: "write_file".into(),
                    effect: Effect::WriteFile {
                        relative_path: relative_path.clone(),
                        contents: contents.clone(),
                    },
                }],
            })?;
            // Link job_id immediately so cancel fan-out can reach the job.
            runs.mark_node_running(run_id, lease, &node.id, invocation_id, Some(job.0))?;

            match runtime.run_next(job) {
                Ok(_) => {}
                Err(RuntimeError::NeedsApproval { .. }) => {
                    if request.auto_grant {
                        let _ = runtime.grant_and_resume(job)?;
                    } else {
                        let result = AgentResult {
                            schema_version: AGENT_RESULT_SCHEMA_VERSION,
                            invocation_id,
                            kind: AgentResultKind::Failed,
                            summary: "write requires SmartDeny approval".into(),
                            error: Some(AgentFailure {
                                code: "approval_required".into(),
                                message: format!("job {} awaiting exact-effect approval", job.0),
                                retryable: true,
                            }),
                            cancellation_reason: None,
                            evidence: vec![],
                            artifacts: vec![],
                            unresolved: vec![],
                        };
                        invocations.settle(&result)?;
                        runs.mark_node_failed(
                            run_id,
                            lease,
                            &node.id,
                            "approval_required",
                            &result.summary,
                        )?;
                        return Ok(NodeOutcome::AwaitingApproval { job_id: job.0 });
                    }
                }
                Err(RuntimeError::Cancelled { .. }) => {
                    let _ = invocations.request_cancellation(invocation_id, "runtime cancelled job")?;
                    settle_agent_cancelled(invocations, invocation_id, "runtime cancelled job")?;
                    runs.mark_node_failed(
                        run_id,
                        lease,
                        &node.id,
                        "cancelled",
                        "runtime cancelled job",
                    )?;
                    return Ok(NodeOutcome::Cancelled {
                        reason: "runtime cancelled job".into(),
                    });
                }
                Err(error) => {
                    let result = AgentResult {
                        schema_version: AGENT_RESULT_SCHEMA_VERSION,
                        invocation_id,
                        kind: AgentResultKind::Failed,
                        summary: "write effect failed".into(),
                        error: Some(AgentFailure {
                            code: "effect_failed".into(),
                            message: error.to_string(),
                            retryable: false,
                        }),
                        cancellation_reason: None,
                        evidence: vec![],
                        artifacts: vec![],
                        unresolved: vec![],
                    };
                    invocations.settle(&result)?;
                    runs.mark_node_failed(
                        run_id,
                        lease,
                        &node.id,
                        "effect_failed",
                        &error.to_string(),
                    )?;
                    return Ok(NodeOutcome::Failed);
                }
            }

            if invocations.sync_cancellation(invocation_id, &token)?
                || runs.cancellation_requested(run_id)?.is_some()
            {
                let _ = runtime.cancel_job(job);
                let reason = runs
                    .cancellation_requested(run_id)?
                    .unwrap_or_else(|| "cancellation observed after effect start".into());
                settle_agent_cancelled(invocations, invocation_id, &reason)?;
                runs.mark_node_cancelled(run_id, lease, &node.id, &reason)?;
                return Ok(NodeOutcome::Cancelled { reason });
            }

            let outcome = runtime.latest_effect_outcome(job)?.ok_or_else(|| {
                KernelError::Model("workspace_writer write produced no terminal effect".into())
            })?;
            if outcome.status != "succeeded" {
                let result = AgentResult {
                    schema_version: AGENT_RESULT_SCHEMA_VERSION,
                    invocation_id,
                    kind: AgentResultKind::Failed,
                    summary: format!("write terminal status {}", outcome.status),
                    error: Some(AgentFailure {
                        code: "effect_not_succeeded".into(),
                        message: format!("effect status {}", outcome.status),
                        retryable: false,
                    }),
                    cancellation_reason: None,
                    evidence: vec![],
                    artifacts: vec![],
                    unresolved: vec![],
                };
                invocations.settle(&result)?;
                runs.mark_node_failed(
                    run_id,
                    lease,
                    &node.id,
                    "effect_not_succeeded",
                    &outcome.status,
                )?;
                return Ok(NodeOutcome::Failed);
            }

            let provenance = DurableEffectProvenance {
                job_id: outcome.job_id.0,
                node_id: outcome.node_id,
                effect_attempt_id: outcome.attempt_id,
                effect_sha256: outcome.effect_hash,
                receipt_sha256: outcome.receipt_hash,
            };
            invocations.link_effect(runtime, invocation_id, &provenance)?;

            let artifact = artifacts.put_bytes(
                contents.as_bytes(),
                "text/plain",
                "workspace_writer",
                &relative_path,
                Some(&invocation_id.to_string()),
            )?;
            let result = AgentResult {
                schema_version: AGENT_RESULT_SCHEMA_VERSION,
                invocation_id,
                kind: AgentResultKind::Succeeded,
                summary: format!(
                    "wrote {} ({} bytes); handoff {}",
                    relative_path,
                    contents.len(),
                    artifact.sha256
                ),
                error: None,
                cancellation_reason: None,
                evidence: vec![],
                artifacts: vec![AgentArtifactRef {
                    uri: format!("artifact:{}", artifact.sha256),
                    sha256: artifact.sha256.clone(),
                }],
                unresolved: vec![],
            };
            // Fence late success if parent cancel raced in — never settle Succeeded after cancel.
            if runs.cancellation_requested(run_id)?.is_some()
                || invocations.sync_cancellation(invocation_id, &token)?
            {
                let reason = runs
                    .cancellation_requested(run_id)?
                    .unwrap_or_else(|| "parent cancel before settle".into());
                settle_agent_cancelled(invocations, invocation_id, &reason)?;
                let _ = runs.mark_node_cancelled(run_id, lease, &node.id, &reason);
                return Ok(NodeOutcome::Cancelled { reason });
            }
            invocations.settle(&result)?;
            runs.mark_node_succeeded(run_id, lease, &node.id, Some(artifact.sha256))?;
            let _ = home;
            let _ = adapt_job_status(runtime.job_status(job)?);
            Ok(NodeOutcome::Succeeded)
        }
        WORKSPACE_READER_ID => {
            runs.mark_node_running(run_id, lease, &node.id, invocation_id, None)?;
            if let Some(name) = Path::new(&relative_path)
                .file_name()
                .and_then(|n| n.to_str())
            {
                if is_denied_name(name) {
                    let result = AgentResult {
                        schema_version: AGENT_RESULT_SCHEMA_VERSION,
                        invocation_id,
                        kind: AgentResultKind::Failed,
                        summary: "secret basename denied".into(),
                        error: Some(AgentFailure {
                            code: "secret_denied".into(),
                            message: name.into(),
                            retryable: false,
                        }),
                        cancellation_reason: None,
                        evidence: vec![],
                        artifacts: vec![],
                        unresolved: vec![],
                    };
                    invocations.settle(&result)?;
                    runs.mark_node_failed(run_id, lease, &node.id, "secret_denied", name)?;
                    return Ok(NodeOutcome::Failed);
                }
            }
            let roots = FsRoots::new(vec![workspace.to_path_buf()]).map_err(|e| {
                KernelError::Model(format!("workspace roots unavailable: {e}"))
            })?;
            let abs = match roots.resolve_existing(&relative_path) {
                Ok(path) => path,
                Err(e) => {
                    let code = if e.to_string().contains("denied") || e.to_string().contains("Denied") {
                        "path_denied"
                    } else {
                        "file_not_found"
                    };
                    let result = AgentResult {
                        schema_version: AGENT_RESULT_SCHEMA_VERSION,
                        invocation_id,
                        kind: AgentResultKind::Failed,
                        summary: format!("workspace read failed: {e}"),
                        error: Some(AgentFailure {
                            code: code.into(),
                            message: e.to_string(),
                            retryable: false,
                        }),
                        cancellation_reason: None,
                        evidence: vec![],
                        artifacts: vec![],
                        unresolved: vec![],
                    };
                    invocations.settle(&result)?;
                    runs.mark_node_failed(run_id, lease, &node.id, code, &e.to_string())?;
                    return Ok(NodeOutcome::Failed);
                }
            };
            if abs.is_dir() {
                let result = AgentResult {
                    schema_version: AGENT_RESULT_SCHEMA_VERSION,
                    invocation_id,
                    kind: AgentResultKind::Failed,
                    summary: "path is a directory".into(),
                    error: Some(AgentFailure {
                        code: "not_a_file".into(),
                        message: relative_path.clone(),
                        retryable: false,
                    }),
                    cancellation_reason: None,
                    evidence: vec![],
                    artifacts: vec![],
                    unresolved: vec![],
                };
                invocations.settle(&result)?;
                runs.mark_node_failed(run_id, lease, &node.id, "not_a_file", &relative_path)?;
                return Ok(NodeOutcome::Failed);
            }
            let bytes = std::fs::read(&abs)?;
            if bytes.len() > MAX_READ_BYTES {
                let result = AgentResult {
                    schema_version: AGENT_RESULT_SCHEMA_VERSION,
                    invocation_id,
                    kind: AgentResultKind::Failed,
                    summary: "file exceeds read bound".into(),
                    error: Some(AgentFailure {
                        code: "read_too_large".into(),
                        message: format!("{} bytes", bytes.len()),
                        retryable: false,
                    }),
                    cancellation_reason: None,
                    evidence: vec![],
                    artifacts: vec![],
                    unresolved: vec![],
                };
                invocations.settle(&result)?;
                runs.mark_node_failed(run_id, lease, &node.id, "read_too_large", "")?;
                return Ok(NodeOutcome::Failed);
            }
            if invocations.sync_cancellation(invocation_id, &token)?
                || runs.cancellation_requested(run_id)?.is_some()
            {
                let reason = runs
                    .cancellation_requested(run_id)?
                    .unwrap_or_else(|| "cancellation during read".into());
                settle_agent_cancelled(invocations, invocation_id, &reason)?;
                runs.mark_node_cancelled(run_id, lease, &node.id, &reason)?;
                return Ok(NodeOutcome::Cancelled { reason });
            }
            let artifact = artifacts.put_bytes(
                &bytes,
                "text/plain",
                "workspace_reader",
                &relative_path,
                Some(&invocation_id.to_string()),
            )?;
            let result = AgentResult {
                schema_version: AGENT_RESULT_SCHEMA_VERSION,
                invocation_id,
                kind: AgentResultKind::Succeeded,
                summary: format!(
                    "read {} ({} bytes); handoff {}",
                    relative_path,
                    bytes.len(),
                    artifact.sha256
                ),
                error: None,
                cancellation_reason: None,
                evidence: vec![],
                artifacts: vec![AgentArtifactRef {
                    uri: format!("artifact:{}", artifact.sha256),
                    sha256: artifact.sha256.clone(),
                }],
                unresolved: vec![],
            };
            invocations.settle(&result)?;
            runs.mark_node_succeeded(run_id, lease, &node.id, Some(artifact.sha256))?;
            Ok(NodeOutcome::Succeeded)
        }
        _ => unreachable!(),
    }
}

fn settle_agent_cancelled(
    invocations: &AgentInvocationStore,
    invocation_id: Uuid,
    reason: &str,
) -> Result<()> {
    let _ = invocations.request_cancellation(invocation_id, reason);
    let result = AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id,
        kind: AgentResultKind::Cancelled,
        summary: "specialist cancelled".into(),
        error: None,
        cancellation_reason: Some(reason.into()),
        evidence: vec![],
        artifacts: vec![],
        unresolved: vec![],
    };
    // Idempotent-ish: ignore if already terminal.
    let _ = invocations.settle(&result);
    Ok(())
}

fn fanout_cancel_children(
    runs: &WorkflowRunStore,
    invocations: &AgentInvocationStore,
    runtime: &Runtime,
    run_id: Uuid,
    reason: &str,
) -> Result<usize> {
    let mut count = 0usize;
    for child in runs.list_children(run_id)? {
        if invocations
            .request_cancellation(child.invocation_id, reason)
            .unwrap_or(false)
        {
            count += 1;
        }
        if let Some(job) = child.job_id {
            let _ = runtime.cancel_job(JobId(job));
        }
    }
    Ok(count)
}

#[allow(clippy::too_many_arguments)]
fn finalize_cancelled_run(
    home: &Path,
    runs: &WorkflowRunStore,
    invocations: &AgentInvocationStore,
    runtime: &Runtime,
    run_id: Uuid,
    lease: &WorkflowRunLease,
    reason: &str,
    inputs: &serde_json::Value,
) -> Result<WorkflowDagReport> {
    let _ = fanout_cancel_children(runs, invocations, runtime, run_id, reason)?;
    let _ = runs.mark_remaining_cancelled(run_id, lease);
    runs.settle_terminal(
        run_id,
        lease,
        WorkflowRunStatus::Cancelled,
        Some(reason),
    )?;
    let artifacts = ArtifactStore::open(home)?;
    let mut report = build_dag_report(runs, &artifacts, run_id, inputs)?;
    report.summary = format!("cancelled: {reason}");
    Ok(report)
}

fn build_dag_report(
    runs: &WorkflowRunStore,
    artifacts: &ArtifactStore,
    run_id: Uuid,
    inputs: &serde_json::Value,
) -> Result<WorkflowDagReport> {
    let run = runs.get(run_id)?;
    let nodes = runs.list_nodes(run_id)?;
    let children = runs.list_children(run_id)?;
    let mut artifact_records = Vec::new();
    let listed = artifacts.list()?;
    for node in &nodes {
        if let Some(ref sha) = node.artifact_sha256 {
            if let Some(record) = listed.iter().find(|row| &row.sha256 == sha && !row.deleted) {
                artifact_records.push(record.clone());
            }
        }
    }
    let summary = format!(
        "workflow {}@{} status={:?} nodes={} artifacts={}",
        run.workflow_id,
        run.workflow_version,
        run.status,
        nodes.len(),
        artifact_records.len()
    );
    Ok(WorkflowDagReport {
        run_id,
        workflow_id: run.workflow_id,
        workflow_version: run.workflow_version,
        status: run.status,
        workflow_terminal: run.status.to_terminal_kind(),
        nodes,
        children,
        relative_path: inputs
            .get("relative_path")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        artifacts: artifact_records,
        summary,
    })
}

fn compact_write_report(dag: WorkflowDagReport) -> Result<WriteFileHandoffReport> {
    let write_node = dag
        .nodes
        .iter()
        .find(|n| n.node_id == "write")
        .cloned();
    let child = dag
        .children
        .iter()
        .find(|c| c.node_id == "write")
        .cloned();
    let invocation_id = child
        .as_ref()
        .map(|c| c.invocation_id)
        .or_else(|| write_node.as_ref().and_then(|n| n.invocation_id))
        .unwrap_or_else(|| Uuid::from_u128(0));
    let job_id = child
        .as_ref()
        .and_then(|c| c.job_id)
        .or_else(|| write_node.as_ref().and_then(|n| n.job_id));
    let artifact = dag.artifacts.first().cloned();
    let (kind, summary, error, cancellation_reason) = match dag.status {
        WorkflowRunStatus::Succeeded => {
            let path = dag.relative_path.clone().unwrap_or_default();
            let bytes = artifact.as_ref().map(|a| a.size_bytes).unwrap_or(0);
            let sha = artifact
                .as_ref()
                .map(|a| a.sha256.as_str())
                .unwrap_or("unknown");
            (
                AgentResultKind::Succeeded,
                format!("wrote {path} ({bytes} bytes); handoff {sha}"),
                None,
                None,
            )
        }
        WorkflowRunStatus::Cancelled => (
            AgentResultKind::Cancelled,
            "workspace_writer cancelled".into(),
            None,
            Some(dag.summary.clone()),
        ),
        WorkflowRunStatus::Failed
            if write_node
                .as_ref()
                .and_then(|n| n.error_code.as_deref())
                == Some("approval_required") =>
        {
            (
                AgentResultKind::Failed,
                "write requires SmartDeny approval".into(),
                Some(AgentFailure {
                    code: "approval_required".into(),
                    message: write_node
                        .as_ref()
                        .and_then(|n| n.error_message.clone())
                        .unwrap_or_else(|| "approval required".into()),
                    retryable: true,
                }),
                None,
            )
        }
        _ => (
            AgentResultKind::Failed,
            dag.summary.clone(),
            Some(AgentFailure {
                code: write_node
                    .as_ref()
                    .and_then(|n| n.error_code.clone())
                    .unwrap_or_else(|| "failed".into()),
                message: write_node
                    .as_ref()
                    .and_then(|n| n.error_message.clone())
                    .unwrap_or_else(|| dag.summary.clone()),
                retryable: false,
            }),
            None,
        ),
    };
    let agent_result = AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id,
        kind,
        summary: summary.clone(),
        error,
        cancellation_reason,
        evidence: vec![],
        artifacts: artifact
            .as_ref()
            .map(|a| {
                vec![AgentArtifactRef {
                    uri: format!("artifact:{}", a.sha256),
                    sha256: a.sha256.clone(),
                }]
            })
            .unwrap_or_default(),
        unresolved: vec![],
    };
    let workflow_terminal = dag
        .workflow_terminal
        .unwrap_or(WorkflowTerminalKind::Failed);
    Ok(WriteFileHandoffReport {
        workflow_id: dag.workflow_id,
        workflow_version: dag.workflow_version,
        agent_id: WORKSPACE_WRITER_ID.into(),
        agent_version: WORKSPACE_WRITER_VERSION.into(),
        invocation_id,
        job_id,
        workflow_terminal,
        agent_result,
        relative_path: dag.relative_path.unwrap_or_default(),
        artifact,
        adapter_status: format!("{:?}", dag.status),
        run_id: Some(dag.run_id),
    })
}

fn input_string(inputs: &serde_json::Value, key: &str) -> Result<String> {
    inputs
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| KernelError::Model(format!("workflow input `{key}` must be a string")))
}

fn validate_write_inputs(relative_path: &str, contents: &str) -> Result<()> {
    validate_relative_path(relative_path)?;
    if contents.len() > MAX_WRITE_BYTES {
        return Err(KernelError::Model(
            "contents exceed workspace_writer bound".into(),
        ));
    }
    Ok(())
}

fn validate_relative_path(relative_path: &str) -> Result<()> {
    if relative_path.is_empty() || relative_path.len() > MAX_RELATIVE_PATH {
        return Err(KernelError::Model(
            "relative_path is empty or exceeds bound".into(),
        ));
    }
    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err(KernelError::Model(
            "relative_path must be relative".into(),
        ));
    }
    for component in path.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(KernelError::Model(
                "relative_path must use normal components only".into(),
            ));
        }
    }
    Ok(())
}

/// Content digest helper for tests and CLI.
pub fn content_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Default workspace path under an Optimus home for the vertical.
pub fn vertical_workspace(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("workspace")
}

/// Open the durable workflow run store under an Optimus home.
pub fn open_workflow_run_store(home: impl AsRef<Path>) -> Result<WorkflowRunStore> {
    WorkflowRunStore::open(home.as_ref().join("workflow-runs.db"))
}

/// Load a workflow run projection.
pub fn get_workflow_run(home: impl AsRef<Path>, run_id: Uuid) -> Result<WorkflowRun> {
    open_workflow_run_store(home)?.get(run_id)
}
