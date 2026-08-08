//! Child supervision (spec-034): the durable coordination layer for
//! recursive children.
//!
//! The supervisor owns the registry state transitions and the usage
//! attribution. It opens `sessions.db` (the `session_children` and
//! `session_child_events` tables) and `execution.db` (the
//! `execution_child_attribution` table) directly, because the
//! orchestrator lives in this crate while the stores are kernel-owned
//! files under the home directory. The status machine mirrors
//! `optimus_kernel::session::children`; the SQL invariants (CHECK
//! constraints) are shared, so the two writers cannot diverge.
//!
//! Responsibilities (R2, R4, R6, R7, R8):
//! - exactly one terminal outcome per child;
//! - the adoption plan after a host restart (re-run only children
//!   without a manifest; settle interrupted and cancel-requested
//!   children);
//! - the durable cancel marker and the `runner_lost` settle;
//! - the tombstone marker;
//! - the attribution row that links the parent turn to the child
//!   manifest.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
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

use crate::{Result, WorkflowError};

/// Mirror of the kernel registry statuses. Terminal states are
/// absorbing; the guarded UPDATE enforces exactly one terminal.
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

/// One registry row projection the supervisor works with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildRow {
    pub parent_session_id: Uuid,
    pub child_session_id: Uuid,
    pub depth: u32,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Inherited-or-explicit policy snapshot (R5). Persisted so the
    /// re-adoption run (R4) rebuilds the child kernel faithfully.
    pub effect_policy: String,
    pub autonomy_profile: String,
    pub command_fs_envelope: Option<String>,
    pub children_max_depth: u32,
    pub status: ChildStatus,
    pub cancel_requested: Option<String>,
    pub deleted_at: Option<String>,
    pub parent_manifest_id: Option<Uuid>,
    pub created_at: String,
}

/// The adoption decision for one child after a host restart (R4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionAction {
    /// Re-run the task turn. Only legal when the child has no
    /// execution manifest (the turn never started).
    Run {
        child_session_id: Uuid,
        provider: Option<String>,
        model: Option<String>,
        effect_policy: String,
        autonomy_profile: String,
        command_fs_envelope: Option<String>,
        children_max_depth: u32,
        parent_manifest_id: Option<Uuid>,
    },
    /// Settle without re-running. `reason` is `crash_interrupted` or
    /// `cancel_requested`.
    Settle {
        child_session_id: Uuid,
        status: ChildStatus,
        reason: &'static str,
    },
}

/// Durable child coordination over `sessions.db` + `execution.db`.
pub struct ChildSupervisor {
    sessions: Connection,
    executions: Connection,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_stamp() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ts:{nanos}")
}

fn uuid_at(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&row.get::<_, String>(index)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            e.to_string().into(),
        )
    })
}

