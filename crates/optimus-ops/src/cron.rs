//! Durable cron schedules stored in SQLite (Work Graph-aligned).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CronError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("cron lease ownership was lost for {job_id}")]
    LeaseLost { job_id: Uuid },
    #[error("cron lease expired for {job_id}")]
    LeaseExpired { job_id: Uuid },
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, CronError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: Uuid,
    pub name: String,
    /// Interval seconds (simple MVP; cron expr later).
    pub every_secs: u64,
    pub prompt: String,
    pub provider: String,
    pub enabled: bool,
    pub next_run_unix: u64,
    pub last_run_unix: Option<u64>,
    pub last_status: Option<String>,
    pub created_at: String,
    pub lease_owner_id: Option<Uuid>,
    pub lease_generation: u64,
    pub lease_deadline_unix: Option<u64>,
}

#[derive(Debug)]
pub struct CronClaim {
    job: CronJob,
    owner_id: Uuid,
    generation: u64,
    lease_token: Uuid,
    attempt_id: Uuid,
    deadline_unix: u64,
}

impl CronClaim {
    pub fn job(&self) -> &CronJob {
        &self.job
    }

    pub fn deadline_unix(&self) -> u64 {
        self.deadline_unix
    }

    pub fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }
}

pub struct CronStore {
    conn: Connection,
}

