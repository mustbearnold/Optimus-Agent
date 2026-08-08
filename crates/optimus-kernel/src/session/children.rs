//! Durable child registry (spec-034): types, status machine, and the
//! `session_children` and `session_child_events` tables in the session
//! store.
//!
//! A child is a full kernel session with its own store context. The
//! parent spawns it with one typed task prompt; the spawn returns an
//! admission handle at once (R1). This module owns the registry record
//! and the closed status machine (R2, R8). The daemon runs the child
//! turn (R4); the attribution rows live in `execution.db` (R7). The
//! registry lives in `sessions.db` because it is session state
//! (ADR-0086); the effect ledger never carries session-scoped records.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// A manual immediate transaction (write lock at BEGIN) that works on
/// `&self` connections. WAL snapshot-upgrade BUSY errors cannot occur
/// because the write lock is taken at BEGIN, inside the busy handler.
struct ImmediateTx<'a> {
    conn: &'a rusqlite::Connection,
    done: bool,
}

impl<'a> ImmediateTx<'a> {
    fn begin(conn: &'a rusqlite::Connection) -> Result<Self> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        Ok(Self { conn, done: false })
    }
    fn commit(mut self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        self.done = true;
        Ok(())
    }
}

impl Drop for ImmediateTx<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

use uuid::Uuid;

use crate::Result;

use super::SessionStore;

/// Additive schema version for the child registry tables.
/// Child lifecycle statuses (spec-034 R2). Terminal states are
/// absorbing: no transition may leave one, and no transition may enter
/// a terminal state twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildStatus {
    Spawned,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ChildStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spawned => "spawned",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "spawned" => Some(Self::Spawned),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Child lifecycle event types (spec-034 R8). The `deleted` event is a
/// lifecycle event, not a terminal: terminal event types appear at
/// most once per child, and the status guard enforces exactly one
/// terminal outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildEventType {
    Spawned,
    Adopted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Deleted,
}

impl ChildEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spawned => "spawned",
            Self::Adopted => "adopted",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Deleted => "deleted",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "spawned" => Some(Self::Spawned),
            "adopted" => Some(Self::Adopted),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

/// One durable registry row (spec-034 R2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildRegistryRow {
    pub parent_session_id: Uuid,
    pub child_session_id: Uuid,
    pub depth: u32,
    pub task_sha256: String,
    /// Provider and model snapshot. `None` means the child inherits the
    /// parent routing (spec-034 R5).
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Inherited-or-explicit policy snapshot (R5). Persisted so the
    /// re-adoption run (R4) rebuilds the child kernel faithfully.
    pub effect_policy: String,
    pub autonomy_profile: String,
    pub command_fs_envelope: Option<String>,
    pub children_max_depth: u32,
    pub status: ChildStatus,
    /// The durable cancel marker (spec-034 R6). Holds the reason while
    /// the child is non-terminal; clears when the terminal lands.
    pub cancel_requested: Option<String>,
    /// The durable tombstone marker (spec-034 R6). `None` until
    /// deletion. Deletion never changes the terminal status.
    pub deleted_at: Option<String>,
    /// The running parent turn manifest at spawn (spec-034 R7). Durable
    /// so the crash settle can attribute without a live parent.
    pub parent_manifest_id: Option<Uuid>,
    pub created_at: String,
    pub adopted_at: Option<String>,
    pub terminal_at: Option<String>,
    pub terminal_reason: Option<String>,
}

/// One ordered child lifecycle event (spec-034 R8).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildEvent {
    pub child_session_id: Uuid,
    pub event_type: ChildEventType,
    pub payload: Option<String>,
    pub recorded_at: String,
}

/// The admission request (spec-034 R1). The child session row exists
/// before the admit; the caller creates it.
#[derive(Debug, Clone)]
pub struct NewChild {
    pub parent_session_id: Uuid,
    pub child_session_id: Uuid,
    pub depth: u32,
    pub task_sha256: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Inherited-or-explicit policy snapshot (R5).
    pub effect_policy: String,
    pub autonomy_profile: String,
    pub command_fs_envelope: Option<String>,
    pub children_max_depth: u32,
    /// The running parent turn manifest (R7 attribution). Durable so
    /// the crash settle can attribute without a live parent.
    pub parent_manifest_id: Option<Uuid>,
}

fn now_stamp() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ts:{nanos}")
}

