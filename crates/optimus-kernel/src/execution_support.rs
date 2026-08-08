//! Small persistence helpers shared by execution manifest operations.

use std::time::{SystemTime, UNIX_EPOCH};

use optimus_packs::ReplayClass;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{KernelError, Result, EXECUTION_MANIFEST_VERSION};

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_manifest(
    connection: &Connection,
    id: Uuid,
    session_id: Uuid,
    turn_id: Uuid,
    provider: &str,
    model: &str,
    autonomy_profile: &str,
    command_fs_envelope: &str,
    prompt: &[u8],
    tool_catalog: &[u8],
    policy: &[u8],
) -> Result<()> {
    if provider.trim().is_empty() || model.trim().is_empty() {
        return Err(KernelError::Model(
            "execution manifest requires provider and model identity".into(),
        ));
    }
    if optimus_graph::AutonomyProfile::parse(autonomy_profile)
        .is_none_or(|profile| profile.as_str() != autonomy_profile)
    {
        return Err(KernelError::Model(
            "execution manifest requires a canonical autonomy profile".into(),
        ));
    }
    if !matches!(
        command_fs_envelope,
        "confined" | "confined_no_network" | "unrestricted_host"
    ) {
        return Err(KernelError::Model(
            "execution manifest requires a canonical command envelope".into(),
        ));
    }
    connection.execute(
        "INSERT INTO execution_manifests(
           id,version,session_id,turn_id,provider,model,autonomy_profile,command_fs_envelope,prompt_sha256,
           tool_catalog_sha256,policy_sha256,status,created_unix
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'running',?12)",
        params![
            id.to_string(),
            EXECUTION_MANIFEST_VERSION as i64,
            session_id.to_string(),
            turn_id.to_string(),
            provider,
            model,
            autonomy_profile,
            command_fs_envelope,
            sha256(prompt),
            sha256(tool_catalog),
            sha256(policy),
            now_unix() as i64
        ],
    )?;
    Ok(())
}

/// Run `body` inside an immediate transaction (schema migrations only).
///
/// Check-then-alter migrations must be atomic: the host's worker pool
/// opens kernels on the same home concurrently, and two openers that both
/// pass the `PRAGMA table_info` check will race the `ALTER TABLE` (one
/// fails with "duplicate column name"). `BEGIN IMMEDIATE` takes the write
/// lock up front; with the connection's `busy_timeout`, a racing opener
/// waits for the migration instead of failing it.
pub(crate) fn with_schema_lock<T>(
    connection: &Connection,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    connection.execute_batch("BEGIN IMMEDIATE")?;
    match body() {
        Ok(value) => {
            connection.execute_batch("COMMIT")?;
            Ok(value)
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

/// Run the open-time schema batch, retrying the transient busy failure that
/// the busy handler does not cover.
///
/// `PRAGMA journal_mode = WAL` takes a file-level lock; SQLite returns
/// SQLITE_BUSY immediately for that statement without consulting the busy
/// handler. The host's worker pool opens kernels on the same home
/// concurrently, so several openers race the journal-mode change on a fresh
/// file. The window is microseconds (the winning opener finishes its DDL),
/// so retrying the idempotent batch is the documented pattern.
pub(crate) fn schema_ddl<E>(connection: &Connection, batch: &str) -> std::result::Result<(), E>
where
    rusqlite::Error: Into<E>,
{
    let mut attempts = 0;
    loop {
        match connection.execute_batch(batch) {
            Ok(()) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(failure, _))
                if failure.code == rusqlite::ErrorCode::DatabaseBusy && attempts < 8 =>
            {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub(crate) fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    sql_type: &str,
) -> Result<()> {
    with_schema_lock(connection, || {
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
    })
}

pub(crate) fn read_classes(
    connection: &Connection,
    sql: &str,
    manifest_id: Uuid,
) -> Result<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![manifest_id.to_string()], |row| row.get(0))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(KernelError::Sqlite)
}

pub(crate) fn replay_name(class: ReplayClass) -> &'static str {
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

pub(crate) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