impl CronStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS cron_jobs (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              every_secs INTEGER NOT NULL,
              prompt TEXT NOT NULL,
              provider TEXT NOT NULL,
              enabled INTEGER NOT NULL,
              next_run_unix INTEGER NOT NULL,
              last_run_unix INTEGER,
              last_status TEXT,
              created_at TEXT NOT NULL,
              lease_owner_id TEXT,
              lease_generation INTEGER NOT NULL DEFAULT 0,
              lease_token TEXT,
              lease_acquired_unix INTEGER,
              lease_heartbeat_unix INTEGER,
              lease_deadline_unix INTEGER
            );
            CREATE TABLE IF NOT EXISTS cron_attempts (
              attempt_id TEXT PRIMARY KEY,
              job_id TEXT NOT NULL REFERENCES cron_jobs(id) ON DELETE CASCADE,
              owner_id TEXT NOT NULL,
              generation INTEGER NOT NULL,
              lease_token TEXT NOT NULL,
              started_unix INTEGER NOT NULL,
              deadline_unix INTEGER NOT NULL,
              status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled','released','expired')),
              completed_unix INTEGER,
              detail TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS one_running_cron_attempt
              ON cron_attempts(job_id) WHERE status='running';
            "#,
        )?;
        ensure_column(&conn, "lease_owner_id", "TEXT")?;
        ensure_column(&conn, "lease_generation", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "lease_token", "TEXT")?;
        ensure_column(&conn, "lease_acquired_unix", "INTEGER")?;
        ensure_column(&conn, "lease_heartbeat_unix", "INTEGER")?;
        ensure_column(&conn, "lease_deadline_unix", "INTEGER")?;
        Ok(Self { conn })
    }

    pub fn add(
        &self,
        name: &str,
        every_secs: u64,
        prompt: &str,
        provider: &str,
    ) -> Result<CronJob> {
        let every_secs = every_secs.max(5);
        let id = Uuid::new_v4();
        let now = now_unix();
        let created = format!("unix:{now}");
        let job = CronJob {
            id,
            name: name.into(),
            every_secs,
            prompt: prompt.into(),
            provider: provider.into(),
            enabled: true,
            next_run_unix: now + every_secs,
            last_run_unix: None,
            last_status: None,
            created_at: created.clone(),
            lease_owner_id: None,
            lease_generation: 0,
            lease_deadline_unix: None,
        };
        self.conn.execute(
            "INSERT INTO cron_jobs(id,name,every_secs,prompt,provider,enabled,next_run_unix,last_run_unix,last_status,created_at)
             VALUES(?1,?2,?3,?4,?5,1,?6,NULL,NULL,?7)",
            params![
                id.to_string(),
                job.name,
                job.every_secs as i64,
                job.prompt,
                job.provider,
                job.next_run_unix as i64,
                created
            ],
        )?;
        Ok(job)
    }

    pub fn list(&self) -> Result<Vec<CronJob>> {
        let mut stmt = self.conn.prepare(
            "SELECT id,name,every_secs,prompt,provider,enabled,next_run_unix,last_run_unix,last_status,created_at,
                    lease_owner_id,lease_generation,lease_deadline_unix
             FROM cron_jobs ORDER BY next_run_unix ASC",
        )?;
        let rows = stmt.query_map([], cron_job_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(CronError::Sqlite)?);
        }
        Ok(out)
    }

    pub fn due(&self, now: u64) -> Result<Vec<CronJob>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|j| {
                j.enabled
                    && j.next_run_unix <= now
                    && j.lease_deadline_unix.is_none_or(|deadline| deadline <= now)
            })
            .collect())
    }

    pub fn claim_due(
        &mut self,
        now: u64,
        owner_id: Uuid,
        lease_secs: u64,
    ) -> Result<Vec<CronClaim>> {
        let deadline = now.saturating_add(lease_secs.max(1));
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids = {
            let mut statement = transaction.prepare(
                "SELECT id FROM cron_jobs
                 WHERE enabled=1 AND next_run_unix<=?1
                   AND (lease_owner_id IS NULL OR lease_deadline_unix<=?1)
                 ORDER BY next_run_unix ASC, id ASC",
            )?;
            let rows = statement.query_map(params![now as i64], |row| row.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut claims = Vec::with_capacity(ids.len());
        for id in ids {
            let token = Uuid::new_v4();
            transaction.execute(
                "UPDATE cron_attempts SET status='expired',completed_unix=?1,detail='lease expired before takeover'
                 WHERE job_id=?2 AND status='running'",
                params![now as i64, id],
            )?;
            let changed = transaction.execute(
                "UPDATE cron_jobs
                 SET lease_owner_id=?1,
                     lease_generation=lease_generation+1,
                     lease_token=?2,
                     lease_acquired_unix=?3,
                     lease_heartbeat_unix=?3,
                     lease_deadline_unix=?4
                 WHERE id=?5 AND enabled=1 AND next_run_unix<=?3
                   AND (lease_owner_id IS NULL OR lease_deadline_unix<=?3)",
                params![
                    owner_id.to_string(),
                    token.to_string(),
                    now as i64,
                    deadline as i64,
                    id
                ],
            )?;
            if changed == 0 {
                continue;
            }
            let job = transaction.query_row(
                "SELECT id,name,every_secs,prompt,provider,enabled,next_run_unix,last_run_unix,last_status,created_at,
                        lease_owner_id,lease_generation,lease_deadline_unix
                 FROM cron_jobs WHERE id=?1",
                params![id],
                cron_job_from_row,
            )?;
            let attempt_id = Uuid::new_v4();
            transaction.execute(
                "INSERT INTO cron_attempts(
                   attempt_id,job_id,owner_id,generation,lease_token,started_unix,deadline_unix,status
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7,'running')",
                params![
                    attempt_id.to_string(),
                    job.id.to_string(),
                    owner_id.to_string(),
                    job.lease_generation as i64,
                    token.to_string(),
                    now as i64,
                    deadline as i64
                ],
            )?;
            claims.push(CronClaim {
                generation: job.lease_generation,
                job,
                owner_id,
                lease_token: token,
                attempt_id,
                deadline_unix: deadline,
            });
        }
        transaction.commit()?;
        Ok(claims)
    }

    pub fn renew_claim(&self, claim: &mut CronClaim, now: u64, lease_secs: u64) -> Result<()> {
        let deadline = now.saturating_add(lease_secs.max(1));
        let changed = self.conn.execute(
            "UPDATE cron_jobs SET lease_heartbeat_unix=?1, lease_deadline_unix=?2
             WHERE id=?3 AND lease_owner_id=?4 AND lease_generation=?5 AND lease_token=?6
               AND lease_deadline_unix>?1",
            params![
                now as i64,
                deadline as i64,
                claim.job.id.to_string(),
                claim.owner_id.to_string(),
                claim.generation as i64,
                claim.lease_token.to_string()
            ],
        )?;
        if changed == 0 {
            return Err(self.lease_error(claim, now)?);
        }
        claim.deadline_unix = deadline;
        claim.job.lease_deadline_unix = Some(deadline);
        Ok(())
    }

    pub fn complete_claim(&self, claim: &CronClaim, status: &str, now: u64) -> Result<()> {
        let transaction = self.conn.unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE cron_jobs
             SET last_run_unix=?1,last_status=?2,
                 next_run_unix=?1+CASE WHEN every_secs<5 THEN 5 ELSE every_secs END,
                 lease_owner_id=NULL,lease_token=NULL,lease_acquired_unix=NULL,
                 lease_heartbeat_unix=NULL,lease_deadline_unix=NULL
             WHERE id=?3 AND lease_owner_id=?4 AND lease_generation=?5 AND lease_token=?6
               AND lease_deadline_unix>?1",
            params![
                now as i64,
                status,
                claim.job.id.to_string(),
                claim.owner_id.to_string(),
                claim.generation as i64,
                claim.lease_token.to_string()
            ],
        )?;
        if changed == 0 {
            return Err(self.lease_error(claim, now)?);
        }
        transaction.execute(
            "UPDATE cron_attempts SET status=?1,completed_unix=?2,detail=?3
             WHERE attempt_id=?4 AND status='running'",
            params![
                if status.starts_with("ok") {
                    "succeeded"
                } else {
                    "failed"
                },
                now as i64,
                status,
                claim.attempt_id.to_string()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn release_claim(&self, claim: &CronClaim, now: u64) -> Result<()> {
        let transaction = self.conn.unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE cron_jobs
             SET lease_owner_id=NULL, lease_token=NULL, lease_acquired_unix=NULL,
                 lease_heartbeat_unix=NULL, lease_deadline_unix=NULL
             WHERE id=?1 AND lease_owner_id=?2 AND lease_generation=?3 AND lease_token=?4
               AND lease_deadline_unix>?5",
            params![
                claim.job.id.to_string(),
                claim.owner_id.to_string(),
                claim.generation as i64,
                claim.lease_token.to_string(),
                now as i64
            ],
        )?;
        if changed == 0 {
            return Err(self.lease_error(claim, now)?);
        }
        transaction.execute(
            "UPDATE cron_attempts SET status='released',completed_unix=?1,detail='claim released'
             WHERE attempt_id=?2 AND status='running'",
            params![now as i64, claim.attempt_id.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn cancel_running(&self, id: Uuid, now: u64) -> Result<bool> {
        let transaction = self.conn.unchecked_transaction()?;
        let attempt_id: Option<String> = transaction
            .query_row(
                "SELECT attempt_id FROM cron_attempts WHERE job_id=?1 AND status='running'",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(attempt_id) = attempt_id else {
            transaction.commit()?;
            return Ok(false);
        };
        transaction.execute(
            "UPDATE cron_attempts SET status='cancelled',completed_unix=?1,detail='operator cancellation'
             WHERE attempt_id=?2 AND status='running'",
            params![now as i64, attempt_id],
        )?;
        transaction.execute(
            "UPDATE cron_jobs
             SET last_run_unix=?1,last_status='cancelled',
                 next_run_unix=?1+CASE WHEN every_secs<5 THEN 5 ELSE every_secs END,
                 lease_generation=lease_generation+1,lease_owner_id=NULL,lease_token=NULL,
                 lease_acquired_unix=NULL,lease_heartbeat_unix=NULL,lease_deadline_unix=NULL
             WHERE id=?2",
            params![now as i64, id.to_string()],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn attempt_status(&self, attempt_id: Uuid) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT status FROM cron_attempts WHERE attempt_id=?1",
                params![attempt_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(CronError::Sqlite)
    }

    fn lease_error(&self, claim: &CronClaim, now: u64) -> Result<CronError> {
        let current = self
            .conn
            .query_row(
                "SELECT lease_owner_id,lease_generation,lease_token,lease_deadline_unix
                 FROM cron_jobs WHERE id=?1",
                params![claim.job.id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()?;
        let exact_but_expired = current.is_some_and(|(owner, generation, token, deadline)| {
            owner.as_deref() == Some(claim.owner_id.to_string().as_str())
                && generation == claim.generation as i64
                && token.as_deref() == Some(claim.lease_token.to_string().as_str())
                && deadline.is_some_and(|value| value <= now as i64)
        });
        if exact_but_expired {
            Ok(CronError::LeaseExpired {
                job_id: claim.job.id,
            })
        } else {
            Ok(CronError::LeaseLost {
                job_id: claim.job.id,
            })
        }
    }

    pub fn set_next_run(&self, id: Uuid, next_run_unix: u64) -> Result<()> {
        self.conn.execute(
            "UPDATE cron_jobs SET next_run_unix=?1 WHERE id=?2",
            params![next_run_unix as i64, id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE cron_jobs
             SET enabled=?1,
                 lease_generation=lease_generation+CASE WHEN ?1=0 THEN 1 ELSE 0 END,
                 lease_owner_id=CASE WHEN ?1=0 THEN NULL ELSE lease_owner_id END,
                 lease_token=CASE WHEN ?1=0 THEN NULL ELSE lease_token END,
                 lease_acquired_unix=CASE WHEN ?1=0 THEN NULL ELSE lease_acquired_unix END,
                 lease_heartbeat_unix=CASE WHEN ?1=0 THEN NULL ELSE lease_heartbeat_unix END,
                 lease_deadline_unix=CASE WHEN ?1=0 THEN NULL ELSE lease_deadline_unix END
             WHERE id=?2",
            params![if enabled { 1 } else { 0 }, id.to_string()],
        )?;
        Ok(n > 0)
    }

    pub fn remove(&self, id: Uuid) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM cron_jobs WHERE id=?1", params![id.to_string()])?;
        Ok(n > 0)
    }

    /// Per-schedule attempt history (newest first), program P25.
    pub fn history(&self, job_id: Uuid, limit: usize) -> Result<Vec<CronAttemptView>> {
        let limit = limit.clamp(1, 100) as i64;
        let mut stmt = self.conn.prepare(
            "SELECT attempt_id,job_id,status,started_unix,completed_unix,detail
             FROM cron_attempts WHERE job_id=?1
             ORDER BY started_unix DESC, attempt_id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![job_id.to_string(), limit], |row| {
            Ok(CronAttemptView {
                attempt_id: parse_uuid(row.get(0)?)?,
                job_id: parse_uuid(row.get(1)?)?,
                status: row.get(2)?,
                started_unix: checked_u64(row.get(3)?)?,
                completed_unix: row.get::<_, Option<i64>>(4)?.map(checked_u64).transpose()?,
                detail: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(CronError::Sqlite)?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CronAttemptView {
    pub attempt_id: Uuid,
    pub job_id: Uuid,
    pub status: String,
    pub started_unix: u64,
    pub completed_unix: Option<u64>,
    pub detail: Option<String>,
}

fn ensure_column(connection: &Connection, name: &str, definition: &str) -> Result<()> {
    let columns = {
        let mut statement = connection.prepare("PRAGMA table_info(cron_jobs)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    if !columns.iter().any(|column| column == name) {
        connection.execute_batch(&format!(
            "ALTER TABLE cron_jobs ADD COLUMN {name} {definition};"
        ))?;
    }
    Ok(())
}

fn parse_uuid(value: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn checked_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn cron_job_from_row(row: &Row<'_>) -> rusqlite::Result<CronJob> {
    Ok(CronJob {
        id: parse_uuid(row.get(0)?)?,
        name: row.get(1)?,
        every_secs: checked_u64(row.get(2)?)?,
        prompt: row.get(3)?,
        provider: row.get(4)?,
        enabled: row.get::<_, i64>(5)? != 0,
        next_run_unix: checked_u64(row.get(6)?)?,
        last_run_unix: row.get::<_, Option<i64>>(7)?.map(checked_u64).transpose()?,
        last_status: row.get(8)?,
        created_at: row.get(9)?,
        lease_owner_id: row
            .get::<_, Option<String>>(10)?
            .map(parse_uuid)
            .transpose()?,
        lease_generation: checked_u64(row.get(11)?)?,
        lease_deadline_unix: row
            .get::<_, Option<i64>>(12)?
            .map(checked_u64)
            .transpose()?,
    })
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn add_list_due_mark() {
        let d = tempdir().unwrap();
        let mut s = CronStore::open(d.path().join("cron.db")).unwrap();
        let j = s.add("ping", 5, "say hi", "offline").unwrap();
        assert_eq!(s.list().unwrap().len(), 1);
        s.set_next_run(j.id, 0).unwrap();
        let due = s.due(now_unix()).unwrap();
        assert_eq!(due.len(), 1);
        let claim = s
            .claim_due(now_unix(), Uuid::new_v4(), 30)
            .unwrap()
            .pop()
            .unwrap();
        let attempt_id = claim.attempt_id();
        s.complete_claim(&claim, "ok", now_unix()).unwrap();
        assert_eq!(
            s.attempt_status(attempt_id).unwrap().as_deref(),
            Some("succeeded")
        );
        assert!(s.due(now_unix()).unwrap().is_empty());
        let hist = s.history(j.id, 10).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].status, "succeeded");
    }

    #[test]
    fn pause_resume_and_remove() {
        let d = tempdir().unwrap();
        let s = CronStore::open(d.path().join("cron.db")).unwrap();
        let j = s.add("job", 60, "p", "offline").unwrap();
        s.set_enabled(j.id, false).unwrap();
        assert!(!s.list().unwrap()[0].enabled);
        s.set_enabled(j.id, true).unwrap();
        assert!(s.list().unwrap()[0].enabled);
        assert!(s.remove(j.id).unwrap());
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn cancellation_terminalizes_attempt_and_fences_stale_owner() {
        let d = tempdir().unwrap();
        let mut store = CronStore::open(d.path().join("cron.db")).unwrap();
        let job = store.add("cancel", 5, "work", "offline").unwrap();
        store.set_next_run(job.id, 10).unwrap();
        let claim = store
            .claim_due(10, Uuid::new_v4(), 30)
            .unwrap()
            .pop()
            .unwrap();
        let attempt_id = claim.attempt_id();

        assert!(store.cancel_running(job.id, 11).unwrap());
        assert_eq!(
            store.attempt_status(attempt_id).unwrap().as_deref(),
            Some("cancelled")
        );
        assert!(matches!(
            store.complete_claim(&claim, "ok", 12),
            Err(CronError::LeaseLost { job_id }) if job_id == job.id
        ));
        let projected = store.list().unwrap().pop().unwrap();
        assert_eq!(projected.last_status.as_deref(), Some("cancelled"));
        assert_eq!(projected.next_run_unix, 16);
    }

    #[test]
    fn release_terminalizes_attempt_without_advancing_schedule() {
        let d = tempdir().unwrap();
        let mut store = CronStore::open(d.path().join("cron.db")).unwrap();
        let job = store.add("release", 5, "work", "offline").unwrap();
        store.set_next_run(job.id, 10).unwrap();
        let claim = store
            .claim_due(10, Uuid::new_v4(), 30)
            .unwrap()
            .pop()
            .unwrap();
        let attempt_id = claim.attempt_id();
        store.release_claim(&claim, 11).unwrap();
        assert_eq!(
            store.attempt_status(attempt_id).unwrap().as_deref(),
            Some("released")
        );
        assert_eq!(store.list().unwrap().pop().unwrap().next_run_unix, 10);
    }

    #[test]
    fn concurrent_stores_cannot_claim_same_due_job() {
        let d = tempdir().unwrap();
        let path = d.path().join("cron.db");
        let mut first = CronStore::open(&path).unwrap();
        let mut second = CronStore::open(&path).unwrap();
        let job = first.add("once", 5, "work", "offline").unwrap();
        first.set_next_run(job.id, 10).unwrap();

        let first_claims = first.claim_due(10, Uuid::new_v4(), 30).unwrap();
        let second_claims = second.claim_due(10, Uuid::new_v4(), 30).unwrap();

        assert_eq!(first_claims.len(), 1);
        assert!(second_claims.is_empty());
    }

    #[test]
    fn expired_takeover_fences_stale_completion() {
        let d = tempdir().unwrap();
        let path = d.path().join("cron.db");
        let mut first = CronStore::open(&path).unwrap();
        let mut second = CronStore::open(&path).unwrap();
        let job = first.add("takeover", 5, "work", "offline").unwrap();
        first.set_next_run(job.id, 10).unwrap();
        let stale = first
            .claim_due(10, Uuid::new_v4(), 5)
            .unwrap()
            .pop()
            .unwrap();
        let live = second
            .claim_due(16, Uuid::new_v4(), 30)
            .unwrap()
            .pop()
            .unwrap();

        assert!(matches!(
            first.complete_claim(&stale, "stale", 16),
            Err(CronError::LeaseLost { job_id }) if job_id == job.id
        ));
        second.complete_claim(&live, "ok", 16).unwrap();
        let projected = second.list().unwrap().pop().unwrap();
        assert_eq!(projected.last_status.as_deref(), Some("ok"));
        assert_eq!(projected.last_run_unix, Some(16));
        assert_eq!(projected.next_run_unix, 21);
        assert!(projected.lease_owner_id.is_none());
    }

    #[test]
    fn disabling_job_fences_live_claim() {
        let d = tempdir().unwrap();
        let mut store = CronStore::open(d.path().join("cron.db")).unwrap();
        let job = store.add("disable", 5, "work", "offline").unwrap();
        store.set_next_run(job.id, 10).unwrap();
        let claim = store
            .claim_due(10, Uuid::new_v4(), 30)
            .unwrap()
            .pop()
            .unwrap();

        store.set_enabled(job.id, false).unwrap();

        assert!(matches!(
            store.complete_claim(&claim, "stale", 11),
            Err(CronError::LeaseLost { job_id }) if job_id == job.id
        ));
        assert!(!store.list().unwrap().pop().unwrap().enabled);
    }

    #[test]
    fn legacy_schema_migrates_without_losing_jobs() {
        let d = tempdir().unwrap();
        let path = d.path().join("cron.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE cron_jobs (
                   id TEXT PRIMARY KEY, name TEXT NOT NULL, every_secs INTEGER NOT NULL,
                   prompt TEXT NOT NULL, provider TEXT NOT NULL, enabled INTEGER NOT NULL,
                   next_run_unix INTEGER NOT NULL, last_run_unix INTEGER,
                   last_status TEXT, created_at TEXT NOT NULL
                 );",
            )
            .unwrap();
        let id = Uuid::new_v4();
        connection
            .execute(
                "INSERT INTO cron_jobs VALUES(?1,'legacy',5,'work','offline',1,10,NULL,NULL,'unix:1')",
                params![id.to_string()],
            )
            .unwrap();
        drop(connection);

        let mut store = CronStore::open(&path).unwrap();
        let projected = store.list().unwrap().pop().unwrap();
        assert_eq!(projected.id, id);
        assert_eq!(projected.lease_generation, 0);
        assert_eq!(store.claim_due(10, Uuid::new_v4(), 30).unwrap().len(), 1);
    }
}
