use rusqlite::{Connection, Result};

pub(super) fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
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
    )
}
