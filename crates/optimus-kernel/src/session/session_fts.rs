//! Session FTS maintenance (extracted from `session.rs` for the
//! 800-line module ratchet, ADR-0049).
//!
//! The backfill runs inside an immediate, retried schema lock: a deferred
//! transaction is the documented failure mode — the check-then-write takes
//! a stale snapshot and fails its first write with SQLITE_BUSY when a
//! concurrent opener commits first (the host's worker pool opens kernels
//! on the same home concurrently).

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::execution_support::with_schema_lock;
use crate::{KernelError, Message, Role};

/// Rebuild FTS when empty (migration / first open after upgrade).
pub(crate) fn backfill_if_empty(conn: &Connection) -> Result<(), KernelError> {
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions_fts", [], |row| row.get(0))
        .map_err(|e| KernelError::Model(e.to_string()))?;
    if n > 0 {
        return Ok(());
    }
    with_schema_lock(conn, || {
        let mut stmt = conn
            .prepare("SELECT id, title, messages_json FROM sessions")
            .map_err(|e| KernelError::Model(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| KernelError::Model(e.to_string()))?;
        for row in rows {
            let (id_s, title, messages_json) =
                row.map_err(|e| KernelError::Model(e.to_string()))?;
            let id = Uuid::parse_str(&id_s).map_err(|e| KernelError::Model(e.to_string()))?;
            let messages: Vec<Message> = serde_json::from_str(&messages_json).unwrap_or_default();
            reindex(conn, id, &title, &messages)?;
        }
        Ok(())
    })
}

/// Replace one session's FTS rows. `Transaction` derefs to `Connection`,
/// so transaction callers pass `&tx` and no second helper is needed.
pub(crate) fn reindex(
    conn: &Connection,
    id: Uuid,
    title: &str,
    messages: &[Message],
) -> Result<(), KernelError> {
    let body = messages
        .iter()
        .filter(|m| matches!(m.role, Role::User | Role::Assistant))
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    conn.execute(
        "DELETE FROM sessions_fts WHERE session_id = ?1",
        params![id.to_string()],
    )
    .map_err(|e| KernelError::Model(e.to_string()))?;
    conn.execute(
        "INSERT INTO sessions_fts(title, body, session_id) VALUES (?1, ?2, ?3)",
        params![title, body, id.to_string()],
    )
    .map_err(|e| KernelError::Model(e.to_string()))?;
    Ok(())
}
