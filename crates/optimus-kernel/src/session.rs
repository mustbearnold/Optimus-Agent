//! Durable chat session store.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{KernelError, Message, Result, Role};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: Uuid,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub packs: Vec<String>,
    /// Durable pin (not presentation-only localStorage).
    #[serde(default)]
    pub pinned: bool,
    /// Soft-hide from active list until unarchived.
    #[serde(default)]
    pub archived: bool,
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
        // program P24 hygiene columns (idempotent migration).
        let mut has_pinned = false;
        let mut has_archived = false;
        {
            let mut stmt = conn.prepare("PRAGMA table_info(sessions)")?;
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(1)?;
                Ok(name)
            })?;
            for name in rows.flatten() {
                if name == "pinned" {
                    has_pinned = true;
                }
                if name == "archived" {
                    has_archived = true;
                }
            }
        }
        if !has_pinned {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_archived {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        conn.execute_batch(
            "
            CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
                title,
                body,
                session_id UNINDEXED,
                tokenize = 'porter unicode61'
            );
            ",
        )?;
        let store = Self { conn };
        store.backfill_fts_if_empty()?;
        Ok(store)
    }

    /// Rebuild FTS when empty (migration / first open after upgrade).
    fn backfill_fts_if_empty(&self) -> Result<()> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions_fts", [], |r| r.get(0))?;
        if n > 0 {
            return Ok(());
        }
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, messages_json FROM sessions")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let tx = self.conn.unchecked_transaction()?;
        for row in rows {
            let (id_s, title, messages_json) =
                row.map_err(|e| KernelError::Model(e.to_string()))?;
            let id = Uuid::parse_str(&id_s).map_err(|e| KernelError::Model(e.to_string()))?;
            let messages: Vec<Message> = serde_json::from_str(&messages_json).unwrap_or_default();
            Self::reindex_fts_tx(&tx, id, &title, &messages)?;
        }
        tx.commit()?;
        Ok(())
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
                "INSERT INTO sessions(id, title, created_at, updated_at, packs_json, messages_json, pinned, archived)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0)",
                params![id.to_string(), title, now, now, packs_json, messages_json],
            )?;
        }
        Self::reindex_fts_tx(&transaction, id, title, messages)?;
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
        Self::reindex_fts_tx(&transaction, session_id, title, messages)?;
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
        Self::reindex_fts_tx(&transaction, session_id, title, messages)?;
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

    /// Load a session and inject missing tool messages for durable effect links.
    ///
    /// When an effect commits but the transcript save fails, effect links can
    /// outlive tool messages. Reopen reconstructs a deterministic tool message
    /// from the link so users see provenance rather than a silent gap.
    pub fn load_repairing_effect_transcript(
        &self,
        id: Uuid,
    ) -> Result<(Vec<String>, Vec<Message>, String, usize)> {
        let (packs, mut messages, title) = self.load(id)?;
        let links = self.effect_links(id)?;
        let mut injected = 0usize;
        for link in &links {
            let present = messages.iter().any(|message| {
                message.role == Role::Tool
                    && message.tool_call_id.as_deref() == Some(link.tool_call_id.as_str())
            });
            if present {
                continue;
            }
            let content = serde_json::json!({
                "repaired": true,
                "ok": link.outcome == "succeeded",
                "data": {
                    "job": link.job_id,
                    "node_id": link.node_id,
                    "effect_attempt_id": link.effect_attempt_id,
                    "effect_hash": link.effect_hash,
                    "outcome": link.outcome,
                    "receipt_hash": link.receipt_hash,
                }
            });
            messages.push(Message {
                role: Role::Tool,
                content: content.to_string(),
                tool_call_id: Some(link.tool_call_id.clone()),
                name: None,
            });
            injected += 1;
        }
        if injected > 0 {
            self.save(id, &title, &packs, &messages)?;
        }
        Ok((packs, messages, title, injected))
    }

    pub fn list(&self) -> Result<Vec<SessionMeta>> {
        self.list_filtered(ListFilter::default())
    }

    /// List with durable sort: pinned first, active before archived, then updated_at.
    pub fn list_filtered(&self, filter: ListFilter) -> Result<Vec<SessionMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, packs_json, messages_json,
                    COALESCE(pinned, 0), COALESCE(archived, 0)
             FROM sessions
             ORDER BY COALESCE(pinned, 0) DESC,
                      COALESCE(archived, 0) ASC,
                      updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| self.meta_from_row(row))?;
        let mut out = Vec::new();
        for r in rows {
            let meta = r.map_err(|e| KernelError::Model(e.to_string()))?;
            if !filter.include_archived && meta.archived {
                continue;
            }
            out.push(meta);
        }
        Ok(out)
    }

    /// FTS over title + message bodies (program P24 / S2.3).
    pub fn search(&self, query: &str, include_archived: bool) -> Result<Vec<SessionMeta>> {
        let q = query.trim();
        if q.is_empty() {
            return self.list_filtered(ListFilter { include_archived });
        }
        // Escape FTS5 special chars lightly: quote terms.
        let match_q = fts_match_query(q);
        if match_q.is_empty() {
            // Punctuation-only / no usable tokens → empty hits, not SQLite error.
            return Ok(vec![]);
        }
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.created_at, s.updated_at, s.packs_json, s.messages_json,
                    COALESCE(s.pinned, 0), COALESCE(s.archived, 0)
             FROM sessions s
             INNER JOIN sessions_fts f ON f.session_id = s.id
             WHERE sessions_fts MATCH ?1
             ORDER BY COALESCE(s.pinned, 0) DESC,
                      COALESCE(s.archived, 0) ASC,
                      s.updated_at DESC
             LIMIT 100",
        )?;
        let rows = stmt.query_map(params![match_q], |row| self.meta_from_row(row))?;
        let mut out = Vec::new();
        for r in rows {
            let meta = r.map_err(|e| KernelError::Model(e.to_string()))?;
            if !include_archived && meta.archived {
                continue;
            }
            out.push(meta);
        }
        Ok(out)
    }

    pub fn set_pinned(&self, id: Uuid, pinned: bool) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE sessions SET pinned = ?1 WHERE id = ?2",
            params![if pinned { 1 } else { 0 }, id.to_string()],
        )?;
        Ok(n > 0)
    }

    pub fn set_archived(&self, id: Uuid, archived: bool) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE sessions SET archived = ?1 WHERE id = ?2",
            params![if archived { 1 } else { 0 }, id.to_string()],
        )?;
        Ok(n > 0)
    }

    fn meta_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMeta> {
        let id = Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        let packs: Vec<String> =
            serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default();
        let messages: Vec<Message> =
            serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
        let pinned: i64 = row.get(6)?;
        let archived: i64 = row.get(7)?;
        Ok(SessionMeta {
            id,
            title: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            message_count: messages.len(),
            packs,
            pinned: pinned != 0,
            archived: archived != 0,
        })
    }

    fn reindex_fts_tx(
        tx: &rusqlite::Transaction<'_>,
        id: Uuid,
        title: &str,
        messages: &[Message],
    ) -> Result<()> {
        let body = messages
            .iter()
            .filter(|m| matches!(m.role, Role::User | Role::Assistant))
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        tx.execute(
            "DELETE FROM sessions_fts WHERE session_id = ?1",
            params![id.to_string()],
        )?;
        tx.execute(
            "INSERT INTO sessions_fts(title, body, session_id) VALUES (?1, ?2, ?3)",
            params![title, body, id.to_string()],
        )?;
        Ok(())
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
        let tx = self.conn.unchecked_transaction()?;
        let n = tx.execute(
            "UPDATE sessions SET title = ?1 WHERE id = ?2",
            params![title, id.to_string()],
        )?;
        if n > 0 {
            let messages_json: String = tx.query_row(
                "SELECT messages_json FROM sessions WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )?;
            let messages: Vec<Message> = serde_json::from_str(&messages_json).unwrap_or_default();
            Self::reindex_fts_tx(&tx, id, title, &messages)?;
        }
        tx.commit()?;
        Ok(n > 0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ListFilter {
    pub include_archived: bool,
}

/// Build a conservative FTS5 MATCH query from free text.
fn fts_match_query(raw: &str) -> String {
    raw.split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| {
            let cleaned: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if cleaned.is_empty() {
                return String::new();
            }
            format!("\"{cleaned}\"*")
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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

#[cfg(test)]
mod hygiene_tests {
    use super::*;
    use crate::{Message, Role};
    use tempfile::tempdir;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
            tool_call_id: None,
            name: None,
        }
    }

    #[test]
    fn pin_archive_and_sort_order() {
        let dir = tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("s.db")).unwrap();
        let a = store.create("alpha").unwrap();
        let b = store.create("beta").unwrap();
        let c = store.create("gamma").unwrap();
        store
            .save(a, "alpha", &[], &[msg(Role::User, "hello world")])
            .unwrap();
        store
            .save(b, "beta", &[], &[msg(Role::User, "other")])
            .unwrap();
        store
            .save(c, "gamma", &[], &[msg(Role::User, "zzz")])
            .unwrap();
        assert!(store.set_pinned(c, true).unwrap());
        assert!(store.set_archived(b, true).unwrap());
        let active = store.list().unwrap();
        assert_eq!(active.len(), 2); // archived filtered
        assert_eq!(active[0].id, c); // pinned first
        assert!(active[0].pinned);
        let all = store
            .list_filtered(ListFilter {
                include_archived: true,
            })
            .unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().any(|s| s.id == b && s.archived));
    }

    #[test]
    fn metadata_changes_do_not_reset_last_message_time() {
        let dir = tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("s.db")).unwrap();
        let id = store.create("thread").unwrap();
        store
            .save(id, "thread", &[], &[msg(Role::User, "last message")])
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET updated_at = 'ts:1' WHERE id = ?1",
                params![id.to_string()],
            )
            .unwrap();
        let before = store.list().unwrap()[0].updated_at.clone();

        assert!(store.set_pinned(id, true).unwrap());
        assert!(store.set_archived(id, true).unwrap());
        assert!(store.rename(id, "renamed thread").unwrap());

        let after = store
            .list_filtered(ListFilter {
                include_archived: true,
            })
            .unwrap()
            .into_iter()
            .find(|session| session.id == id)
            .unwrap();
        assert_eq!(after.updated_at, before);

        store
            .save(id, "renamed thread", &[], &[msg(Role::User, "new message")])
            .unwrap();
        let after_message = store
            .list_filtered(ListFilter {
                include_archived: true,
            })
            .unwrap()
            .into_iter()
            .find(|session| session.id == id)
            .unwrap();
        assert!(after_message.updated_at > after.updated_at);
    }

    #[test]
    fn fts_finds_message_body() {
        let dir = tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("s.db")).unwrap();
        let id = store.create("notes").unwrap();
        store
            .save(
                id,
                "notes",
                &[],
                &[
                    msg(Role::User, "find the plutonium isotope"),
                    msg(Role::Assistant, "done"),
                ],
            )
            .unwrap();
        let hits = store.search("plutonium", false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, id);
        assert!(store.search("missingtokenxyz", false).unwrap().is_empty());
        assert!(store.search("!!!", false).unwrap().is_empty());
    }

    #[test]
    fn fts_indexes_finish_turn_path() {
        let dir = tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("s.db")).unwrap();
        let id = store.create("live").unwrap();
        let system = msg(Role::System, "sys");
        store.save(id, "live", &[], &[system.clone()]).unwrap();
        let mut messages = vec![system, msg(Role::User, "needleword unique")];
        let turn = store.begin_turn(id, "live", &[], &messages, 1).unwrap();
        messages.push(msg(Role::Assistant, "ok"));
        store
            .finish_turn(
                turn,
                id,
                "live",
                &[],
                &messages,
                TurnStatus::Succeeded,
                None,
            )
            .unwrap();
        let hits = store.search("needleword", false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, id);
    }
}
