//! Open-time schema helpers for the memory database.
//!
//! The host's worker pool opens kernels on the same home concurrently. Two
//! SQLite quirks force these helpers:
//!
//! - `PRAGMA journal_mode = WAL` takes a file-level lock that the busy
//!   handler does not cover; concurrent first-opens of a fresh file race it
//!   and fail with SQLITE_BUSY. The idempotent batch is retried.
//! - Check-then-alter column migrations are TOCTOU races; `BEGIN IMMEDIATE`
//!   takes the write lock up front so a racing opener waits via
//!   `busy_timeout` instead of failing a duplicate `ALTER`.

use std::time::Duration;

use rusqlite::Connection;

use crate::{MemoryError, Result};

/// Run the open-time schema batch, retrying the transient busy failure that
/// the busy handler does not cover (see module docs).
pub(super) fn schema_ddl(connection: &Connection, batch: &str) -> Result<()> {
    let mut attempts = 0;
    loop {
        match connection.execute_batch(batch) {
            Ok(()) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(failure, _))
                if failure.code == rusqlite::ErrorCode::DatabaseBusy && attempts < 8 =>
            {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// Idempotent additive column migration, serialized against concurrent
/// openers (see module docs).
pub(super) fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let valid_identifier = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    };
    if !valid_identifier(table) || !valid_identifier(column) {
        return Err(MemoryError::Invariant(
            "invalid schema migration identifier".into(),
        ));
    }
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement.query_map([], |row| row.get::<_, String>(1))?;
        for name in names {
            if name? == column {
                return Ok(());
            }
        }
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            connection.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}
