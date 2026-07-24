//! Phase-3 multi-agent vertical: one built-in specialist + one executed workflow.
//!
//! ## Vertical: `write_file_handoff` → `workspace_writer`
//!
//! 1. Seed immutable agent + workflow registries with the built-in definitions.
//! 2. Begin a durable agent invocation for `workspace_writer@1.0.0`.
//! 3. Persist a Work Graph `WriteFile` effect under SmartDeny (or unrestricted).
//! 4. Link exact runtime provenance to the invocation.
//! 5. Publish a content-addressed handoff artifact.
//! 6. Settle exactly one agent terminal outcome; map it to a workflow terminal.
//!
//! Cancellation: a requested agent cancel fences late success settlement and can
//! cancel a pending/awaiting Work Graph job before the write lands.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use optimus_graph::{Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_packs::{DurableEffectProvenance, ToolId};
use optimus_runtime::{JobId, JobStatus, Runtime, RuntimeError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    adapt_job_status, AgentArtifactRef, AgentBudget, AgentDescriptor, AgentFailure, AgentId,
    AgentInvocationStore, AgentPermissions, AgentRegistry, AgentRequest, AgentResult,
    AgentResultKind, AgentVersion, ArtifactRecord, ArtifactStore, CancellationToken, KernelError,
    Result, WorkflowAdapterKind, WorkflowAgentRef, WorkflowDefinition, WorkflowId, WorkflowNode,
    WorkflowObservability, WorkflowPort, WorkflowRegistry, WorkflowTerminalKind,
    WorkflowTerminalPolicy, WorkflowTrigger, WorkflowVersion, AGENT_REQUEST_SCHEMA_VERSION,
    AGENT_RESULT_SCHEMA_VERSION, WORKFLOW_SCHEMA_VERSION,
};

pub const WORKSPACE_WRITER_ID: &str = "workspace_writer";
pub const WORKSPACE_WRITER_VERSION: &str = "1.0.0";
pub const WRITE_FILE_HANDOFF_WORKFLOW_ID: &str = "write_file_handoff";
pub const WRITE_FILE_HANDOFF_WORKFLOW_VERSION: &str = "1.0.0";

const MAX_WRITE_BYTES: usize = 256 * 1024;
const MAX_RELATIVE_PATH: usize = 512;

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
        nodes: vec![WorkflowNode {
            id: "write".into(),
            adapter: WorkflowAdapterKind::Job,
            agent: Some(WorkflowAgentRef {
                id: AgentId::parse(WORKSPACE_WRITER_ID)?,
                version: AgentVersion::parse(WORKSPACE_WRITER_VERSION)?,
            }),
            dependencies: vec![],
            retry: crate::RetryPolicy {
                max_attempts: 1,
                backoff_ms: 0,
                retryable: BTreeSet::new(),
            },
            timeout_ms: 60_000,
            cancellation: crate::CancellationPolicy::Cooperative,
            approval: crate::ApprovalPolicy::Required {
                effect_kinds: BTreeSet::from(["write_file".into()]),
            },
            rollback: crate::RollbackPolicy::Unsupported,
        }],
        terminal: WorkflowTerminalPolicy {
            handled: BTreeSet::from([
                WorkflowTerminalKind::Succeeded,
                WorkflowTerminalKind::Failed,
                WorkflowTerminalKind::Cancelled,
                WorkflowTerminalKind::Ambiguous,
            ]),
        },
        observability: WorkflowObservability {
            trace_required: true,
            event_classes: BTreeSet::from([
                "accepted".into(),
                "running".into(),
                "terminal".into(),
            ]),
        },
    })
}

/// Host permission ceiling used when seeding the built-in agent registry.
pub fn builtin_agent_permission_ceiling() -> AgentPermissions {
    AgentPermissions {
        filesystem_roots: BTreeSet::from(["workspace".into()]),
        network_hosts: BTreeSet::new(),
        effects: BTreeSet::from(["write_file".into()]),
    }
}

/// Open (or create) agent registry and ensure built-in specialists are registered.
pub fn open_seeded_agent_registry(path: impl AsRef<Path>) -> Result<AgentRegistry> {
    let tools = BTreeSet::from([ToolId::new("write_file")]);
    let registry = AgentRegistry::open(path, tools, builtin_agent_permission_ceiling())?;
    let descriptor = workspace_writer_descriptor()?;
    if registry
        .get(&descriptor.id, &descriptor.version)?
        .is_none()
    {
        registry.register(&descriptor)?;
    }
    Ok(registry)
}

