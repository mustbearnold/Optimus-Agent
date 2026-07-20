//! Versioned execution manifests, call provenance, and honest replay reports.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use optimus_packs::{ReplayClass, ToolOutcome};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{CompletionRequest, CompletionResponse, KernelError, Result, ToolCall};

pub const EXECUTION_MANIFEST_VERSION: u16 = 1;

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
               prompt_sha256 TEXT NOT NULL CHECK(length(prompt_sha256)=64),
               tool_catalog_sha256 TEXT NOT NULL CHECK(length(tool_catalog_sha256)=64),
               policy_sha256 TEXT NOT NULL CHECK(length(policy_sha256)=64),
               status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),
               created_unix INTEGER NOT NULL,completed_unix INTEGER
             );
             CREATE TABLE IF NOT EXISTS execution_model_calls(
               manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
               step INTEGER NOT NULL,provider TEXT NOT NULL,model TEXT NOT NULL,
               request_sha256 TEXT NOT NULL CHECK(length(request_sha256)=64),
               response_sha256 TEXT NOT NULL CHECK(length(response_sha256)=64),
               replay_class TEXT NOT NULL,
               PRIMARY KEY(manifest_id,step)
             );
             CREATE TABLE IF NOT EXISTS execution_tool_calls(
               manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
               call_id TEXT NOT NULL,tool_id TEXT NOT NULL,
               arguments_sha256 TEXT NOT NULL CHECK(length(arguments_sha256)=64),
               outcome_sha256 TEXT NOT NULL CHECK(length(outcome_sha256)=64),
               replay_class TEXT NOT NULL,effect_attempt_id TEXT,effect_sha256 TEXT,
               receipt_sha256 TEXT,
               PRIMARY KEY(manifest_id,call_id)
             );",
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
        if provider.trim().is_empty() || model.trim().is_empty() {
            return Err(KernelError::Model(
                "execution manifest requires provider and model identity".into(),
            ));
        }
        let id = Uuid::new_v4();
        self.conn.execute(
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
        Ok(id)
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

    pub fn record_model_call(
        &self,
        manifest_id: Uuid,
        step: u32,
        provider: &str,
        model: &str,
        request: &CompletionRequest,
        response: &CompletionResponse,
    ) -> Result<()> {
        let replay = if provider == "offline" {
            ReplayClass::FixtureReplayable
        } else {
            ReplayClass::ModelNondeterministic
        };
        self.conn.execute(
            "INSERT INTO execution_model_calls(
               manifest_id,step,provider,model,request_sha256,response_sha256,replay_class
             ) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                manifest_id.to_string(),
                step as i64,
                provider,
                model,
                sha256(&serde_json::to_vec(request)?),
                sha256(&serde_json::to_vec(response)?),
                replay_name(replay)
            ],
        )?;
        Ok(())
    }

    pub fn record_tool_call(
        &self,
        manifest_id: Uuid,
        call: &ToolCall,
        outcome: &ToolOutcome,
    ) -> Result<()> {
        let provenance = outcome.provenance.as_ref();
        self.conn.execute(
            "INSERT INTO execution_tool_calls(
               manifest_id,call_id,tool_id,arguments_sha256,outcome_sha256,replay_class,
               effect_attempt_id,effect_sha256,receipt_sha256
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                manifest_id.to_string(),
                call.id,
                outcome.tool_id.as_str(),
                sha256(&serde_json::to_vec(&call.arguments)?),
                sha256(&serde_json::to_vec(outcome)?),
                replay_name(outcome.replay),
                provenance.map(|value| value.effect_attempt_id.to_string()),
                provenance.map(|value| value.effect_sha256.as_str()),
                provenance.and_then(|value| value.receipt_sha256.as_deref())
            ],
        )?;
        Ok(())
    }

    pub fn finish(&self, manifest_id: Uuid, status: ExecutionStatus) -> Result<()> {
        if status == ExecutionStatus::Running {
            return Err(KernelError::Model(
                "execution settlement requires terminal status".into(),
            ));
        }
        let changed = self.conn.execute(
            "UPDATE execution_manifests SET status=?1,completed_unix=?2
             WHERE id=?3 AND status='running'",
            params![status.as_str(), now_unix() as i64, manifest_id.to_string()],
        )?;
        if changed != 1 {
            return Err(KernelError::Model(format!(
                "execution manifest is missing or already terminal: {manifest_id}"
            )));
        }
        Ok(())
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
                "codex",
                "gpt-5.6-terra",
                &CompletionRequest::default(),
                &CompletionResponse {
                    text: Some("answer".into()),
                    tool_calls: vec![],
                },
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
            )
            .unwrap();
        assert_eq!(
            store.replay_report(manifest).unwrap().classification,
            ReplayClassification::Ambiguous
        );
    }
}