impl ChildSupervisor {
    /// Open the two stores under the home directory. Creates the
    /// attribution table if the kernel has not done so yet.
    pub fn open(home: &Path) -> Result<Self> {
        let sessions = Connection::open(home.join("sessions.db"))?;
        sessions.busy_timeout(std::time::Duration::from_secs(15))?;
        // The registry tables (additive, law 12). The kernel's session
        // store also ensures them; this self-bootstrap makes the
        // supervisor work on a fresh home before any kernel opens. WAL
        // is set here, not by the first kernel open, so a mid-flight
        // journal-mode switch never collides with the spawn burst.
        sessions.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS session_children (
                parent_session_id TEXT NOT NULL,
                child_session_id TEXT PRIMARY KEY NOT NULL,
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
        let executions = Connection::open(home.join("execution.db"))?;
        executions.busy_timeout(std::time::Duration::from_secs(15))?;
        executions.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS execution_child_attribution (
                parent_manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
                child_session_id TEXT NOT NULL,
                child_manifest_id TEXT NOT NULL UNIQUE REFERENCES execution_manifests(id) ON DELETE CASCADE,
                input_tokens INTEGER NOT NULL DEFAULT 0 CHECK(input_tokens >= 0),
                output_tokens INTEGER NOT NULL DEFAULT 0 CHECK(output_tokens >= 0),
                total_tokens INTEGER NOT NULL DEFAULT 0 CHECK(total_tokens >= 0),
                reasoning_tokens INTEGER NOT NULL DEFAULT 0 CHECK(reasoning_tokens >= 0),
                cached_input_tokens INTEGER NOT NULL DEFAULT 0 CHECK(cached_input_tokens >= 0),
                cache_write_tokens INTEGER NOT NULL DEFAULT 0 CHECK(cache_write_tokens >= 0),
                duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0),
                attributed_at_unix INTEGER NOT NULL,
                PRIMARY KEY(parent_manifest_id, child_manifest_id)
            );
            ",
        )?;
        Ok(Self {
            sessions,
            executions,
        })
    }

    /// One registry row by child id (R2 reads).
    pub fn row(&self, child_session_id: Uuid) -> Result<Option<ChildRow>> {
        self.sessions
            .query_row(
                "SELECT parent_session_id, child_session_id, depth, provider, model,
                        effect_policy, autonomy_profile, command_fs_envelope,
                        children_max_depth, status, cancel_requested, deleted_at,
                        parent_manifest_id, created_at
                 FROM session_children WHERE child_session_id = ?1",
                params![child_session_id.to_string()],
                |row| {
                    let manifest: Option<String> = row.get(12)?;
                    Ok(ChildRow {
                        parent_session_id: uuid_at(row, 0)?,
                        child_session_id: uuid_at(row, 1)?,
                        depth: row.get(2)?,
                        provider: row.get(3)?,
                        model: row.get(4)?,

                        effect_policy: row.get::<_, String>(5)?,

                        autonomy_profile: row.get::<_, String>(6)?,

                        command_fs_envelope: row.get(7)?,

                        children_max_depth: row.get::<_, u32>(8)?,
                        status: ChildStatus::parse(&row.get::<_, String>(9)?)
                            .expect("registry status is checked"),
                        cancel_requested: row.get(10)?,
                        deleted_at: row.get(11)?,
                        parent_manifest_id: match manifest {
                            Some(value) => Some(Uuid::parse_str(&value).map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    12,
                                    rusqlite::types::Type::Text,
                                    e.to_string().into(),
                                )
                            })?),
                            None => None,
                        },
                        created_at: row.get(13)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// The child manifest id, if the child turn ever started (R4: the
    /// runner records the manifest before the status leaves `spawned`).
    pub fn child_manifest(&self, child_session_id: Uuid) -> Result<Option<Uuid>> {
        self.executions
            .query_row(
                "SELECT id FROM execution_manifests
                 WHERE session_id = ?1 ORDER BY created_unix DESC LIMIT 1",
                params![child_session_id.to_string()],
                |row| uuid_at(row, 0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Transition `spawned` -> `running`. The runner calls this before
    /// the task turn. `adopted` records the `adopted` event.
    pub fn mark_running(
        &self,
        child_session_id: Uuid,
        adopted: bool,
        resolved_provider: Option<&str>,
        resolved_model: Option<&str>,
    ) -> Result<()> {
        let tx = ImmediateTx::begin(&self.sessions)?;
        let status: Option<String> = self
            .sessions
            .query_row(
                "SELECT status FROM session_children WHERE child_session_id = ?1",
                params![child_session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(status) = status else {
            return Err(WorkflowError::Msg(format!(
                "child session {child_session_id} is not in the registry"
            )));
        };
        if status != "spawned" {
            return Err(WorkflowError::Msg(format!(
                "child {child_session_id} cannot enter running from {status}"
            )));
        }
        self.sessions.execute(
            "UPDATE session_children
             SET status = 'running', adopted_at = COALESCE(adopted_at, ?2),
                 provider = COALESCE(?3, provider),
                 model = COALESCE(?4, model)
             WHERE child_session_id = ?1",
            params![
                child_session_id.to_string(),
                now_stamp(),
                resolved_provider,
                resolved_model,
            ],
        )?;
        if adopted {
            self.sessions.execute(
                "INSERT OR IGNORE INTO session_child_events
                   (child_session_id, event_type, payload, recorded_at)
                 VALUES (?1, 'adopted', NULL, ?2)",
                params![child_session_id.to_string(), now_stamp()],
            )?;
        }
        self.sessions.execute(
            "INSERT OR IGNORE INTO session_child_events
               (child_session_id, event_type, payload, recorded_at)
             VALUES (?1, 'running', NULL, ?2)",
            params![child_session_id.to_string(), now_stamp()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Settle a child to a terminal outcome (exactly once), finish an
    /// orphaned running manifest when present, and write the
    /// attribution row. `parent_manifest_id` overrides the durable
    /// registry value (the live settle knows it directly; the crash
    /// settle reads it from the registry).
    pub fn settle(
        &self,
        child_session_id: Uuid,
        status: ChildStatus,
        reason: Option<&str>,
        duration_ms: u64,
        parent_manifest_id: Option<Uuid>,
    ) -> Result<()> {
        if !status.is_terminal() {
            return Err(WorkflowError::Msg(format!(
                "settle requires a terminal status, got {}",
                status.as_str()
            )));
        }
        // Exactly one terminal outcome: the guarded UPDATE refuses a
        // second settle and a settle after a tombstone.
        let tx = ImmediateTx::begin(&self.sessions)?;
        let now = now_stamp();
        let updated = self.sessions.execute(
            "UPDATE session_children
             SET status = ?2, terminal_at = ?3, terminal_reason = ?4,
                 cancel_requested = NULL
             WHERE child_session_id = ?1 AND status IN ('spawned','running')
               AND deleted_at IS NULL",
            params![child_session_id.to_string(), status.as_str(), now, reason,],
        )?;
        if updated != 1 {
            return Err(WorkflowError::Msg(format!(
                "child {child_session_id} is not settleable (terminal, deleted, or gone)"
            )));
        }
        self.sessions.execute(
            "INSERT OR IGNORE INTO session_child_events
               (child_session_id, event_type, payload, recorded_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![child_session_id.to_string(), status.as_str(), reason, now,],
        )?;
        tx.commit()?;

        // Crash window: an orphaned running manifest settles to match
        // the registry terminal. A settled manifest is left alone.
        let child_manifest = self.child_manifest(child_session_id)?;
        if let Some(manifest_id) = child_manifest {
            let _ = self.executions.execute(
                "UPDATE execution_manifests
                 SET status = ?2, completed_unix = ?3
                 WHERE id = ?1 AND status = 'running'",
                params![manifest_id.to_string(), status.as_str(), now_unix(),],
            )?;
        }

        // Attribution (R7): link the parent turn to the child manifest.
        // The row is unique per child manifest; a re-settle cannot
        // double-attribute.
        let parent = match parent_manifest_id {
            Some(value) => Some(value),
            None => self
                .row(child_session_id)?
                .and_then(|r| r.parent_manifest_id),
        };
        if let (Some(parent_manifest_id), Some(child_manifest)) = (parent, child_manifest) {
            let totals: (i64, i64, i64, i64, i64, i64) = self
                .executions
                .query_row(
                    "SELECT COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0),
                            COALESCE(SUM(total_tokens),0), COALESCE(SUM(reasoning_tokens),0),
                            COALESCE(SUM(cached_input_tokens),0), COALESCE(SUM(cache_write_tokens),0)
                     FROM execution_model_calls WHERE manifest_id = ?1",
                    params![child_manifest.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
                )?;
            self.executions.execute(
                "INSERT OR IGNORE INTO execution_child_attribution
                   (parent_manifest_id, child_session_id, child_manifest_id,
                    input_tokens, output_tokens, total_tokens, reasoning_tokens,
                    cached_input_tokens, cache_write_tokens, duration_ms,
                    attributed_at_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    parent_manifest_id.to_string(),
                    child_session_id.to_string(),
                    child_manifest.to_string(),
                    totals.0,
                    totals.1,
                    totals.2,
                    totals.3,
                    totals.4,
                    totals.5,
                    duration_ms as i64,
                    now_unix(),
                ],
            )?;
        }
        Ok(())
    }

    /// The adoption plan (R4): every non-terminal, non-deleted child
    /// either re-runs (no manifest) or settles (interrupted or
    /// cancel-requested). Terminal and tombstoned children are left
    /// alone.
    pub fn adoption_plan(&self) -> Result<Vec<AdoptionAction>> {
        let mut stmt = self.sessions.prepare(
            "SELECT parent_session_id, child_session_id, depth, provider, model,
                    effect_policy, autonomy_profile, command_fs_envelope,
                    children_max_depth, status, cancel_requested, deleted_at,
                    parent_manifest_id, created_at
             FROM session_children
             WHERE status IN ('spawned','running') AND deleted_at IS NULL
             ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let manifest: Option<String> = row.get(12)?;
                Ok(ChildRow {
                    parent_session_id: uuid_at(row, 0)?,
                    child_session_id: uuid_at(row, 1)?,
                    depth: row.get(2)?,
                    provider: row.get(3)?,
                    model: row.get(4)?,

                    effect_policy: row.get::<_, String>(5)?,

                    autonomy_profile: row.get::<_, String>(6)?,

                    command_fs_envelope: row.get(7)?,

                    children_max_depth: row.get::<_, u32>(8)?,
                    status: ChildStatus::parse(&row.get::<_, String>(9)?)
                        .expect("registry status is checked"),
                    cancel_requested: row.get(10)?,
                    deleted_at: row.get(11)?,
                    parent_manifest_id: manifest.and_then(|value| Uuid::parse_str(&value).ok()),
                    created_at: row.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut plan = Vec::new();
        for row in rows {
            if row.cancel_requested.is_some() {
                plan.push(AdoptionAction::Settle {
                    child_session_id: row.child_session_id,
                    status: ChildStatus::Cancelled,
                    reason: "cancel_requested",
                });
                continue;
            }
            // The manifest-before-running ordering (R4): a manifest
            // means the turn started, so the run was interrupted.
            // Re-running would apply its effects twice; settle instead.
            let manifest = self.child_manifest(row.child_session_id)?;
            if manifest.is_some() {
                plan.push(AdoptionAction::Settle {
                    child_session_id: row.child_session_id,
                    status: ChildStatus::Failed,
                    reason: "crash_interrupted",
                });
            } else {
                plan.push(AdoptionAction::Run {
                    child_session_id: row.child_session_id,
                    provider: row.provider.clone(),
                    model: row.model.clone(),
                    effect_policy: row.effect_policy.clone(),
                    autonomy_profile: row.autonomy_profile.clone(),
                    command_fs_envelope: row.command_fs_envelope.clone(),
                    children_max_depth: row.children_max_depth,
                    parent_manifest_id: row.parent_manifest_id,
                });
            }
        }
        Ok(plan)
    }

    /// Record the durable cancel marker (R6). Legal only on a
    /// non-terminal child.
    pub fn cancel_request(&self, child_session_id: Uuid, reason: &str) -> Result<()> {
        let updated = self.sessions.execute(
            "UPDATE session_children
             SET cancel_requested = ?2
             WHERE child_session_id = ?1 AND status IN ('spawned','running')
               AND cancel_requested IS NULL AND deleted_at IS NULL",
            params![child_session_id.to_string(), reason],
        )?;
        if updated == 0 {
            return Err(WorkflowError::Msg(format!(
                "child {child_session_id} is not cancellable (terminal, deleted, or already requested)"
            )));
        }
        Ok(())
    }

    /// The bounded-wait fallback (R3 gate): a non-terminal child with
    /// no live runner settles at the cancel or delete call. The
    /// terminal is `cancelled` with the reason `runner_lost` or
    /// `cancel_runner_lost`.
    pub fn settle_runner_lost(
        &self,
        child_session_id: Uuid,
        status: ChildStatus,
        reason: &str,
    ) -> Result<()> {
        self.settle(child_session_id, status, Some(reason), 0, None)
    }

    /// Write the durable tombstone (R6). Legal only on a terminal
    /// child; the terminal status never changes.
    pub fn tombstone(&self, child_session_id: Uuid) -> Result<()> {
        let tx = ImmediateTx::begin(&self.sessions)?;
        let updated = self.sessions.execute(
            "UPDATE session_children SET deleted_at = ?2
             WHERE child_session_id = ?1
               AND status IN ('succeeded','failed','cancelled')
               AND deleted_at IS NULL",
            params![child_session_id.to_string(), now_stamp()],
        )?;
        if updated != 1 {
            return Err(WorkflowError::Msg(format!(
                "child {child_session_id} is not tombstoneable (non-terminal or already deleted)"
            )));
        }
        self.sessions.execute(
            "INSERT OR IGNORE INTO session_child_events
               (child_session_id, event_type, payload, recorded_at)
             VALUES (?1, 'deleted', NULL, ?2)",
            params![child_session_id.to_string(), now_stamp()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The cancel cascade input (R6): all descendant sessions of a
    /// child, direct and transitive, in registry order. The depth is
    /// bounded (default 1) so the recursion is shallow.
    pub fn descendants(&self, child_session_id: Uuid) -> Result<Vec<Uuid>> {
        let mut pending = vec![child_session_id];
        let mut out = Vec::new();
        while let Some(parent) = pending.pop() {
            let mut stmt = self.sessions.prepare(
                "SELECT child_session_id FROM session_children
                 WHERE parent_session_id = ?1 ORDER BY created_at ASC",
            )?;
            let kids = stmt
                .query_map([parent.to_string()], |row| uuid_at(row, 0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for kid in kids {
                out.push(kid);
                pending.push(kid);
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    /// The child task prompt from the transcript (R4 adoption): the
    /// last user message of the child session.
    pub fn task_prompt(&self, child_session_id: Uuid) -> Result<Option<String>> {
        let raw: Option<String> = self
            .sessions
            .query_row(
                "SELECT messages_json FROM sessions WHERE id = ?1",
                params![child_session_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let messages: Vec<serde_json::Value> =
            serde_json::from_str(&raw).map_err(|e| WorkflowError::Msg(e.to_string()))?;
        for message in messages.iter().rev() {
            if message.get("role").and_then(|r| r.as_str()) == Some("user") {
                let text = message
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    return Ok(Some(text));
                }
            }
        }
        Ok(None)
    }

    /// Direct children of a parent session, oldest first (R2).
    pub fn children_of(&self, parent_session_id: Uuid) -> Result<Vec<ChildRow>> {
        let mut stmt = self.sessions.prepare(
            "SELECT parent_session_id, child_session_id, depth, provider, model,
                    effect_policy, autonomy_profile, command_fs_envelope,
                    children_max_depth, status, cancel_requested, deleted_at,
                    parent_manifest_id, created_at
             FROM session_children WHERE parent_session_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([parent_session_id.to_string()], |row| {
                let manifest: Option<String> = row.get(12)?;
                Ok(ChildRow {
                    parent_session_id: uuid_at(row, 0)?,
                    child_session_id: uuid_at(row, 1)?,
                    depth: row.get(2)?,
                    provider: row.get(3)?,
                    model: row.get(4)?,

                    effect_policy: row.get::<_, String>(5)?,

                    autonomy_profile: row.get::<_, String>(6)?,

                    command_fs_envelope: row.get(7)?,

                    children_max_depth: row.get::<_, u32>(8)?,
                    status: ChildStatus::parse(&row.get::<_, String>(9)?)
                        .expect("registry status is checked"),
                    cancel_requested: row.get(10)?,
                    deleted_at: row.get(11)?,
                    parent_manifest_id: manifest.and_then(|value| Uuid::parse_str(&value).ok()),
                    created_at: row.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("optimus-supervisor-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // The supervisor expects the kernel tables; bootstrap the
        // registry schema exactly as the kernel does.
        let sessions = Connection::open(dir.join("sessions.db")).unwrap();
        sessions
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS sessions (
                    id TEXT PRIMARY KEY NOT NULL,
                    title TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    packs_json TEXT NOT NULL,
                    messages_json TEXT NOT NULL
                );
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
                    created_at TEXT NOT NULL,
                    adopted_at TEXT,
                    terminal_at TEXT,
                    terminal_reason TEXT,
                    parent_manifest_id TEXT,
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
            )
            .unwrap();
        let executions = Connection::open(dir.join("execution.db")).unwrap();
        executions
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS execution_manifests (
                    id TEXT PRIMARY KEY,version INTEGER NOT NULL,session_id TEXT NOT NULL,
                    turn_id TEXT NOT NULL UNIQUE,provider TEXT NOT NULL,model TEXT NOT NULL,
                    autonomy_profile TEXT NOT NULL DEFAULT 'review_changes',
                    command_fs_envelope TEXT NOT NULL DEFAULT 'confined_no_network',
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
                    input_tokens INTEGER CHECK(input_tokens IS NULL OR input_tokens >= 0),
                    output_tokens INTEGER CHECK(output_tokens IS NULL OR output_tokens >= 0),
                    total_tokens INTEGER CHECK(total_tokens IS NULL OR total_tokens >= 0),
                    reasoning_tokens INTEGER CHECK(reasoning_tokens IS NULL OR reasoning_tokens >= 0),
                    cached_input_tokens INTEGER CHECK(cached_input_tokens IS NULL OR cached_input_tokens >= 0),
                    cache_write_tokens INTEGER CHECK(cache_write_tokens IS NULL OR cache_write_tokens >= 0),
                    PRIMARY KEY(manifest_id,step)
                );
                ",
            )
            .unwrap();
        dir
    }

    fn seed_session(dir: &std::path::Path, id: Uuid) {
        let sessions = Connection::open(dir.join("sessions.db")).unwrap();
        sessions
            .execute(
                "INSERT INTO sessions (id, title, created_at, updated_at, packs_json, messages_json)
                 VALUES (?1, 't', 'ts:1', 'ts:1', '[]', '[]')",
                params![id.to_string()],
            )
            .unwrap();
    }

    fn seed_child(
        dir: &std::path::Path,
        parent: Uuid,
        child: Uuid,
        status: &str,
        cancel_requested: Option<&str>,
        manifest: Option<Uuid>,
    ) {
        let sessions = Connection::open(dir.join("sessions.db")).unwrap();
        sessions
            .execute(
                "INSERT INTO sessions (id, title, created_at, updated_at, packs_json, messages_json)
                 VALUES (?1, 't', 'ts:1', 'ts:1', '[]', '[]')",
                params![child.to_string()],
            )
            .unwrap();
        sessions
            .execute(
                "INSERT INTO session_children
                   (parent_session_id, child_session_id, depth, task_sha256, status, created_at, parent_manifest_id)
                 VALUES (?1, ?2, 1, ?3, ?4, 'ts:1', ?5)",
                params![
                    parent.to_string(),
                    child.to_string(),
                    "a".repeat(64),
                    status,
                    manifest.map(|m| m.to_string()),
                ],
            )
            .unwrap();
        if let Some(reason) = cancel_requested {
            sessions
                .execute(
                    "UPDATE session_children SET cancel_requested = ?2 WHERE child_session_id = ?1",
                    params![child.to_string(), reason],
                )
                .unwrap();
        }
        drop(sessions);
        if let Some(manifest) = manifest {
            let executions = Connection::open(dir.join("execution.db")).unwrap();
            executions
                .execute(
                    "INSERT INTO execution_manifests
                       (id, version, session_id, turn_id, provider, model, prompt_sha256,
                        tool_catalog_sha256, policy_sha256, status, created_unix, duration_ms)
                     VALUES (?1, 1, ?2, ?3, 'offline', 'demo', ?4, ?4, ?4, 'running', 1, 0)",
                    params![
                        manifest.to_string(),
                        child.to_string(),
                        Uuid::new_v4().to_string(),
                        "a".repeat(64),
                    ],
                )
                .unwrap();
            executions
                .execute(
                    "INSERT INTO execution_model_calls
                       (manifest_id, step, provider, model, request_sha256, response_sha256,
                        replay_class, input_tokens, output_tokens, total_tokens)
                     VALUES (?1, 1, 'offline', 'demo', ?2, ?2, 'fresh', 10, 20, 30)",
                    params![manifest.to_string(), "a".repeat(64)],
                )
                .unwrap();
        }
    }

    #[test]
    fn adoption_plan_runs_never_started_and_settles_interrupted() {
        let dir = home();
        let parent = Uuid::new_v4();
        seed_session(&dir, parent);
        let never_started = Uuid::new_v4();
        let interrupted = Uuid::new_v4();
        seed_child(&dir, parent, never_started, "spawned", None, None);
        seed_child(
            &dir,
            parent,
            interrupted,
            "running",
            None,
            Some(Uuid::new_v4()),
        );
        let supervisor = ChildSupervisor::open(&dir).unwrap();
        let plan = supervisor.adoption_plan().unwrap();
        assert_eq!(
            plan,
            vec![
                AdoptionAction::Run {
                    child_session_id: never_started,
                    provider: None,
                    model: None,
                    effect_policy: "smart_deny".into(),
                    autonomy_profile: "review_changes".into(),
                    command_fs_envelope: None,
                    children_max_depth: 1,
                    parent_manifest_id: None,
                },
                AdoptionAction::Settle {
                    child_session_id: interrupted,
                    status: ChildStatus::Failed,
                    reason: "crash_interrupted",
                },
            ]
        );
    }

    #[test]
    fn adoption_settles_cancel_requested_and_never_runs_it() {
        let dir = home();
        let parent = Uuid::new_v4();
        seed_session(&dir, parent);
        let child = Uuid::new_v4();
        seed_child(&dir, parent, child, "running", Some("user asked"), None);
        let supervisor = ChildSupervisor::open(&dir).unwrap();
        let plan = supervisor.adoption_plan().unwrap();
        assert_eq!(
            plan,
            vec![AdoptionAction::Settle {
                child_session_id: child,
                status: ChildStatus::Cancelled,
                reason: "cancel_requested",
            }]
        );
        supervisor
            .settle(
                child,
                ChildStatus::Cancelled,
                Some("cancel_requested"),
                0,
                None,
            )
            .unwrap();
        assert!(supervisor.adoption_plan().unwrap().is_empty());
    }

    #[test]
    fn settle_finishes_orphaned_manifest_and_attributes_usage() {
        let dir = home();
        let parent = Uuid::new_v4();
        seed_session(&dir, parent);
        let child = Uuid::new_v4();
        let child_manifest = Uuid::new_v4();
        let parent_manifest = Uuid::new_v4();
        seed_child(&dir, parent, child, "running", None, Some(child_manifest));
        // The parent manifest row (for the FK).
        let executions = Connection::open(dir.join("execution.db")).unwrap();
        executions
            .execute(
                "INSERT INTO execution_manifests
                   (id, version, session_id, turn_id, provider, model, prompt_sha256,
                    tool_catalog_sha256, policy_sha256, status, created_unix, duration_ms)
                 VALUES (?1, 1, ?2, ?3, 'offline', 'demo', ?4, ?4, ?4, 'running', 1, 0)",
                params![
                    parent_manifest.to_string(),
                    parent.to_string(),
                    Uuid::new_v4().to_string(),
                    "a".repeat(64),
                ],
            )
            .unwrap();
        drop(executions);

        let supervisor = ChildSupervisor::open(&dir).unwrap();
        supervisor
            .settle(
                child,
                ChildStatus::Failed,
                Some("crash_interrupted"),
                42,
                Some(parent_manifest),
            )
            .unwrap();

        // The orphaned manifest is no longer running.
        let status: String = supervisor
            .executions
            .query_row(
                "SELECT status FROM execution_manifests WHERE id = ?1",
                params![child_manifest.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "failed");
        // The attribution row carries the aggregated model calls.
        let (total, input): (i64, i64) = supervisor
            .executions
            .query_row(
                "SELECT total_tokens, input_tokens FROM execution_child_attribution
                 WHERE child_manifest_id = ?1",
                params![child_manifest.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(total, 30);
        assert_eq!(input, 10);
        // A second settle refuses: exactly one terminal.
        assert!(supervisor
            .settle(
                child,
                ChildStatus::Failed,
                Some("crash_interrupted"),
                0,
                None
            )
            .is_err());
    }

    #[test]
    fn tombstone_requires_terminal_and_keeps_status() {
        let dir = home();
        let parent = Uuid::new_v4();
        seed_session(&dir, parent);
        let child = Uuid::new_v4();
        seed_child(&dir, parent, child, "spawned", None, None);
        let supervisor = ChildSupervisor::open(&dir).unwrap();
        assert!(supervisor.tombstone(child).is_err());
        supervisor
            .settle_runner_lost(child, ChildStatus::Cancelled, "runner_lost")
            .unwrap();
        supervisor.tombstone(child).unwrap();
        let row = supervisor.row(child).unwrap().unwrap();
        assert_eq!(row.status, ChildStatus::Cancelled);
        assert!(row.deleted_at.is_some());
        assert!(supervisor.tombstone(child).is_err());
    }

    #[test]
    fn cancel_cascade_collects_descendants() {
        let dir = home();
        let parent = Uuid::new_v4();
        seed_session(&dir, parent);
        let child = Uuid::new_v4();
        let grandchild = Uuid::new_v4();
        let great = Uuid::new_v4();
        seed_child(&dir, parent, child, "running", None, None);
        seed_child(&dir, child, grandchild, "running", None, None);
        seed_child(&dir, grandchild, great, "running", None, None);
        let supervisor = ChildSupervisor::open(&dir).unwrap();
        // descendants() returns sorted ids; the expectation must be sorted
        // too (Uuid ordering is lexical, not insertion order).
        let mut expected = vec![grandchild, great];
        expected.sort();
        assert_eq!(supervisor.descendants(child).unwrap(), expected);
    }
}
