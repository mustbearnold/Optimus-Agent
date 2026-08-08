use std::time::Duration;

use rusqlite::{Connection, Result};

pub(super) fn initialize(conn: &Connection) -> Result<()> {
    // Concurrent kernel opens wait via busy_timeout, never fail locked.
    conn.busy_timeout(Duration::from_secs(5))?;
    // schema_ddl: the journal-mode pragma takes a file-level lock that the
    // busy handler does not cover; concurrent first-opens retry instead.
    crate::execution_support::schema_ddl::<rusqlite::Error>(
        conn,
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS execution_manifests(
           id TEXT PRIMARY KEY,version INTEGER NOT NULL,session_id TEXT NOT NULL,
           turn_id TEXT NOT NULL UNIQUE,provider TEXT NOT NULL,model TEXT NOT NULL,
           autonomy_profile TEXT NOT NULL DEFAULT 'review_changes' CHECK(autonomy_profile IN (
             'standard','review_changes','read_only','full_project','developer_full_access','unrestricted_host'
           )),
           command_fs_envelope TEXT NOT NULL DEFAULT 'confined_no_network' CHECK(command_fs_envelope IN (
             'confined','confined_no_network','unrestricted_host'
           )),
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
         CREATE TABLE IF NOT EXISTS execution_tool_calls(
           manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
           call_id TEXT NOT NULL,tool_id TEXT NOT NULL,
           arguments_sha256 TEXT NOT NULL CHECK(length(arguments_sha256)=64),
           outcome_sha256 TEXT NOT NULL CHECK(length(outcome_sha256)=64),
           replay_class TEXT NOT NULL,effect_attempt_id TEXT,effect_sha256 TEXT,
           receipt_sha256 TEXT,duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0),
           suppressed INTEGER NOT NULL DEFAULT 0 CHECK(suppressed IN (0,1)),
           PRIMARY KEY(manifest_id,call_id)
         );
         CREATE TABLE IF NOT EXISTS execution_trace_links(
           manifest_id TEXT PRIMARY KEY REFERENCES execution_manifests(id) ON DELETE CASCADE,
           trace_id TEXT NOT NULL,span_id TEXT NOT NULL UNIQUE,parent_span_id TEXT
         );
         CREATE TABLE IF NOT EXISTS execution_timing_events(
           sequence INTEGER PRIMARY KEY AUTOINCREMENT,
           manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
           kind TEXT NOT NULL,step INTEGER,call_id TEXT,name TEXT,duration_ms INTEGER,
           elapsed_ms INTEGER NOT NULL CHECK(elapsed_ms >= 0),status TEXT,
           suppressed INTEGER NOT NULL CHECK(suppressed IN (0,1))
         );
         CREATE TABLE IF NOT EXISTS execution_tool_events(
           sequence INTEGER PRIMARY KEY AUTOINCREMENT,
           event_id TEXT NOT NULL UNIQUE,
           manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
           call_id TEXT NOT NULL,phase TEXT NOT NULL,
           event_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS execution_chat_approvals(
           manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
           call_id TEXT NOT NULL,
           binding_json TEXT NOT NULL,
           call_json TEXT NOT NULL,
           status TEXT NOT NULL CHECK(status IN ('pending','approved','denied')),
           PRIMARY KEY(manifest_id,call_id)
         );",
    )?;
    migrate_developer_full_access_profile(conn)
}

fn migrate_developer_full_access_profile(conn: &Connection) -> Result<()> {
    let schema: String = conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='execution_manifests'",
        [],
        |row| row.get(0),
    )?;
    if schema.contains("'developer_full_access'") {
        return Ok(());
    }
    // A pre-authority table has no `autonomy_profile` at all — the column (and
    // its CHECK) arrives later via `ensure_column`, already spelled with
    // `developer_full_access`. There is no constraint to widen, and selecting
    // the column here would fail on the very databases this path exists for.
    let has_profile_column: bool = conn
        .prepare(
            "SELECT 1 FROM pragma_table_info('execution_manifests') WHERE name='autonomy_profile'",
        )?
        .exists([])?;
    if !has_profile_column {
        return Ok(());
    }

    // SQLite cannot widen a CHECK constraint in place. Rebuild only the parent
    // table while foreign-key enforcement is suspended; child tables continue
    // to reference the final `execution_manifests` name.
    conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
    let migrated = conn.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE execution_manifests_v2(
           id TEXT PRIMARY KEY,version INTEGER NOT NULL,session_id TEXT NOT NULL,
           turn_id TEXT NOT NULL UNIQUE,provider TEXT NOT NULL,model TEXT NOT NULL,
           autonomy_profile TEXT NOT NULL DEFAULT 'review_changes' CHECK(autonomy_profile IN (
             'standard','review_changes','read_only','full_project','developer_full_access','unrestricted_host'
           )),
           command_fs_envelope TEXT NOT NULL DEFAULT 'confined_no_network' CHECK(command_fs_envelope IN (
             'confined','confined_no_network','unrestricted_host'
           )),
           prompt_sha256 TEXT NOT NULL CHECK(length(prompt_sha256)=64),
           tool_catalog_sha256 TEXT NOT NULL CHECK(length(tool_catalog_sha256)=64),
           policy_sha256 TEXT NOT NULL CHECK(length(policy_sha256)=64),
           status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),
           created_unix INTEGER NOT NULL,completed_unix INTEGER,
           duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0)
         );
         INSERT INTO execution_manifests_v2
           SELECT id,version,session_id,turn_id,provider,model,autonomy_profile,
                  command_fs_envelope,prompt_sha256,tool_catalog_sha256,policy_sha256,
                  status,created_unix,completed_unix,duration_ms
             FROM execution_manifests;
         DROP TABLE execution_manifests;
         ALTER TABLE execution_manifests_v2 RENAME TO execution_manifests;
         COMMIT;",
    );
    if let Err(error) = migrated {
        let _ = conn.execute_batch("ROLLBACK; PRAGMA foreign_keys=ON;");
        return Err(error);
    }
    conn.execute_batch("PRAGMA foreign_keys=ON;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn legacy_manifest_profile_constraint_is_migrated_without_losing_rows() {
        let dir = tempdir().unwrap();
        let conn = Connection::open(dir.path().join("execution.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE execution_manifests(
               id TEXT PRIMARY KEY,version INTEGER NOT NULL,session_id TEXT NOT NULL,
               turn_id TEXT NOT NULL UNIQUE,provider TEXT NOT NULL,model TEXT NOT NULL,
               autonomy_profile TEXT NOT NULL DEFAULT 'review_changes' CHECK(autonomy_profile IN (
                 'standard','review_changes','read_only','full_project','unrestricted_host'
               )),
               command_fs_envelope TEXT NOT NULL DEFAULT 'confined_no_network' CHECK(command_fs_envelope IN (
                 'confined','confined_no_network','unrestricted_host'
               )),
               prompt_sha256 TEXT NOT NULL CHECK(length(prompt_sha256)=64),
               tool_catalog_sha256 TEXT NOT NULL CHECK(length(tool_catalog_sha256)=64),
               policy_sha256 TEXT NOT NULL CHECK(length(policy_sha256)=64),
               status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),
               created_unix INTEGER NOT NULL,completed_unix INTEGER,
               duration_ms INTEGER NOT NULL DEFAULT 0 CHECK(duration_ms >= 0)
             );
             INSERT INTO execution_manifests VALUES(
               'old',1,'session-old','turn-old','codex','gpt-5.6-sol','full_project',
               'confined',printf('%064d',0),printf('%064d',0),printf('%064d',0),
               'succeeded',1,2,1
             );",
        )
        .unwrap();

        initialize(&conn).unwrap();

        let retained: String = conn
            .query_row(
                "SELECT autonomy_profile FROM execution_manifests WHERE id='old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retained, "full_project");
        conn.execute(
            "INSERT INTO execution_manifests VALUES(
               'new',1,'session-new','turn-new','codex','gpt-5.6-sol','developer_full_access',
               'confined',printf('%064d',0),printf('%064d',0),printf('%064d',0),
               'running',3,NULL,0
             )",
            [],
        )
        .unwrap();
    }
}
