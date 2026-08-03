//! Versioned execution manifests, call provenance, and honest replay reports.

use std::path::Path;

use optimus_packs::{ReplayClass, ToolOutcome};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    execution_support::{
        ensure_column, insert_manifest, now_unix, read_classes, replay_name, sha256,
    },
    ChatApprovalStatus, CompletionRequest, CompletionResponse, KernelError, Result, SpanId,
    ToolApprovalBinding, ToolCall, ToolLifecycleEvent, ToolLifecyclePhase, TraceContext, TraceId,
};

pub const EXECUTION_MANIFEST_VERSION: u16 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimingEventKind {
    TurnStarted,
    ModelStarted,
    FirstResponse,
    ModelFinished,
    ToolStarted,
    ToolFinished,
    TurnFinished,
}

impl TimingEventKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TurnStarted => "turn_started",
            Self::ModelStarted => "model_started",
            Self::FirstResponse => "first_response",
            Self::ModelFinished => "model_finished",
            Self::ToolStarted => "tool_started",
            Self::ToolFinished => "tool_finished",
            Self::TurnFinished => "turn_finished",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimingEvent {
    pub kind: TimingEventKind,
    pub step: Option<u32>,
    pub call_id: Option<String>,
    pub name: Option<String>,
    pub duration_ms: Option<u64>,
    pub elapsed_ms: u64,
    pub status: Option<String>,
    pub suppressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ExecutionTimingSummary {
    pub total_ms: u64,
    pub first_response_ms: Option<u64>,
    pub model_ms: u64,
    pub tool_ms: u64,
    pub model_call_count: usize,
    pub executed_tool_call_count: usize,
    pub suppressed_tool_call_count: usize,
    pub terminal_status: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionManifest {
    pub id: Uuid,
    pub version: u16,
    pub session_id: Uuid,
    pub turn_id: Uuid,
    pub provider: String,
    pub model: String,
    pub autonomy_profile: String,
    pub command_fs_envelope: String,
    pub prompt_sha256: String,
    pub tool_catalog_sha256: String,
    pub policy_sha256: String,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClassification {
    Deterministic,
    FixtureReplayable,
    NonReplayable,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayReport {
    pub manifest_id: Uuid,
    pub classification: ReplayClassification,
    pub blockers: Vec<String>,
    pub model_call_count: usize,
    pub tool_call_count: usize,
}

/// Bounded projection of one model step for causal reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionModelCallSummary {
    pub step: u32,
    pub provider: String,
    pub model: String,
    pub request_sha256: String,
    pub response_sha256: String,
    pub replay_class: String,
    pub duration_ms: u64,
}

/// Bounded projection of one tool invocation for causal reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionToolCallSummary {
    pub call_id: String,
    pub tool_id: String,
    pub arguments_sha256: String,
    pub outcome_sha256: String,
    pub replay_class: String,
    pub effect_attempt_id: Option<String>,
    pub effect_sha256: Option<String>,
    pub receipt_sha256: Option<String>,
    pub duration_ms: u64,
    pub suppressed: bool,
}

/// Ordered tool lifecycle phase row for causal reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionToolLifecycleSummary {
    pub sequence: u64,
    pub event_id: String,
    pub call_id: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedToolLifecycle {
    pub sequence: u64,
    pub turn_id: Uuid,
    pub event: ToolLifecycleEvent,
}

pub struct ExecutionStore {
    conn: Connection,
}

impl ExecutionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS execution_manifests(
               id TEXT PRIMARY KEY,version INTEGER NOT NULL,session_id TEXT NOT NULL,
               turn_id TEXT NOT NULL UNIQUE,provider TEXT NOT NULL,model TEXT NOT NULL,
               autonomy_profile TEXT NOT NULL DEFAULT 'review_changes' CHECK(autonomy_profile IN (
                 'standard','review_changes','read_only','full_project','unrestricted_host'
               )),
               command_fs_envelope TEXT NOT NULL DEFAULT 'confined_no_network' CHECK(command_fs_envelope IN (
                 'confined','confined_no_network','unrestricted_host'
               )),
               prompt_sha256 TEXT NOT NULL CHECK(length(prompt_sha256)=64),
               tool_catalog_sha256 TEXT NOT NULL CHECK(length(tool_catalog_sha256)=64),
               policy_sha256 TEXT NOT NULL CHECK(length(policy_sha256)=64),
               status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),
               created_unix INTEGER NOT NULL,completed_unix INTEGER,
               duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0)
             );
             CREATE TABLE IF NOT EXISTS execution_model_calls(
               manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
               step INTEGER NOT NULL,provider TEXT NOT NULL,model TEXT NOT NULL,
               request_sha256 TEXT NOT NULL CHECK(length(request_sha256)=64),
               response_sha256 TEXT NOT NULL CHECK(length(response_sha256)=64),
               replay_class TEXT NOT NULL,
               duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0),
               PRIMARY KEY(manifest_id,step)
             );
             CREATE TABLE IF NOT EXISTS execution_tool_calls(
               manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
               call_id TEXT NOT NULL,tool_id TEXT NOT NULL,
               arguments_sha256 TEXT NOT NULL CHECK(length(arguments_sha256)=64),
               outcome_sha256 TEXT NOT NULL CHECK(length(outcome_sha256)=64),
               replay_class TEXT NOT NULL,effect_attempt_id TEXT,effect_sha256 TEXT,
               receipt_sha256 TEXT,duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0),
               suppressed INTEGER NOT NULL DEFAULT 0 CHECK(suppressed IN (0,1)),
               PRIMARY KEY(manifest_id,call_id)
             );
             CREATE TABLE IF NOT EXISTS execution_trace_links(
               manifest_id TEXT PRIMARY KEY REFERENCES execution_manifests(id) ON DELETE CASCADE,
               trace_id TEXT NOT NULL,span_id TEXT NOT NULL UNIQUE,parent_span_id TEXT
             );
             CREATE TABLE IF NOT EXISTS execution_timing_events(
               sequence INTEGER PRIMARY KEY AUTOINCREMENT,
               manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
               kind TEXT NOT NULL,step INTEGER,call_id TEXT,name TEXT,duration_ms INTEGER,
               elapsed_ms INTEGER NOT NULL CHECK(elapsed_ms >= 0),status TEXT,
               suppressed INTEGER NOT NULL CHECK(suppressed IN (0,1))
             );
             CREATE TABLE IF NOT EXISTS execution_tool_events(
               sequence INTEGER PRIMARY KEY AUTOINCREMENT,
               event_id TEXT NOT NULL UNIQUE,
               manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
               call_id TEXT NOT NULL,phase TEXT NOT NULL,
               event_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS execution_chat_approvals(
               manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
               call_id TEXT NOT NULL,
               binding_json TEXT NOT NULL,
               call_json TEXT NOT NULL,
               status TEXT NOT NULL CHECK(status IN ('pending','approved','denied')),
               PRIMARY KEY(manifest_id,call_id)
             );",
        )?;
        ensure_column(
            &conn,
            "execution_manifests",
            "autonomy_profile",
            "TEXT NOT NULL DEFAULT 'review_changes' CHECK(autonomy_profile IN ('standard','review_changes','read_only','full_project','unrestricted_host'))",
        )?;
        ensure_column(
            &conn,
            "execution_manifests",
            "command_fs_envelope",
            "TEXT NOT NULL DEFAULT 'confined_no_network' CHECK(command_fs_envelope IN ('confined','confined_no_network','unrestricted_host'))",
        )?;
        ensure_column(
            &conn,
            "execution_manifests",
            "duration_ms",
            "INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0)",
        )?;
        ensure_column(
            &conn,
            "execution_model_calls",
            "duration_ms",
            "INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0)",
        )?;
        ensure_column(
            &conn,
            "execution_tool_calls",
            "duration_ms",
            "INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0)",
        )?;
        ensure_column(
            &conn,
            "execution_tool_calls",
            "suppressed",
            "INTEGER NOT NULL DEFAULT 0 CHECK(suppressed IN (0,1))",
        )?;
        Ok(Self { conn })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
        provider: &str,
        model: &str,
        prompt: &[u8],
        tool_catalog: &[u8],
        policy: &[u8],
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        insert_manifest(
            &self.conn,
            id,
            session_id,
            turn_id,
            provider,
            model,
            "review_changes",
            "confined_no_network",
            prompt,
            tool_catalog,
            policy,
        )?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_traced(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
        provider: &str,
        model: &str,
        autonomy_profile: &str,
        command_fs_envelope: &str,
        prompt: &[u8],
        tool_catalog: &[u8],
        policy: &[u8],
    ) -> Result<(Uuid, TraceContext)> {
        let id = Uuid::new_v4();
        let context = TraceContext::new(TraceId::new(), SpanId::new(), None);
        let transaction = self.conn.unchecked_transaction()?;
        insert_manifest(
            &transaction,
            id,
            session_id,
            turn_id,
            provider,
            model,
            autonomy_profile,
            command_fs_envelope,
            prompt,
            tool_catalog,
            policy,
        )?;
        transaction.execute(
            "INSERT INTO execution_trace_links(manifest_id,trace_id,span_id,parent_span_id)
             VALUES(?1,?2,?3,NULL)",
            params![
                id.to_string(),
                context.trace_id.to_string(),
                context.span_id.to_string()
            ],
        )?;
        transaction.commit()?;
        Ok((id, context))
    }

    pub fn find_by_turn(&self, turn_id: Uuid) -> Result<Option<Uuid>> {
        self.conn
            .query_row(
                "SELECT id FROM execution_manifests WHERE turn_id=?1",
                params![turn_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|id| Uuid::parse_str(&id).map_err(KernelError::Uuid))
            .transpose()
    }

    /// Locate the execution manifest bound to a root (or any) trace identity.
    pub fn find_by_trace_id(&self, trace_id: TraceId) -> Result<Option<Uuid>> {
        self.conn
            .query_row(
                "SELECT manifest_id FROM execution_trace_links WHERE trace_id=?1
                 ORDER BY manifest_id LIMIT 1",
                params![trace_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|id| Uuid::parse_str(&id).map_err(KernelError::Uuid))
            .transpose()
    }

    pub fn list_model_calls(&self, manifest_id: Uuid) -> Result<Vec<ExecutionModelCallSummary>> {
        let mut statement = self.conn.prepare(
            "SELECT step,provider,model,request_sha256,response_sha256,replay_class,duration_ms
             FROM execution_model_calls WHERE manifest_id=?1 ORDER BY step",
        )?;
        let rows = statement.query_map(params![manifest_id.to_string()], |row| {
            Ok(ExecutionModelCallSummary {
                step: row.get::<_, i64>(0)? as u32,
                provider: row.get(1)?,
                model: row.get(2)?,
                request_sha256: row.get(3)?,
                response_sha256: row.get(4)?,
                replay_class: row.get(5)?,
                duration_ms: row.get::<_, i64>(6)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(KernelError::Sqlite)
    }

    pub fn list_tool_calls(&self, manifest_id: Uuid) -> Result<Vec<ExecutionToolCallSummary>> {
        let mut statement = self.conn.prepare(
            "SELECT call_id,tool_id,arguments_sha256,outcome_sha256,replay_class,
                    effect_attempt_id,effect_sha256,receipt_sha256,duration_ms,suppressed
             FROM execution_tool_calls WHERE manifest_id=?1 ORDER BY call_id",
        )?;
        let rows = statement.query_map(params![manifest_id.to_string()], |row| {
            Ok(ExecutionToolCallSummary {
                call_id: row.get(0)?,
                tool_id: row.get(1)?,
                arguments_sha256: row.get(2)?,
                outcome_sha256: row.get(3)?,
                replay_class: row.get(4)?,
                effect_attempt_id: row.get(5)?,
                effect_sha256: row.get(6)?,
                receipt_sha256: row.get(7)?,
                duration_ms: row.get::<_, i64>(8)? as u64,
                suppressed: row.get::<_, i64>(9)? != 0,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(KernelError::Sqlite)
    }

    pub fn list_tool_lifecycle_phases(
        &self,
        manifest_id: Uuid,
    ) -> Result<Vec<ExecutionToolLifecycleSummary>> {
        let mut statement = self.conn.prepare(
            "SELECT event_id,call_id,phase,sequence FROM execution_tool_events
             WHERE manifest_id=?1 ORDER BY sequence",
        )?;
        let rows = statement.query_map(params![manifest_id.to_string()], |row| {
            Ok(ExecutionToolLifecycleSummary {
                event_id: row.get(0)?,
                call_id: row.get(1)?,
                phase: row.get(2)?,
                sequence: row.get::<_, i64>(3)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(KernelError::Sqlite)
    }

    pub fn list_recent_manifests(&self, limit: usize) -> Result<Vec<ExecutionManifest>> {
        let limit = limit.clamp(1, 200) as i64;
        let mut statement = self.conn.prepare(
            "SELECT id FROM execution_manifests ORDER BY created_unix DESC, id DESC LIMIT ?1",
        )?;
        let ids = statement
            .query_map(params![limit], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(KernelError::Sqlite)?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(self.manifest(Uuid::parse_str(&id)?)?);
        }
        Ok(out)
    }

    pub fn bind_trace(&self, manifest_id: Uuid, context: TraceContext) -> Result<()> {
        self.conn.execute(
            "INSERT INTO execution_trace_links(manifest_id,trace_id,span_id,parent_span_id)
             VALUES(?1,?2,?3,?4)",
            params![
                manifest_id.to_string(),
                context.trace_id.to_string(),
                context.span_id.to_string(),
                context.parent_span_id.map(|value| value.to_string())
            ],
        )?;
        Ok(())
    }

    pub fn trace_context(&self, manifest_id: Uuid) -> Result<Option<TraceContext>> {
        self.conn
            .query_row(
                "SELECT trace_id,span_id,parent_span_id FROM execution_trace_links
                 WHERE manifest_id=?1",
                params![manifest_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(trace, span, parent)| {
                Ok(TraceContext::new(
                    TraceId::parse(&trace)?,
                    SpanId::parse(&span)?,
                    parent.as_deref().map(SpanId::parse).transpose()?,
                ))
            })
            .transpose()
    }

    pub fn record_model_call(
        &self,
        manifest_id: Uuid,
        step: u32,
        identity: (&str, &str),
        request: &CompletionRequest,
        response: &CompletionResponse,
        duration_ms: u64,
    ) -> Result<()> {
        let (provider, model) = identity;
        let replay = if provider == "offline" {
            ReplayClass::FixtureReplayable
        } else {
            ReplayClass::ModelNondeterministic
        };
        self.conn.execute(
            "INSERT INTO execution_model_calls(
               manifest_id,step,provider,model,request_sha256,response_sha256,replay_class,duration_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                manifest_id.to_string(),
                step as i64,
                provider,
                model,
                sha256(&serde_json::to_vec(request)?),
                sha256(&serde_json::to_vec(response)?),
                replay_name(replay),
                duration_ms as i64
            ],
        )?;
        Ok(())
    }

    pub fn record_tool_call(
        &self,
        manifest_id: Uuid,
        call: &ToolCall,
        outcome: &ToolOutcome,
        duration_ms: u64,
        suppressed: bool,
    ) -> Result<()> {
        let provenance = outcome.provenance.as_ref();
        self.conn.execute(
            "INSERT INTO execution_tool_calls(
               manifest_id,call_id,tool_id,arguments_sha256,outcome_sha256,replay_class,
               effect_attempt_id,effect_sha256,receipt_sha256,duration_ms,suppressed
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                manifest_id.to_string(),
                call.id,
                outcome.tool_id.as_str(),
                sha256(&serde_json::to_vec(&call.arguments)?),
                sha256(&serde_json::to_vec(outcome)?),
                replay_name(outcome.replay),
                provenance.map(|value| value.effect_attempt_id.to_string()),
                provenance.map(|value| value.effect_sha256.as_str()),
                provenance.and_then(|value| value.receipt_sha256.as_deref()),
                duration_ms as i64,
                i64::from(suppressed)
            ],
        )?;
        Ok(())
    }

    /// Persist the runtime-owned lifecycle transition before it is projected to a stream.
    /// Repeated delivery attempts are idempotent by stable event identity.
    pub fn record_tool_lifecycle_event(
        &self,
        manifest_id: Uuid,
        event: &ToolLifecycleEvent,
    ) -> Result<()> {
        if event.run_id != manifest_id.to_string() {
            return Err(KernelError::Model(
                "tool lifecycle event run identity does not match execution manifest".into(),
            ));
        }
        let event_json = serde_json::to_string(event)?;
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO execution_tool_events(
               event_id,manifest_id,call_id,phase,event_json
             ) VALUES(?1,?2,?3,?4,?5)",
            params![
                event.event_id,
                manifest_id.to_string(),
                event.call_id,
                event.phase.as_str(),
                event_json
            ],
        )?;
        if inserted == 0 {
            let existing: String = self.conn.query_row(
                "SELECT event_json FROM execution_tool_events WHERE event_id=?1",
                params![event.event_id],
                |row| row.get(0),
            )?;
            if existing != event_json {
                return Err(KernelError::Model(format!(
                    "conflicting durable tool lifecycle event: {}",
                    event.event_id
                )));
            }
        }
        Ok(())
    }

    /// Persist the approval-required event and its exact runtime identity in one transaction.
    pub fn record_chat_approval_required(
        &self,
        manifest_id: Uuid,
        call: &ToolCall,
        event: &ToolLifecycleEvent,
        binding: &ToolApprovalBinding,
    ) -> Result<()> {
        if event.phase != ToolLifecyclePhase::ApprovalRequired
            || event.approval.as_ref() != Some(binding)
            || event.run_id != manifest_id.to_string()
            || event.call_id != call.id
            || binding.run_id != manifest_id
            || binding.call_id != call.id
        {
            return Err(KernelError::Model(
                "approval lifecycle event and exact runtime binding disagree".into(),
            ));
        }
        let event_json = serde_json::to_string(event)?;
        let binding_json = serde_json::to_string(binding)?;
        let call_json = serde_json::to_string(call)?;
        let transaction = self.conn.unchecked_transaction()?;
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO execution_tool_events(
               event_id,manifest_id,call_id,phase,event_json
             ) VALUES(?1,?2,?3,?4,?5)",
            params![
                event.event_id,
                manifest_id.to_string(),
                event.call_id,
                event.phase.as_str(),
                event_json
            ],
        )?;
        if inserted == 0 {
            let existing: String = transaction.query_row(
                "SELECT event_json FROM execution_tool_events WHERE event_id=?1",
                params![event.event_id],
                |row| row.get(0),
            )?;
            if existing != event_json {
                return Err(KernelError::Model(format!(
                    "conflicting durable tool lifecycle event: {}",
                    event.event_id
                )));
            }
        }
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO execution_chat_approvals(
               manifest_id,call_id,binding_json,call_json,status
             ) VALUES(?1,?2,?3,?4,'pending')",
            params![manifest_id.to_string(), call.id, binding_json, call_json],
        )?;
        if inserted == 0 {
            let existing: (String, String, String) = transaction.query_row(
                "SELECT binding_json,call_json,status FROM execution_chat_approvals
                 WHERE manifest_id=?1 AND call_id=?2",
                params![manifest_id.to_string(), call.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            if existing != (binding_json, call_json, "pending".to_string()) {
                return Err(KernelError::Model(format!(
                    "conflicting or already resolved chat approval for call {}",
                    call.id
                )));
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn pending_chat_approval(
        &self,
        manifest_id: Uuid,
        call_id: &str,
    ) -> Result<Option<(ToolApprovalBinding, ToolCall)>> {
        self.conn
            .query_row(
                "SELECT binding_json,call_json FROM execution_chat_approvals
                 WHERE manifest_id=?1 AND call_id=?2 AND status='pending'",
                params![manifest_id.to_string(), call_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(binding, call)| {
                Ok((
                    serde_json::from_str(&binding)?,
                    serde_json::from_str(&call)?,
                ))
            })
            .transpose()
    }

    /// Find the exact chat approval a surface must restore for one session.
    ///
    /// A parked turn remains running while its approval row remains pending.
    /// Looking it up through the manifest keeps the renderer from having to
    /// know the execution schema or infer a call from painted transcript text.
    /// There can be only one actionable pending call for a session at a time,
    /// but the ordering makes recovery deterministic if an interrupted older
    /// manifest is present as well.
    pub fn pending_chat_approval_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Option<(ToolApprovalBinding, ToolCall)>> {
        self.conn
            .query_row(
                "SELECT a.binding_json,a.call_json
                 FROM execution_chat_approvals a
                 JOIN execution_manifests m ON m.id=a.manifest_id
                 WHERE m.session_id=?1 AND m.status='running' AND a.status='pending'
                 ORDER BY m.created_unix DESC,m.id DESC,a.rowid DESC
                 LIMIT 1",
                params![session_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(binding, call)| {
                Ok((
                    serde_json::from_str(&binding)?,
                    serde_json::from_str(&call)?,
                ))
            })
            .transpose()
    }

    pub fn has_pending_chat_approval(&self, manifest_id: Uuid) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM execution_chat_approvals
                   WHERE manifest_id=?1 AND status='pending'
                 )",
                params![manifest_id.to_string()],
                |row| row.get(0),
            )
            .map_err(KernelError::Sqlite)
    }

    pub fn finish_chat_approval(
        &self,
        manifest_id: Uuid,
        call_id: &str,
        status: ChatApprovalStatus,
    ) -> Result<()> {
        let status = match status {
            ChatApprovalStatus::Approved => "approved",
            ChatApprovalStatus::Denied => "denied",
        };
        let changed = self.conn.execute(
            "UPDATE execution_chat_approvals SET status=?1
             WHERE manifest_id=?2 AND call_id=?3 AND status='pending'",
            params![status, manifest_id.to_string(), call_id],
        )?;
        if changed != 1 {
            return Err(KernelError::Model(format!(
                "chat approval is missing, foreign, or already resolved: {call_id}"
            )));
        }
        Ok(())
    }

    pub fn tool_lifecycle_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<PersistedToolLifecycle>> {
        let mut statement = self.conn.prepare(
            "SELECT e.sequence,m.turn_id,e.event_json
             FROM execution_tool_events e
             JOIN execution_manifests m ON m.id=e.manifest_id
             WHERE m.session_id=?1
             ORDER BY e.sequence",
        )?;
        let rows = statement.query_map(params![session_id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (sequence, turn_id, event_json) = row?;
            Ok(PersistedToolLifecycle {
                sequence: sequence as u64,
                turn_id: Uuid::parse_str(&turn_id)?,
                event: serde_json::from_str(&event_json)?,
            })
        })
        .collect()
    }

    pub fn record_timing_event(&self, manifest_id: Uuid, event: &TimingEvent) -> Result<()> {
        self.conn.execute(
            "INSERT INTO execution_timing_events(
               manifest_id,kind,step,call_id,name,duration_ms,elapsed_ms,status,suppressed
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                manifest_id.to_string(),
                event.kind.as_str(),
                event.step.map(i64::from),
                event.call_id,
                event.name,
                event.duration_ms.map(|value| value as i64),
                event.elapsed_ms as i64,
                event.status,
                i64::from(event.suppressed)
            ],
        )?;
        Ok(())
    }

    pub fn finish(&self, manifest_id: Uuid, status: ExecutionStatus) -> Result<()> {
        self.finish_timed(manifest_id, status, 0)
    }

    pub fn finish_timed(
        &self,
        manifest_id: Uuid,
        status: ExecutionStatus,
        duration_ms: u64,
    ) -> Result<()> {
        if status == ExecutionStatus::Running {
            return Err(KernelError::Model(
                "execution settlement requires terminal status".into(),
            ));
        }
        let changed = self.conn.execute(
            "UPDATE execution_manifests SET status=?1,completed_unix=?2,duration_ms=?3
             WHERE id=?4 AND status='running'",
            params![
                status.as_str(),
                now_unix() as i64,
                duration_ms as i64,
                manifest_id.to_string()
            ],
        )?;
        if changed != 1 {
            return Err(KernelError::Model(format!(
                "execution manifest is missing or already terminal: {manifest_id}"
            )));
        }
        Ok(())
    }

    pub fn timing_summary(&self, manifest_id: Uuid) -> Result<ExecutionTimingSummary> {
        self.conn
            .query_row(
                "SELECT m.duration_ms,
                        (SELECT elapsed_ms FROM execution_timing_events WHERE manifest_id=m.id AND kind='first_response' ORDER BY sequence LIMIT 1),
                        COALESCE((SELECT sum(duration_ms) FROM execution_timing_events WHERE manifest_id=m.id AND kind='model_finished'),0),
                        COALESCE((SELECT sum(duration_ms) FROM execution_timing_events WHERE manifest_id=m.id AND kind='tool_finished' AND suppressed=0),0),
                        (SELECT count(*) FROM execution_timing_events WHERE manifest_id=m.id AND kind='model_finished'),
                        (SELECT count(*) FROM execution_timing_events WHERE manifest_id=m.id AND kind='tool_finished' AND suppressed=0),
                        (SELECT count(*) FROM execution_timing_events WHERE manifest_id=m.id AND kind='tool_finished' AND suppressed=1),
                        CASE WHEN m.status='running' THEN NULL ELSE m.status END
                 FROM execution_manifests m WHERE m.id=?1",
                params![manifest_id.to_string()],
                |row| {
                    Ok(ExecutionTimingSummary {
                        total_ms: row.get::<_, i64>(0)? as u64,
                        first_response_ms: row
                            .get::<_, Option<i64>>(1)?
                            .map(|value| value as u64),
                        model_ms: row.get::<_, i64>(2)? as u64,
                        tool_ms: row.get::<_, i64>(3)? as u64,
                        model_call_count: row.get::<_, i64>(4)? as usize,
                        executed_tool_call_count: row.get::<_, i64>(5)? as usize,
                        suppressed_tool_call_count: row.get::<_, i64>(6)? as usize,
                        terminal_status: row.get(7)?,
                    })
                },
            )
            .map_err(KernelError::Sqlite)
    }

    pub fn manifest(&self, id: Uuid) -> Result<ExecutionManifest> {
        self.conn
            .query_row(
                "SELECT id,version,session_id,turn_id,provider,model,autonomy_profile,command_fs_envelope,prompt_sha256,
                        tool_catalog_sha256,policy_sha256,status
                 FROM execution_manifests WHERE id=?1",
                params![id.to_string()],
                |row| {
                    let parse = |value: String| {
                        Uuid::parse_str(&value).map_err(|error| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                        })
                    };
                    let status = match row.get::<_, String>(11)?.as_str() {
                        "running" => ExecutionStatus::Running,
                        "succeeded" => ExecutionStatus::Succeeded,
                        "failed" => ExecutionStatus::Failed,
                        "cancelled" => ExecutionStatus::Cancelled,
                        other => {
                            return Err(rusqlite::Error::ToSqlConversionFailure(
                                format!("invalid execution status: {other}").into(),
                            ))
                        }
                    };
                    Ok(ExecutionManifest {
                        id: parse(row.get(0)?)?,
                        version: row.get::<_, i64>(1)? as u16,
                        session_id: parse(row.get(2)?)?,
                        turn_id: parse(row.get(3)?)?,
                        provider: row.get(4)?,
                        model: row.get(5)?,
                        autonomy_profile: row.get(6)?,
                        command_fs_envelope: row.get(7)?,
                        prompt_sha256: row.get(8)?,
                        tool_catalog_sha256: row.get(9)?,
                        policy_sha256: row.get(10)?,
                        status,
                    })
                },
            )
            .map_err(KernelError::Sqlite)
    }

    pub fn replay_report(&self, manifest_id: Uuid) -> Result<ReplayReport> {
        let model_classes = read_classes(
            &self.conn,
            "SELECT replay_class FROM execution_model_calls WHERE manifest_id=?1 ORDER BY step",
            manifest_id,
        )?;
        let tool_classes = read_classes(
            &self.conn,
            "SELECT replay_class FROM execution_tool_calls WHERE manifest_id=?1 ORDER BY call_id",
            manifest_id,
        )?;
        let mut blockers = Vec::new();
        let mut ambiguous = false;
        let mut non_replayable = false;
        for class in model_classes.iter().chain(tool_classes.iter()) {
            match class.as_str() {
                "ambiguous" => ambiguous = true,
                "model_nondeterministic" | "external_nondeterministic" | "destructive" => {
                    non_replayable = true;
                    blockers.push(class.clone());
                }
                _ => {}
            }
        }
        blockers.sort();
        blockers.dedup();
        let classification = if ambiguous {
            ReplayClassification::Ambiguous
        } else if non_replayable {
            ReplayClassification::NonReplayable
        } else if model_classes
            .iter()
            .any(|value| value == "fixture_replayable")
        {
            ReplayClassification::FixtureReplayable
        } else {
            ReplayClassification::Deterministic
        };
        Ok(ReplayReport {
            manifest_id,
            classification,
            blockers,
            model_call_count: model_classes.len(),
            tool_call_count: tool_classes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optimus_packs::{ReplayClass, ToolErrorDetail, ToolOutcome, ToolOutcomeKind};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn remote_model_call_is_honestly_non_replayable() {
        let directory = tempdir().unwrap();
        let store = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
        let manifest = store
            .begin(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "codex",
                "gpt-5.6-terra",
                b"prompt",
                b"tools",
                b"policy",
            )
            .unwrap();
        store
            .record_model_call(
                manifest,
                1,
                ("codex", "gpt-5.6-terra"),
                &CompletionRequest::default(),
                &CompletionResponse {
                    text: Some("answer".into()),
                    tool_calls: vec![],
                },
                17,
            )
            .unwrap();
        let report = store.replay_report(manifest).unwrap();
        assert_eq!(report.classification, ReplayClassification::NonReplayable);
        assert_eq!(report.blockers, vec!["model_nondeterministic"]);
    }

    #[test]
    fn ambiguous_tool_outcome_dominates_replay_report() {
        let directory = tempdir().unwrap();
        let store = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
        let manifest = store
            .begin(
                Uuid::new_v4(),
                Uuid::new_v4(),
                "offline",
                "offline-scripted",
                b"prompt",
                b"tools",
                b"policy",
            )
            .unwrap();
        let mut outcome = ToolOutcome::failed(
            "call-1",
            "terminal",
            "terminal outcome is unknown",
            ToolErrorDetail {
                code: "effect_ambiguous".into(),
                message: "effect terminal state is unknown".into(),
                retryable: false,
            },
            ReplayClass::Ambiguous,
        );
        outcome.kind = ToolOutcomeKind::Ambiguous;
        store
            .record_tool_call(
                manifest,
                &ToolCall {
                    id: "call-1".into(),
                    name: "terminal".into(),
                    arguments: json!({"program":"x"}),
                },
                &outcome,
                9,
                false,
            )
            .unwrap();
        assert_eq!(
            store.replay_report(manifest).unwrap().classification,
            ReplayClassification::Ambiguous
        );
    }

    #[test]
    fn tool_lifecycle_is_durable_ordered_and_idempotent() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("execution.db");
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let store = ExecutionStore::open(&path).unwrap();
        let manifest = store
            .begin(
                session_id,
                turn_id,
                "offline",
                "offline-scripted",
                b"prompt",
                b"tools",
                b"policy",
            )
            .unwrap();
        let started = ToolLifecycleEvent {
            schema_version: crate::TOOL_LIFECYCLE_SCHEMA_VERSION,
            event_id: format!("{manifest}:call-1:started"),
            run_id: manifest.to_string(),
            call_id: "call-1".into(),
            tool_id: optimus_packs::ToolId::new("read_file"),
            phase: crate::ToolLifecyclePhase::Started,
            summary: "Reading".into(),
            duration_ms: None,
            outcome: None,
            approval: None,
        };
        let completed = ToolLifecycleEvent {
            event_id: format!("{manifest}:call-1:succeeded"),
            phase: crate::ToolLifecyclePhase::Succeeded,
            summary: "Read file".into(),
            duration_ms: Some(8),
            outcome: Some(ToolOutcome::succeeded(
                "call-1",
                "read_file",
                "Read file",
                json!({"text":"ok"}),
                ReplayClass::Deterministic,
            )),
            ..started.clone()
        };
        store
            .record_tool_lifecycle_event(manifest, &started)
            .unwrap();
        store
            .record_tool_lifecycle_event(manifest, &started)
            .unwrap();
        store
            .record_tool_lifecycle_event(manifest, &completed)
            .unwrap();
        drop(store);

        let reopened = ExecutionStore::open(&path).unwrap();
        let events = reopened.tool_lifecycle_for_session(session_id).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].turn_id, turn_id);
        assert_eq!(events[0].event.phase, crate::ToolLifecyclePhase::Started);
        assert_eq!(events[1].event.phase, crate::ToolLifecyclePhase::Succeeded);
        assert_eq!(
            events[1].event.outcome.as_ref().unwrap().summary,
            "Read file"
        );
        assert!(events[0].sequence < events[1].sequence);
    }

    #[test]
    fn pending_chat_approval_is_recoverable_by_session_after_reopen() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("execution.db");
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let store = ExecutionStore::open(&path).unwrap();
        let manifest = store
            .begin(
                session_id,
                turn_id,
                "open-ai-compat",
                "fixture",
                b"prompt",
                b"tools",
                b"policy",
            )
            .unwrap();
        let call = ToolCall {
            id: "approval-call-1".into(),
            name: "write_file".into(),
            arguments: json!({"path":"proof.txt","contents":"ok"}),
        };
        let binding = ToolApprovalBinding {
            run_id: manifest,
            call_id: call.id.clone(),
            tool_id: optimus_packs::ToolId::new("write_file"),
            job_id: optimus_runtime::job_id(Uuid::new_v4()),
            node_id: Uuid::new_v4(),
            node_index: 0,
            effect_sha256: "0".repeat(64),
            summary: "Write proof.txt (2 bytes)".into(),
        };
        let event = ToolLifecycleEvent {
            schema_version: crate::TOOL_LIFECYCLE_SCHEMA_VERSION,
            event_id: format!("{manifest}:{}:approval_required", call.id),
            run_id: manifest.to_string(),
            call_id: call.id.clone(),
            tool_id: binding.tool_id.clone(),
            phase: crate::ToolLifecyclePhase::ApprovalRequired,
            summary: binding.summary.clone(),
            duration_ms: Some(3),
            outcome: None,
            approval: Some(binding.clone()),
        };
        store
            .record_chat_approval_required(manifest, &call, &event, &binding)
            .unwrap();
        drop(store);

        let reopened = ExecutionStore::open(&path).unwrap();
        let (found_binding, found_call) = reopened
            .pending_chat_approval_for_session(session_id)
            .unwrap()
            .expect("pending approval should survive reopening the store");
        assert_eq!(found_binding, binding);
        assert_eq!(found_call, call);
    }
}
