//! Spec-025 session messaging columns and queries: inbound policy, peer
//! discovery opt-in, and dialog expiry, persisted on the session row.
//!
//! Split out of `session.rs` under the module-size ratchet (ADR-0049). The
//! durable message store itself lives in `optimus_ops::message_plane`.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::{SessionMeta, SessionStore, INBOUND_POLICIES};
use crate::execution_support::with_schema_lock;
use crate::{KernelError, Result};

impl SessionStore {
    /// spec-025: additive columns for inbound policy, discovery opt-in, and
    /// dialog expiry (idempotent migration; P24-style additive columns).
    pub(crate) fn ensure_messaging_columns(&self) -> Result<()> {
        with_schema_lock(&self.conn, || {
            let mut has_policy = false;
            let mut has_discoverable = false;
            let mut has_dialog_expiry = false;
            {
                let mut stmt = self.conn.prepare("PRAGMA table_info(sessions)")?;
                let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for name in rows.flatten() {
                    if name == "inbound_policy" {
                        has_policy = true;
                    }
                    if name == "discoverable" {
                        has_discoverable = true;
                    }
                    if name == "dialog_expiry_seconds" {
                        has_dialog_expiry = true;
                    }
                }
            }
            if !has_policy {
                self.conn.execute(
                    "ALTER TABLE sessions ADD COLUMN inbound_policy TEXT NOT NULL DEFAULT 'hold-approval'",
                    [],
                )?;
            }
            if !has_discoverable {
                self.conn.execute(
                    "ALTER TABLE sessions ADD COLUMN discoverable INTEGER NOT NULL DEFAULT 0",
                    [],
                )?;
            }
            if !has_dialog_expiry {
                self.conn.execute(
                    "ALTER TABLE sessions ADD COLUMN dialog_expiry_seconds INTEGER",
                    [],
                )?;
            }
            Ok(())
        })
    }

    /// Load one session's metadata (spec-025 R3 reads policy at send time).
    pub fn meta(&self, id: Uuid) -> Result<Option<SessionMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, packs_json, messages_json,
                    COALESCE(pinned, 0), COALESCE(archived, 0), project,
                    inbound_policy, COALESCE(discoverable, 0), dialog_expiry_seconds
             FROM sessions WHERE id = ?1",
        )?;
        let row = stmt
            .query_row(params![id.to_string()], messaging_meta_from_row)
            .optional()?;
        Ok(row)
    }

    /// spec-025 R2: every session opted into peer discovery.
    pub fn list_discoverable(&self) -> Result<Vec<SessionMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, packs_json, messages_json,
                    COALESCE(pinned, 0), COALESCE(archived, 0), project,
                    inbound_policy, COALESCE(discoverable, 0), dialog_expiry_seconds
             FROM sessions WHERE discoverable = 1
             ORDER BY updated_at DESC, rowid DESC",
        )?;
        let rows = stmt.query_map([], messaging_meta_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// spec-025 R3: persist the session's inbound policy. `deny` refuses
    /// inbound messages; `hold-approval` holds them until approved or
    /// expired; `auto-accept` delivers them.
    pub fn set_inbound_policy(&self, id: Uuid, policy: &str) -> Result<bool> {
        if !INBOUND_POLICIES.contains(&policy) {
            return Err(KernelError::Tool(format!(
                "inbound policy must be one of {INBOUND_POLICIES:?}, got {policy}"
            )));
        }
        let n = self.conn.execute(
            "UPDATE sessions SET inbound_policy = ?1 WHERE id = ?2",
            params![policy, id.to_string()],
        )?;
        Ok(n > 0)
    }

    /// spec-025 R2: opt a session into (or out of) peer discovery.
    pub fn set_discoverable(&self, id: Uuid, discoverable: bool) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE sessions SET discoverable = ?1 WHERE id = ?2",
            params![discoverable, id.to_string()],
        )?;
        Ok(n > 0)
    }

    /// spec-025 R3: per-session dialog expiry for held messages (seconds);
    /// `None` restores the plane default (30 minutes).
    pub fn set_dialog_expiry(&self, id: Uuid, seconds: Option<u64>) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE sessions SET dialog_expiry_seconds = ?1 WHERE id = ?2",
            params![seconds, id.to_string()],
        )?;
        Ok(n > 0)
    }
}

fn messaging_meta_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMeta> {
    let id = Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let packs: Vec<String> = serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default();
    let messages: Vec<crate::Message> =
        serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default();
    let pinned: i64 = row.get(6)?;
    let archived: i64 = row.get(7)?;
    let discoverable: i64 = row.get(10)?;
    Ok(SessionMeta {
        id,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        message_count: messages.len(),
        packs,
        pinned: pinned != 0,
        archived: archived != 0,
        project: row.get(8)?,
        inbound_policy: row.get(9)?,
        discoverable: discoverable != 0,
        dialog_expiry_seconds: row.get(11)?,
    })
}
