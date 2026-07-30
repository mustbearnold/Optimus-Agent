//! Small persistence helpers shared by execution manifest operations.

use std::time::{SystemTime, UNIX_EPOCH};

use optimus_packs::ReplayClass;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{KernelError, Result};

pub(crate) fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    sql_type: &str,
) -> Result<()> {
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
