//! Versioned execution manifests, call provenance, and honest replay reports.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use optimus_packs::{ReplayClass, ToolOutcome};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    CompletionRequest, CompletionResponse, KernelError, Result, SpanId, ToolCall,
    ToolLifecycleEvent, TraceContext, TraceId,
};

pub const EXECUTION_MANIFEST_VERSION: u16 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedToolLifecycle {
    pub sequence: u64,
    pub turn_id: Uuid,
    pub event: ToolLifecycleEvent,
}

pub struct ExecutionStore {
    conn: Connection,
}

#[allow(clippy::too_many_arguments)]
fn insert_manifest(
    connection: &Connection,
    id: Uuid,
    session_id: Uuid,
    turn_id: Uuid,
    provider: &str,
    model: &str,
    prompt: &[u8],
    tool_catalog: &[u8],
    policy: &[u8],
) -> Result<()> {
    if provider.trim().is_empty() || model.trim().is_empty() {
        return Err(KernelError::Model(
            "execution manifest requires provider and model identity".into(),
        ));
    }
    connection.execute(
        "INSERT INTO execution_manifests(
           id,version,session_id,turn_id,provider,model,prompt_sha256,
           tool_catalog_sha256,policy_sha256,status,created_unix
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'running',?10)",
        params![
            id.to_string(),
            EXECUTION_MANIFEST_VERSION as i64,
            session_id.to_string(),
            turn_id.to_string(),
            provider,
            model,
            sha256(prompt),
            sha256(tool_catalog),
            sha256(policy),
            now_unix() as i64
        ],
    )?;
    Ok(())
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
             );",
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
                "SELECT id,version,session_id,turn_id,provider,model,prompt_sha256,
                        tool_catalog_sha256,policy_sha256,status
                 FROM execution_manifests WHERE id=?1",
                params![id.to_string()],
                |row| {
                    let parse = |value: String| {
                        Uuid::parse_str(&value).map_err(|error| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(error))
                        })
                    };
                    let status = match row.get::<_, String>(9)?.as_str() {
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
                        prompt_sha256: row.get(6)?,
                        tool_catalog_sha256: row.get(7)?,
                        policy_sha256: row.get(8)?,
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

fn ensure_column(connection: &Connection, table: &str, column: &str, sql_type: &str) -> Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !columns.iter().any(|value| value == column) {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {sql_type}"
        ))?;
    }
    Ok(())
}

fn read_classes(connection: &Connection, sql: &str, manifest_id: Uuid) -> Result<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![manifest_id.to_string()], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(KernelError::Sqlite)
}

fn replay_name(class: ReplayClass) -> &'static str {
    match class {
        ReplayClass::Deterministic => "deterministic",
        ReplayClass::Convergent => "convergent",
        ReplayClass::FixtureReplayable => "fixture_replayable",
        ReplayClass::ModelNondeterministic => "model_nondeterministic",
        ReplayClass::ExternalNondeterministic => "external_nondeterministic",
        ReplayClass::Destructive => "destructive",
        ReplayClass::Ambiguous => "ambiguous",
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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
}
