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
    /// Create a directory (and parents) under the workspace.
    Mkdir { relative_path: String },
    ProjectMkdir {
        workspace_sha256: String,
        relative_path: String,
    },
    /// Delete a file or empty directory under the workspace.
    DeletePath { relative_path: String },
    ProjectDeletePath {
        workspace_sha256: String,
        relative_path: String,
    },
    /// Rename/move within the same workspace (source and dest confined).
    RenamePath {
        from_relative_path: String,
        to_relative_path: String,
    },
    ProjectRenamePath {
        workspace_sha256: String,
        from_relative_path: String,
        to_relative_path: String,
    },
    /// Exact single-occurrence string replace within a file (fail if 0 or >1 matches).
    PatchFile {
        relative_path: String,
        old_string: String,
        new_string: String,
    },
    ProjectPatchFile {
        workspace_sha256: String,
        relative_path: String,
        old_string: String,
        new_string: String,
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
    /// Start a project server and take an owned-localhost lease on it (ADR-0060).
    ///
    /// Unlike [`Effect::ProjectRunCommand`] this does not wait for the process to
    /// exit. It returns once the runtime has proven that the listener on
    /// `127.0.0.1:port` belongs to the process tree it just started, and holds a
    /// lease that expires after `ttl_seconds` or when the run settles.
    ///
    /// There is no unscoped `Serve`: a lease binds the project root hash, so a
    /// serve that is not project-bound has nothing to bind to.
    ProjectServe {
        workspace_sha256: String,
        program: String,
        args: Vec<String>,
        port: u16,
        ttl_seconds: u64,
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
                | Effect::Mkdir { .. }
                | Effect::ProjectMkdir { .. }
                | Effect::DeletePath { .. }
                | Effect::ProjectDeletePath { .. }
                | Effect::RenamePath { .. }
                | Effect::ProjectRenamePath { .. }
                | Effect::PatchFile { .. }
                | Effect::ProjectPatchFile { .. }
                | Effect::RunCommand { .. }
                | Effect::ProjectRunCommand { .. }
                | Effect::ProjectServe { .. }
        )
    }

    /// FsWorkspace skill class covers all host-mutating file ops (not commands).
    pub fn requires_fs_workspace_skill(&self) -> bool {
        matches!(
            self,
            Effect::WriteFile { .. }
                | Effect::ProjectWriteFile { .. }
                | Effect::Mkdir { .. }
                | Effect::ProjectMkdir { .. }
                | Effect::DeletePath { .. }
                | Effect::ProjectDeletePath { .. }
                | Effect::RenamePath { .. }
                | Effect::ProjectRenamePath { .. }
                | Effect::PatchFile { .. }
                | Effect::ProjectPatchFile { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    /// Default: high-risk effects are gated. Whether they pause for a human or
    /// are auto-authorized by a project trust profile is decided by
    /// [`RuntimeConfig::autonomy_profile`] (ADR-0044 / optimus-policy).
    #[default]
    SmartDeny,
    /// Break-glass / test mode: all effects auto-run (approval auto-grant only;
    /// filesystem envelope is separate — see [`CommandFsEnvelope`]).
    Unrestricted,
}

/// Product autonomy profile (when Optimus asks). Re-exported shape kept in graph
/// so `RuntimeConfig` stays self-describing without a hard graph→policy cycle
/// at the type layer for older callers; runtime maps this to `optimus_policy`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyProfile {
    /// Recommended product default: ordinary project work auto-authorized.
    Standard,
    /// Pause project writes and commands (legacy “Ask before effects”).
    #[default]
    ReviewChanges,
    ReadOnly,
    FullProject,
    /// Explicit local self-development authority, bounded by a persisted grant.
    DeveloperFullAccess,
    /// Expert break-glass marker; pair with [`PolicyMode::Unrestricted`] in product.
    UnrestrictedHost,
}

impl AutonomyProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::ReviewChanges => "review_changes",
            Self::ReadOnly => "read_only",
            Self::FullProject => "full_project",
            Self::DeveloperFullAccess => "developer_full_access",
            Self::UnrestrictedHost => "unrestricted_host",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "standard" | "std" => Some(Self::Standard),
            "review_changes" | "review" | "ask" => Some(Self::ReviewChanges),
            "read_only" | "readonly" | "read" | "read-only" => Some(Self::ReadOnly),
            "full_project" | "full-project" | "project_full" => Some(Self::FullProject),
            "developer_full_access" | "developer-full-access" | "developer" => {
                Some(Self::DeveloperFullAccess)
            }
            // Mirrors optimus-policy: break-glass answers only to words that
            // cannot be misread as ordinary. `yolo` stays because the CLI flag
            // of that name is unmistakable; `full` and `host` are gone (#118).
            "unrestricted_host" | "unrestricted" | "yolo" => Some(Self::UnrestrictedHost),
            _ => None,
        }
    }
}

