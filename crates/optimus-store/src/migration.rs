//! Versioned schema migration for the Work Graph store.
//!
//! `migrate` is the single writer of `schema_version`; every migration
//! block runs in a `BEGIN IMMEDIATE` transaction. Concurrent kernel opens
//! (the host's worker pool, one home) serialize on the write lock via the
//! connection's `busy_timeout`; a deferred transaction would instead take a
//! stale snapshot and fail its first write with SQLITE_BUSY (snapshot
//! conflict) when another opener commits first.

use rusqlite::{Connection, OptionalExtension};

use crate::Result;

pub(crate) fn schema_version(connection: &Connection) -> Result<String> {
    let v: Option<String> = connection
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(v.unwrap_or_else(|| "0".into()))
}

pub(crate) fn migrate(connection: &Connection) -> Result<()> {
    let version = schema_version(connection)?;
    let v: u32 = version.parse().map_err(|_| {
        crate::StoreError::Invariant(format!("invalid Work Graph schema version {version:?}"))
    })?;
    if v > 7 {
        return Err(crate::StoreError::Invariant(format!(
            "unsupported future Work Graph schema version {v}"
        )));
    }
    if v < 2 {
        // Serialize against concurrent openers (the host's worker pool
        // opens kernels on one home): BEGIN IMMEDIATE takes the write
        // lock up front, so a racing opener blocks (busy_timeout)
        // until this migration commits and its column checks see the
        // result instead of racing the same ALTERs.
        connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            ensure_job_column(connection, "max_steps", "INTEGER NOT NULL DEFAULT 100")?;
            ensure_job_column(connection, "steps_executed", "INTEGER NOT NULL DEFAULT 0")?;
            ensure_job_column(
                connection,
                "consecutive_failures",
                "INTEGER NOT NULL DEFAULT 0",
            )?;
            ensure_job_column(
                connection,
                "max_consecutive_failures",
                "INTEGER NOT NULL DEFAULT 3",
            )?;
            ensure_job_column(
                connection,
                "command_timeout_ms",
                "INTEGER NOT NULL DEFAULT 30000",
            )?;
            connection.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS approvals (
                    id TEXT PRIMARY KEY NOT NULL,
                    job_id TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                );
                ",
            )?;
            connection.execute(
                "INSERT INTO meta(key, value) VALUES ('schema_version', '2')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [],
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error);
        }
        connection.execute_batch("COMMIT")?;
    }
    if v < 3 {
        // Same serialization as the v2 block: the terminal_slot check
        // must see the committed result of any racing migration.
        connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let has_terminal_slot = table_has_column(connection, "events", "terminal_slot")?;
            if !has_terminal_slot {
                connection.execute("ALTER TABLE events ADD COLUMN terminal_slot INTEGER", [])?;
            }
            connection.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS job_quarantine (
                    job_id TEXT PRIMARY KEY NOT NULL,
                    reason TEXT NOT NULL,
                    detected_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                );
                INSERT OR IGNORE INTO job_quarantine(job_id, reason)
                SELECT job_id, 'multiple legacy job_terminal events'
                FROM events
                WHERE kind = 'job_terminal' AND job_id IS NOT NULL
                GROUP BY job_id
                HAVING COUNT(*) > 1;
                UPDATE events
                SET terminal_slot = CASE
                    WHEN kind = 'job_terminal'
                     AND job_id NOT IN (SELECT job_id FROM job_quarantine)
                    THEN 1
                    ELSE NULL
                END;
                CREATE UNIQUE INDEX IF NOT EXISTS one_terminal_event_per_job
                    ON events(job_id) WHERE terminal_slot = 1;
                CREATE TRIGGER IF NOT EXISTS enforce_terminal_slot_insert
                BEFORE INSERT ON events
                WHEN (NEW.kind = 'job_terminal' AND COALESCE(NEW.terminal_slot, 0) != 1)
                  OR (NEW.kind != 'job_terminal' AND NEW.terminal_slot IS NOT NULL)
                BEGIN
                    SELECT RAISE(ABORT, 'invalid terminal event slot');
                END;
                CREATE TRIGGER IF NOT EXISTS enforce_terminal_slot_update
                BEFORE UPDATE OF kind, terminal_slot, job_id ON events
                WHEN (NEW.kind = 'job_terminal' AND COALESCE(NEW.terminal_slot, 0) != 1)
                  OR (NEW.kind != 'job_terminal' AND NEW.terminal_slot IS NOT NULL)
                BEGIN
                    SELECT RAISE(ABORT, 'invalid terminal event slot');
                END;
                INSERT INTO meta(key, value) VALUES ('schema_version', '3')
                ON CONFLICT(key) DO UPDATE SET value = excluded.value;
                ",
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error);
        }
        connection.execute_batch("COMMIT")?;
    }
    if v < 4 {
        // Serialize against concurrent openers: a deferred BEGIN
        // would take a stale snapshot and fail its first write
        // with SQLITE_BUSY (snapshot conflict) when another
        // opener commits first; BEGIN IMMEDIATE waits instead.
        connection.execute_batch("BEGIN IMMEDIATE")?;
        connection.execute_batch(
            "
            CREATE TRIGGER IF NOT EXISTS reject_quarantined_job_update
            BEFORE UPDATE ON jobs
            WHEN EXISTS (SELECT 1 FROM job_quarantine WHERE job_id = OLD.id)
            BEGIN
                SELECT RAISE(ABORT, 'job is quarantined');
            END;
            CREATE TRIGGER IF NOT EXISTS reject_quarantined_node_update
            BEFORE UPDATE ON nodes
            WHEN EXISTS (SELECT 1 FROM job_quarantine WHERE job_id = OLD.job_id)
            BEGIN
                SELECT RAISE(ABORT, 'job is quarantined');
            END;
            INSERT INTO meta(key, value) VALUES ('schema_version', '4')
            ON CONFLICT(key) DO UPDATE SET value = excluded.value;
            ",
        )?;
        connection.execute_batch("COMMIT")?;
    }
    if v < 5 {
        // Serialize against concurrent openers: a deferred BEGIN
        // would take a stale snapshot and fail its first write
        // with SQLITE_BUSY (snapshot conflict) when another
        // opener commits first; BEGIN IMMEDIATE waits instead.
        connection.execute_batch("BEGIN IMMEDIATE")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS effect_attempts (
                attempt_id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                attempt_no INTEGER NOT NULL CHECK(attempt_no >= 1),
                intent_json TEXT NOT NULL,
                status TEXT NOT NULL CHECK(status IN (
                    'prepared','succeeded','failed','ambiguous','interrupted','cancelled'
                )),
                receipt_json TEXT,
                started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                finished_at TEXT,
                UNIQUE(node_id, attempt_no)
             );
             INSERT INTO meta(key, value) VALUES ('schema_version', '5')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
        )?;
        connection.execute_batch("COMMIT")?;
    }
    if v < 6 {
        // Serialize against concurrent openers: a deferred BEGIN
        // would take a stale snapshot and fail its first write
        // with SQLITE_BUSY (snapshot conflict) when another
        // opener commits first; BEGIN IMMEDIATE waits instead.
        connection.execute_batch("BEGIN IMMEDIATE")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS cancellation_requests (
                job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                reason TEXT NOT NULL,
                requested_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             );
             INSERT INTO meta(key, value) VALUES ('schema_version', '6')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
        )?;
        connection.execute_batch("COMMIT")?;
    }
    if v < 7 {
        // Serialize against concurrent openers: a deferred BEGIN
        // would take a stale snapshot and fail its first write
        // with SQLITE_BUSY (snapshot conflict) when another
        // opener commits first; BEGIN IMMEDIATE waits instead.
        connection.execute_batch("BEGIN IMMEDIATE")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS action_approvals (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                effect_hash TEXT NOT NULL CHECK(length(effect_hash) = 64),
                actor TEXT NOT NULL CHECK(length(actor) > 0),
                decision TEXT NOT NULL CHECK(decision IN ('granted','denied','revoked')),
                created_unix INTEGER NOT NULL CHECK(created_unix >= 0),
                expires_unix INTEGER NOT NULL CHECK(expires_unix > created_unix),
                revoked_unix INTEGER,
                revoked_by TEXT,
                reason TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_action_approvals_exact
             ON action_approvals(job_id,node_id,effect_hash,decision,expires_unix);
             INSERT INTO meta(key, value) VALUES ('schema_version', '7')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
        )?;
        connection.execute_batch("COMMIT")?;
    }
    Ok(())
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_job_column(connection: &Connection, name: &str, decl: &str) -> Result<()> {
    let mut stmt = connection.prepare("PRAGMA table_info(jobs)")?;
    let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut found = false;
    for c in cols {
        if c? == name {
            found = true;
            break;
        }
    }
    if !found {
        connection.execute(&format!("ALTER TABLE jobs ADD COLUMN {name} {decl}"), [])?;
    }
    Ok(())
}
