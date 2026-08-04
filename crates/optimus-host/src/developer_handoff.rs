//! Session handoff: a stable snapshot of the parent stores for the child.
//!
//! Split out of `developer.rs` under the ADR-0049 module-size ratchet. Copying
//! rather than sharing is the point: once the child has its own snapshot, its
//! messages and tool traces cannot mutate the parent control channel.

use std::fs;
use std::path::Path;

use optimus_kernel::{atomic_write_user_only, SessionStore};
use rusqlite::{params, Connection};
use serde_json::json;

use crate::developer::{now_unix_ms, SupervisorSpec};

pub(crate) fn optional_handoff_session_id(
    home: &Path,
    params: &serde_json::Value,
) -> Result<Option<String>, String> {
    let Some(raw) = params.get("session_id") else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let id = raw
        .as_str()
        .ok_or_else(|| "session_id must be a UUID string".to_string())?;
    let id = uuid::Uuid::parse_str(id).map_err(|error| format!("invalid session_id: {error}"))?;
    let store = SessionStore::open(home.join("sessions.db")).map_err(|error| error.to_string())?;
    if !store.exists(id).map_err(|error| error.to_string())? {
        return Err(format!("session not found for handoff: {id}"));
    }
    Ok(Some(id.to_string()))
}

/// Copy a stable snapshot of the parent session stores into the child home.
/// The child is independent after this point: later messages and tool traces
/// cannot mutate the parent control channel.
pub(crate) fn snapshot_session_handoff(home: &Path, spec: &SupervisorSpec) -> Result<(), String> {
    let Some(session_id) = spec.handoff_session_id.as_deref() else {
        return Ok(());
    };
    let session_id = uuid::Uuid::parse_str(session_id)
        .map_err(|error| format!("invalid handoff session_id: {error}"))?;
    let sessions =
        SessionStore::open(home.join("sessions.db")).map_err(|error| error.to_string())?;
    if !sessions
        .exists(session_id)
        .map_err(|error| error.to_string())?
    {
        return Err(format!("session disappeared before handoff: {session_id}"));
    }
    for meta in sessions.list().map_err(|error| error.to_string())? {
        if sessions
            .active_turn(meta.id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(
                "cannot hand off a session while any parent session has an active turn".to_string(),
            );
        }
    }

    let destination = Path::new(&spec.child_home);
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for name in ["sessions.db", "execution.db"] {
        copy_sqlite_snapshot(&home.join(name), &destination.join(name))?;
    }
    prune_session_snapshot(destination, session_id)?;
    let marker = json!({
        "version": 1,
        "session_id": session_id,
        "source_home": home.display().to_string(),
        "created_unix_ms": now_unix_ms(),
    });
    let marker_path = destination.join("handoff.json");
    atomic_write_user_only(&marker_path, marker.to_string().as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn copy_sqlite_snapshot(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_file() {
        let _ = fs::remove_file(target);
        for suffix in ["-wal", "-shm"] {
            let _ = fs::remove_file(format!("{}{}", target.display(), suffix));
        }
        return Ok(());
    }
    let connection = Connection::open(source).map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| format!("could not checkpoint {}: {error}", source.display()))?;
    drop(connection);

    let temporary = target.with_file_name(format!(
        ".{}.handoff-{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("db"),
        uuid::Uuid::new_v4().simple()
    ));
    let _ = fs::remove_file(&temporary);
    fs::copy(source, &temporary)
        .map_err(|error| format!("could not snapshot {}: {error}", source.display()))?;
    fs::rename(&temporary, target)
        .map_err(|error| format!("could not install snapshot {}: {error}", target.display()))?;
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{}", target.display(), suffix));
    }
    Ok(())
}

pub(crate) fn prune_session_snapshot(
    destination: &Path,
    session_id: uuid::Uuid,
) -> Result<(), String> {
    let session_path = destination.join("sessions.db");
    let sessions = Connection::open(&session_path).map_err(|error| error.to_string())?;
    sessions
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|error| error.to_string())?;
    sessions
        .execute(
            "DELETE FROM sessions WHERE id <> ?1",
            params![session_id.to_string()],
        )
        .map_err(|error| format!("could not prune handed-off sessions: {error}"))?;
    sessions
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| error.to_string())?;
    drop(sessions);

    let execution_path = destination.join("execution.db");
    if !execution_path.is_file() {
        return Ok(());
    }
    let executions = Connection::open(&execution_path).map_err(|error| error.to_string())?;
    executions
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|error| error.to_string())?;
    executions
        .execute(
            "DELETE FROM execution_manifests WHERE session_id <> ?1",
            params![session_id.to_string()],
        )
        .map_err(|error| format!("could not prune handed-off execution traces: {error}"))?;
    executions
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| error.to_string())?;
    Ok(())
}
