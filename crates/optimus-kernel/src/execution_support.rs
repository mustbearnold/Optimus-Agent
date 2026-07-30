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