/// Filesystem/network envelope for approved `RunCommand` / `ProjectRunCommand`.
///
/// Orthogonal to [`PolicyMode`]: SmartDeny still gates host mutation; this
/// controls how far an **approved** (or Unrestricted-auto) command can reach.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommandFsEnvelope {
    /// Linux: workspace is the only writable tree (confined bwrap). Windows:
    /// Job Object process-tree ownership only (product-visible residual).
    #[default]
    Confined,
    /// Confined FS plus Linux network namespace unshare. Non-Linux refuses
    /// command spawn fail-closed (no AppContainer yet).
    ConfinedNoNetwork,
    /// Operator break-glass: host FS visible to the child (Linux still uses
    /// systemd-run + process-tree ownership; Windows Job Object). Must be
    /// explicit — never the product default.
    UnrestrictedHost,
}

impl CommandFsEnvelope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confined => "confined",
            Self::ConfinedNoNetwork => "confined_no_network",
            Self::UnrestrictedHost => "unrestricted_host",
        }
    }

    /// Parse a canonical envelope name, exactly as [`CommandFsEnvelope::as_str`]
    /// renders it. Unknown spellings return `None` so callers keep their own
    /// fail-closed default (e.g. `Confined` for a surface access string) rather
    /// than this method silently choosing a mode.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "confined" => Some(Self::Confined),
            "confined_no_network" => Some(Self::ConfinedNoNetwork),
            "unrestricted_host" => Some(Self::UnrestrictedHost),
            _ => None,
        }
    }

    /// Whether this mode claims workspace-only writable FS on Linux.
    pub fn linux_workspace_only_writable(self) -> bool {
        matches!(self, Self::Confined | Self::ConfinedNoNetwork)
    }

    /// Whether Linux should unshare the network namespace.
    pub fn linux_unshare_net(self) -> bool {
        matches!(self, Self::ConfinedNoNetwork)
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub policy: PolicyMode,
    /// Command capability envelope (default: confined).
    #[serde(default)]
    pub command_fs_envelope: CommandFsEnvelope,
    /// When Optimus asks (default: review_changes preserves classic SmartDeny
    /// pause semantics for tests; product UI sets `standard`).
    #[serde(default)]
    pub autonomy_profile: AutonomyProfile,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            policy: PolicyMode::SmartDeny,
            command_fs_envelope: CommandFsEnvelope::Confined,
            autonomy_profile: AutonomyProfile::ReviewChanges,
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
        current,
        JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
    ) {
        // Terminal job states are final: a recompute must never resurrect a
        // completed job, even if node rows disagree (mirrors the guard in
        // `mark_job_terminal`).
        return Err(GraphError::InvalidTransition(format!(
            "terminal job {job_id:?} cannot become {next:?}"
        )));
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_store() -> (Store, tempfile::TempDir) {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(&dir.path().join("work.graph.sqlite")).expect("open store");
        (store, dir)
    }

    fn write_spec(label: &str) -> JobSpec {
        JobSpec {
            label: label.to_string(),
            budget: JobBudget::default(),
            nodes: vec![NodeSpec {
                label: format!("{label}-node"),
                effect: Effect::WriteFile {
                    relative_path: format!("{label}.txt"),
                    contents: "payload".to_string(),
                },
            }],
        }
    }

    #[test]
    fn create_job_rejects_empty_node_list() {
        let (store, _dir) = test_store();
        let err = create_job_with_id(
            &store,
            JobId(Uuid::new_v4()),
            JobSpec {
                label: "empty".into(),
                budget: JobBudget::default(),
                nodes: vec![],
            },
        )
        .expect_err("empty node list must be rejected");
        assert!(matches!(err, GraphError::InvalidTransition(_)));
    }

    #[test]
    fn recompute_job_status_never_resurrects_terminal_job() {
        let (store, _dir) = test_store();
        let job_id = JobId(Uuid::new_v4());
        let node_id = Uuid::new_v4();
        // A job that is already terminal (Succeeded) but whose node rows are
        // stale (one node still Pending) must not be dragged back to Pending
        // by a recompute: terminal states are final.
        store
            .insert_job_graph(NewJobGraph {
                id: job_id.0,
                label: "already-done".into(),
                status: JobStatus::Succeeded,
                max_steps: 100,
                max_consecutive_failures: 3,
                command_timeout_ms: 30_000,
                event_payload_json: "{}".into(),
                nodes: vec![NewNodeGraph {
                    id: node_id,
                    idx: 0,
                    label: "write".into(),
                    status: NodeStatus::Pending,
                    effect_json: serde_json::to_string(&Effect::WriteFile {
                        relative_path: "a.txt".into(),
                        contents: "x".into(),
                    })
                    .expect("serialize effect"),
                    event_payload_json: "{}".into(),
                }],
            })
            .expect("insert graph");

        let err = recompute_job_status(&store, job_id)
            .expect_err("terminal job with stale nodes must error instead of resurrecting");
        assert!(matches!(err, GraphError::InvalidTransition(_)));
        assert_eq!(
            store.get_job(job_id.0).expect("job").status,
            JobStatus::Succeeded
        );
    }

    #[test]
    fn recompute_job_status_is_idempotent_for_terminal_jobs() {
        let (store, _dir) = test_store();
        let created = create_job(&store, write_spec("done")).expect("create job");
        mark_job_running(&store, created.id).expect("job running");
        let attempt =
            mark_node_running(&store, created.id, created.node_ids[0]).expect("node running");
        mark_node_succeeded(
            &store,
            created.id,
            created.node_ids[0],
            attempt,
            &serde_json::json!({ "ok": true }),
        )
        .expect("node succeeded");
        assert_eq!(
            recompute_job_status(&store, created.id).expect("recompute"),
            JobStatus::Succeeded
        );
        // Recomputing a terminal job that agrees with its nodes stays a no-op.
        assert_eq!(
            recompute_job_status(&store, created.id).expect("recompute again"),
            JobStatus::Succeeded
        );
    }

    #[test]
    fn two_node_job_runs_to_succeeded() {
        let (store, _dir) = test_store();
        let created = create_job(
            &store,
            JobSpec {
                label: "two-node".into(),
                budget: JobBudget::default(),
                nodes: vec![
                    NodeSpec {
                        label: "first".into(),
                        effect: Effect::WriteFile {
                            relative_path: "a.txt".into(),
                            contents: "1".into(),
                        },
                    },
                    NodeSpec {
                        label: "second".into(),
                        effect: Effect::WriteFile {
                            relative_path: "b.txt".into(),
                            contents: "2".into(),
                        },
                    },
                ],
            },
        )
        .expect("create job");

        mark_job_running(&store, created.id).expect("job running");
        let attempt1 =
            mark_node_running(&store, created.id, created.node_ids[0]).expect("run node 1");
        mark_node_succeeded(
            &store,
            created.id,
            created.node_ids[0],
            attempt1,
            &serde_json::json!({ "ok": true }),
        )
        .expect("succeed node 1");

        mark_job_running(&store, created.id).expect("job running again");
        let attempt2 =
            mark_node_running(&store, created.id, created.node_ids[1]).expect("run node 2");
        mark_node_succeeded(
            &store,
            created.id,
            created.node_ids[1],
            attempt2,
            &serde_json::json!({ "ok": true }),
        )
        .expect("succeed node 2");
        assert_eq!(
            recompute_job_status(&store, created.id).expect("recompute"),
            JobStatus::Succeeded
        );
    }

    #[test]
    fn mark_job_terminal_rejects_non_terminal_status() {
        let (store, _dir) = test_store();
        let created = create_job(&store, write_spec("t")).expect("create job");
        let err = mark_job_terminal(&store, created.id, JobStatus::Running)
            .expect_err("Running is not a terminal status");
        assert!(matches!(err, GraphError::InvalidTransition(_)));
    }

    #[test]
    fn mark_node_running_rejects_second_running_node() {
        let (store, _dir) = test_store();
        let created = create_job(
            &store,
            JobSpec {
                label: "two".into(),
                budget: JobBudget::default(),
                nodes: vec![
                    NodeSpec {
                        label: "a".into(),
                        effect: Effect::WriteFile {
                            relative_path: "a.txt".into(),
                            contents: "1".into(),
                        },
                    },
                    NodeSpec {
                        label: "b".into(),
                        effect: Effect::WriteFile {
                            relative_path: "b.txt".into(),
                            contents: "2".into(),
                        },
                    },
                ],
            },
        )
        .expect("create job");
        mark_job_running(&store, created.id).expect("job running");
        mark_node_running(&store, created.id, created.node_ids[0]).expect("run node 1");
        let err = mark_node_running(&store, created.id, created.node_ids[1])
            .expect_err("second running node must be rejected");
        assert!(matches!(err, GraphError::InvalidTransition(_)));
    }

    #[test]
    fn effect_risk_classification_is_stable() {
        let write = Effect::WriteFile {
            relative_path: "a.txt".into(),
            contents: "b".into(),
        };
        assert!(write.is_high_risk());
        assert!(write.requires_fs_workspace_skill());

        let assert_equals = Effect::AssertFileEquals {
            relative_path: "a.txt".into(),
            expected: "b".into(),
        };
        assert!(!assert_equals.is_high_risk());
        assert!(!assert_equals.requires_fs_workspace_skill());

        let run = Effect::RunCommand {
            program: "true".into(),
            args: vec![],
        };
        assert!(run.is_high_risk());
        assert!(!run.requires_fs_workspace_skill());

        let serve = Effect::ProjectServe {
            workspace_sha256: "w".into(),
            program: "srv".into(),
            args: vec![],
            port: 8080,
            ttl_seconds: 30,
        };
        assert!(serve.is_high_risk());
        assert!(!serve.requires_fs_workspace_skill());
    }

    #[test]
    fn autonomy_profile_parse_round_trips_canonical_names() {
        for profile in [
            AutonomyProfile::Standard,
            AutonomyProfile::ReviewChanges,
            AutonomyProfile::ReadOnly,
            AutonomyProfile::FullProject,
            AutonomyProfile::DeveloperFullAccess,
            AutonomyProfile::UnrestrictedHost,
        ] {
            assert_eq!(
                AutonomyProfile::parse(profile.as_str()),
                Some(profile),
                "canonical name must round-trip: {}",
                profile.as_str()
            );
        }
        assert_eq!(
            AutonomyProfile::parse("  YOLO "),
            Some(AutonomyProfile::UnrestrictedHost)
        );
        assert_eq!(
            AutonomyProfile::parse("ask"),
            Some(AutonomyProfile::ReviewChanges)
        );
        // Parity with optimus-policy (53d865f): the hyphenated spellings of
        // the other profiles ("full-project", "developer-full-access") parse,
        // so "read-only" must too instead of failing closed to None here.
        assert_eq!(
            AutonomyProfile::parse("read-only"),
            Some(AutonomyProfile::ReadOnly)
        );
        assert_eq!(
            AutonomyProfile::parse("READ-ONLY"),
            Some(AutonomyProfile::ReadOnly)
        );
        assert_eq!(AutonomyProfile::parse("full"), None);
        assert_eq!(AutonomyProfile::parse("host"), None);
        assert_eq!(AutonomyProfile::parse(""), None);
    }

    #[test]
    fn command_fs_envelope_parse_round_trips_canonical_names() {
        for envelope in [
            CommandFsEnvelope::Confined,
            CommandFsEnvelope::ConfinedNoNetwork,
            CommandFsEnvelope::UnrestrictedHost,
        ] {
            assert_eq!(
                CommandFsEnvelope::parse(envelope.as_str()),
                Some(envelope),
                "canonical name must round-trip: {}",
                envelope.as_str()
            );
        }
        // Unknown spellings fail closed to None: the caller keeps its own
        // default rather than parse silently choosing a mode.
        assert_eq!(CommandFsEnvelope::parse("confined_no_net"), None);
        assert_eq!(CommandFsEnvelope::parse("CONFINED"), None);
        assert_eq!(CommandFsEnvelope::parse(""), None);
    }
}
