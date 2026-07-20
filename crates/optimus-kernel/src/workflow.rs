//! Versioned general workflow contracts, immutable registry, and honest lifecycle adapters.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use optimus_runtime::{CampaignStatus, JobStatus};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AgentId, AgentVersion, KernelError, Result};

pub const WORKFLOW_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkflowId(String);

impl WorkflowId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if !valid_identifier(&value) {
            return Err(invalid("workflow id must match [a-z][a-z0-9_-]{0,63}"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkflowVersion(String);

impl WorkflowVersion {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != 3
            || !parts.iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| byte.is_ascii_digit())
                    && (*part == "0" || !part.starts_with('0'))
                    && part.parse::<u32>().is_ok()
            })
        {
            return Err(invalid(
                "workflow version must be canonical major.minor.patch",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowTrigger {
    Manual,
    Schedule { expression: String },
    Message { channel: String },
    Dependency { workflow_id: WorkflowId },
    RuntimeEvent { kind: String },
}

impl WorkflowTrigger {
    fn validate(&self) -> Result<()> {
        let value = match self {
            Self::Manual => return Ok(()),
            Self::Schedule { expression } => expression,
            Self::Message { channel } => channel,
            Self::Dependency { workflow_id } => {
                return WorkflowId::parse(workflow_id.as_str()).map(drop)
            }
            Self::RuntimeEvent { kind } => kind,
        };
        if value.trim().is_empty() || value.len() > 1024 {
            return Err(invalid("workflow trigger value is empty or too large"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowPort {
    pub name: String,
    pub schema: Value,
}

impl WorkflowPort {
    fn validate(&self) -> Result<()> {
        if !valid_identifier(&self.name)
            || !self.schema.is_object()
            || self.schema.get("type").and_then(Value::as_str).is_none()
            || serde_json::to_vec(&self.schema)?.len() > 64 * 1024
        {
            return Err(invalid("workflow port name or schema is invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAdapterKind {
    Job,
    Campaign,
    Cron,
    Gateway,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u16,
    pub backoff_ms: u64,
    pub retryable: BTreeSet<WorkflowTerminalKind>,
}

impl RetryPolicy {
    fn validate(&self) -> Result<()> {
        if self.max_attempts == 0
            || self.max_attempts > 100
            || self.backoff_ms > 86_400_000
            || self.retryable.contains(&WorkflowTerminalKind::Succeeded)
        {
            return Err(invalid("workflow retry policy is invalid or unbounded"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CancellationPolicy {
    Cooperative,
    ImmediateUnsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalPolicy {
    None,
    Required { effect_kinds: BTreeSet<String> },
}

impl ApprovalPolicy {
    fn validate(&self) -> Result<()> {
        if let Self::Required { effect_kinds } = self {
            if effect_kinds.is_empty()
                || effect_kinds
                    .iter()
                    .any(|value| value.trim().is_empty() || value.len() > 256)
            {
                return Err(invalid("workflow approval effect kinds are invalid"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RollbackPolicy {
    Supported,
    Compensating,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTerminalKind {
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowTerminalPolicy {
    pub handled: BTreeSet<WorkflowTerminalKind>,
}

impl WorkflowTerminalPolicy {
    fn validate(&self) -> Result<()> {
        let required = BTreeSet::from([
            WorkflowTerminalKind::Succeeded,
            WorkflowTerminalKind::Failed,
            WorkflowTerminalKind::Cancelled,
            WorkflowTerminalKind::Ambiguous,
        ]);
        if self.handled != required {
            return Err(invalid("workflow must handle exactly all terminal kinds"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowObservability {
    pub trace_required: bool,
    pub event_classes: BTreeSet<String>,
}

impl WorkflowObservability {
    fn validate(&self) -> Result<()> {
        let required = BTreeSet::from([
            "accepted".to_string(),
            "running".to_string(),
            "terminal".to_string(),
        ]);
        if !self.trace_required
            || !required.is_subset(&self.event_classes)
            || self
                .event_classes
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 256)
        {
            return Err(invalid("workflow observability contract is incomplete"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowNode {
    pub id: String,
    pub adapter: WorkflowAdapterKind,
    pub agent: Option<WorkflowAgentRef>,
    pub dependencies: Vec<String>,
    pub retry: RetryPolicy,
    pub timeout_ms: u64,
    pub cancellation: CancellationPolicy,
    pub approval: ApprovalPolicy,
    pub rollback: RollbackPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowAgentRef {
    pub id: AgentId,
    pub version: AgentVersion,
}

impl WorkflowNode {
    fn validate(&self) -> Result<()> {
        if !valid_identifier(&self.id)
            || self.timeout_ms == 0
            || self.timeout_ms > 86_400_000
            || self.dependencies.len() > 1024
        {
            return Err(invalid(
                "workflow node identity, timeout, or dependencies are invalid",
            ));
        }
        if let Some(agent) = &self.agent {
            AgentId::parse(agent.id.as_str())?;
            AgentVersion::parse(agent.version.as_str())?;
        }
        self.retry.validate()?;
        self.approval.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowDefinition {
    pub schema_version: u16,
    pub id: WorkflowId,
    pub version: WorkflowVersion,
    pub description: String,
    pub triggers: Vec<WorkflowTrigger>,
    pub inputs: Vec<WorkflowPort>,
    pub outputs: Vec<WorkflowPort>,
    pub nodes: Vec<WorkflowNode>,
    pub terminal: WorkflowTerminalPolicy,
    pub observability: WorkflowObservability,
}

impl WorkflowDefinition {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != WORKFLOW_SCHEMA_VERSION {
            return Err(invalid("unsupported workflow schema version"));
        }
        WorkflowId::parse(self.id.as_str())?;
        WorkflowVersion::parse(self.version.as_str())?;
        if self.description.trim().is_empty()
            || self.description.len() > 4096
            || self.triggers.is_empty()
            || self.triggers.len() > 64
            || self.nodes.is_empty()
            || self.nodes.len() > 1024
        {
            return Err(invalid(
                "workflow description, triggers, or nodes are invalid",
            ));
        }
        for trigger in &self.triggers {
            trigger.validate()?;
        }
        validate_ports(&self.inputs)?;
        validate_ports(&self.outputs)?;
        for node in &self.nodes {
            node.validate()?;
        }
        validate_graph(&self.nodes)?;
        self.terminal.validate()?;
        self.observability.validate()
    }
}

pub struct WorkflowRegistry {
    conn: Connection,
}

impl WorkflowRegistry {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS workflow_registry(
               workflow_id TEXT NOT NULL,
               workflow_version TEXT NOT NULL,
               definition_json TEXT NOT NULL,
               PRIMARY KEY(workflow_id,workflow_version)
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn register(&self, definition: &WorkflowDefinition) -> Result<()> {
        definition.validate()?;
        if self.get(&definition.id, &definition.version)?.is_some() {
            return Err(invalid("workflow identity/version already exists"));
        }
        self.conn.execute(
            "INSERT INTO workflow_registry(workflow_id,workflow_version,definition_json)
             VALUES(?1,?2,?3)",
            params![
                definition.id.as_str(),
                definition.version.as_str(),
                serde_json::to_string(definition)?,
            ],
        )?;
        Ok(())
    }

    pub fn get(
        &self,
        id: &WorkflowId,
        version: &WorkflowVersion,
    ) -> Result<Option<WorkflowDefinition>> {
        let raw = self
            .conn
            .query_row(
                "SELECT definition_json FROM workflow_registry
                 WHERE workflow_id=?1 AND workflow_version=?2",
                params![id.as_str(), version.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        raw.map(|raw| {
            let definition: WorkflowDefinition = serde_json::from_str(&raw)?;
            if definition.id != *id || definition.version != *version {
                return Err(invalid("persisted workflow identity mismatch"));
            }
            definition.validate()?;
            Ok(definition)
        })
        .transpose()
    }

    pub fn list(&self) -> Result<Vec<WorkflowDefinition>> {
        let mut statement = self.conn.prepare(
            "SELECT workflow_id,workflow_version FROM workflow_registry
             ORDER BY workflow_id,workflow_version",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut definitions = Vec::new();
        for row in rows {
            let (id, version) = row?;
            let id = WorkflowId::parse(id)?;
            let version = WorkflowVersion::parse(version)?;
            definitions.push(
                self.get(&id, &version)?
                    .ok_or_else(|| invalid("workflow registry row disappeared during read"))?,
            );
        }
        Ok(definitions)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterCapability {
    Retry,
    Cancellation,
    Approval,
    Observability,
    Rollback,
    Acknowledgement,
    DeadLetter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowAdapterDescriptor {
    pub kind: WorkflowAdapterKind,
    pub capabilities: BTreeMap<AdapterCapability, CapabilitySupport>,
    pub terminal_outcomes: BTreeSet<WorkflowTerminalKind>,
}

impl WorkflowAdapterDescriptor {
    pub fn validate(&self) -> Result<()> {
        let required_capabilities = BTreeSet::from([
            AdapterCapability::Retry,
            AdapterCapability::Cancellation,
            AdapterCapability::Approval,
            AdapterCapability::Observability,
            AdapterCapability::Rollback,
            AdapterCapability::Acknowledgement,
            AdapterCapability::DeadLetter,
        ]);
        if self.capabilities.keys().copied().collect::<BTreeSet<_>>() != required_capabilities {
            return Err(invalid("workflow adapter capability matrix is incomplete"));
        }
        WorkflowTerminalPolicy {
            handled: self.terminal_outcomes.clone(),
        }
        .validate()?;
        if self.capabilities[&AdapterCapability::Observability] != CapabilitySupport::Supported
            || self.capabilities[&AdapterCapability::Cancellation] != CapabilitySupport::Supported
        {
            return Err(invalid(
                "workflow adapters require cancellation and observability support",
            ));
        }
        Ok(())
    }
}

pub fn builtin_workflow_adapters() -> Vec<WorkflowAdapterDescriptor> {
    use AdapterCapability::*;
    use CapabilitySupport::{Supported, Unsupported};
    let terminals = BTreeSet::from([
        WorkflowTerminalKind::Succeeded,
        WorkflowTerminalKind::Failed,
        WorkflowTerminalKind::Cancelled,
        WorkflowTerminalKind::Ambiguous,
    ]);
    let matrix = |supported: &[AdapterCapability]| {
        [
            Retry,
            Cancellation,
            Approval,
            Observability,
            Rollback,
            Acknowledgement,
            DeadLetter,
        ]
        .into_iter()
        .map(|capability| {
            let state = if supported.contains(&capability) {
                Supported
            } else {
                Unsupported
            };
            (capability, state)
        })
        .collect()
    };
    vec![
        WorkflowAdapterDescriptor {
            kind: WorkflowAdapterKind::Job,
            capabilities: matrix(&[Cancellation, Approval, Observability]),
            terminal_outcomes: terminals.clone(),
        },
        WorkflowAdapterDescriptor {
            kind: WorkflowAdapterKind::Campaign,
            capabilities: matrix(&[Retry, Cancellation, Approval, Observability]),
            terminal_outcomes: terminals.clone(),
        },
        WorkflowAdapterDescriptor {
            kind: WorkflowAdapterKind::Cron,
            capabilities: matrix(&[Retry, Cancellation, Observability]),
            terminal_outcomes: terminals.clone(),
        },
        WorkflowAdapterDescriptor {
            kind: WorkflowAdapterKind::Gateway,
            capabilities: matrix(&[
                Retry,
                Cancellation,
                Observability,
                Acknowledgement,
                DeadLetter,
            ]),
            terminal_outcomes: terminals,
        },
    ]
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterLifecycleStatus {
    Pending,
    Running,
    AwaitingApproval,
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

pub fn adapt_job_status(status: JobStatus) -> AdapterLifecycleStatus {
    match status {
        JobStatus::Pending => AdapterLifecycleStatus::Pending,
        JobStatus::Running => AdapterLifecycleStatus::Running,
        JobStatus::AwaitingApproval => AdapterLifecycleStatus::AwaitingApproval,
        JobStatus::Succeeded => AdapterLifecycleStatus::Succeeded,
        JobStatus::Failed => AdapterLifecycleStatus::Failed,
        JobStatus::Cancelled => AdapterLifecycleStatus::Cancelled,
        JobStatus::Interrupted => AdapterLifecycleStatus::Ambiguous,
    }
}

pub fn adapt_campaign_status(status: CampaignStatus) -> AdapterLifecycleStatus {
    match status {
        CampaignStatus::Pending => AdapterLifecycleStatus::Pending,
        CampaignStatus::Running => AdapterLifecycleStatus::Running,
        CampaignStatus::AwaitingApproval => AdapterLifecycleStatus::AwaitingApproval,
        CampaignStatus::Succeeded => AdapterLifecycleStatus::Succeeded,
        CampaignStatus::Failed => AdapterLifecycleStatus::Failed,
        CampaignStatus::Cancelled => AdapterLifecycleStatus::Cancelled,
    }
}

pub fn adapt_cron_attempt_status(status: &str) -> Result<AdapterLifecycleStatus> {
    match status {
        "running" => Ok(AdapterLifecycleStatus::Running),
        "succeeded" => Ok(AdapterLifecycleStatus::Succeeded),
        "failed" => Ok(AdapterLifecycleStatus::Failed),
        "cancelled" => Ok(AdapterLifecycleStatus::Cancelled),
        "released" | "expired" => Ok(AdapterLifecycleStatus::Ambiguous),
        _ => Err(invalid("unknown cron attempt status")),
    }
}

pub fn adapt_gateway_status(
    status: &str,
    terminal_reason: Option<&str>,
) -> Result<AdapterLifecycleStatus> {
    match status {
        "pending" => Ok(AdapterLifecycleStatus::Pending),
        "claimed" => Ok(AdapterLifecycleStatus::Running),
        "succeeded" => Ok(AdapterLifecycleStatus::Succeeded),
        "failed" if terminal_reason == Some("cancelled") => Ok(AdapterLifecycleStatus::Cancelled),
        "failed" => Ok(AdapterLifecycleStatus::Failed),
        _ => Err(invalid("unknown gateway message status")),
    }
}

fn validate_ports(ports: &[WorkflowPort]) -> Result<()> {
    if ports.len() > 256 {
        return Err(invalid("too many workflow ports"));
    }
    let mut names = BTreeSet::new();
    for port in ports {
        port.validate()?;
        if !names.insert(port.name.as_str()) {
            return Err(invalid("duplicate workflow port name"));
        }
    }
    Ok(())
}

fn validate_graph(nodes: &[WorkflowNode]) -> Result<()> {
    let mut indices = BTreeMap::new();
    for (index, node) in nodes.iter().enumerate() {
        if indices.insert(node.id.as_str(), index).is_some() {
            return Err(invalid("duplicate workflow node id"));
        }
    }
    let mut indegree = vec![0usize; nodes.len()];
    let mut dependents = vec![Vec::new(); nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        let mut unique = BTreeSet::new();
        for dependency in &node.dependencies {
            if dependency == &node.id || !unique.insert(dependency.as_str()) {
                return Err(invalid("workflow node has self or duplicate dependency"));
            }
            let dependency_index = *indices
                .get(dependency.as_str())
                .ok_or_else(|| invalid("workflow node names missing dependency"))?;
            indegree[index] += 1;
            dependents[dependency_index].push(index);
        }
    }
    let mut ready: VecDeque<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(index))
        .collect();
    let mut visited = 0usize;
    while let Some(index) = ready.pop_front() {
        visited += 1;
        for dependent in &dependents[index] {
            indegree[*dependent] -= 1;
            if indegree[*dependent] == 0 {
                ready.push_back(*dependent);
            }
        }
    }
    if visited != nodes.len() {
        return Err(invalid("workflow dependency graph contains a cycle"));
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
}

fn invalid(message: impl Into<String>) -> KernelError {
    KernelError::Tool(message.into())
}