/// Read a UUID column (stored as TEXT) the same way the goals store
/// does: explicit parse, never a `FromSql` impl on `Uuid`.
fn uuid_at(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&row.get::<_, String>(index)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            e.to_string().into(),
        )
    })
}

impl SessionStore {
    pub(crate) fn ensure_children_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS session_children (
                parent_session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                child_session_id TEXT PRIMARY KEY NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                depth INTEGER NOT NULL CHECK(depth >= 1),
                task_sha256 TEXT NOT NULL CHECK(length(task_sha256)=64),
                provider TEXT,
                model TEXT,
                effect_policy TEXT NOT NULL DEFAULT 'smart_deny'
                    CHECK(effect_policy IN ('smart_deny','unrestricted')),
                autonomy_profile TEXT NOT NULL DEFAULT 'review_changes',
                command_fs_envelope TEXT,
                children_max_depth INTEGER NOT NULL DEFAULT 1 CHECK(children_max_depth >= 1),
                status TEXT NOT NULL CHECK(status IN (
                    'spawned','running','succeeded','failed','cancelled'
                )),
                cancel_requested TEXT,
                deleted_at TEXT,
                parent_manifest_id TEXT,
                created_at TEXT NOT NULL,
                adopted_at TEXT,
                terminal_at TEXT,
                terminal_reason TEXT,
                CHECK((status IN ('succeeded','failed','cancelled') AND terminal_at IS NOT NULL)
                   OR (status IN ('spawned','running') AND terminal_at IS NULL)),
                CHECK(cancel_requested IS NULL OR status IN ('spawned','running')),
                CHECK(deleted_at IS NULL OR status IN ('succeeded','failed','cancelled'))
            );
            CREATE TABLE IF NOT EXISTS session_child_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                child_session_id TEXT NOT NULL REFERENCES session_children(child_session_id) ON DELETE CASCADE,
                event_type TEXT NOT NULL CHECK(event_type IN (
                    'spawned','adopted','running','succeeded','failed','cancelled','deleted'
                )),
                payload TEXT,
                recorded_at TEXT NOT NULL,
                UNIQUE(child_session_id, event_type)
            );
            ",
        )?;
        Ok(())
    }

    /// Admit a child (spec-034 R1): insert the registry row and the
    /// `spawned` event atomically. The row is durable before the
    /// admission handle returns to the parent.
    pub fn child_admit(&self, new: &NewChild) -> Result<()> {
        let tx = ImmediateTx::begin(&self.conn)?;
        self.conn.execute(
            "INSERT INTO session_children
               (parent_session_id, child_session_id, depth, task_sha256,
                provider, model, effect_policy, autonomy_profile,
                command_fs_envelope, children_max_depth, status,
                parent_manifest_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'spawned',
                     ?11, ?12)",
            params![
                new.parent_session_id.to_string(),
                new.child_session_id.to_string(),
                new.depth,
                new.task_sha256,
                new.provider,
                new.model,
                new.effect_policy,
                new.autonomy_profile,
                new.command_fs_envelope,
                new.children_max_depth,
                new.parent_manifest_id.map(|id| id.to_string()),
                now_stamp(),
            ],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO session_child_events
               (child_session_id, event_type, payload, recorded_at)
             VALUES (?1, 'spawned', NULL, ?2)",
            params![new.child_session_id.to_string(), now_stamp()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Load one registry row by child session id.
    pub fn child_get(&self, child_session_id: Uuid) -> Result<Option<ChildRegistryRow>> {
        self.conn
            .query_row(
                "SELECT parent_session_id, child_session_id, depth, task_sha256,
                        provider, model, effect_policy, autonomy_profile,
                        command_fs_envelope, children_max_depth, status,
                        cancel_requested, deleted_at, parent_manifest_id,
                        created_at, adopted_at, terminal_at, terminal_reason
                 FROM session_children WHERE child_session_id = ?1",
                params![child_session_id.to_string()],
                |row| {
                    Ok(ChildRegistryRow {
                        parent_session_id: uuid_at(row, 0)?,
                        child_session_id: uuid_at(row, 1)?,
                        depth: row.get(2)?,
                        task_sha256: row.get(3)?,
                        provider: row.get(4)?,
                        model: row.get(5)?,
                        effect_policy: row.get::<_, String>(6)?,
                        autonomy_profile: row.get::<_, String>(7)?,
                        command_fs_envelope: row.get(8)?,
                        children_max_depth: row.get::<_, u32>(9)?,
                        status: ChildStatus::parse(&row.get::<_, String>(10)?)
                            .expect("registry status is checked"),
                        cancel_requested: row.get(11)?,
                        deleted_at: row.get(12)?,
                        parent_manifest_id: row
                            .get::<_, Option<String>>(13)?
                            .and_then(|value| Uuid::parse_str(&value).ok()),
                        created_at: row.get(14)?,
                        adopted_at: row.get(15)?,
                        terminal_at: row.get(16)?,
                        terminal_reason: row.get(17)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Load one registry row scoped to a parent (spec-034 R2: the
    /// parent keeps a registry of DIRECT children only).
    pub fn child_of_parent(
        &self,
        parent_session_id: Uuid,
        child_session_id: Uuid,
    ) -> Result<Option<ChildRegistryRow>> {
        self.conn
            .query_row(
                "SELECT parent_session_id, child_session_id, depth, task_sha256,
                        provider, model, effect_policy, autonomy_profile,
                        command_fs_envelope, children_max_depth, status,
                        cancel_requested, deleted_at, parent_manifest_id,
                        created_at, adopted_at, terminal_at, terminal_reason
                 FROM session_children
                 WHERE child_session_id = ?1 AND parent_session_id = ?2",
                params![child_session_id.to_string(), parent_session_id.to_string()],
                |row| {
                    Ok(ChildRegistryRow {
                        parent_session_id: uuid_at(row, 0)?,
                        child_session_id: uuid_at(row, 1)?,
                        depth: row.get(2)?,
                        task_sha256: row.get(3)?,
                        provider: row.get(4)?,
                        model: row.get(5)?,
                        effect_policy: row.get::<_, String>(6)?,
                        autonomy_profile: row.get::<_, String>(7)?,
                        command_fs_envelope: row.get(8)?,
                        children_max_depth: row.get::<_, u32>(9)?,
                        status: ChildStatus::parse(&row.get::<_, String>(10)?)
                            .expect("registry status is checked"),
                        cancel_requested: row.get(11)?,
                        deleted_at: row.get(12)?,
                        parent_manifest_id: row
                            .get::<_, Option<String>>(13)?
                            .and_then(|value| Uuid::parse_str(&value).ok()),
                        created_at: row.get(14)?,
                        adopted_at: row.get(15)?,
                        terminal_at: row.get(16)?,
                        terminal_reason: row.get(17)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// The registry depth of a session, if the session is a child.
    pub fn child_depth(&self, session_id: Uuid) -> Result<Option<u32>> {
        self.conn
            .query_row(
                "SELECT depth FROM session_children WHERE child_session_id = ?1",
                params![session_id.to_string()],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Direct children of a parent, oldest first (spec-034 R2).
    pub fn child_children(&self, parent_session_id: Uuid) -> Result<Vec<ChildRegistryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT parent_session_id, child_session_id, depth, task_sha256,
                    provider, model, effect_policy, autonomy_profile,
                    command_fs_envelope, children_max_depth, status,
                    cancel_requested, deleted_at, parent_manifest_id,
                    created_at, adopted_at, terminal_at, terminal_reason
             FROM session_children WHERE parent_session_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([parent_session_id.to_string()], |row| {
            Ok(ChildRegistryRow {
                parent_session_id: uuid_at(row, 0)?,
                child_session_id: uuid_at(row, 1)?,
                depth: row.get(2)?,
                task_sha256: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
                effect_policy: row.get::<_, String>(6)?,
                autonomy_profile: row.get::<_, String>(7)?,
                command_fs_envelope: row.get(8)?,
                children_max_depth: row.get::<_, u32>(9)?,
                status: ChildStatus::parse(&row.get::<_, String>(10)?)
                    .expect("registry status is checked"),
                cancel_requested: row.get(11)?,
                deleted_at: row.get(12)?,
                parent_manifest_id: row
                    .get::<_, Option<String>>(13)?
                    .and_then(|value| Uuid::parse_str(&value).ok()),
                created_at: row.get(14)?,
                adopted_at: row.get(15)?,
                terminal_at: row.get(16)?,
                terminal_reason: row.get(17)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Lifecycle events for one child, in record order (spec-034 R8).
    pub fn child_events(&self, child_session_id: Uuid) -> Result<Vec<ChildEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT child_session_id, event_type, payload, recorded_at
             FROM session_child_events WHERE child_session_id = ?1
             ORDER BY sequence ASC",
        )?;
        let rows = stmt.query_map([child_session_id.to_string()], |row| {
            Ok(ChildEvent {
                child_session_id: uuid_at(row, 0)?,
                event_type: ChildEventType::parse(&row.get::<_, String>(1)?)
                    .expect("event type is checked"),
                payload: row.get(2)?,
                recorded_at: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// All non-terminal, non-deleted children (the adoption sweep
    /// input, spec-034 R4).
    pub fn child_non_terminal(&self) -> Result<Vec<ChildRegistryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT parent_session_id, child_session_id, depth, task_sha256,
                    provider, model, effect_policy, autonomy_profile,
                    command_fs_envelope, children_max_depth, status,
                    cancel_requested, deleted_at, parent_manifest_id,
                    created_at, adopted_at, terminal_at, terminal_reason
             FROM session_children
             WHERE status IN ('spawned','running') AND deleted_at IS NULL
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChildRegistryRow {
                parent_session_id: uuid_at(row, 0)?,
                child_session_id: uuid_at(row, 1)?,
                depth: row.get(2)?,
                task_sha256: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
                effect_policy: row.get::<_, String>(6)?,
                autonomy_profile: row.get::<_, String>(7)?,
                command_fs_envelope: row.get(8)?,
                children_max_depth: row.get::<_, u32>(9)?,
                status: ChildStatus::parse(&row.get::<_, String>(10)?)
                    .expect("registry status is checked"),
                cancel_requested: row.get(11)?,
                deleted_at: row.get(12)?,
                parent_manifest_id: row
                    .get::<_, Option<String>>(13)?
                    .and_then(|value| Uuid::parse_str(&value).ok()),
                created_at: row.get(14)?,
                adopted_at: row.get(15)?,
                terminal_at: row.get(16)?,
                terminal_reason: row.get(17)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Mark a child `running` (spec-034 R4). `adopted` adds the
    /// `adopted` event before the `running` event. The transition is
    /// only legal from `spawned`; a terminal or running child errors.
    pub fn child_mark_running(&self, child_session_id: Uuid, adopted: bool) -> Result<()> {
        let tx = ImmediateTx::begin(&self.conn)?;
        let status: Option<String> = self
            .conn
            .query_row(
                "SELECT status FROM session_children WHERE child_session_id = ?1",
                params![child_session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(status) = status else {
            return Err(crate::KernelError::Tool(format!(
                "child session {child_session_id} is not in the registry"
            )));
        };
        if status != "spawned" {
            return Err(crate::KernelError::Tool(format!(
                "child {child_session_id} cannot enter running from {status}"
            )));
        }
        self.conn.execute(
            "UPDATE session_children
             SET status = 'running', adopted_at = COALESCE(adopted_at, ?2)
             WHERE child_session_id = ?1",
            params![child_session_id.to_string(), now_stamp()],
        )?;
        if adopted {
            self.conn.execute(
                "INSERT OR IGNORE INTO session_child_events
                   (child_session_id, event_type, payload, recorded_at)
                 VALUES (?1, 'adopted', NULL, ?2)",
                params![child_session_id.to_string(), now_stamp()],
            )?;
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO session_child_events
               (child_session_id, event_type, payload, recorded_at)
             VALUES (?1, 'running', NULL, ?2)",
            params![child_session_id.to_string(), now_stamp()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Settle a child to a terminal outcome (spec-034 R2, R6, R8).
    /// Exactly one terminal outcome per child: the status guard
    /// rejects any settle on a terminal row, and terminal event types
    /// are unique per child. The settle clears the cancel marker.
    pub fn child_settle(
        &self,
        child_session_id: Uuid,
        status: ChildStatus,
        reason: Option<&str>,
    ) -> Result<()> {
        if !status.is_terminal() {
            return Err(crate::KernelError::Tool(format!(
                "settle requires a terminal status, got {}",
                status.as_str()
            )));
        }
        let tx = ImmediateTx::begin(&self.conn)?;
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT status FROM session_children WHERE child_session_id = ?1",
                params![child_session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            return Err(crate::KernelError::Tool(format!(
                "child session {child_session_id} is not in the registry"
            )));
        };
        let current = ChildStatus::parse(&current).expect("registry status is checked");
        if current.is_terminal() {
            return Err(crate::KernelError::Tool(format!(
                "child {child_session_id} already has the terminal outcome {}",
                current.as_str()
            )));
        }
        let now = now_stamp();
        let updated = self.conn.execute(
            "UPDATE session_children
             SET status = ?2, terminal_at = ?3, terminal_reason = ?4,
                 cancel_requested = NULL
             WHERE child_session_id = ?1 AND status IN ('spawned','running')",
            params![child_session_id.to_string(), status.as_str(), now, reason,],
        )?;
        if updated != 1 {
            return Err(crate::KernelError::Tool(format!(
                "child {child_session_id} settled twice or vanished"
            )));
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO session_child_events
               (child_session_id, event_type, payload, recorded_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![child_session_id.to_string(), status.as_str(), reason, now,],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Record the durable cancel marker (spec-034 R6). Legal only on a
    /// non-terminal child; the call happens before the token cancel.
    pub fn child_cancel_request(&self, child_session_id: Uuid, reason: &str) -> Result<()> {
        let updated = self.conn.execute(
            "UPDATE session_children
             SET cancel_requested = ?2
             WHERE child_session_id = ?1 AND status IN ('spawned','running')
               AND cancel_requested IS NULL",
            params![child_session_id.to_string(), reason],
        )?;
        if updated == 0 {
            return Err(crate::KernelError::Tool(format!(
                "child {child_session_id} is not cancellable (terminal, deleted, or already requested)"
            )));
        }
        Ok(())
    }

    /// Write the durable tombstone (spec-034 R6). Legal only on a
    /// terminal child; the terminal status never changes.
    pub fn child_tombstone(&self, child_session_id: Uuid) -> Result<()> {
        let tx = ImmediateTx::begin(&self.conn)?;
        let updated = self.conn.execute(
            "UPDATE session_children SET deleted_at = ?2
             WHERE child_session_id = ?1
               AND status IN ('succeeded','failed','cancelled')
               AND deleted_at IS NULL",
            params![child_session_id.to_string(), now_stamp()],
        )?;
        if updated != 1 {
            return Err(crate::KernelError::Tool(format!(
                "child {child_session_id} is not tombstoneable (non-terminal or already deleted)"
            )));
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO session_child_events
               (child_session_id, event_type, payload, recorded_at)
             VALUES (?1, 'deleted', NULL, ?2)",
            params![child_session_id.to_string(), now_stamp()],
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Cross-store usage aggregation (spec-034 R7). The attribution rows
/// live in `execution.db`; the registry rows live in `sessions.db`.
/// Defined here with the registry because the caller already holds the
/// child ids it read from the registry.
impl crate::ExecutionStore {
    /// Aggregated child usage: total, input, output, and reasoning
    /// tokens per child session, summed over every attribution row of
    /// the given child sessions.
    pub fn child_usage(
        &self,
        child_session_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, (u64, u64, u64, u64)>> {
        if child_session_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let placeholders = std::iter::repeat_n("?", child_session_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT child_session_id,
                    COALESCE(SUM(total_tokens), 0),
                    COALESCE(SUM(input_tokens), 0),
                    COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(reasoning_tokens), 0)
             FROM execution_child_attribution
             WHERE child_session_id IN ({placeholders})
             GROUP BY child_session_id"
        );
        let params = child_session_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        e.to_string().into(),
                    )
                })?,
                (
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, i64>(4)? as u64,
                ),
            ))
        })?;
        let collected = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(collected.into_iter().collect())
    }

    /// The running manifest of a session, if a turn is in flight
    /// (spec-034 R7: the child attribution links to the parent turn).
    pub fn running_manifest_for_session(&self, session_id: Uuid) -> Result<Option<Uuid>> {
        self.conn
            .query_row(
                "SELECT id FROM execution_manifests
                 WHERE session_id = ?1 AND status = 'running'
                 ORDER BY created_unix DESC LIMIT 1",
                params![session_id.to_string()],
                |row| {
                    Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            e.to_string().into(),
                        )
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionStore;

    fn store() -> SessionStore {
        let dir = std::env::temp_dir().join(format!("optimus-children-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = SessionStore::open(dir.join("sessions.db")).unwrap();
        store.ensure_children_schema().unwrap();
        store
    }

    /// Create a real parent session row; return its id.
    fn parent(store: &SessionStore) -> Uuid {
        store.create("parent").unwrap()
    }

    fn admit(store: &SessionStore, parent: Uuid, child: Uuid, depth: u32) {
        store
            .child_admit(&NewChild {
                parent_session_id: parent,
                child_session_id: child,
                depth,
                task_sha256: "a".repeat(64),
                provider: None,
                model: None,
                effect_policy: "smart_deny".into(),
                autonomy_profile: "review_changes".into(),
                command_fs_envelope: None,
                children_max_depth: 1,
                parent_manifest_id: None,
            })
            .unwrap();
    }

    /// Create the child session row and admit it. Returns the child id.
    fn admit_new(store: &SessionStore, parent: Uuid, depth: u32) -> Uuid {
        let child = store.create("child").unwrap();
        admit(store, parent, child, depth);
        child
    }

    #[test]
    fn admit_then_settle_records_exactly_one_terminal() {
        let store = store();
        let parent = parent(&store);
        let child = admit_new(&store, parent, 1);
        let row = store.child_get(child).unwrap().unwrap();
        assert_eq!(row.status, ChildStatus::Spawned);
        assert_eq!(row.depth, 1);
        store.child_mark_running(child, false).unwrap();
        store
            .child_settle(child, ChildStatus::Succeeded, None)
            .unwrap();
        let row = store.child_get(child).unwrap().unwrap();
        assert_eq!(row.status, ChildStatus::Succeeded);
        assert!(row.terminal_at.is_some());
        // A second settle is refused: exactly one terminal outcome.
        let err = store.child_settle(child, ChildStatus::Failed, Some("again"));
        assert!(err.is_err());
        // The terminal event type is unique.
        let events = store.child_events(child).unwrap();
        let terminal = events
            .iter()
            .filter(|e| e.event_type == ChildEventType::Succeeded)
            .count();
        assert_eq!(terminal, 1);
    }

    #[test]
    fn cancel_marker_is_durable_and_clears_on_settle() {
        let store = store();
        let parent = parent(&store);
        let child = admit_new(&store, parent, 1);
        store.child_cancel_request(child, "user asked").unwrap();
        let row = store.child_get(child).unwrap().unwrap();
        assert_eq!(row.cancel_requested.as_deref(), Some("user asked"));
        store
            .child_settle(child, ChildStatus::Cancelled, Some("cancel_requested"))
            .unwrap();
        let row = store.child_get(child).unwrap().unwrap();
        assert!(row.cancel_requested.is_none());
        assert_eq!(row.terminal_reason.as_deref(), Some("cancel_requested"));
    }

    #[test]
    fn tombstone_requires_terminal_and_keeps_status() {
        let store = store();
        let parent = parent(&store);
        let child = admit_new(&store, parent, 1);
        assert!(store.child_tombstone(child).is_err());
        store
            .child_settle(child, ChildStatus::Failed, Some("crash_interrupted"))
            .unwrap();
        store.child_tombstone(child).unwrap();
        let row = store.child_get(child).unwrap().unwrap();
        assert_eq!(row.status, ChildStatus::Failed);
        assert!(row.deleted_at.is_some());
        assert!(store.child_tombstone(child).is_err());
    }

    #[test]
    fn adoption_sweep_lists_only_non_terminal_non_deleted() {
        let store = store();
        let parent = parent(&store);
        let a = admit_new(&store, parent, 1);
        let b = admit_new(&store, parent, 1);
        let c = admit_new(&store, parent, 1);
        store.child_settle(a, ChildStatus::Succeeded, None).unwrap();
        store
            .child_settle(c, ChildStatus::Cancelled, Some("cancel_requested"))
            .unwrap();
        store.child_tombstone(c).unwrap();
        let pending = store.child_non_terminal().unwrap();
        let ids: Vec<Uuid> = pending.iter().map(|r| r.child_session_id).collect();
        assert_eq!(ids, vec![b]);
    }

    #[test]
    fn running_is_guarded_and_mark_running_is_single_use() {
        let store = store();
        let parent = parent(&store);
        let child = admit_new(&store, parent, 1);
        store.child_mark_running(child, true).unwrap();
        let events = store.child_events(child).unwrap();
        let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert_eq!(types, vec!["spawned", "adopted", "running"]);
        assert!(store.child_mark_running(child, false).is_err());
        store
            .child_settle(child, ChildStatus::Succeeded, None)
            .unwrap();
        assert!(store.child_mark_running(child, false).is_err());
    }
}
