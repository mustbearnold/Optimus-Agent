//! SQLite event ledger and job/node projections.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invariant: {0}")]
    Invariant(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
    AwaitingApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Cancelled,
    AwaitingApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRow {
    pub id: Uuid,
    pub label: String,
    pub status: JobStatus,
    pub max_steps: u32,
    pub steps_executed: u32,
    pub consecutive_failures: u32,
    pub max_consecutive_failures: u32,
    pub command_timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRow {
    pub id: Uuid,
    pub job_id: Uuid,
    pub idx: u32,
    pub label: String,
    pub status: NodeStatus,
    pub effect_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: i64,
    pub job_id: Option<Uuid>,
    pub node_id: Option<Uuid>,
    pub kind: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobQuarantineRow {
    pub job_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct PreparedEffectAttemptRow {
    pub attempt_id: Uuid,
    pub job_id: Uuid,
    pub node_id: Uuid,
    pub intent_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectAttemptOutcomeRow {
    pub attempt_id: Uuid,
    pub job_id: Uuid,
    pub node_id: Uuid,
    pub intent_json: String,
    pub status: String,
    pub receipt_json: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedAttemptDisposition {
    Interrupted,
    Ambiguous,
}

impl PreparedAttemptDisposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Interrupted => "interrupted",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewNodeGraph {
    pub id: Uuid,
    pub idx: u32,
    pub label: String,
    pub status: NodeStatus,
    pub effect_json: String,
    pub event_payload_json: String,
}

#[derive(Debug, Clone)]
pub struct NewJobGraph {
    pub id: Uuid,
    pub label: String,
    pub status: JobStatus,
    pub max_steps: u32,
    pub max_consecutive_failures: u32,
    pub command_timeout_ms: u32,
    pub event_payload_json: String,
    pub nodes: Vec<NewNodeGraph>,
}

#[derive(Debug, Clone, Copy)]
pub struct CoupledStatusTransition {
    pub job_id: Uuid,
    pub expected_job: JobStatus,
    pub next_job: JobStatus,
    pub node_id: Uuid,
    pub expected_node: NodeStatus,
    pub next_node: NodeStatus,
}

#[derive(Debug, Clone)]
pub struct NewActionApproval {
    pub id: Uuid,
    pub job_id: Uuid,
    pub node_id: Uuid,
    pub effect_hash: String,
    pub actor: String,
    pub created_unix: u64,
    pub expires_unix: u64,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        if let Some(advertised) = Self::advertised_schema_version(&conn)? {
            let version: u32 = advertised.parse().map_err(|_| {
                StoreError::Invariant(format!("invalid Work Graph schema version {advertised:?}"))
            })?;
            if version > 7 {
                return Err(StoreError::Invariant(format!(
                    "unsupported future Work Graph schema version {version}"
                )));
            }
        }
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY NOT NULL,
                label TEXT NOT NULL,
                status TEXT NOT NULL,
                max_steps INTEGER NOT NULL DEFAULT 100,
                steps_executed INTEGER NOT NULL DEFAULT 0,
                consecutive_failures INTEGER NOT NULL DEFAULT 0,
                max_consecutive_failures INTEGER NOT NULL DEFAULT 3,
                command_timeout_ms INTEGER NOT NULL DEFAULT 30000
            );
            CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                idx INTEGER NOT NULL,
                label TEXT NOT NULL,
                status TEXT NOT NULL,
                effect_json TEXT NOT NULL,
                UNIQUE(job_id, idx)
            );
            CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id TEXT,
                node_id TEXT,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                terminal_slot INTEGER,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            );
            CREATE TABLE IF NOT EXISTS job_quarantine (
                job_id TEXT PRIMARY KEY NOT NULL,
                reason TEXT NOT NULL,
                detected_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            );
            CREATE TABLE IF NOT EXISTS approvals (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            );
            CREATE TABLE IF NOT EXISTS effect_attempts (
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
            CREATE TABLE IF NOT EXISTS cancellation_requests (
                job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                reason TEXT NOT NULL,
                requested_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
            );
            CREATE TABLE IF NOT EXISTS action_approvals (
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
            ",
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '1')",
            [],
        )?;
        let store = Self { conn };
        store.migrate()?;
        store.refresh_job_quarantine()?;
        Ok(store)
    }

    fn advertised_schema_version(connection: &Connection) -> Result<Option<String>> {
        let has_meta: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master
                           WHERE type = 'table' AND name = 'meta')",
            [],
            |row| row.get(0),
        )?;
        if !has_meta {
            return Ok(None);
        }
        Ok(connection
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn migrate(&self) -> Result<()> {
        let version = self.schema_version()?;
        let v: u32 = version.parse().map_err(|_| {
            StoreError::Invariant(format!("invalid Work Graph schema version {version:?}"))
        })?;
        if v > 7 {
            return Err(StoreError::Invariant(format!(
                "unsupported future Work Graph schema version {v}"
            )));
        }
        if v < 2 {
            self.ensure_job_column("max_steps", "INTEGER NOT NULL DEFAULT 100")?;
            self.ensure_job_column("steps_executed", "INTEGER NOT NULL DEFAULT 0")?;
            self.ensure_job_column("consecutive_failures", "INTEGER NOT NULL DEFAULT 0")?;
            self.ensure_job_column("max_consecutive_failures", "INTEGER NOT NULL DEFAULT 3")?;
            self.ensure_job_column("command_timeout_ms", "INTEGER NOT NULL DEFAULT 30000")?;
            self.conn.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS approvals (
                    id TEXT PRIMARY KEY NOT NULL,
                    job_id TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                );
                ",
            )?;
            self.conn.execute(
                "INSERT INTO meta(key, value) VALUES ('schema_version', '2')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [],
            )?;
        }
        if v < 3 {
            let has_terminal_slot = self.table_has_column("events", "terminal_slot")?;
            let transaction = self.conn.unchecked_transaction()?;
            if !has_terminal_slot {
                transaction.execute("ALTER TABLE events ADD COLUMN terminal_slot INTEGER", [])?;
            }
            transaction.execute_batch(
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
            transaction.commit()?;
        }
        if v < 4 {
            let transaction = self.conn.unchecked_transaction()?;
            transaction.execute_batch(
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
            transaction.commit()?;
        }
        if v < 5 {
            let transaction = self.conn.unchecked_transaction()?;
            transaction.execute_batch(
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
            transaction.commit()?;
        }
        if v < 6 {
            let transaction = self.conn.unchecked_transaction()?;
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS cancellation_requests (
                    job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                    reason TEXT NOT NULL,
                    requested_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                 );
                 INSERT INTO meta(key, value) VALUES ('schema_version', '6')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
            )?;
            transaction.commit()?;
        }
        if v < 7 {
            let transaction = self.conn.unchecked_transaction()?;
            transaction.execute_batch(
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
            transaction.commit()?;
        }
        Ok(())
    }

    fn refresh_job_quarantine(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            INSERT OR IGNORE INTO job_quarantine(job_id, reason)
            SELECT DISTINCT n.job_id, 'node projection lacks corresponding lifecycle event'
            FROM nodes n
            WHERE n.status IN ('running', 'succeeded', 'failed', 'interrupted',
                               'cancelled', 'awaiting_approval')
              AND NOT EXISTS (
                  SELECT 1 FROM events e
                  WHERE e.node_id = n.id
                    AND e.kind = CASE n.status
                        WHEN 'running' THEN 'node_running'
                        WHEN 'succeeded' THEN 'node_succeeded'
                        WHEN 'failed' THEN 'node_failed'
                        WHEN 'interrupted' THEN 'node_interrupted'
                        WHEN 'cancelled' THEN 'node_cancelled'
                        WHEN 'awaiting_approval' THEN 'node_awaiting_approval'
                    END
              );
            INSERT OR IGNORE INTO job_quarantine(job_id, reason)
            SELECT j.id, 'terminal job projection lacks exactly one terminal event'
            FROM jobs j
            WHERE j.status IN ('succeeded', 'failed', 'cancelled')
              AND (SELECT COUNT(*) FROM events e
                   WHERE e.job_id = j.id AND e.terminal_slot = 1) != 1;
            INSERT OR IGNORE INTO job_quarantine(job_id, reason)
            SELECT j.id, 'terminal event contradicts nonterminal job projection'
            FROM jobs j
            WHERE j.status NOT IN ('succeeded', 'failed', 'cancelled')
              AND EXISTS (SELECT 1 FROM events e
                          WHERE e.job_id = j.id AND e.terminal_slot = 1);
            ",
        )?;
        Ok(())
    }

    fn ensure_job_not_quarantined(&self, job_id: Uuid) -> Result<()> {
        let reason: Option<String> = self
            .conn
            .query_row(
                "SELECT reason FROM job_quarantine WHERE job_id = ?1",
                params![job_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(reason) = reason {
            return Err(StoreError::Invariant(format!(
                "job {job_id} is quarantined: {reason}"
            )));
        }
        Ok(())
    }

    fn table_has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for existing in columns {
            if existing? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ensure_job_column(&self, name: &str, decl: &str) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(jobs)")?;
        let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for c in cols {
            if c? == name {
                found = true;
                break;
            }
        }
        if !found {
            self.conn
                .execute(&format!("ALTER TABLE jobs ADD COLUMN {name} {decl}"), [])?;
        }
        Ok(())
    }

    pub fn append_event(
        &self,
        job_id: Option<Uuid>,
        node_id: Option<Uuid>,
        kind: &str,
        payload: &impl Serialize,
    ) -> Result<i64> {
        let payload_json = serde_json::to_string(payload)?;
        self.conn.execute(
            "INSERT INTO events(job_id, node_id, kind, payload_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                job_id.map(|id| id.to_string()),
                node_id.map(|id| id.to_string()),
                kind,
                payload_json
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_events(&self, job_id: Option<Uuid>) -> Result<Vec<EventRow>> {
        let mut out = Vec::new();
        if let Some(jid) = job_id {
            let mut stmt = self.conn.prepare(
                "SELECT seq, job_id, node_id, kind, payload_json FROM events
                 WHERE job_id = ?1 ORDER BY seq ASC",
            )?;
            let rows = stmt.query_map(params![jid.to_string()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for r in rows {
                let (id, job, node, kind, payload) = r?;
                out.push(EventRow {
                    id,
                    job_id: job.and_then(|s| Uuid::parse_str(&s).ok()),
                    node_id: node.and_then(|s| Uuid::parse_str(&s).ok()),
                    kind,
                    payload_json: payload,
                });
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT seq, job_id, node_id, kind, payload_json FROM events ORDER BY seq ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?;
            for r in rows {
                let (id, job, node, kind, payload) = r?;
                out.push(EventRow {
                    id,
                    job_id: job.and_then(|s| Uuid::parse_str(&s).ok()),
                    node_id: node.and_then(|s| Uuid::parse_str(&s).ok()),
                    kind,
                    payload_json: payload,
                });
            }
        }
        Ok(out)
    }

    pub fn insert_job(
        &self,
        id: Uuid,
        label: &str,
        status: JobStatus,
        max_steps: u32,
        max_consecutive_failures: u32,
        command_timeout_ms: u32,
    ) -> Result<()> {
        let status = status_str_job(status)?;
        self.conn.execute(
            "INSERT INTO jobs(id, label, status, max_steps, steps_executed, consecutive_failures, max_consecutive_failures, command_timeout_ms)
             VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?6)",
            params![
                id.to_string(),
                label,
                status,
                max_steps,
                max_consecutive_failures,
                command_timeout_ms
            ],
        )?;
        Ok(())
    }

    /// Atomically insert a complete job projection and its creation events.
    /// Any late node or event failure rolls back every row in the bundle.
    pub fn insert_job_graph(&self, job: NewJobGraph) -> Result<()> {
        let job_status = status_str_job(job.status)?;
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO jobs(id, label, status, max_steps, steps_executed, consecutive_failures, max_consecutive_failures, command_timeout_ms)
             VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?6)",
            params![
                job.id.to_string(),
                job.label,
                job_status,
                job.max_steps,
                job.max_consecutive_failures,
                job.command_timeout_ms
            ],
        )?;
        transaction.execute(
            "INSERT INTO events(job_id, node_id, kind, payload_json)
             VALUES (?1, NULL, 'job_created', ?2)",
            params![job.id.to_string(), job.event_payload_json],
        )?;
        for node in job.nodes {
            let node_status = status_str_node(node.status)?;
            transaction.execute(
                "INSERT INTO nodes(id, job_id, idx, label, status, effect_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    node.id.to_string(),
                    job.id.to_string(),
                    node.idx,
                    node.label,
                    node_status,
                    node.effect_json
                ],
            )?;
            transaction.execute(
                "INSERT INTO events(job_id, node_id, kind, payload_json)
                 VALUES (?1, ?2, 'node_created', ?3)",
                params![
                    job.id.to_string(),
                    node.id.to_string(),
                    node.event_payload_json
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_node(
        &self,
        id: Uuid,
        job_id: Uuid,
        idx: u32,
        label: &str,
        status: NodeStatus,
        effect_json: &str,
    ) -> Result<()> {
        let status = status_str_node(status)?;
        self.conn.execute(
            "INSERT INTO nodes(id, job_id, idx, label, status, effect_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.to_string(),
                job_id.to_string(),
                idx,
                label,
                status,
                effect_json
            ],
        )?;
        Ok(())
    }

    pub fn set_job_status(&self, id: Uuid, status: JobStatus) -> Result<()> {
        let status = status_str_job(status)?;
        let n = self.conn.execute(
            "UPDATE jobs SET status = ?1 WHERE id = ?2",
            params![status, id.to_string()],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("job {id}")));
        }
        Ok(())
    }

    pub fn set_node_status(&self, id: Uuid, status: NodeStatus) -> Result<()> {
        let status = status_str_node(status)?;
        let n = self.conn.execute(
            "UPDATE nodes SET status = ?1 WHERE id = ?2",
            params![status, id.to_string()],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("node {id}")));
        }
        Ok(())
    }

    /// Compare-and-swap one node projection and append its ledger event in the
    /// same transaction. A stale projection or failed event insert changes
    /// neither surface.
    pub fn transition_node_with_event(
        &self,
        job_id: Uuid,
        node_id: Uuid,
        expected: NodeStatus,
        next: NodeStatus,
        kind: &str,
        payload: &impl Serialize,
    ) -> Result<i64> {
        self.ensure_job_not_quarantined(job_id)?;
        let expected = status_str_node(expected)?;
        let next = status_str_node(next)?;
        let payload_json = serde_json::to_string(payload)?;
        let transaction = self.conn.unchecked_transaction()?;
        let updated = transaction.execute(
            "UPDATE nodes SET status = ?1
             WHERE id = ?2 AND job_id = ?3 AND status = ?4",
            params![next, node_id.to_string(), job_id.to_string(), expected],
        )?;
        if updated != 1 {
            return Err(StoreError::Invariant(format!(
                "node {node_id} is not {expected} in job {job_id}"
            )));
        }
        transaction.execute(
            "INSERT INTO events(job_id, node_id, kind, payload_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![job_id.to_string(), node_id.to_string(), kind, payload_json],
        )?;
        let event_id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(event_id)
    }

    /// Atomically persist an immutable effect intent, move its node to running,
    /// and append the correlated lifecycle event.
    pub fn begin_effect_attempt(
        &self,
        job_id: Uuid,
        node_id: Uuid,
        expected: NodeStatus,
        intent_json: &str,
    ) -> Result<Uuid> {
        self.ensure_job_not_quarantined(job_id)?;
        let expected = status_str_node(expected)?;
        let attempt_id = Uuid::new_v4();
        let transaction = self.conn.unchecked_transaction()?;
        let next_attempt: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(attempt_no),0) + 1
             FROM effect_attempts WHERE node_id=?1",
            params![node_id.to_string()],
            |row| row.get(0),
        )?;
        let updated = transaction.execute(
            "UPDATE nodes SET status='running'
             WHERE id=?1 AND job_id=?2 AND status=?3",
            params![node_id.to_string(), job_id.to_string(), expected],
        )?;
        if updated != 1 {
            return Err(StoreError::Invariant(format!(
                "node {node_id} is not {expected} in job {job_id}"
            )));
        }
        transaction.execute(
            "INSERT INTO effect_attempts(
               attempt_id,job_id,node_id,attempt_no,intent_json,status
             ) VALUES (?1,?2,?3,?4,?5,'prepared')",
            params![
                attempt_id.to_string(),
                job_id.to_string(),
                node_id.to_string(),
                next_attempt,
                intent_json
            ],
        )?;
        let payload_json = serde_json::to_string(&serde_json::json!({
            "attempt_id": attempt_id,
            "attempt_no": next_attempt,
        }))?;
        transaction.execute(
            "INSERT INTO events(job_id,node_id,kind,payload_json)
             VALUES (?1,?2,'node_running',?3)",
            params![job_id.to_string(), node_id.to_string(), payload_json],
        )?;
        transaction.commit()?;
        Ok(attempt_id)
    }

    /// Atomically close one prepared effect attempt as succeeded, persist its
    /// receipt, advance the node projection, and append the correlated event.
    pub fn complete_effect_attempt_success(
        &self,
        job_id: Uuid,
        node_id: Uuid,
        attempt_id: Uuid,
        receipt: &impl Serialize,
    ) -> Result<()> {
        self.ensure_job_not_quarantined(job_id)?;
        let receipt_json = serde_json::to_string(receipt)?;
        let transaction = self.conn.unchecked_transaction()?;
        let attempt_updated = transaction.execute(
            "UPDATE effect_attempts
             SET status='succeeded',receipt_json=?1,
                 finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE attempt_id=?2 AND job_id=?3 AND node_id=?4 AND status='prepared'",
            params![
                receipt_json,
                attempt_id.to_string(),
                job_id.to_string(),
                node_id.to_string()
            ],
        )?;
        let node_updated = transaction.execute(
            "UPDATE nodes SET status='succeeded'
             WHERE id=?1 AND job_id=?2 AND status='running'",
            params![node_id.to_string(), job_id.to_string()],
        )?;
        if attempt_updated != 1 || node_updated != 1 {
            return Err(StoreError::Invariant(format!(
                "stale successful effect attempt {attempt_id} for node {node_id}"
            )));
        }
        let event_payload = serde_json::to_string(&serde_json::json!({
            "attempt_id": attempt_id,
            "receipt": receipt,
        }))?;
        transaction.execute(
            "INSERT INTO events(job_id,node_id,kind,payload_json)
             VALUES (?1,?2,'node_succeeded',?3)",
            params![job_id.to_string(), node_id.to_string(), event_payload],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn complete_effect_attempt_failure(
        &self,
        job_id: Uuid,
        node_id: Uuid,
        attempt_id: Uuid,
        receipt: &impl Serialize,
    ) -> Result<()> {
        self.ensure_job_not_quarantined(job_id)?;
        let receipt_json = serde_json::to_string(receipt)?;
        let transaction = self.conn.unchecked_transaction()?;
        let attempt_updated = transaction.execute(
            "UPDATE effect_attempts
             SET status='failed',receipt_json=?1,
                 finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE attempt_id=?2 AND job_id=?3 AND node_id=?4 AND status='prepared'",
            params![
                receipt_json,
                attempt_id.to_string(),
                job_id.to_string(),
                node_id.to_string()
            ],
        )?;
        let node_updated = transaction.execute(
            "UPDATE nodes SET status='failed'
             WHERE id=?1 AND job_id=?2 AND status='running'",
            params![node_id.to_string(), job_id.to_string()],
        )?;
        if attempt_updated != 1 || node_updated != 1 {
            return Err(StoreError::Invariant(format!(
                "stale failed effect attempt {attempt_id} for node {node_id}"
            )));
        }
        let event_payload = serde_json::to_string(&serde_json::json!({
            "attempt_id": attempt_id,
            "receipt": receipt,
        }))?;
        transaction.execute(
            "INSERT INTO events(job_id,node_id,kind,payload_json)
             VALUES (?1,?2,'node_failed',?3)",
            params![job_id.to_string(), node_id.to_string(), event_payload],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn resolve_prepared_effect_attempt(
        &self,
        attempt: &PreparedEffectAttemptRow,
        disposition: PreparedAttemptDisposition,
        reason: &str,
    ) -> Result<()> {
        self.ensure_job_not_quarantined(attempt.job_id)?;
        let status = disposition.as_str();
        let receipt_json = serde_json::to_string(&serde_json::json!({ "reason": reason }))?;
        let transaction = self.conn.unchecked_transaction()?;
        let attempt_updated = transaction.execute(
            "UPDATE effect_attempts
             SET status=?1,receipt_json=?2,
                 finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE attempt_id=?3 AND job_id=?4 AND node_id=?5 AND status='prepared'",
            params![
                status,
                receipt_json,
                attempt.attempt_id.to_string(),
                attempt.job_id.to_string(),
                attempt.node_id.to_string()
            ],
        )?;
        let node_updated = transaction.execute(
            "UPDATE nodes SET status='interrupted'
             WHERE id=?1 AND job_id=?2 AND status='running'",
            params![attempt.node_id.to_string(), attempt.job_id.to_string()],
        )?;
        if attempt_updated != 1 || node_updated != 1 {
            return Err(StoreError::Invariant(format!(
                "stale prepared effect attempt {} for node {}",
                attempt.attempt_id, attempt.node_id
            )));
        }
        let event_payload = serde_json::to_string(&serde_json::json!({
            "attempt_id": attempt.attempt_id,
            "attempt_status": status,
            "reason": reason,
        }))?;
        transaction.execute(
            "INSERT INTO events(job_id,node_id,kind,payload_json)
             VALUES (?1,?2,'node_interrupted',?3)",
            params![
                attempt.job_id.to_string(),
                attempt.node_id.to_string(),
                event_payload
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Compare-and-swap one job projection and append its ledger event in one
    /// transaction.
    pub fn transition_job_with_event(
        &self,
        job_id: Uuid,
        expected: JobStatus,
        next: JobStatus,
        kind: &str,
        payload: &impl Serialize,
    ) -> Result<i64> {
        self.ensure_job_not_quarantined(job_id)?;
        let expected = status_str_job(expected)?;
        let next = status_str_job(next)?;
        let payload_json = serde_json::to_string(payload)?;
        let transaction = self.conn.unchecked_transaction()?;
        let updated = transaction.execute(
            "UPDATE jobs SET status = ?1 WHERE id = ?2 AND status = ?3",
            params![next, job_id.to_string(), expected],
        )?;
        if updated != 1 {
            return Err(StoreError::Invariant(format!(
                "job {job_id} is not {expected}"
            )));
        }
        let terminal_slot = (kind == "job_terminal").then_some(1_i64);
        transaction.execute(
            "INSERT INTO events(job_id, node_id, kind, payload_json, terminal_slot)
             VALUES (?1, NULL, ?2, ?3, ?4)",
            params![job_id.to_string(), kind, payload_json, terminal_slot],
        )?;
        let event_id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(event_id)
    }

    /// Atomically transition a node and its owning job and append one event.
    pub fn transition_node_and_job_with_event(
        &self,
        transition: CoupledStatusTransition,
        kind: &str,
        payload: &impl Serialize,
    ) -> Result<i64> {
        self.ensure_job_not_quarantined(transition.job_id)?;
        let expected_job = status_str_job(transition.expected_job)?;
        let next_job = status_str_job(transition.next_job)?;
        let expected_node = status_str_node(transition.expected_node)?;
        let next_node = status_str_node(transition.next_node)?;
        let payload_json = serde_json::to_string(payload)?;
        let transaction = self.conn.unchecked_transaction()?;
        let node_updated = transaction.execute(
            "UPDATE nodes SET status = ?1
             WHERE id = ?2 AND job_id = ?3 AND status = ?4",
            params![
                next_node,
                transition.node_id.to_string(),
                transition.job_id.to_string(),
                expected_node
            ],
        )?;
        let job_updated = transaction.execute(
            "UPDATE jobs SET status = ?1 WHERE id = ?2 AND status = ?3",
            params![next_job, transition.job_id.to_string(), expected_job],
        )?;
        if node_updated != 1 || job_updated != 1 {
            return Err(StoreError::Invariant(format!(
                "stale coupled transition for job {} node {}",
                transition.job_id, transition.node_id
            )));
        }
        transaction.execute(
            "INSERT INTO events(job_id, node_id, kind, payload_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                transition.job_id.to_string(),
                transition.node_id.to_string(),
                kind,
                payload_json
            ],
        )?;
        let event_id = transaction.last_insert_rowid();
        transaction.commit()?;
        Ok(event_id)
    }

    /// Atomically interrupt every running node in one job, append each node
    /// event, and transition the owning job with its event.
    pub fn interrupt_running_job(&self, job_id: Uuid, reason: &str) -> Result<bool> {
        self.ensure_job_not_quarantined(job_id)?;
        let transaction = self.conn.unchecked_transaction()?;
        let mut stmt = transaction.prepare(
            "SELECT id FROM nodes WHERE job_id = ?1 AND status = 'running' ORDER BY idx",
        )?;
        let rows = stmt.query_map(params![job_id.to_string()], |row| row.get::<_, String>(0))?;
        let mut node_ids = Vec::new();
        for row in rows {
            node_ids.push(row?);
        }
        drop(stmt);
        if node_ids.is_empty() {
            return Ok(false);
        }
        let node_payload = serde_json::to_string(&serde_json::json!({ "reason": reason }))?;
        for node_id in &node_ids {
            let updated = transaction.execute(
                "UPDATE nodes SET status = 'interrupted'
                 WHERE id = ?1 AND job_id = ?2 AND status = 'running'",
                params![node_id, job_id.to_string()],
            )?;
            if updated != 1 {
                return Err(StoreError::Invariant(format!(
                    "running node {node_id} changed during recovery"
                )));
            }
            transaction.execute(
                "INSERT INTO events(job_id, node_id, kind, payload_json)
                 VALUES (?1, ?2, 'node_interrupted', ?3)",
                params![job_id.to_string(), node_id, node_payload],
            )?;
        }
        let job_updated = transaction.execute(
            "UPDATE jobs SET status = 'interrupted'
             WHERE id = ?1 AND status = 'running'",
            params![job_id.to_string()],
        )?;
        if job_updated != 1 {
            return Err(StoreError::Invariant(format!(
                "running job {job_id} changed during recovery"
            )));
        }
        transaction.execute(
            "INSERT INTO events(job_id, node_id, kind, payload_json)
             VALUES (?1, NULL, 'job_interrupted', ?2)",
            params![job_id.to_string(), node_payload],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn bump_steps_executed(&self, id: Uuid) -> Result<u32> {
        self.conn.execute(
            "UPDATE jobs SET steps_executed = steps_executed + 1 WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(self.get_job(id)?.steps_executed)
    }

    pub fn set_consecutive_failures(&self, id: Uuid, value: u32) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE jobs SET consecutive_failures = ?1 WHERE id = ?2",
            params![value, id.to_string()],
        )?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("job {id}")));
        }
        Ok(())
    }

    pub fn insert_approval(&self, id: Uuid, job_id: Uuid, scope: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO approvals(id, job_id, scope) VALUES (?1, ?2, ?3)",
            params![id.to_string(), job_id.to_string(), scope],
        )?;
        Ok(())
    }

    pub fn job_has_approval(&self, job_id: Uuid) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(1) FROM approvals WHERE job_id = ?1",
            params![job_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn insert_action_approval(&self, approval: &NewActionApproval) -> Result<()> {
        self.insert_action_decision(approval, "granted", None)
    }

    pub fn insert_action_denial(&self, approval: &NewActionApproval, reason: &str) -> Result<()> {
        self.insert_action_decision(approval, "denied", Some(reason))
    }

    fn insert_action_decision(
        &self,
        approval: &NewActionApproval,
        decision: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        self.ensure_job_not_quarantined(approval.job_id)?;
        if approval.effect_hash.len() != 64
            || approval.actor.is_empty()
            || approval.expires_unix <= approval.created_unix
            || !matches!(decision, "granted" | "denied")
        {
            return Err(StoreError::Invariant(
                "invalid action approval identity, actor, decision, or expiry".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO action_approvals(
               id,job_id,node_id,effect_hash,actor,decision,created_unix,expires_unix,reason
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                approval.id.to_string(),
                approval.job_id.to_string(),
                approval.node_id.to_string(),
                approval.effect_hash,
                approval.actor,
                decision,
                approval.created_unix,
                approval.expires_unix,
                reason,
            ],
        )?;
        let payload = serde_json::to_string(&serde_json::json!({
            "approval_id": approval.id,
            "node_id": approval.node_id,
            "effect_hash": approval.effect_hash,
            "actor": approval.actor,
            "decision": decision,
            "created_unix": approval.created_unix,
            "expires_unix": approval.expires_unix,
            "reason": reason,
        }))?;
        let event_kind = if decision == "granted" {
            "approval_granted"
        } else {
            "approval_denied"
        };
        transaction.execute(
            "INSERT INTO events(job_id,node_id,kind,payload_json)
             VALUES (?1,?2,?3,?4)",
            params![
                approval.job_id.to_string(),
                approval.node_id.to_string(),
                event_kind,
                payload
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn revoke_action_approval(
        &self,
        job_id: Uuid,
        node_id: Uuid,
        effect_hash: &str,
        actor: &str,
        now_unix: u64,
        reason: &str,
    ) -> Result<bool> {
        self.ensure_job_not_quarantined(job_id)?;
        if actor.is_empty() || reason.is_empty() {
            return Err(StoreError::Invariant(
                "revocation actor and reason are required".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE action_approvals
             SET decision='revoked',revoked_unix=?1,revoked_by=?2,reason=?3
             WHERE job_id=?4 AND node_id=?5 AND effect_hash=?6
               AND decision='granted' AND revoked_unix IS NULL",
            params![
                now_unix,
                actor,
                reason,
                job_id.to_string(),
                node_id.to_string(),
                effect_hash,
            ],
        )?;
        if changed > 0 {
            let payload = serde_json::to_string(&serde_json::json!({
                "node_id": node_id,
                "effect_hash": effect_hash,
                "actor": actor,
                "revoked_unix": now_unix,
                "reason": reason,
                "revoked_count": changed,
            }))?;
            transaction.execute(
                "INSERT INTO events(job_id,node_id,kind,payload_json)
                 VALUES (?1,?2,'approval_revoked',?3)",
                params![job_id.to_string(), node_id.to_string(), payload],
            )?;
        }
        transaction.commit()?;
        Ok(changed > 0)
    }

    pub fn action_has_valid_approval(
        &self,
        job_id: Uuid,
        node_id: Uuid,
        effect_hash: &str,
        now_unix: u64,
    ) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM action_approvals
                WHERE job_id=?1 AND node_id=?2 AND effect_hash=?3
                  AND decision='granted' AND revoked_unix IS NULL
                  AND created_unix<=?4 AND expires_unix>?4
             )",
            params![
                job_id.to_string(),
                node_id.to_string(),
                effect_hash,
                now_unix
            ],
            |row| row.get(0),
        )?)
    }

    pub fn get_job(&self, id: Uuid) -> Result<JobRow> {
        self.conn
            .query_row(
                "SELECT id, label, status, max_steps, steps_executed, consecutive_failures, max_consecutive_failures, command_timeout_ms
                 FROM jobs WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    let status: String = row.get(2)?;
                    Ok(JobRow {
                        id: parse_uuid(row.get::<_, String>(0)?, 0)?,
                        label: row.get(1)?,
                        status: parse_job_status(status)?,
                        max_steps: row.get::<_, i64>(3)? as u32,
                        steps_executed: row.get::<_, i64>(4)? as u32,
                        consecutive_failures: row.get::<_, i64>(5)? as u32,
                        max_consecutive_failures: row.get::<_, i64>(6)? as u32,
                        command_timeout_ms: row.get::<_, i64>(7)? as u32,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound(format!("job {id}")),
                other => StoreError::from(other),
            })
    }

    pub fn list_jobs(&self) -> Result<Vec<JobRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, status, max_steps, steps_executed, consecutive_failures, max_consecutive_failures, command_timeout_ms
             FROM jobs ORDER BY label, id",
        )?;
        let rows = stmt.query_map([], |row| {
            let status: String = row.get(2)?;
            Ok(JobRow {
                id: parse_uuid(row.get::<_, String>(0)?, 0)?,
                label: row.get(1)?,
                status: parse_job_status(status)?,
                max_steps: row.get::<_, i64>(3)? as u32,
                steps_executed: row.get::<_, i64>(4)? as u32,
                consecutive_failures: row.get::<_, i64>(5)? as u32,
                max_consecutive_failures: row.get::<_, i64>(6)? as u32,
                command_timeout_ms: row.get::<_, i64>(7)? as u32,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_quarantined_jobs(&self) -> Result<Vec<JobQuarantineRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT job_id, reason FROM job_quarantine ORDER BY job_id")?;
        let rows = stmt.query_map([], |row| {
            Ok(JobQuarantineRow {
                job_id: parse_uuid(row.get::<_, String>(0)?, 0)?,
                reason: row.get(1)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_prepared_effect_attempts(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<PreparedEffectAttemptRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT attempt_id,job_id,node_id,intent_json
             FROM effect_attempts
             WHERE job_id=?1 AND status='prepared' ORDER BY attempt_no",
        )?;
        let rows = stmt.query_map(params![job_id.to_string()], |row| {
            Ok(PreparedEffectAttemptRow {
                attempt_id: parse_uuid(row.get::<_, String>(0)?, 0)?,
                job_id: parse_uuid(row.get::<_, String>(1)?, 1)?,
                node_id: parse_uuid(row.get::<_, String>(2)?, 2)?,
                intent_json: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn latest_effect_attempt_outcome(
        &self,
        job_id: Uuid,
    ) -> Result<Option<EffectAttemptOutcomeRow>> {
        self.conn
            .query_row(
                "SELECT attempt_id,job_id,node_id,intent_json,status,receipt_json
                 FROM effect_attempts
                 WHERE job_id=?1 AND status<>'prepared'
                 ORDER BY attempt_no DESC LIMIT 1",
                params![job_id.to_string()],
                |row| {
                    Ok(EffectAttemptOutcomeRow {
                        attempt_id: parse_uuid(row.get::<_, String>(0)?, 0)?,
                        job_id: parse_uuid(row.get::<_, String>(1)?, 1)?,
                        node_id: parse_uuid(row.get::<_, String>(2)?, 2)?,
                        intent_json: row.get(3)?,
                        status: row.get(4)?,
                        receipt_json: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn job_has_ambiguous_effect(&self, job_id: Uuid) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM effect_attempts WHERE job_id=?1 AND status='ambiguous'
             )",
            params![job_id.to_string()],
            |row| row.get(0),
        )?)
    }

    pub fn job_cancellation_requested(&self, job_id: Uuid) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM cancellation_requests WHERE job_id=?1)",
            params![job_id.to_string()],
            |row| row.get(0),
        )?)
    }

    pub fn request_job_cancellation(&self, job_id: Uuid, reason: &str) -> Result<JobStatus> {
        self.ensure_job_not_quarantined(job_id)?;
        let transaction = self.conn.unchecked_transaction()?;
        let current = Self::job_status_in_transaction(&transaction, job_id)?;
        if Self::is_terminal_job_status(current) {
            return Ok(current);
        }
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO cancellation_requests(job_id,reason) VALUES (?1,?2)",
            params![job_id.to_string(), reason],
        )?;
        if inserted == 1 {
            let payload = serde_json::to_string(&serde_json::json!({ "reason": reason }))?;
            transaction.execute(
                "INSERT INTO events(job_id,node_id,kind,payload_json)
                 VALUES (?1,NULL,'cancellation_requested',?2)",
                params![job_id.to_string(), payload],
            )?;
        }
        let running: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM nodes WHERE job_id=?1 AND status='running')",
            params![job_id.to_string()],
            |row| row.get(0),
        )?;
        if running {
            transaction.commit()?;
            return Ok(current);
        }
        Self::finalize_job_cancellation_in_transaction(&transaction, job_id, current, reason)?;
        transaction.commit()?;
        Ok(JobStatus::Cancelled)
    }

    pub fn finalize_job_cancellation(&self, job_id: Uuid, reason: &str) -> Result<JobStatus> {
        self.ensure_job_not_quarantined(job_id)?;
        let transaction = self.conn.unchecked_transaction()?;
        let current = Self::job_status_in_transaction(&transaction, job_id)?;
        if Self::is_terminal_job_status(current) {
            return Ok(current);
        }
        let requested: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM cancellation_requests WHERE job_id=?1)",
            params![job_id.to_string()],
            |row| row.get(0),
        )?;
        if !requested {
            return Err(StoreError::Invariant(format!(
                "job {job_id} has no durable cancellation request"
            )));
        }
        Self::finalize_job_cancellation_in_transaction(&transaction, job_id, current, reason)?;
        transaction.commit()?;
        Ok(JobStatus::Cancelled)
    }

    fn job_status_in_transaction(transaction: &Transaction<'_>, job_id: Uuid) -> Result<JobStatus> {
        let status: Option<String> = transaction
            .query_row(
                "SELECT status FROM jobs WHERE id=?1",
                params![job_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let status = status.ok_or_else(|| StoreError::NotFound(format!("job {job_id}")))?;
        serde_json::from_value(serde_json::Value::String(status)).map_err(StoreError::from)
    }

    fn is_terminal_job_status(status: JobStatus) -> bool {
        matches!(
            status,
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
        )
    }

    fn finalize_job_cancellation_in_transaction(
        transaction: &Transaction<'_>,
        job_id: Uuid,
        current: JobStatus,
        reason: &str,
    ) -> Result<()> {
        let unfinished = {
            let mut statement = transaction.prepare(
                "SELECT id FROM nodes
                 WHERE job_id=?1 AND status IN (
                    'pending','running','interrupted','awaiting_approval'
                 ) ORDER BY idx",
            )?;
            let rows = statement.query_map(params![job_id.to_string()], |row| {
                parse_uuid(row.get::<_, String>(0)?, 0)
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let receipt = serde_json::to_string(&serde_json::json!({ "reason": reason }))?;
        transaction.execute(
            "UPDATE effect_attempts
             SET status='cancelled',receipt_json=?1,
                 finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE job_id=?2 AND status='prepared'",
            params![receipt, job_id.to_string()],
        )?;
        for node_id in unfinished {
            let updated = transaction.execute(
                "UPDATE nodes SET status='cancelled'
                 WHERE id=?1 AND job_id=?2 AND status IN (
                    'pending','running','interrupted','awaiting_approval'
                 )",
                params![node_id.to_string(), job_id.to_string()],
            )?;
            if updated != 1 {
                return Err(StoreError::Invariant(format!(
                    "node {node_id} changed during cancellation"
                )));
            }
            let payload = serde_json::to_string(&serde_json::json!({ "reason": reason }))?;
            transaction.execute(
                "INSERT INTO events(job_id,node_id,kind,payload_json)
                 VALUES (?1,?2,'node_cancelled',?3)",
                params![job_id.to_string(), node_id.to_string(), payload],
            )?;
        }
        let expected = status_str_job(current)?;
        let updated = transaction.execute(
            "UPDATE jobs SET status='cancelled' WHERE id=?1 AND status=?2",
            params![job_id.to_string(), expected],
        )?;
        if updated != 1 {
            return Err(StoreError::Invariant(format!(
                "job {job_id} changed during cancellation"
            )));
        }
        let payload = serde_json::to_string(&serde_json::json!({
            "status": JobStatus::Cancelled,
            "reason": reason,
        }))?;
        transaction.execute(
            "INSERT INTO events(job_id,node_id,kind,payload_json,terminal_slot)
             VALUES (?1,NULL,'job_terminal',?2,1)",
            params![job_id.to_string(), payload],
        )?;
        Ok(())
    }

    pub fn list_nodes(&self, job_id: Uuid) -> Result<Vec<NodeRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, job_id, idx, label, status, effect_json FROM nodes WHERE job_id = ?1 ORDER BY idx ASC",
        )?;
        let rows = stmt.query_map(params![job_id.to_string()], |row| {
            let status: String = row.get(4)?;
            Ok(NodeRow {
                id: parse_uuid(row.get::<_, String>(0)?, 0)?,
                job_id: parse_uuid(row.get::<_, String>(1)?, 1)?,
                idx: row.get::<_, i64>(2)? as u32,
                label: row.get(3)?,
                status: parse_node_status(status)?,
                effect_json: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_running_node_job_ids(&self) -> Result<Vec<Uuid>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT job_id FROM nodes WHERE status = 'running' ORDER BY job_id",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            parse_uuid(id, 0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn schema_version(&self) -> Result<String> {
        let v: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or_else(|| "0".into()))
    }
}

fn status_str_job(status: JobStatus) -> Result<&'static str> {
    match status {
        JobStatus::Pending => Ok("pending"),
        JobStatus::Running => Ok("running"),
        JobStatus::Succeeded => Ok("succeeded"),
        JobStatus::Failed => Ok("failed"),
        JobStatus::Cancelled => Ok("cancelled"),
        JobStatus::Interrupted => Ok("interrupted"),
        JobStatus::AwaitingApproval => Ok("awaiting_approval"),
    }
}

fn status_str_node(status: NodeStatus) -> Result<&'static str> {
    match status {
        NodeStatus::Pending => Ok("pending"),
        NodeStatus::Running => Ok("running"),
        NodeStatus::Succeeded => Ok("succeeded"),
        NodeStatus::Failed => Ok("failed"),
        NodeStatus::Interrupted => Ok("interrupted"),
        NodeStatus::Cancelled => Ok("cancelled"),
        NodeStatus::AwaitingApproval => Ok("awaiting_approval"),
    }
}

fn parse_job_status(s: String) -> std::result::Result<JobStatus, rusqlite::Error> {
    serde_json::from_value(serde_json::Value::String(s)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_node_status(s: String) -> std::result::Result<NodeStatus, rusqlite::Error> {
    serde_json::from_value(serde_json::Value::String(s)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_uuid(s: String, idx: usize) -> std::result::Result<Uuid, rusqlite::Error> {
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn opens_and_reports_schema_version() {
        let dir = tempdir().unwrap();
        let store = Store::open(&dir.path().join("t.db")).unwrap();
        assert_eq!(store.schema_version().unwrap(), "7");
    }

    #[test]
    fn cancelled_job_status_round_trips_through_the_typed_model() {
        let encoded = serde_json::to_string(&JobStatus::Cancelled).unwrap();
        assert_eq!(encoded, "\"cancelled\"");
        assert_eq!(
            serde_json::from_str::<JobStatus>(&encoded).unwrap(),
            JobStatus::Cancelled
        );
    }

    #[test]
    fn job_graph_insert_rolls_back_after_late_node_failure() {
        let dir = tempdir().unwrap();
        let store = Store::open(&dir.path().join("t.db")).unwrap();
        let job_id = Uuid::new_v4();
        let result = store.insert_job_graph(NewJobGraph {
            id: job_id,
            label: "atomic".into(),
            status: JobStatus::Pending,
            max_steps: 2,
            max_consecutive_failures: 1,
            command_timeout_ms: 1000,
            event_payload_json: "{}".into(),
            nodes: vec![
                NewNodeGraph {
                    id: Uuid::new_v4(),
                    idx: 0,
                    label: "one".into(),
                    status: NodeStatus::Pending,
                    effect_json: "{}".into(),
                    event_payload_json: "{}".into(),
                },
                NewNodeGraph {
                    id: Uuid::new_v4(),
                    idx: 0,
                    label: "duplicate index".into(),
                    status: NodeStatus::Pending,
                    effect_json: "{}".into(),
                    event_payload_json: "{}".into(),
                },
            ],
        });
        assert!(result.is_err());
        assert!(matches!(
            store.get_job(job_id),
            Err(StoreError::NotFound(_))
        ));
        assert!(store.list_nodes(job_id).unwrap().is_empty());
        assert!(store.list_events(Some(job_id)).unwrap().is_empty());
    }

    #[test]
    fn node_transition_rolls_back_when_event_insert_fails() {
        let dir = tempdir().unwrap();
        let store = Store::open(&dir.path().join("t.db")).unwrap();
        let job_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        store
            .insert_job_graph(NewJobGraph {
                id: job_id,
                label: "atomic transition".into(),
                status: JobStatus::Pending,
                max_steps: 1,
                max_consecutive_failures: 1,
                command_timeout_ms: 1000,
                event_payload_json: "{}".into(),
                nodes: vec![NewNodeGraph {
                    id: node_id,
                    idx: 0,
                    label: "one".into(),
                    status: NodeStatus::Pending,
                    effect_json: "{}".into(),
                    event_payload_json: "{}".into(),
                }],
            })
            .unwrap();
        store
            .conn
            .execute_batch(
                "CREATE TRIGGER fail_node_running
                 BEFORE INSERT ON events
                 WHEN NEW.kind = 'node_running'
                 BEGIN
                     SELECT RAISE(ABORT, 'injected event failure');
                 END;",
            )
            .unwrap();

        let result = store.transition_node_with_event(
            job_id,
            node_id,
            NodeStatus::Pending,
            NodeStatus::Running,
            "node_running",
            &serde_json::json!({}),
        );

        assert!(result.is_err());
        let node = store
            .list_nodes(job_id)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(node.status, NodeStatus::Pending);
        assert_eq!(
            store
                .list_events(Some(job_id))
                .unwrap()
                .iter()
                .filter(|event| event.kind == "node_running")
                .count(),
            0
        );
    }

    #[test]
    fn terminal_event_requires_the_unique_storage_slot() {
        let dir = tempdir().unwrap();
        let store = Store::open(&dir.path().join("t.db")).unwrap();
        let job_id = Uuid::new_v4();
        store
            .insert_job_graph(NewJobGraph {
                id: job_id,
                label: "one terminal".into(),
                status: JobStatus::Pending,
                max_steps: 1,
                max_consecutive_failures: 1,
                command_timeout_ms: 1000,
                event_payload_json: "{}".into(),
                nodes: vec![NewNodeGraph {
                    id: Uuid::new_v4(),
                    idx: 0,
                    label: "one".into(),
                    status: NodeStatus::Pending,
                    effect_json: "{}".into(),
                    event_payload_json: "{}".into(),
                }],
            })
            .unwrap();
        store
            .transition_job_with_event(
                job_id,
                JobStatus::Pending,
                JobStatus::Succeeded,
                "job_terminal",
                &serde_json::json!({ "status": "succeeded" }),
            )
            .unwrap();

        let duplicate = store.append_event(
            Some(job_id),
            None,
            "job_terminal",
            &serde_json::json!({ "status": "succeeded" }),
        );

        assert!(duplicate.is_err());
        assert_eq!(
            store
                .list_events(Some(job_id))
                .unwrap()
                .iter()
                .filter(|event| event.kind == "job_terminal")
                .count(),
            1
        );
    }

    #[test]
    fn legacy_partial_projection_is_quarantined_and_cannot_transition() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let job_id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
                 INSERT INTO meta(key, value) VALUES ('schema_version', '3');
                 CREATE TABLE jobs (
                    id TEXT PRIMARY KEY NOT NULL, label TEXT NOT NULL, status TEXT NOT NULL,
                    max_steps INTEGER NOT NULL DEFAULT 100,
                    steps_executed INTEGER NOT NULL DEFAULT 0,
                    consecutive_failures INTEGER NOT NULL DEFAULT 0,
                    max_consecutive_failures INTEGER NOT NULL DEFAULT 3,
                    command_timeout_ms INTEGER NOT NULL DEFAULT 30000
                 );
                 CREATE TABLE nodes (
                    id TEXT PRIMARY KEY NOT NULL,
                    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                    idx INTEGER NOT NULL, label TEXT NOT NULL, status TEXT NOT NULL,
                    effect_json TEXT NOT NULL, UNIQUE(job_id, idx)
                 );
                 CREATE TABLE events (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT, job_id TEXT, node_id TEXT,
                    kind TEXT NOT NULL, payload_json TEXT NOT NULL, terminal_slot INTEGER,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                 );
                 CREATE TABLE approvals (
                    id TEXT PRIMARY KEY NOT NULL, job_id TEXT NOT NULL, scope TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                 );
                 CREATE TABLE job_quarantine (
                    job_id TEXT PRIMARY KEY NOT NULL, reason TEXT NOT NULL,
                    detected_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                 );",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs(id, label, status) VALUES (?1, 'legacy partial', 'pending')",
                params![job_id.to_string()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO nodes(id, job_id, idx, label, status, effect_json)
                 VALUES (?1, ?2, 0, 'missing event', 'succeeded', '{}')",
                params![node_id.to_string(), job_id.to_string()],
            )
            .unwrap();
        drop(connection);

        let store = Store::open(&path).unwrap();
        let quarantined = store.list_quarantined_jobs().unwrap();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(quarantined[0].job_id, job_id);
        assert!(quarantined[0].reason.contains("node projection"));
        let transition = store.transition_job_with_event(
            job_id,
            JobStatus::Pending,
            JobStatus::Running,
            "job_running",
            &serde_json::json!({}),
        );
        assert!(matches!(transition, Err(StoreError::Invariant(_))));
        assert_eq!(store.get_job(job_id).unwrap().status, JobStatus::Pending);
    }

    #[test]
    fn future_schema_is_rejected_before_store_initialization() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("future.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
                 INSERT INTO meta(key, value) VALUES ('schema_version', '99');",
            )
            .unwrap();
        drop(connection);

        let result = Store::open(&path);

        assert!(matches!(result, Err(StoreError::Invariant(_))));
        let connection = Connection::open(&path).unwrap();
        let tables: Vec<String> = connection
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(tables, vec!["meta"]);
    }
}