/// Open (or create) workflow registry and ensure the handoff workflow is registered.
pub fn open_seeded_workflow_registry(path: impl AsRef<Path>) -> Result<WorkflowRegistry> {
    let registry = WorkflowRegistry::open(path)?;
    let definition = write_file_handoff_workflow()?;
    if registry.get(&definition.id, &definition.version)?.is_none() {
        registry.register(&definition)?;
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
}

/// Execute the built-in write-file handoff vertical end-to-end.
pub fn run_write_file_handoff(
    home: impl AsRef<Path>,
    request: WriteFileHandoffRequest,
) -> Result<WriteFileHandoffReport> {
    validate_write_inputs(&request.relative_path, &request.contents)?;
    let home = home.as_ref();
    std::fs::create_dir_all(home)?;

    let workflow_registry = open_seeded_workflow_registry(home.join("workflow-registry.db"))?;
    let definition = workflow_registry
        .get(
            &WorkflowId::parse(WRITE_FILE_HANDOFF_WORKFLOW_ID)?,
            &WorkflowVersion::parse(WRITE_FILE_HANDOFF_WORKFLOW_VERSION)?,
        )?
        .ok_or_else(|| KernelError::Model("write_file_handoff workflow missing after seed".into()))?;
    definition.validate()?;
    let node = definition
        .nodes
        .iter()
        .find(|node| node.id == "write")
        .ok_or_else(|| KernelError::Model("write_file_handoff missing write node".into()))?;
    let agent_ref = node
        .agent
        .as_ref()
        .ok_or_else(|| KernelError::Model("write node requires workspace_writer agent".into()))?;
    if agent_ref.id.as_str() != WORKSPACE_WRITER_ID
        || agent_ref.version.as_str() != WORKSPACE_WRITER_VERSION
    {
        return Err(KernelError::Model(
            "write_file_handoff agent binding mismatch".into(),
        ));
    }

    let agent_registry = open_seeded_agent_registry(home.join("agent-registry.db"))?;
    let invocations = AgentInvocationStore::open(home.join("agent-invocations.db"))?;
    let agent_request = AgentRequest {
        schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        agent_id: agent_ref.id.clone(),
        agent_version: agent_ref.version.clone(),
        task: format!(
            "Write {} bytes to relative path {}",
            request.contents.len(),
            request.relative_path
        ),
        context: vec![],
        constraints: vec![
            "Only WriteFile through Work Graph".into(),
            "No shell or network".into(),
        ],
        tools: vec![ToolId::new("write_file")],
        permissions: AgentPermissions {
            filesystem_roots: BTreeSet::from(["workspace".into()]),
            network_hosts: BTreeSet::new(),
            effects: BTreeSet::from(["write_file".into()]),
        },
        budget: AgentBudget {
            max_steps: 1,
            timeout_ms: node.timeout_ms,
            max_context_chars: 8_192,
            max_output_chars: 8_192,
        },
        cancellation_id: Uuid::new_v4(),
        trace_id: Uuid::new_v4(),
    };
    let invocation_id = invocations.begin(&agent_registry, &agent_request)?;
    let token = CancellationToken::new();

    let workspace = home.join("workspace");
    std::fs::create_dir_all(&workspace)?;
    let runtime = Runtime::open_with_config(
        &home.join("optimus.db"),
        &workspace,
        RuntimeConfig {
            policy: request.policy,
        },
    )?;

    if invocations.sync_cancellation(invocation_id, &token)? {
        return finalize_cancelled(
            &invocations,
            invocation_id,
            &definition,
            &request.relative_path,
            None,
            "cancellation observed before effect",
        );
    }

    let job = runtime.create_job(JobSpec {
        label: format!("agent:{WORKSPACE_WRITER_ID}:write"),
        budget: Default::default(),
        nodes: vec![NodeSpec {
            label: "write_file".into(),
            effect: Effect::WriteFile {
                relative_path: request.relative_path.clone(),
                contents: request.contents.clone(),
            },
        }],
    })?;

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
                return Ok(report(
                    &definition,
                    invocation_id,
                    Some(job.0),
                    WorkflowTerminalKind::Failed,
                    result,
                    &request.relative_path,
                    None,
                    adapt_job_status(JobStatus::AwaitingApproval),
                ));
            }
        }
        Err(RuntimeError::Cancelled { .. }) => {
            let _ = invocations.request_cancellation(invocation_id, "runtime cancelled job")?;
            return finalize_cancelled(
                &invocations,
                invocation_id,
                &definition,
                &request.relative_path,
                Some(job.0),
                "runtime cancelled job",
            );
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
            return Ok(report(
                &definition,
                invocation_id,
                Some(job.0),
                WorkflowTerminalKind::Failed,
                result,
                &request.relative_path,
                None,
                adapt_job_status(runtime.job_status(job).unwrap_or(JobStatus::Failed)),
            ));
        }
    }

    if invocations.sync_cancellation(invocation_id, &token)? {
        let _ = runtime.cancel_job(job);
        return finalize_cancelled(
            &invocations,
            invocation_id,
            &definition,
            &request.relative_path,
            Some(job.0),
            "cancellation observed after effect start",
        );
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
        return Ok(report(
            &definition,
            invocation_id,
            Some(job.0),
            WorkflowTerminalKind::Failed,
            result,
            &request.relative_path,
            None,
            adapt_job_status(runtime.job_status(job)?),
        ));
    }

    let provenance = DurableEffectProvenance {
        job_id: outcome.job_id.0,
        node_id: outcome.node_id,
        effect_attempt_id: outcome.attempt_id,
        effect_sha256: outcome.effect_hash,
        receipt_sha256: outcome.receipt_hash,
    };
    invocations.link_effect(&runtime, invocation_id, &provenance)?;

    let artifacts = ArtifactStore::open(home)?;
    let artifact = artifacts.put_bytes(
        request.contents.as_bytes(),
        "text/plain",
        "workspace_writer",
        &request.relative_path,
        Some(&invocation_id.to_string()),
    )?;

    let result = AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id,
        kind: AgentResultKind::Succeeded,
        summary: format!(
            "wrote {} ({} bytes); handoff {}",
            request.relative_path,
            request.contents.len(),
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

    Ok(report(
        &definition,
        invocation_id,
        Some(job.0),
        WorkflowTerminalKind::Succeeded,
        result,
        &request.relative_path,
        Some(artifact),
        adapt_job_status(runtime.job_status(job)?),
    ))
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

fn validate_write_inputs(relative_path: &str, contents: &str) -> Result<()> {
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
    if contents.len() > MAX_WRITE_BYTES {
        return Err(KernelError::Model(
            "contents exceed workspace_writer bound".into(),
        ));
    }
    Ok(())
}

fn finalize_cancelled(
    invocations: &AgentInvocationStore,
    invocation_id: Uuid,
    definition: &WorkflowDefinition,
    relative_path: &str,
    job_id: Option<Uuid>,
    reason: &str,
) -> Result<WriteFileHandoffReport> {
    let result = AgentResult {
        schema_version: AGENT_RESULT_SCHEMA_VERSION,
        invocation_id,
        kind: AgentResultKind::Cancelled,
        summary: "workspace_writer cancelled".into(),
        error: None,
        cancellation_reason: Some(reason.into()),
        evidence: vec![],
        artifacts: vec![],
        unresolved: vec![],
    };
    invocations.settle(&result)?;
    Ok(report(
        definition,
        invocation_id,
        job_id,
        WorkflowTerminalKind::Cancelled,
        result,
        relative_path,
        None,
        crate::AdapterLifecycleStatus::Cancelled,
    ))
}

#[allow(clippy::too_many_arguments)]
fn report(
    definition: &WorkflowDefinition,
    invocation_id: Uuid,
    job_id: Option<Uuid>,
    workflow_terminal: WorkflowTerminalKind,
    agent_result: AgentResult,
    relative_path: &str,
    artifact: Option<ArtifactRecord>,
    adapter_status: crate::AdapterLifecycleStatus,
) -> WriteFileHandoffReport {
    WriteFileHandoffReport {
        workflow_id: definition.id.as_str().into(),
        workflow_version: definition.version.as_str().into(),
        agent_id: WORKSPACE_WRITER_ID.into(),
        agent_version: WORKSPACE_WRITER_VERSION.into(),
        invocation_id,
        job_id,
        workflow_terminal,
        agent_result,
        relative_path: relative_path.into(),
        artifact,
        adapter_status: format!("{adapter_status:?}"),
    }
}

/// Content digest helper for tests and CLI.
pub fn content_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Default workspace path under an Optimus home for the vertical.
pub fn vertical_workspace(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("workspace")
}
