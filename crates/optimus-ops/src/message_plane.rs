//! Durable session-to-session message plane (spec-025).
//!
//! Any session may message any other session (live or dormant), with peer
//! discovery (opt-in), per-session inbound policy (auto-accept /
//! hold-approval with expiry / deny), permission-classified delivery,
//! idempotent at-least-once delivery, failure-honest receipts, and exactly
//! one terminal outcome per message. Same host in v1; the envelope carries a
//! machine id so cross-machine relay (spec-017 transports) is a later add,
//! not a redesign (ADR-0087).
//!
//! The store follows the gateway authority pattern (ADR-0070): SQLite with
//! ordered event rows, idempotent identity (message id is the primary key),
//! and terminal outcomes recorded once.

mod queries;

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Additive schema version for the message-plane database.
pub const MESSAGE_PLANE_SCHEMA_VERSION: u32 = 1;

/// Default per-session dialog expiry for held messages (30 minutes, spec-025
/// R3). A session may override this with its own `dialog_expiry_seconds`.
pub const DEFAULT_DIALOG_EXPIRY_SECONDS: u64 = 30 * 60;

/// Default maximum message payload size in bytes (spec-025 R4).
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// How fresh a `live_sessions` row must be to count as live (lease-style
/// staleness; there is no daemon to unregister closed kernels).
pub const LIVE_SESSION_RECENCY_SECS: u64 = 3600;

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("message plane: {0}")]
    Msg(String),
    #[error("message {0} is already terminal ({1})")]
    AlreadyTerminal(Uuid, String),
    #[error("message {0} not found")]
    NotFound(Uuid),
    #[error("payload exceeds the {0}-byte cap; refused with message_too_large")]
    TooLarge(usize),
    #[error("target session {0} does not exist (session_send_failed)")]
    TargetGone(String),
}

pub type Result<T> = std::result::Result<T, MessageError>;

/// Delivery mode (Prime Agent patterns; spec-025 R1, roadmap 1a.2).
/// `auto` and `steer` surface on the receiving session's next turn;
/// `follow_up` is delivered but polled through the inbox tool instead of
/// being injected (it answers after the current task, not during it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageMode {
    Auto,
    Steer,
    FollowUp,
}

impl MessageMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Steer => "steer",
            Self::FollowUp => "follow_up",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "auto" => Ok(Self::Auto),
            "steer" => Ok(Self::Steer),
            "follow_up" => Ok(Self::FollowUp),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid message mode: {other}").into(),
            )),
        }
    }
}

/// Message kinds (spec-025 R1). Requests may carry effect requests that the
/// permission classifier vets (R5); replies carry the correlation id (R6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Request,
    Reply,
    Notice,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Reply => "reply",
            Self::Notice => "notice",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "request" => Ok(Self::Request),
            "reply" => Ok(Self::Reply),
            "notice" => Ok(Self::Notice),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid message kind: {other}").into(),
            )),
        }
    }
}

/// Message lifecycle states (spec-025 R1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Queued,
    Delivered,
    Held,
    Approved,
    Expired,
    Refused,
    Failed,
}

impl MessageState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Delivered => "delivered",
            Self::Held => "held",
            Self::Approved => "approved",
            Self::Expired => "expired",
            Self::Refused => "refused",
            Self::Failed => "failed",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Expired | Self::Refused | Self::Failed)
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "delivered" => Ok(Self::Delivered),
            "held" => Ok(Self::Held),
            "approved" => Ok(Self::Approved),
            "expired" => Ok(Self::Expired),
            "refused" => Ok(Self::Refused),
            "failed" => Ok(Self::Failed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid message state: {other}").into(),
            )),
        }
    }
}

/// Permission-classification result recorded with a message (spec-025 R5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageClassification {
    Approved,
    Denied,
    Pending,
}

impl MessageClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Pending => "pending",
        }
    }
}

/// One durable session message (spec-025 R1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: Uuid,
    pub from_session: Uuid,
    pub to_session: Uuid,
    pub kind: MessageKind,
    pub payload: String,
    pub reply_to: Option<Uuid>,
    pub mode: MessageMode,
    pub machine_id: String,
    pub state: MessageState,
    pub classification: Option<MessageClassification>,
    pub created_at: String,
    pub updated_at: String,
    pub delivered_at: Option<String>,
    pub surfaced_at: Option<String>,
}

/// One ordered lifecycle event row (spec-025 R7, law 11).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    pub message_id: Uuid,
    pub event_type: String,
    pub recorded_at: String,
    pub sequence: i64,
}

pub(crate) fn now_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ts:{nanos}")
}

