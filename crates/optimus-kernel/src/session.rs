//! Durable chat session store.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{KernelError, Message, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: Uuid,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub packs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionEffectLink {
    pub tool_call_id: String,
    pub job_id: Uuid,
    pub node_id: Uuid,
    pub effect_attempt_id: Uuid,
    pub effect_hash: String,
    pub outcome: String,
    pub receipt_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TurnStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid turn status: {other}").into(),
            )),
        }
    }

    fn is_terminal(self) -> bool {
        self != Self::Running
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub status: TurnStatus,
    pub start_message_count: usize,
    pub accepted_message_count: usize,
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                packs_json TEXT NOT NULL,
                messages_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_effect_links (
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                tool_call_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                effect_attempt_id TEXT NOT NULL,
                effect_hash TEXT NOT NULL CHECK(length(effect_hash)=64),
                outcome TEXT NOT NULL CHECK(outcome IN (
                    'succeeded','failed','ambiguous','interrupted','cancelled'
                )),
                receipt_hash TEXT CHECK(receipt_hash IS NULL OR length(receipt_hash)=64),
                linked_at TEXT NOT NULL,
                PRIMARY KEY(session_id,tool_call_id),
                UNIQUE(session_id,effect_attempt_id)
            );
            CREATE TABLE IF NOT EXISTS session_turns (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),
                start_message_count INTEGER NOT NULL CHECK(start_message_count >= 0),
                accepted_message_count INTEGER NOT NULL CHECK(accepted_message_count > start_message_count),
                error_code TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                CHECK((status IN ('running','succeeded') AND error_code IS NULL)
                   OR (status IN ('failed','cancelled') AND error_code IS NOT NULL))
            );
            CREATE UNIQUE INDEX IF NOT EXISTS one_running_turn_per_session
              ON session_turns(session_id) WHERE status='running';
            CREATE TABLE IF NOT EXISTS session_turn_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                turn_id TEXT NOT NULL REFERENCES session_turns(id) ON DELETE CASCADE,
                event_type TEXT NOT NULL CHECK(event_type IN ('accepted','succeeded','failed','cancelled')),
                recorded_at TEXT NOT NULL,
                UNIQUE(turn_id,event_type)
            );
            ",
        )?;
        Ok(Self { conn })
    }

    pub fn create(&self, title: &str) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = chrono_stamp();
        self.conn.execute(
            "INSERT INTO sessions(id, title, created_at, updated_at, packs_json, messages_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id.to_string(), title, now, now, "[]", "[]"],
        )?;
        Ok(id)
    }

    pub fn save(
        &self,
        id: Uuid,
        title: &str,
        packs: &[String],
        messages: &[Message],
    ) -> Result<()> {
        self.save_with_effect_links(id, title, packs, messages, &[])
    }

    pub fn save_with_effect_links(
        &self,
        id: Uuid,
        title: &str,
        packs: &[String],
        messages: &[Message],
        links: &[SessionEffectLink],
    ) -> Result<()> {
        let now = chrono_stamp();
        let packs_json = serde_json::to_string(packs)?;
        let messages_json = serde_json::to_string(messages)?;
        let transaction = self.conn.unchecked_transaction()?;
        let n = transaction.execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2, packs_json = ?3, messages_json = ?4
             WHERE id = ?5",
            params![title, now, packs_json, messages_json, id.to_string()],
        )?;
        if n == 0 {
            transaction.execute(
                "INSERT INTO sessions(id, title, created_at, updated_at, packs_json, messages_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id.to_string(), title, now, now, packs_json, messages_json],
            )?;
        }
        for link in links {
            if link.tool_call_id.trim().is_empty() {
                return Err(KernelError::Model(
                    "session effect link requires tool_call_id".into(),
                ));
            }
            let inserted = transaction.execute(
                "INSERT OR IGNORE INTO session_effect_links(
                   session_id,tool_call_id,job_id,node_id,effect_attempt_id,effect_hash,
                   outcome,receipt_hash,linked_at
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    id.to_string(),
                    link.tool_call_id,
                    link.job_id.to_string(),
                    link.node_id.to_string(),
                    link.effect_attempt_id.to_string(),
                    link.effect_hash,
                    link.outcome,
                    link.receipt_hash,
                    now
                ],
            )?;
            if inserted == 0 {
                let existing = transaction.query_row(
                    "SELECT job_id,node_id,effect_attempt_id,effect_hash,outcome,receipt_hash,tool_call_id
                     FROM session_effect_links WHERE session_id=?1 AND tool_call_id=?2",
                    params![id.to_string(), link.tool_call_id],
                    effect_link_from_row,
                )?;
                if existing != *link {
                    return Err(KernelError::Model(format!(
                        "conflicting durable effect provenance for tool call {}",
                        link.tool_call_id
                    )));
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn begin_turn(
        &self,
        session_id: Uuid,
        title: &str,
        packs: &[String],
        messages: &[Message],
        start_message_count: usize,
    ) -> Result<Uuid> {
        if messages.len() <= start_message_count {
            return Err(KernelError::Model(
                "accepted turn must append a transcript segment".into(),
            ));
        }
        let id = Uuid::new_v4();
        let now = chrono_stamp();
        let transaction = self.conn.unchecked_transaction()?;
        let updated = transaction.execute(
            "UPDATE sessions SET title=?1,updated_at=?2,packs_json=?3,messages_json=?4 WHERE id=?5",
            params![
                title,
                now,
                serde_json::to_string(packs)?,
                serde_json::to_string(messages)?,
                session_id.to_string()
            ],
        )?;
        if updated != 1 {
            return Err(KernelError::Model(format!(
                "session not found: {session_id}"
            )));
        }
        transaction.execute(
            "INSERT INTO session_turns(
               id,session_id,status,start_message_count,accepted_message_count,error_code,created_at,updated_at
             ) VALUES(?1,?2,'running',?3,?4,NULL,?5,?5)",
            params![
                id.to_string(),
                session_id.to_string(),
                start_message_count as i64,
                messages.len() as i64,
                now
            ],
        )?;
        transaction.execute(
            "INSERT INTO session_turn_events(turn_id,event_type,recorded_at) VALUES(?1,'accepted',?2)",
            params![id.to_string(), now],
        )?;
        transaction.commit()?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_turn(
        &self,
        turn_id: Uuid,
        session_id: Uuid,
        title: &str,
        packs: &[String],
        messages: &[Message],
        status: TurnStatus,
        error_code: Option<&str>,
    ) -> Result<()> {
        if !status.is_terminal() {
            return Err(KernelError::Model(
                "turn settlement requires a terminal status".into(),
            ));
        }
        if status == TurnStatus::Succeeded && error_code.is_some()
            || matches!(status, TurnStatus::Failed | TurnStatus::Cancelled) && error_code.is_none()
        {
            return Err(KernelError::Model(
                "turn terminal status and error code disagree".into(),
            ));
        }
        let now = chrono_stamp();
        let transaction = self.conn.unchecked_transaction()?;
        let settled = transaction.execute(
            "UPDATE session_turns SET status=?1,error_code=?2,updated_at=?3
             WHERE id=?4 AND session_id=?5 AND status='running'",
            params![
                status.as_str(),
                error_code,
                now,
                turn_id.to_string(),
                session_id.to_string()
            ],
        )?;
        if settled != 1 {
            return Err(KernelError::Model(format!(
                "turn is missing, foreign, or already terminal: {turn_id}"
            )));
        }
        let updated = transaction.execute(
            "UPDATE sessions SET title=?1,updated_at=?2,packs_json=?3,messages_json=?4 WHERE id=?5",
            params![
                title,
                now,
                serde_json::to_string(packs)?,
                serde_json::to_string(messages)?,
                session_id.to_string()
            ],
        )?;
        if updated != 1 {
            return Err(KernelError::Model(format!(
                "session not found: {session_id}"
            )));
        }
        transaction.execute(
            "INSERT INTO session_turn_events(turn_id,event_type,recorded_at) VALUES(?1,?2,?3)",
            params![turn_id.to_string(), status.as_str(), now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn active_turn(&self, session_id: Uuid) -> Result<Option<TurnRecord>> {
        self.conn
            .query_row(
                "SELECT id,session_id,status,start_message_count,accepted_message_count,error_code,created_at,updated_at
                 FROM session_turns WHERE session_id=?1 AND status='running'",
                params![session_id.to_string()],
                turn_from_row,
            )
            .optional()
            .map_err(KernelError::Sqlite)
    }

    pub fn turns(&self, session_id: Uuid) -> Result<Vec<TurnRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT id,session_id,status,start_message_count,accepted_message_count,error_code,created_at,updated_at
             FROM session_turns WHERE session_id=?1 ORDER BY created_at,id",
        )?;
        let rows = statement.query_map(params![session_id.to_string()], turn_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(KernelError::Sqlite)
    }

    pub fn turn_event_count(&self, turn_id: Uuid) -> Result<usize> {
        self.conn
            .query_row(
                "SELECT count(*) FROM session_turn_events WHERE turn_id=?1",
                params![turn_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)
            .map_err(KernelError::Sqlite)
    }

    pub fn effect_links(&self, id: Uuid) -> Result<Vec<SessionEffectLink>> {
        let mut statement = self.conn.prepare(
            "SELECT job_id,node_id,effect_attempt_id,effect_hash,outcome,receipt_hash,tool_call_id
             FROM session_effect_links WHERE session_id=?1 ORDER BY linked_at,tool_call_id",
        )?;
        let rows = statement.query_map(params![id.to_string()], effect_link_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(KernelError::Sqlite)
    }

    pub fn load(&self, id: Uuid) -> Result<(Vec<String>, Vec<Message>, String)> {
        let row = self
            .conn
            .query_row(
                "SELECT packs_json, messages_json, title FROM sessions WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    KernelError::Model(format!("session not found: {id}"))
                }
                other => KernelError::Model(other.to_string()),
            })?;
        let packs: Vec<String> = serde_json::from_str(&row.0)?;
        let messages: Vec<Message> = serde_json::from_str(&row.1)?;
        Ok((packs, messages, row.2))
    }

    pub fn list(&self) -> Result<Vec<SessionMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, packs_json, messages_json
             FROM sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id = Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            let packs: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default();
            let messages: Vec<Message> =
                serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
            Ok(SessionMeta {
                id,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                message_count: messages.len(),
                packs,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| KernelError::Model(e.to_string()))?);
        }
        Ok(out)
    }

    pub fn exists(&self, id: Uuid) -> Result<bool> {
        let n: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(n.is_some())
    }

    pub fn delete(&self, id: Uuid) -> Result<bool> {
        let n = self.conn.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(n > 0)
    }

    /// Rename session title only (preserves packs + messages).
    pub fn rename(&self, id: Uuid, title: &str) -> Result<bool> {
        let title = title.trim();
        if title.is_empty() {
            return Err(KernelError::Model("title required".into()));
        }
        let now = chrono_stamp();
        let n = self.conn.execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id.to_string()],
        )?;
        Ok(n > 0)
    }
}

fn parse_uuid(value: String, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn effect_link_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionEffectLink> {
    Ok(SessionEffectLink {
        job_id: parse_uuid(row.get(0)?, 0)?,
        node_id: parse_uuid(row.get(1)?, 1)?,
        effect_attempt_id: parse_uuid(row.get(2)?, 2)?,
        effect_hash: row.get(3)?,
        outcome: row.get(4)?,
        receipt_hash: row.get(5)?,
        tool_call_id: row.get(6)?,
    })
}

fn turn_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TurnRecord> {
    Ok(TurnRecord {
        id: parse_uuid(row.get(0)?, 0)?,
        session_id: parse_uuid(row.get(1)?, 1)?,
        status: TurnStatus::parse(&row.get::<_, String>(2)?)?,
        start_message_count: row.get::<_, i64>(3)? as usize,
        accepted_message_count: row.get::<_, i64>(4)? as usize,
        error_code: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn chrono_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("ts:{secs}")
}
