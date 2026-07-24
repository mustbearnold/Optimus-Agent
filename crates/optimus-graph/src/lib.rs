//! Work Graph domain types and transition helpers.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub use optimus_store::{
    CoupledStatusTransition, JobStatus, NewActionApproval, NewJobGraph, NewNodeGraph, NodeStatus,
    PreparedAttemptDisposition, PreparedEffectAttemptRow, Store, StoreError,
};

#[derive(Debug, Error)]
pub enum GraphError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("invalid transition: {0}")]
    InvalidTransition(String),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, GraphError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    WriteFile {
        relative_path: String,
        contents: String,
    },
    /// Project-root mutation bound to the canonical workspace fingerprint.
    ProjectWriteFile {
        workspace_sha256: String,
        relative_path: String,
        contents: String,
    },
    AssertFileEquals {
        relative_path: String,
        expected: String,
    },
    /// Run a shell command in the workspace (high-risk under SmartDeny).
    RunCommand { program: String, args: Vec<String> },
    /// Project-root command bound to the canonical workspace fingerprint.
    ProjectRunCommand {
        workspace_sha256: String,
        program: String,
        args: Vec<String>,
    },
}

impl Effect {
    /// Host-mutating effects require SmartDeny approval (or Unrestricted policy).
    ///
    /// `AssertFileEquals` is intentionally excluded: it only reads and compares.
    pub fn is_high_risk(&self) -> bool {
        matches!(
            self,
            Effect::WriteFile { .. }
                | Effect::ProjectWriteFile { .. }
                | Effect::RunCommand { .. }
                | Effect::ProjectRunCommand { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    /// Default: high-risk effects require an explicit approval grant.
    #[default]
    SmartDeny,
    /// Break-glass / test mode: all effects auto-run.
    Unrestricted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobBudget {
    pub max_steps: u32,
    pub max_consecutive_failures: u32,
    pub command_timeout_ms: u32,
}

impl Default for JobBudget {
    fn default() -> Self {
        Self {
            max_steps: 100,
            max_consecutive_failures: 3,
            command_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub policy: PolicyMode,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            policy: PolicyMode::SmartDeny,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSpec {
    pub label: String,
    pub effect: Effect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub label: String,
    #[serde(default)]
    pub budget: JobBudget,
    pub nodes: Vec<NodeSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct CreatedJob {
    pub id: JobId,
    pub node_ids: Vec<Uuid>,
}

/// Create a pending job with ordered pending nodes and ledger events.
pub fn create_job(store: &Store, spec: JobSpec) -> Result<CreatedJob> {
    create_job_with_id(store, JobId(Uuid::new_v4()), spec)
}

/// Create a complete job under a durable caller-supplied identity.
pub fn create_job_with_id(store: &Store, job_id: JobId, spec: JobSpec) -> Result<CreatedJob> {
    if spec.nodes.is_empty() {
        return Err(GraphError::InvalidTransition(
            "job must have at least one node".into(),
        ));
    }
    let JobId(job_uuid) = job_id;
    let job_event_payload = serde_json::to_string(&serde_json::json!({
        "label": spec.label,
        "budget": spec.budget,
    }))?;
    let mut node_ids = Vec::with_capacity(spec.nodes.len());
    let mut nodes = Vec::with_capacity(spec.nodes.len());
    for (idx, node) in spec.nodes.into_iter().enumerate() {
        let node_id = Uuid::new_v4();
        let effect_json = serde_json::to_string(&node.effect)?;
        let event_payload_json =
            serde_json::to_string(&serde_json::json!({ "idx": idx, "label": node.label }))?;
        nodes.push(NewNodeGraph {
            id: node_id,
            idx: idx as u32,
            label: node.label,
            status: NodeStatus::Pending,
            effect_json,
            event_payload_json,
        });
        node_ids.push(node_id);
    }
    store.insert_job_graph(NewJobGraph {
        id: job_uuid,
        label: spec.label,
        status: JobStatus::Pending,
        max_steps: spec.budget.max_steps,
        max_consecutive_failures: spec.budget.max_consecutive_failures,
        command_timeout_ms: spec.budget.command_timeout_ms,
        event_payload_json: job_event_payload,
        nodes,
    })?;
    Ok(CreatedJob {
        id: job_id,
        node_ids,
    })
}

pub fn mark_job_running(store: &Store, job_id: JobId) -> Result<()> {
    let job = store.get_job(job_id.0)?;
    match job.status {
        JobStatus::Running => return Ok(()),
        JobStatus::Pending | JobStatus::Interrupted | JobStatus::AwaitingApproval => {}
        other => {
            return Err(GraphError::InvalidTransition(format!(
                "job {:?} cannot enter running",
                other
            )));
        }
    }
    store.transition_job_with_event(
        job_id.0,
        job.status,
        JobStatus::Running,
        "job_running",
        &serde_json::json!({}),
    )?;
    Ok(())
}

pub fn mark_job_terminal(store: &Store, job_id: JobId, status: JobStatus) -> Result<()> {
    match status {
        JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled => {}
        other => {
            return Err(GraphError::InvalidTransition(format!(
                "not a terminal job status: {:?}",
                other
            )));
        }
    }
    let job = store.get_job(job_id.0)?;
    if job.status == status {
        return Ok(());
    }
    if matches!(
        job.status,
        JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
    ) {
        return Err(GraphError::InvalidTransition(format!(
            "terminal job {:?} cannot become {:?}",
            job.status, status
        )));
    }
    store.transition_job_with_event(
        job_id.0,
        job.status,
        status,
        "job_terminal",
        &serde_json::json!({ "status": status }),
    )?;
    Ok(())
}

pub fn mark_node_running(store: &Store, job_id: JobId, node_id: Uuid) -> Result<Uuid> {
    let nodes = store.list_nodes(job_id.0)?;
    let node = nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| StoreError::NotFound(format!("node {node_id}")))?;
    match node.status {
        NodeStatus::Pending | NodeStatus::Interrupted | NodeStatus::AwaitingApproval => {}
        other => {
            return Err(GraphError::InvalidTransition(format!(
                "node {:?} cannot enter running",
                other
            )));
        }
    }
    if nodes.iter().any(|n| n.status == NodeStatus::Running) {
        return Err(GraphError::InvalidTransition(
            "job already has a running node".into(),
        ));
    }
    Ok(store.begin_effect_attempt(job_id.0, node_id, node.status, &node.effect_json)?)
}

pub fn mark_node_succeeded(
    store: &Store,
    job_id: JobId,
    node_id: Uuid,
    attempt_id: Uuid,
    receipt: &serde_json::Value,
) -> Result<()> {
    store.complete_effect_attempt_success(job_id.0, node_id, attempt_id, receipt)?;
    Ok(())
}

pub fn mark_node_failed(
    store: &Store,
    job_id: JobId,
    node_id: Uuid,
    attempt_id: Uuid,
    receipt: &serde_json::Value,
) -> Result<()> {
    store.complete_effect_attempt_failure(job_id.0, node_id, attempt_id, receipt)?;
    Ok(())
}

pub fn mark_node_awaiting_approval(store: &Store, job_id: JobId, node_id: Uuid) -> Result<()> {
    let job = store.get_job(job_id.0)?;
    let node = store
        .list_nodes(job_id.0)?
        .into_iter()
        .find(|node| node.id == node_id)
        .ok_or_else(|| StoreError::NotFound(format!("node {node_id}")))?;
    store.transition_node_and_job_with_event(
        CoupledStatusTransition {
            job_id: job_id.0,
            expected_job: job.status,
            next_job: JobStatus::AwaitingApproval,
            node_id,
            expected_node: node.status,
            next_node: NodeStatus::AwaitingApproval,
        },
        "node_awaiting_approval",
        &serde_json::json!({}),
    )?;
    Ok(())
}

/// Crash recovery: any `running` node becomes `interrupted` (never silently succeeded).
pub fn interrupt_running_nodes(store: &Store) -> Result<Vec<JobId>> {
    let job_ids = store.list_running_node_job_ids()?;
    let mut out = Vec::new();
    for jid in job_ids {
        let job_id = JobId(jid);
        if interrupt_running_nodes_for_job(store, job_id)? {
            out.push(job_id);
        }
    }
    Ok(out)
}

/// Recover only one crashed job, leaving unrelated running work untouched.
pub fn interrupt_running_nodes_for_job(store: &Store, job_id: JobId) -> Result<bool> {
    Ok(store.interrupt_running_job(job_id.0, "process_crash_recovery")?)
}

pub fn next_runnable_node(store: &Store, job_id: JobId) -> Result<Option<optimus_store::NodeRow>> {
    let nodes = store.list_nodes(job_id.0)?;
    for node in nodes {
        match node.status {
            NodeStatus::Succeeded | NodeStatus::Cancelled => continue,
            NodeStatus::Failed => return Ok(None),
            NodeStatus::Pending
            | NodeStatus::Interrupted
            | NodeStatus::Running
            | NodeStatus::AwaitingApproval => {
                return Ok(Some(node));
            }
        }
    }
    Ok(None)
}

pub fn recompute_job_status(store: &Store, job_id: JobId) -> Result<JobStatus> {
    let nodes = store.list_nodes(job_id.0)?;
    let next = if nodes.iter().any(|n| n.status == NodeStatus::Failed) {
        JobStatus::Failed
    } else if nodes.iter().any(|n| n.status == NodeStatus::Cancelled) {
        JobStatus::Cancelled
    } else if nodes
        .iter()
        .any(|n| n.status == NodeStatus::AwaitingApproval)
    {
        JobStatus::AwaitingApproval
    } else if nodes.iter().any(|n| n.status == NodeStatus::Interrupted) {
        JobStatus::Interrupted
    } else if nodes.iter().any(|n| n.status == NodeStatus::Running) {
        JobStatus::Running
    } else if nodes
        .iter()
        .all(|n| matches!(n.status, NodeStatus::Succeeded | NodeStatus::Cancelled))
    {
        JobStatus::Succeeded
    } else {
        JobStatus::Pending
    };
    let current = store.get_job(job_id.0)?.status;
    if current == next {
        return Ok(next);
    }
    if matches!(
        next,
        JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
    ) {
        mark_job_terminal(store, job_id, next)?;
    } else {
        store.transition_job_with_event(
            job_id.0,
            current,
            next,
            "job_status_recomputed",
            &serde_json::json!({ "status": next }),
        )?;
    }
    Ok(next)
}