pub(crate) fn stamp_unix_secs(stamp: &str) -> Option<u64> {
    stamp
        .strip_prefix("ts:")
        .and_then(|n| n.parse::<u128>().ok())
        .map(|nanos| (nanos / 1_000_000_000) as u64)
}

pub struct MessageStore {
    conn: Connection,
    machine_id: String,
}

impl MessageStore {
    /// Open (creating) the message-plane database under `home/messages.db`.
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let path = home.as_ref().join("messages.db");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // Concurrent kernel opens (the host's worker pool, one home) must
        // wait for a migrating opener instead of failing "database is
        // locked" (gateway.rs convention).
        conn.busy_timeout(Duration::from_secs(5))?;
        // The journal-mode pragma takes a file-level lock that the busy
        // handler does not cover; concurrent first-opens (the host's worker
        // pool) retry the idempotent batch instead of failing.
        let mut attempts = 0;
        loop {
            match conn.execute_batch(
                "
                PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS message_meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_messages (
                id TEXT PRIMARY KEY NOT NULL,
                from_session TEXT NOT NULL,
                to_session TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('request','reply','notice')),
                payload TEXT NOT NULL CHECK(length(payload) > 0),
                reply_to TEXT,
                mode TEXT NOT NULL CHECK(mode IN ('auto','steer','follow_up')),
                machine_id TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN (
                    'queued','delivered','held','approved','expired','refused','failed'
                )),
                classification TEXT CHECK(classification IN (
                    'approved','denied','pending'
                )),
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                delivered_at TEXT,
                surfaced_at TEXT,
                terminal TEXT CHECK(terminal IS NULL OR terminal IN (
                    'delivered','refused','expired','failed'
                ))
            );
            CREATE TABLE IF NOT EXISTS session_message_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id TEXT NOT NULL REFERENCES session_messages(id) ON DELETE CASCADE,
                event_type TEXT NOT NULL CHECK(event_type IN (
                    'queued','delivered','held','approved','expired','refused','failed'
                )),
                recorded_at TEXT NOT NULL,
                UNIQUE(message_id, event_type)
            );
            CREATE INDEX IF NOT EXISTS idx_sm_to ON session_messages(to_session, state);
            CREATE INDEX IF NOT EXISTS idx_sm_from ON session_messages(from_session, created_at);
            CREATE INDEX IF NOT EXISTS idx_sm_reply ON session_messages(reply_to);
            CREATE TABLE IF NOT EXISTS live_sessions (
                session_id TEXT PRIMARY KEY NOT NULL,
                opened_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_reply_waits (
                wait_id TEXT PRIMARY KEY NOT NULL,
                message_id TEXT NOT NULL,
                outcome TEXT NOT NULL CHECK(outcome IN ('reply_received','reply_wait_expired')),
                recorded_at TEXT NOT NULL
            );
            ",
            ) {
                Ok(()) => break,
                Err(rusqlite::Error::SqliteFailure(failure, _))
                    if failure.code == rusqlite::ErrorCode::DatabaseBusy && attempts < 8 =>
                {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let machine_id = match conn
            .query_row(
                "SELECT value FROM message_meta WHERE key = 'machine_id'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4().to_string();
                // INSERT OR IGNORE: concurrent openers (the host's worker
                // pool) race this seed row; the winner's id wins and the
                // loser reads it back below.
                conn.execute(
                    "INSERT OR IGNORE INTO message_meta(key, value) VALUES ('machine_id', ?1)",
                    params![id],
                )?;
                conn.query_row(
                    "SELECT value FROM message_meta WHERE key = 'machine_id'",
                    [],
                    |row| row.get::<_, String>(0),
                )?
            }
        };
        Ok(Self { conn, machine_id })
    }

    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    fn record_event(&self, message_id: Uuid, event_type: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO session_message_events(message_id, event_type, recorded_at)
             VALUES (?1, ?2, ?3)",
            params![message_id.to_string(), event_type, now_stamp()],
        )?;
        Ok(())
    }

    fn set_state(&self, message: &mut SessionMessage, state: MessageState) -> Result<()> {
        if message.state.is_terminal() {
            return Err(MessageError::AlreadyTerminal(
                message.id,
                message.state.as_str().into(),
            ));
        }
        message.state = state;
        message.updated_at = now_stamp();
        if state == MessageState::Delivered && message.delivered_at.is_none() {
            message.delivered_at = Some(message.updated_at.clone());
        }
        self.conn.execute(
            "UPDATE session_messages
             SET state = ?2, updated_at = ?3, delivered_at = ?4,
                 terminal = CASE WHEN ?2 IN ('delivered','refused','expired','failed')
                                 THEN ?2 ELSE NULL END
             WHERE id = ?1",
            params![
                message.id.to_string(),
                state.as_str(),
                message.updated_at,
                message.delivered_at,
            ],
        )?;
        self.record_event(message.id, state.as_str())?;
        Ok(())
    }

    fn load(&self, id: Uuid) -> Result<Option<SessionMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_session, to_session, kind, payload, reply_to, mode,
                    machine_id, state, classification, created_at, updated_at,
                    delivered_at, surfaced_at
             FROM session_messages WHERE id = ?1",
        )?;
        let row = stmt
            .query_row(params![id.to_string()], |row| {
                let kind: String = row.get(3)?;
                let mode: String = row.get(6)?;
                let state: String = row.get(8)?;
                let classification: Option<String> = row.get(9)?;
                Ok(SessionMessage {
                    id: parse_uuid(&row.get::<_, String>(0)?, 0)?,
                    from_session: parse_uuid(&row.get::<_, String>(1)?, 1)?,
                    to_session: parse_uuid(&row.get::<_, String>(2)?, 2)?,
                    kind: MessageKind::parse(&kind)?,
                    payload: row.get(4)?,
                    reply_to: row
                        .get::<_, Option<String>>(5)?
                        .map(|v| parse_uuid(&v, 5))
                        .transpose()?,
                    mode: MessageMode::parse(&mode)?,
                    machine_id: row.get(7)?,
                    state: MessageState::parse(&state)?,
                    classification: classification
                        .as_deref()
                        .map(|c| match c {
                            "approved" => Ok(MessageClassification::Approved),
                            "denied" => Ok(MessageClassification::Denied),
                            "pending" => Ok(MessageClassification::Pending),
                            other => Err(rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                format!("invalid classification: {other}").into(),
                            )),
                        })
                        .transpose()?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    delivered_at: row.get(12)?,
                    surfaced_at: row.get(13)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    /// Enqueue a message in `queued` state and record the queued event.
    /// Idempotent by message id: re-enqueueing the same id is a no-op that
    /// returns the existing record (spec-025 R4).
    pub fn enqueue(&self, message: SessionMessage) -> Result<SessionMessage> {
        if message.payload.len() > DEFAULT_MAX_MESSAGE_BYTES {
            return Err(MessageError::TooLarge(DEFAULT_MAX_MESSAGE_BYTES));
        }
        if let Some(existing) = self.load(message.id)? {
            return Ok(existing);
        }
        let now = now_stamp();
        self.conn.execute(
            "INSERT INTO session_messages(
                 id, from_session, to_session, kind, payload, reply_to, mode,
                 machine_id, state, classification, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', NULL, ?9, ?9)",
            params![
                message.id.to_string(),
                message.from_session.to_string(),
                message.to_session.to_string(),
                message.kind.as_str(),
                message.payload,
                message.reply_to.map(|id| id.to_string()),
                message.mode.as_str(),
                message.machine_id,
                now,
            ],
        )?;
        self.record_event(message.id, "queued")?;
        Ok(self.load(message.id)?.expect("enqueued message must load"))
    }

    /// Mark a session as live (kernel open/resume). The row is refreshed on
    /// every open; staleness is judged by recency (spec-025 R1, A1/A2).
    pub fn register_live(&self, session_id: Uuid) -> Result<()> {
        self.conn.execute(
            "INSERT INTO live_sessions(session_id, opened_at) VALUES (?1, ?2)
             ON CONFLICT(session_id) DO UPDATE SET opened_at = excluded.opened_at",
            params![session_id.to_string(), now_stamp()],
        )?;
        Ok(())
    }

    /// Is the session live right now? A row opened within
    /// [`LIVE_SESSION_RECENCY_SECS`] counts as live; anything older is a
    /// stale lease and the session is treated as dormant (its messages stay
    /// queued until it resumes).
    pub fn is_live(&self, session_id: Uuid) -> Result<bool> {
        let opened: Option<String> = self
            .conn
            .query_row(
                "SELECT opened_at FROM live_sessions WHERE session_id = ?1",
                params![session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(opened) = opened else {
            return Ok(false);
        };
        let opened = stamp_unix_secs(&opened).unwrap_or(0);
        let now = stamp_unix_secs(&now_stamp()).unwrap_or(0);
        Ok(now.saturating_sub(opened) < LIVE_SESSION_RECENCY_SECS)
    }
}

pub(super) fn parse_uuid(value: &str, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            e.to_string().into(),
        )
    })
}
