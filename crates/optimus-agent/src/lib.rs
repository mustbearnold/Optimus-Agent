//! Versioned specialist-agent contracts and a fail-closed immutable registry.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use optimus_graph::JobId;
use optimus_packs::{DurableEffectProvenance, ToolId};
use optimus_runtime::{CancellationToken, Runtime};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("uuid: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("runtime: {0}")]
    Runtime(#[from] optimus_runtime::RuntimeError),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, AgentError>;

pub const AGENT_REQUEST_SCHEMA_VERSION: u16 = 1;
pub const AGENT_RESULT_SCHEMA_VERSION: u16 = 1;
const MAX_TASK_BYTES: usize = 64 * 1024;
const MAX_SUMMARY_BYTES: usize = 16 * 1024;
const MAX_CONTEXT_REFS: usize = 128;
const MAX_ARTIFACTS: usize = 128;
const MAX_UNRESOLVED: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let valid = (1..=64).contains(&value.len())
            && value
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase())
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            });
        if !valid {
            return Err(invalid("agent id must match [a-z][a-z0-9_-]{0,63}"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentVersion(String);

impl AgentVersion {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let parts: Vec<_> = value.split('.').collect();
        let valid = parts.len() == 3
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| byte.is_ascii_digit())
                    && (part == &"0" || !part.starts_with('0'))
                    && part.parse::<u32>().is_ok()
            });
        if !valid {
            return Err(invalid("agent version must be canonical major.minor.patch"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentContextRef {
    pub source_id: String,
    pub sha256: String,
}

impl AgentContextRef {
    fn validate(&self) -> Result<()> {
        if self.source_id.trim().is_empty() || self.source_id.len() > 2048 {
            return Err(invalid("agent context source id is empty or too large"));
        }
        validate_sha256(&self.sha256, "agent context")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPermissions {
    pub filesystem_roots: BTreeSet<String>,
    pub network_hosts: BTreeSet<String>,
    pub effects: BTreeSet<String>,
}

impl AgentPermissions {
    pub fn is_subset_of(&self, ceiling: &Self) -> bool {
        self.filesystem_roots.is_subset(&ceiling.filesystem_roots)
            && self.network_hosts.is_subset(&ceiling.network_hosts)
            && self.effects.is_subset(&ceiling.effects)
    }

    fn validate(&self) -> Result<()> {
        for value in self
            .filesystem_roots
            .iter()
            .chain(self.network_hosts.iter())
            .chain(self.effects.iter())
        {
            if value.trim().is_empty() || value.len() > 1024 {
                return Err(invalid(
                    "agent permission values must be bounded and non-empty",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBudget {
    pub max_steps: u32,
    pub timeout_ms: u64,
    pub max_context_chars: u32,
    pub max_output_chars: u32,
}

impl AgentBudget {
    fn validate(&self) -> Result<()> {
        if self.max_steps == 0
            || self.max_steps > 10_000
            || self.timeout_ms == 0
            || self.timeout_ms > 86_400_000
            || self.max_context_chars == 0
            || self.max_context_chars > 16_000_000
            || self.max_output_chars == 0
            || self.max_output_chars > 4_000_000
        {
            return Err(invalid("agent budget is zero or exceeds host bounds"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRequest {
    pub schema_version: u16,
    pub agent_id: AgentId,
    pub agent_version: AgentVersion,
    pub task: String,
    pub context: Vec<AgentContextRef>,
    pub constraints: Vec<String>,
    pub tools: Vec<ToolId>,
    pub permissions: AgentPermissions,
    pub budget: AgentBudget,
    pub cancellation_id: Uuid,
    pub trace_id: Uuid,
}

impl AgentRequest {
    pub fn validate(
        &self,
        available_tools: &BTreeSet<ToolId>,
        permission_ceiling: &AgentPermissions,
    ) -> Result<()> {
        if self.schema_version != AGENT_REQUEST_SCHEMA_VERSION {
            return Err(invalid("unsupported agent request schema version"));
        }
        AgentId::parse(self.agent_id.as_str())?;
        AgentVersion::parse(self.agent_version.as_str())?;
        if self.task.trim().is_empty() || self.task.len() > MAX_TASK_BYTES {
            return Err(invalid("agent task is empty or too large"));
        }
        if self.context.len() > MAX_CONTEXT_REFS {
            return Err(invalid("too many agent context references"));
        }
        for reference in &self.context {
            reference.validate()?;
        }
        if self.constraints.len() > 128
            || self
                .constraints
                .iter()
                .any(|constraint| constraint.trim().is_empty() || constraint.len() > 4096)
        {
            return Err(invalid("agent constraints are invalid or unbounded"));
        }
        let unique_tools: BTreeSet<_> = self.tools.iter().cloned().collect();
        if unique_tools.len() != self.tools.len() || !unique_tools.is_subset(available_tools) {
            return Err(invalid("agent request has duplicate or unavailable tools"));
        }
        self.permissions.validate()?;
        if !self.permissions.is_subset_of(permission_ceiling) {
            return Err(invalid("agent request permissions exceed host ceiling"));
        }
        self.budget.validate()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentResultKind {
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentArtifactRef {
    pub uri: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentResult {
    pub schema_version: u16,
    pub invocation_id: Uuid,
    pub kind: AgentResultKind,
    pub summary: String,
    pub error: Option<AgentFailure>,
    pub cancellation_reason: Option<String>,
    pub evidence: Vec<AgentContextRef>,
    pub artifacts: Vec<AgentArtifactRef>,
    pub unresolved: Vec<String>,
}

impl AgentResult {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != AGENT_RESULT_SCHEMA_VERSION {
            return Err(invalid("unsupported agent result schema version"));
        }
        if self.summary.len() > MAX_SUMMARY_BYTES
            || self.evidence.len() > MAX_CONTEXT_REFS
            || self.artifacts.len() > MAX_ARTIFACTS
            || self.unresolved.len() > MAX_UNRESOLVED
            || self
                .unresolved
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 4096)
        {
            return Err(invalid("agent result exceeds bounded fields"));
        }
        for evidence in &self.evidence {
            evidence.validate()?;
        }
        for artifact in &self.artifacts {
            if artifact.uri.trim().is_empty() || artifact.uri.len() > 2048 {
                return Err(invalid("agent artifact uri is empty or too large"));
            }
            validate_sha256(&artifact.sha256, "agent artifact")?;
        }
        let valid = match self.kind {
            AgentResultKind::Succeeded => {
                self.error.is_none() && self.cancellation_reason.is_none()
            }
            AgentResultKind::Failed => {
                self.error.as_ref().is_some_and(|error| {
                    !error.code.trim().is_empty() && !error.message.trim().is_empty()
                }) && self.cancellation_reason.is_none()
            }
            AgentResultKind::Cancelled => {
                self.error.is_none()
                    && self
                        .cancellation_reason
                        .as_ref()
                        .is_some_and(|reason| !reason.trim().is_empty())
            }
            AgentResultKind::Ambiguous => {
                self.cancellation_reason.is_none() && !self.unresolved.is_empty()
            }
        };
        if !valid {
            return Err(invalid("agent result fields contradict terminal kind"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDescriptor {
    pub id: AgentId,
    pub version: AgentVersion,
    pub responsibility: String,
    pub request_schema_version: u16,
    pub result_schema_version: u16,
    pub required_tools: Vec<ToolId>,
    pub permissions: AgentPermissions,
}

impl AgentDescriptor {
    pub fn validate(
        &self,
        available_tools: &BTreeSet<ToolId>,
        permission_ceiling: &AgentPermissions,
    ) -> Result<()> {
        AgentId::parse(self.id.as_str())?;
        AgentVersion::parse(self.version.as_str())?;
        if self.responsibility.trim().is_empty() || self.responsibility.len() > 4096 {
            return Err(invalid("agent responsibility is empty or too large"));
        }
        if self.request_schema_version != AGENT_REQUEST_SCHEMA_VERSION
            || self.result_schema_version != AGENT_RESULT_SCHEMA_VERSION
        {
            return Err(invalid(
                "agent descriptor names unsupported schema versions",
            ));
        }
        let tools: BTreeSet<_> = self.required_tools.iter().cloned().collect();
        if tools.len() != self.required_tools.len() || !tools.is_subset(available_tools) {
            return Err(invalid(
                "agent descriptor has duplicate or unavailable tools",
            ));
        }
        self.permissions.validate()?;
        if !self.permissions.is_subset_of(permission_ceiling) {
            return Err(invalid("agent descriptor permissions exceed host ceiling"));
        }
        Ok(())
    }
}

pub struct AgentRegistry {
    conn: Connection,
    available_tools: BTreeSet<ToolId>,
    permission_ceiling: AgentPermissions,
}

impl AgentRegistry {
    pub fn open(
        path: impl AsRef<Path>,
        available_tools: BTreeSet<ToolId>,
        permission_ceiling: AgentPermissions,
    ) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS agent_registry(
               agent_id TEXT NOT NULL,
               agent_version TEXT NOT NULL,
               descriptor_json TEXT NOT NULL,
               PRIMARY KEY(agent_id,agent_version)
             );",
        )?;
        Ok(Self {
            conn,
            available_tools,
            permission_ceiling,
        })
    }

    pub fn register(&self, descriptor: &AgentDescriptor) -> Result<()> {
        descriptor.validate(&self.available_tools, &self.permission_ceiling)?;
        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM agent_registry WHERE agent_id=?1 AND agent_version=?2",
                params![descriptor.id.as_str(), descriptor.version.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(invalid("agent descriptor identity/version already exists"));
        }
        self.conn.execute(
            "INSERT INTO agent_registry(agent_id,agent_version,descriptor_json)
             VALUES(?1,?2,?3)",
            params![
                descriptor.id.as_str(),
                descriptor.version.as_str(),
                serde_json::to_string(descriptor)?
            ],
        )?;
        Ok(())
    }

    pub fn validate_request(&self, request: &AgentRequest) -> Result<AgentDescriptor> {
        request.validate(&self.available_tools, &self.permission_ceiling)?;
        let descriptor = self
            .get(&request.agent_id, &request.agent_version)?
            .ok_or_else(|| invalid("agent request names an unregistered descriptor"))?;
        let requested_tools: BTreeSet<_> = request.tools.iter().cloned().collect();
        let descriptor_tools: BTreeSet<_> = descriptor.required_tools.iter().cloned().collect();
        if !requested_tools.is_subset(&descriptor_tools)
            || !request.permissions.is_subset_of(&descriptor.permissions)
        {
            return Err(invalid(
                "agent request exceeds descriptor tools or permission ceiling",
            ));
        }
        Ok(descriptor)
    }

    pub fn get(&self, id: &AgentId, version: &AgentVersion) -> Result<Option<AgentDescriptor>> {
        let raw = self
            .conn
            .query_row(
                "SELECT descriptor_json FROM agent_registry
                 WHERE agent_id=?1 AND agent_version=?2",
                params![id.as_str(), version.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        raw.map(|raw| {
            let descriptor: AgentDescriptor = serde_json::from_str(&raw)?;
            if descriptor.id != *id || descriptor.version != *version {
                return Err(invalid("persisted agent descriptor identity mismatch"));
            }
            descriptor.validate(&self.available_tools, &self.permission_ceiling)?;
            Ok(descriptor)
        })
        .transpose()
    }

    pub fn list(&self) -> Result<Vec<AgentDescriptor>> {
        let mut statement = self.conn.prepare(
            "SELECT agent_id,agent_version FROM agent_registry
             ORDER BY agent_id,agent_version",
        )?;
        let keys = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut descriptors = Vec::new();
        for key in keys {
            let (id, version) = key?;
            let id = AgentId::parse(id)?;
            let version = AgentVersion::parse(version)?;
            descriptors.push(
                self.get(&id, &version)?
                    .ok_or_else(|| invalid("agent registry row disappeared during read"))?,
            );
        }
        Ok(descriptors)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentInvocationStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

impl AgentInvocationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Ambiguous => "ambiguous",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "ambiguous" => Ok(Self::Ambiguous),
            _ => Err(invalid("persisted agent invocation status is invalid")),
        }
    }

    fn from_result(kind: AgentResultKind) -> Self {
        match kind {
            AgentResultKind::Succeeded => Self::Succeeded,
            AgentResultKind::Failed => Self::Failed,
            AgentResultKind::Cancelled => Self::Cancelled,
            AgentResultKind::Ambiguous => Self::Ambiguous,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInvocation {
    pub id: Uuid,
    pub request: AgentRequest,
    pub retry_of: Option<Uuid>,
    pub status: AgentInvocationStatus,
    pub cancellation_reason: Option<String>,
    pub result: Option<AgentResult>,
    pub created_unix: u64,
    pub completed_unix: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInvocationEvent {
    pub seq: i64,
    pub event_id: Uuid,
    pub invocation_id: Uuid,
    pub kind: String,
    pub created_unix: u64,
}

pub struct AgentInvocationStore {
    conn: Connection,
}

impl AgentInvocationStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS agent_invocations(
               id TEXT PRIMARY KEY,
               agent_id TEXT NOT NULL,
               agent_version TEXT NOT NULL,
               request_json TEXT NOT NULL,
               retry_of TEXT REFERENCES agent_invocations(id),
               status TEXT NOT NULL CHECK(status IN (
                 'running','succeeded','failed','cancelled','ambiguous'
               )),
               cancellation_reason TEXT,
               result_json TEXT,
               created_unix INTEGER NOT NULL,
               completed_unix INTEGER
             );
             CREATE TABLE IF NOT EXISTS agent_invocation_events(
               seq INTEGER PRIMARY KEY AUTOINCREMENT,
               event_id TEXT NOT NULL UNIQUE,
               invocation_id TEXT NOT NULL REFERENCES agent_invocations(id) ON DELETE CASCADE,
               kind TEXT NOT NULL,
               payload_json TEXT NOT NULL,
               created_unix INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX IF NOT EXISTS one_agent_terminal_event
               ON agent_invocation_events(invocation_id)
               WHERE kind IN ('succeeded','failed','cancelled','ambiguous');
             CREATE TABLE IF NOT EXISTS agent_invocation_effects(
               invocation_id TEXT NOT NULL REFERENCES agent_invocations(id) ON DELETE CASCADE,
               effect_attempt_id TEXT NOT NULL UNIQUE,
               job_id TEXT NOT NULL,
               node_id TEXT NOT NULL,
               effect_sha256 TEXT NOT NULL CHECK(length(effect_sha256)=64),
               receipt_sha256 TEXT CHECK(receipt_sha256 IS NULL OR length(receipt_sha256)=64),
               PRIMARY KEY(invocation_id,effect_attempt_id)
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn begin(&self, registry: &AgentRegistry, request: &AgentRequest) -> Result<Uuid> {
        registry.validate_request(request)?;
        self.begin_validated(request, None)
    }

    pub fn begin_retry(
        &self,
        registry: &AgentRegistry,
        prior_id: Uuid,
        request: &AgentRequest,
    ) -> Result<Uuid> {
        registry.validate_request(request)?;
        let prior = self.get(prior_id)?;
        if prior.status == AgentInvocationStatus::Running
            || prior.request.agent_id != request.agent_id
            || prior.request.agent_version != request.agent_version
        {
            return Err(invalid(
                "agent retry requires terminal predecessor with matching descriptor",
            ));
        }
        self.begin_validated(request, Some(prior_id))
    }

    fn begin_validated(&self, request: &AgentRequest, retry_of: Option<Uuid>) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = now_unix();
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO agent_invocations(
               id,agent_id,agent_version,request_json,retry_of,status,created_unix
             ) VALUES(?1,?2,?3,?4,?5,'running',?6)",
            params![
                id.to_string(),
                request.agent_id.as_str(),
                request.agent_version.as_str(),
                serde_json::to_string(request)?,
                retry_of.map(|value| value.to_string()),
                now as i64,
            ],
        )?;
        insert_invocation_event(
            &transaction,
            id,
            "accepted",
            &serde_json::json!({"retry_of": retry_of}),
            now,
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn get(&self, id: Uuid) -> Result<AgentInvocation> {
        self.conn
            .query_row(
                "SELECT request_json,retry_of,status,cancellation_reason,result_json,
                        created_unix,completed_unix
                 FROM agent_invocations WHERE id=?1",
                params![id.to_string()],
                |row| {
                    let request_json: String = row.get(0)?;
                    let retry_of: Option<String> = row.get(1)?;
                    let status: String = row.get(2)?;
                    let result_json: Option<String> = row.get(4)?;
                    Ok((
                        request_json,
                        retry_of,
                        status,
                        row.get::<_, Option<String>>(3)?,
                        result_json,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| invalid("agent invocation not found"))
            .and_then(
                |(
                    request_json,
                    retry_of,
                    status,
                    cancellation_reason,
                    result_json,
                    created,
                    completed,
                )| {
                    let request: AgentRequest = serde_json::from_str(&request_json)?;
                    let retry_of = retry_of
                        .map(|value| Uuid::parse_str(&value).map_err(AgentError::Uuid))
                        .transpose()?;
                    let result = result_json
                        .map(|value| serde_json::from_str::<AgentResult>(&value))
                        .transpose()?;
                    if let Some(result) = &result {
                        result.validate()?;
                        if result.invocation_id != id {
                            return Err(invalid("persisted agent result identity mismatch"));
                        }
                    }
                    Ok(AgentInvocation {
                        id,
                        request,
                        retry_of,
                        status: AgentInvocationStatus::parse(&status)?,
                        cancellation_reason,
                        result,
                        created_unix: created as u64,
                        completed_unix: completed.map(|value| value as u64),
                    })
                },
            )
    }

    pub fn events(&self, id: Uuid) -> Result<Vec<AgentInvocationEvent>> {
        self.get(id)?;
        let mut statement = self.conn.prepare(
            "SELECT seq,event_id,kind,created_unix FROM agent_invocation_events
             WHERE invocation_id=?1 ORDER BY seq",
        )?;
        let rows = statement.query_map(params![id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (seq, event_id, kind, created) = row?;
            events.push(AgentInvocationEvent {
                seq,
                event_id: Uuid::parse_str(&event_id)?,
                invocation_id: id,
                kind,
                created_unix: created as u64,
            });
        }
        Ok(events)
    }

    pub fn request_cancellation(&self, id: Uuid, reason: &str) -> Result<bool> {
        if reason.trim().is_empty() || reason.len() > 4096 {
            return Err(invalid("agent cancellation reason is empty or too large"));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let current = transaction
            .query_row(
                "SELECT status,cancellation_reason FROM agent_invocations WHERE id=?1",
                params![id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?
            .ok_or_else(|| invalid("agent invocation not found"))?;
        if current.0 != "running" || current.1.is_some() {
            transaction.commit()?;
            return Ok(false);
        }
        transaction.execute(
            "UPDATE agent_invocations SET cancellation_reason=?1 WHERE id=?2",
            params![reason, id.to_string()],
        )?;
        insert_invocation_event(
            &transaction,
            id,
            "cancellation_requested",
            &serde_json::json!({"reason": reason}),
            now_unix(),
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn sync_cancellation(&self, id: Uuid, token: &CancellationToken) -> Result<bool> {
        let invocation = self.get(id)?;
        if invocation.cancellation_reason.is_some() {
            token.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn settle(&self, result: &AgentResult) -> Result<()> {
        result.validate()?;
        let transaction = self.conn.unchecked_transaction()?;
        let (status, cancellation_reason) = transaction
            .query_row(
                "SELECT status,cancellation_reason FROM agent_invocations WHERE id=?1",
                params![result.invocation_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?
            .ok_or_else(|| invalid("agent invocation not found"))?;
        if status != "running" {
            return Err(invalid("agent invocation already has a terminal outcome"));
        }
        if cancellation_reason.is_some() && result.kind != AgentResultKind::Cancelled {
            return Err(invalid(
                "cancelled agent invocation rejects late non-cancel outcome",
            ));
        }
        if result.kind == AgentResultKind::Cancelled && cancellation_reason.is_none() {
            return Err(invalid(
                "agent cancellation must be requested before settlement",
            ));
        }
        let terminal = AgentInvocationStatus::from_result(result.kind);
        let now = now_unix();
        let changed = transaction.execute(
            "UPDATE agent_invocations SET status=?1,result_json=?2,completed_unix=?3
             WHERE id=?4 AND status='running'",
            params![
                terminal.as_str(),
                serde_json::to_string(result)?,
                now as i64,
                result.invocation_id.to_string(),
            ],
        )?;
        if changed != 1 {
            return Err(invalid("agent terminal settlement lost ownership"));
        }
        insert_invocation_event(
            &transaction,
            result.invocation_id,
            terminal.as_str(),
            &serde_json::json!({"kind": terminal.as_str()}),
            now,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn link_effect(
        &self,
        runtime: &Runtime,
        invocation_id: Uuid,
        provenance: &DurableEffectProvenance,
    ) -> Result<()> {
        self.get(invocation_id)?;
        let actual = runtime
            .latest_effect_outcome(JobId(provenance.job_id))?
            .ok_or_else(|| invalid("agent effect provenance has no terminal runtime attempt"))?;
        if actual.attempt_id != provenance.effect_attempt_id
            || actual.job_id.0 != provenance.job_id
            || actual.node_id != provenance.node_id
            || actual.effect_hash != provenance.effect_sha256
            || actual.receipt_hash != provenance.receipt_sha256
        {
            return Err(invalid(
                "agent effect provenance does not match runtime outcome",
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO agent_invocation_effects(
               invocation_id,effect_attempt_id,job_id,node_id,effect_sha256,receipt_sha256
             ) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                invocation_id.to_string(),
                provenance.effect_attempt_id.to_string(),
                provenance.job_id.to_string(),
                provenance.node_id.to_string(),
                provenance.effect_sha256,
                provenance.receipt_sha256,
            ],
        )?;
        insert_invocation_event(
            &transaction,
            invocation_id,
            "effect_linked",
            &serde_json::json!({"effect_attempt_id": provenance.effect_attempt_id}),
            now_unix(),
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn insert_invocation_event(
    transaction: &Transaction<'_>,
    invocation_id: Uuid,
    kind: &str,
    payload: &impl Serialize,
    created_unix: u64,
) -> Result<()> {
    let payload = serde_json::to_string(payload)?;
    if payload.len() > 16 * 1024 {
        return Err(invalid("agent invocation event payload exceeds bound"));
    }
    transaction.execute(
        "INSERT INTO agent_invocation_events(
           event_id,invocation_id,kind,payload_json,created_unix
         ) VALUES(?1,?2,?3,?4,?5)",
        params![
            Uuid::new_v4().to_string(),
            invocation_id.to_string(),
            kind,
            payload,
            created_unix as i64,
        ],
    )?;
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{label} sha256 is invalid")));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> AgentError {
    AgentError::Msg(message.into())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_accepts_valid_ids() {
        for id in [
            "a",
            "agent_1",
            "my-agent",
            "a-b_c9",
            "x".repeat(64).as_str(),
        ] {
            let parsed = AgentId::parse(id).expect("valid id should parse");
            assert_eq!(parsed.as_str(), id);
        }
    }

    #[test]
    fn agent_id_rejects_invalid_ids() {
        for id in [
            "",
            "A",
            "Agent",
            "with space",
            "-lead",
            "9lead",
            "x".repeat(65).as_str(),
        ] {
            assert!(AgentId::parse(id).is_err(), "id {id:?} should be rejected");
        }
    }

    #[test]
    fn agent_version_accepts_canonical_semver() {
        for version in ["0.0.0", "1.2.3", "10.20.30", "0.1.0"] {
            let parsed = AgentVersion::parse(version).expect("valid version should parse");
            assert_eq!(parsed.as_str(), version);
        }
    }

    #[test]
    fn agent_version_rejects_non_canonical_semver() {
        for version in [
            "1", "1.2", "1.2.3.4", "01.2.3", "1.02.3", "1.2.03", "1.2.a", "1.2.-3", "",
        ] {
            assert!(
                AgentVersion::parse(version).is_err(),
                "version {version:?} should be rejected"
            );
        }
    }

    #[test]
    fn validate_sha256_accepts_only_64_hex_digits() {
        assert!(validate_sha256(
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            "test"
        )
        .is_ok());
        assert!(validate_sha256("short", "test").is_err());
        assert!(validate_sha256(&"a".repeat(64).replace('a', "g"), "test").is_err());
    }
}
