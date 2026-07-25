//! Durable workflow run ledger and bounded DAG scheduler (P10 / ADR-0033).
//!
//! Runs are keyed by UUID, bound to an immutable registered workflow
//! identity/version, and store per-node projections plus a fenced owner lease.
//! Storage enforces exactly one terminal outcome per run. Child agent
//! invocations are linked for parent/child cancel trees.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    KernelError, Result, WorkflowDefinition, WorkflowId, WorkflowTerminalKind, WorkflowVersion,
};

const MAX_INPUT_JSON_BYTES: usize = 256 * 1024;
const DEFAULT_LEASE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    AwaitingApproval,
}

impl WorkflowRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::AwaitingApproval => "awaiting_approval",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            _ => Err(invalid("persisted workflow run status is invalid")),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled
        )
    }

    pub fn to_terminal_kind(self) -> Option<WorkflowTerminalKind> {
        match self {
            Self::Succeeded => Some(WorkflowTerminalKind::Succeeded),
            Self::Failed => Some(WorkflowTerminalKind::Failed),
            Self::Cancelled => Some(WorkflowTerminalKind::Cancelled),
            Self::AwaitingApproval | Self::Pending | Self::Running => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeRunStatus {
    Pending,
    Ready,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Skipped,
}

impl WorkflowNodeRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "skipped" => Ok(Self::Skipped),
            _ => Err(invalid("persisted workflow node status is invalid")),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Skipped
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunLease {
    pub owner: String,
    pub token: Uuid,
    pub generation: u64,
    pub deadline_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRun {
    pub id: Uuid,
    pub workflow_id: String,
    pub workflow_version: String,
    pub status: WorkflowRunStatus,
    pub inputs: serde_json::Value,
    pub cancellation_reason: Option<String>,
    pub lease: Option<WorkflowRunLease>,
    pub created_unix: u64,
    pub completed_unix: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowNodeRun {
    pub run_id: Uuid,
    pub node_id: String,
    pub status: WorkflowNodeRunStatus,
    pub invocation_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub artifact_sha256: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_unix: Option<u64>,
    pub completed_unix: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunChild {
    pub run_id: Uuid,
    pub invocation_id: Uuid,
    pub node_id: String,
    pub job_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunEvent {
    pub seq: i64,
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub kind: String,
    pub created_unix: u64,
}

pub struct WorkflowRunStore {
    conn: Connection,
}

impl WorkflowRunStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS workflow_runs(
               id TEXT PRIMARY KEY,
               workflow_id TEXT NOT NULL,
               workflow_version TEXT NOT NULL,
               status TEXT NOT NULL CHECK(status IN (
                 'pending','running','succeeded','failed','cancelled','awaiting_approval'
               )),
               inputs_json TEXT NOT NULL,
               cancellation_reason TEXT,
               lease_owner TEXT,
               lease_token TEXT,
               lease_generation INTEGER NOT NULL DEFAULT 0,
               lease_deadline_unix INTEGER,
               created_unix INTEGER NOT NULL,
               completed_unix INTEGER
             );
             CREATE TABLE IF NOT EXISTS workflow_run_nodes(
               run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
               node_id TEXT NOT NULL,
               status TEXT NOT NULL CHECK(status IN (
                 'pending','ready','running','succeeded','failed','cancelled','skipped'
               )),
               invocation_id TEXT,
               job_id TEXT,
               artifact_sha256 TEXT CHECK(
                 artifact_sha256 IS NULL OR length(artifact_sha256)=64
               ),
               error_code TEXT,
               error_message TEXT,
               started_unix INTEGER,
               completed_unix INTEGER,
               PRIMARY KEY(run_id, node_id)
             );
             CREATE TABLE IF NOT EXISTS workflow_run_children(
               run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
               invocation_id TEXT NOT NULL,
               node_id TEXT NOT NULL,
               job_id TEXT,
               PRIMARY KEY(run_id, invocation_id)
             );
             CREATE TABLE IF NOT EXISTS workflow_run_events(
               seq INTEGER PRIMARY KEY AUTOINCREMENT,
               event_id TEXT NOT NULL UNIQUE,
               run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
               kind TEXT NOT NULL,
               payload_json TEXT NOT NULL,
               created_unix INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX IF NOT EXISTS one_workflow_run_terminal_event
               ON workflow_run_events(run_id)
               WHERE kind IN ('succeeded','failed','cancelled');",
        )?;
        Ok(Self { conn })
    }

    /// Create a pending run with one projection row per definition node.
    pub fn begin(
        &self,
        definition: &WorkflowDefinition,
        inputs: serde_json::Value,
    ) -> Result<Uuid> {
        definition.validate()?;
        let raw = serde_json::to_vec(&inputs)?;
        if raw.len() > MAX_INPUT_JSON_BYTES {
            return Err(invalid("workflow run inputs exceed bound"));
        }
        if !inputs.is_object() {
            return Err(invalid("workflow run inputs must be a JSON object"));
        }
        let id = Uuid::new_v4();
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO workflow_runs(
               id,workflow_id,workflow_version,status,inputs_json,created_unix
             ) VALUES(?1,?2,?3,'pending',?4,?5)",
            params![
                id.to_string(),
                definition.id.as_str(),
                definition.version.as_str(),
                serde_json::to_string(&inputs)?,
                now as i64
            ],
        )?;
        for node in &definition.nodes {
            tx.execute(
                "INSERT INTO workflow_run_nodes(run_id,node_id,status)
                 VALUES(?1,?2,'pending')",
                params![id.to_string(), node.id],
            )?;
        }
        append_event(
            &tx,
            id,
            "accepted",
            serde_json::json!({
                "workflow_id": definition.id.as_str(),
                "workflow_version": definition.version.as_str(),
            }),
            now,
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn get(&self, run_id: Uuid) -> Result<WorkflowRun> {
        self.conn
            .query_row(
                "SELECT id,workflow_id,workflow_version,status,inputs_json,
                        cancellation_reason,lease_owner,lease_token,lease_generation,
                        lease_deadline_unix,created_unix,completed_unix
                 FROM workflow_runs WHERE id=?1",
                params![run_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, Option<i64>>(11)?,
                    ))
                },
            )
            .optional()?
            .map(
                |(
                    id,
                    workflow_id,
                    workflow_version,
                    status,
                    inputs_json,
                    cancellation_reason,
                    lease_owner,
                    lease_token,
                    lease_generation,
                    lease_deadline_unix,
                    created_unix,
                    completed_unix,
                )|
                 -> Result<WorkflowRun> {
                    let lease = match (lease_owner, lease_token, lease_deadline_unix) {
                        (Some(owner), Some(token), Some(deadline)) => Some(WorkflowRunLease {
                            owner,
                            token: Uuid::parse_str(&token).map_err(|_| {
                                invalid("persisted workflow lease token is invalid")
                            })?,
                            generation: lease_generation as u64,
                            deadline_unix: deadline as u64,
                        }),
                        _ => None,
                    };
                    Ok(WorkflowRun {
                        id: Uuid::parse_str(&id)
                            .map_err(|_| invalid("persisted workflow run id is invalid"))?,
                        workflow_id,
                        workflow_version,
                        status: WorkflowRunStatus::parse(&status)?,
                        inputs: serde_json::from_str(&inputs_json)?,
                        cancellation_reason,
                        lease,
                        created_unix: created_unix as u64,
                        completed_unix: completed_unix.map(|v| v as u64),
                    })
                },
            )
            .transpose()?
            .ok_or_else(|| invalid("workflow run not found"))
    }

    pub fn list_nodes(&self, run_id: Uuid) -> Result<Vec<WorkflowNodeRun>> {
        let mut statement = self.conn.prepare(
            "SELECT run_id,node_id,status,invocation_id,job_id,artifact_sha256,
                    error_code,error_message,started_unix,completed_unix
             FROM workflow_run_nodes WHERE run_id=?1 ORDER BY node_id",
        )?;
        let rows = statement.query_map(params![run_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<i64>>(9)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (
                run_id_s,
                node_id,
                status,
                invocation_id,
                job_id,
                artifact_sha256,
                error_code,
                error_message,
                started_unix,
                completed_unix,
            ) = row?;
            out.push(WorkflowNodeRun {
                run_id: Uuid::parse_str(&run_id_s)
                    .map_err(|_| invalid("persisted workflow run id is invalid"))?,
                node_id,
                status: WorkflowNodeRunStatus::parse(&status)?,
                invocation_id: invocation_id
                    .map(|v| Uuid::parse_str(&v))
                    .transpose()
                    .map_err(|_| invalid("persisted invocation id is invalid"))?,
                job_id: job_id
                    .map(|v| Uuid::parse_str(&v))
                    .transpose()
                    .map_err(|_| invalid("persisted job id is invalid"))?,
                artifact_sha256,
                error_code,
                error_message,
                started_unix: started_unix.map(|v| v as u64),
                completed_unix: completed_unix.map(|v| v as u64),
            });
        }
        Ok(out)
    }

    pub fn list_children(&self, run_id: Uuid) -> Result<Vec<WorkflowRunChild>> {
        let mut statement = self.conn.prepare(
            "SELECT run_id,invocation_id,node_id,job_id
             FROM workflow_run_children WHERE run_id=?1 ORDER BY node_id",
        )?;
        let rows = statement.query_map(params![run_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (run_id_s, invocation_id, node_id, job_id) = row?;
            out.push(WorkflowRunChild {
                run_id: Uuid::parse_str(&run_id_s)
                    .map_err(|_| invalid("persisted workflow run id is invalid"))?,
                invocation_id: Uuid::parse_str(&invocation_id)
                    .map_err(|_| invalid("persisted invocation id is invalid"))?,
                node_id,
                job_id: job_id
                    .map(|v| Uuid::parse_str(&v))
                    .transpose()
                    .map_err(|_| invalid("persisted job id is invalid"))?,
            });
        }
        Ok(out)
    }

    /// Claim exclusive execution lease. Fails if run is terminal or held by a live owner.
    pub fn claim_lease(
        &self,
        run_id: Uuid,
        owner: &str,
        ttl_secs: Option<u64>,
    ) -> Result<WorkflowRunLease> {
        if owner.trim().is_empty() || owner.len() > 256 {
            return Err(invalid("workflow lease owner is empty or too large"));
        }
        let ttl = ttl_secs.unwrap_or(DEFAULT_LEASE_TTL_SECS).min(86_400);
        let now = now_unix();
        let deadline = now.saturating_add(ttl);
        let token = Uuid::new_v4();
        let tx = self.conn.unchecked_transaction()?;
        let (status, generation, existing_deadline, existing_owner) = tx.query_row(
            "SELECT status,lease_generation,lease_deadline_unix,lease_owner
             FROM workflow_runs WHERE id=?1",
            params![run_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )?;
        let status = WorkflowRunStatus::parse(&status)?;
        if status.is_terminal() {
            return Err(invalid("cannot claim lease on terminal workflow run"));
        }
        if let (Some(deadline_existing), Some(owner_existing)) = (existing_deadline, existing_owner)
        {
            if (deadline_existing as u64) > now && owner_existing != owner {
                return Err(invalid("workflow run lease held by another owner"));
            }
        }
        let next_gen = (generation as u64).saturating_add(1);
        tx.execute(
            "UPDATE workflow_runs SET
               status=CASE WHEN status='pending' THEN 'running' ELSE status END,
               lease_owner=?2,
               lease_token=?3,
               lease_generation=?4,
               lease_deadline_unix=?5
             WHERE id=?1",
            params![
                run_id.to_string(),
                owner,
                token.to_string(),
                next_gen as i64,
                deadline as i64
            ],
        )?;
        append_event(
            &tx,
            run_id,
            "running",
            serde_json::json!({"owner": owner, "generation": next_gen}),
            now,
        )?;
        tx.commit()?;
        Ok(WorkflowRunLease {
            owner: owner.into(),
            token,
            generation: next_gen,
            deadline_unix: deadline,
        })
    }

    pub fn renew_lease(&self, run_id: Uuid, lease: &WorkflowRunLease) -> Result<WorkflowRunLease> {
        let now = now_unix();
        let deadline = now.saturating_add(DEFAULT_LEASE_TTL_SECS);
        let changed = self.conn.execute(
            "UPDATE workflow_runs SET lease_deadline_unix=?5
             WHERE id=?1 AND lease_owner=?2 AND lease_token=?3 AND lease_generation=?4
               AND status NOT IN ('succeeded','failed','cancelled')",
            params![
                run_id.to_string(),
                lease.owner,
                lease.token.to_string(),
                lease.generation as i64,
                deadline as i64
            ],
        )?;
        if changed != 1 {
            return Err(invalid("workflow lease renew rejected (stale or terminal)"));
        }
        Ok(WorkflowRunLease {
            owner: lease.owner.clone(),
            token: lease.token,
            generation: lease.generation,
            deadline_unix: deadline,
        })
    }

    fn require_live_lease(&self, tx: &Transaction<'_>, run_id: Uuid, lease: &WorkflowRunLease) -> Result<WorkflowRunStatus> {
        let now = now_unix();
        let (status, owner, token, generation, deadline) = tx.query_row(
            "SELECT status,lease_owner,lease_token,lease_generation,lease_deadline_unix
             FROM workflow_runs WHERE id=?1",
            params![run_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )?;
        let status = WorkflowRunStatus::parse(&status)?;
        if status.is_terminal() {
            return Err(invalid("workflow run already terminal"));
        }
        if owner.as_deref() != Some(lease.owner.as_str())
            || token.as_deref() != Some(&lease.token.to_string())
            || generation as u64 != lease.generation
            || deadline.map(|d| d as u64).unwrap_or(0) <= now
        {
            return Err(invalid("workflow run lease is stale or expired"));
        }
        Ok(status)
    }

    pub fn request_cancellation(&self, run_id: Uuid, reason: &str) -> Result<bool> {
        if reason.trim().is_empty() || reason.len() > 4096 {
            return Err(invalid("cancellation reason is empty or too large"));
        }
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        let status = WorkflowRunStatus::parse(
            &tx.query_row(
                "SELECT status FROM workflow_runs WHERE id=?1",
                params![run_id.to_string()],
                |row| row.get::<_, String>(0),
            )?,
        )?;
        if status.is_terminal() {
            return Ok(false);
        }
        let already = tx
            .query_row(
                "SELECT cancellation_reason FROM workflow_runs WHERE id=?1",
                params![run_id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )?
            .is_some();
        if already {
            return Ok(false);
        }
        tx.execute(
            "UPDATE workflow_runs SET cancellation_reason=?2 WHERE id=?1",
            params![run_id.to_string(), reason],
        )?;
        append_event(
            &tx,
            run_id,
            "cancel_requested",
            serde_json::json!({"reason": reason}),
            now,
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn cancellation_requested(&self, run_id: Uuid) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT cancellation_reason FROM workflow_runs WHERE id=?1",
                params![run_id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Rejects beginning children once the parent run is terminal or cancel-requested.
    pub fn assert_can_begin_child(&self, run_id: Uuid) -> Result<()> {
        let run = self.get(run_id)?;
        if run.status.is_terminal() {
            return Err(invalid(
                "cannot begin child invocation on terminal workflow run",
            ));
        }
        if run.cancellation_reason.is_some() {
            return Err(invalid(
                "cannot begin child invocation on cancelled workflow run",
            ));
        }
        Ok(())
    }

    pub fn link_child(
        &self,
        run_id: Uuid,
        node_id: &str,
        invocation_id: Uuid,
        job_id: Option<Uuid>,
    ) -> Result<()> {
        self.assert_can_begin_child(run_id)?;
        self.conn.execute(
            "INSERT INTO workflow_run_children(run_id,invocation_id,node_id,job_id)
             VALUES(?1,?2,?3,?4)",
            params![
                run_id.to_string(),
                invocation_id.to_string(),
                node_id,
                job_id.map(|id| id.to_string())
            ],
        )?;
        Ok(())
    }

    pub fn mark_node_running(
        &self,
        run_id: Uuid,
        lease: &WorkflowRunLease,
        node_id: &str,
        invocation_id: Uuid,
        job_id: Option<Uuid>,
    ) -> Result<()> {
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        self.require_live_lease(&tx, run_id, lease)?;
        if tx
            .query_row(
                "SELECT cancellation_reason FROM workflow_runs WHERE id=?1",
                params![run_id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )?
            .is_some()
        {
            return Err(invalid("workflow run cancellation requested"));
        }
        let changed = tx.execute(
            "UPDATE workflow_run_nodes SET
               status='running',
               invocation_id=?3,
               job_id=?4,
               started_unix=?5
             WHERE run_id=?1 AND node_id=?2 AND status IN ('pending','ready')",
            params![
                run_id.to_string(),
                node_id,
                invocation_id.to_string(),
                job_id.map(|id| id.to_string()),
                now as i64
            ],
        )?;
        if changed != 1 {
            return Err(invalid("workflow node is not runnable"));
        }
        // keep child link job id in sync if re-linked
        let _ = tx.execute(
            "UPDATE workflow_run_children SET job_id=?3
             WHERE run_id=?1 AND invocation_id=?2",
            params![
                run_id.to_string(),
                invocation_id.to_string(),
                job_id.map(|id| id.to_string())
            ],
        );
        append_event(
            &tx,
            run_id,
            "node_running",
            serde_json::json!({
                "node_id": node_id,
                "invocation_id": invocation_id,
                "job_id": job_id,
            }),
            now,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_node_succeeded(
        &self,
        run_id: Uuid,
        lease: &WorkflowRunLease,
        node_id: &str,
        artifact_sha256: Option<String>,
    ) -> Result<()> {
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        self.require_live_lease(&tx, run_id, lease)?;
        if let Some(ref digest) = artifact_sha256 {
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid("artifact sha256 must be 64 hex chars"));
            }
        }
        let changed = tx.execute(
            "UPDATE workflow_run_nodes SET
               status='succeeded',
               artifact_sha256=?3,
               completed_unix=?4,
               error_code=NULL,
               error_message=NULL
             WHERE run_id=?1 AND node_id=?2 AND status='running'",
            params![
                run_id.to_string(),
                node_id,
                artifact_sha256,
                now as i64
            ],
        )?;
        if changed != 1 {
            return Err(invalid("workflow node success rejected"));
        }
        append_event(
            &tx,
            run_id,
            "node_succeeded",
            serde_json::json!({"node_id": node_id, "artifact_sha256": artifact_sha256}),
            now,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_node_failed(
        &self,
        run_id: Uuid,
        lease: &WorkflowRunLease,
        node_id: &str,
        code: &str,
        message: &str,
    ) -> Result<()> {
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        self.require_live_lease(&tx, run_id, lease)?;
        let changed = tx.execute(
            "UPDATE workflow_run_nodes SET
               status='failed',
               error_code=?3,
               error_message=?4,
               completed_unix=?5
             WHERE run_id=?1 AND node_id=?2 AND status IN ('pending','ready','running')",
            params![
                run_id.to_string(),
                node_id,
                code,
                message,
                now as i64
            ],
        )?;
        if changed != 1 {
            return Err(invalid("workflow node failure rejected"));
        }
        append_event(
            &tx,
            run_id,
            "node_failed",
            serde_json::json!({"node_id": node_id, "code": code, "message": message}),
            now,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_remaining_cancelled(&self, run_id: Uuid, lease: &WorkflowRunLease) -> Result<()> {
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        // Allow cancel settlement even if lease expired when reason is set — still require matching generation if lease provided.
        let _ = self.require_live_lease(&tx, run_id, lease).or_else(|_| {
            // If lease stale but cancel requested, still cancel pending nodes under a soft path.
            let reason: Option<String> = tx.query_row(
                "SELECT cancellation_reason FROM workflow_runs WHERE id=?1",
                params![run_id.to_string()],
                |row| row.get(0),
            )?;
            if reason.is_some() {
                Ok(WorkflowRunStatus::Running)
            } else {
                Err(invalid("cannot cancel nodes without live lease or cancel request"))
            }
        })?;
        tx.execute(
            "UPDATE workflow_run_nodes SET status='cancelled', completed_unix=?2
             WHERE run_id=?1 AND status IN ('pending','ready','running')",
            params![run_id.to_string(), now as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn settle_terminal(
        &self,
        run_id: Uuid,
        lease: &WorkflowRunLease,
        status: WorkflowRunStatus,
        reason: Option<&str>,
    ) -> Result<()> {
        if !matches!(
            status,
            WorkflowRunStatus::Succeeded
                | WorkflowRunStatus::Failed
                | WorkflowRunStatus::Cancelled
        ) {
            return Err(invalid("settle_terminal requires a terminal status"));
        }
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        // Terminal cancel may proceed with cancel request even if lease race lost.
        let lease_ok = self.require_live_lease(&tx, run_id, lease);
        if lease_ok.is_err() {
            if status != WorkflowRunStatus::Cancelled {
                return lease_ok.map(|_| ());
            }
            let existing = WorkflowRunStatus::parse(
                &tx.query_row(
                    "SELECT status FROM workflow_runs WHERE id=?1",
                    params![run_id.to_string()],
                    |row| row.get::<_, String>(0),
                )?,
            )?;
            if existing.is_terminal() {
                return Ok(());
            }
        }
        let kind = status.as_str();
        let changed = tx.execute(
            "UPDATE workflow_runs SET
               status=?2,
               cancellation_reason=COALESCE(cancellation_reason, ?3),
               completed_unix=?4,
               lease_owner=NULL,
               lease_token=NULL,
               lease_deadline_unix=NULL
             WHERE id=?1 AND status NOT IN ('succeeded','failed','cancelled')",
            params![
                run_id.to_string(),
                kind,
                reason,
                now as i64
            ],
        )?;
        if changed != 1 {
            // already terminal — idempotent success for matching terminal
            let existing = WorkflowRunStatus::parse(
                &tx.query_row(
                    "SELECT status FROM workflow_runs WHERE id=?1",
                    params![run_id.to_string()],
                    |row| row.get::<_, String>(0),
                )?,
            )?;
            if existing == status {
                return Ok(());
            }
            return Err(invalid("workflow run terminal already differs"));
        }
        append_event(
            &tx,
            run_id,
            kind,
            serde_json::json!({"reason": reason}),
            now,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn mark_awaiting_approval(
        &self,
        run_id: Uuid,
        lease: &WorkflowRunLease,
        node_id: &str,
    ) -> Result<()> {
        let now = now_unix();
        let tx = self.conn.unchecked_transaction()?;
        self.require_live_lease(&tx, run_id, lease)?;
        tx.execute(
            "UPDATE workflow_runs SET status='awaiting_approval' WHERE id=?1
             AND status IN ('pending','running','awaiting_approval')",
            params![run_id.to_string()],
        )?;
        append_event(
            &tx,
            run_id,
            "awaiting_approval",
            serde_json::json!({"node_id": node_id}),
            now,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Nodes whose dependencies are all succeeded and status is still pending/ready.
    pub fn ready_nodes(
        &self,
        run_id: Uuid,
        definition: &WorkflowDefinition,
    ) -> Result<Vec<String>> {
        let nodes = self.list_nodes(run_id)?;
        let by_id: BTreeMap<_, _> = nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();
        let mut ready = Vec::new();
        for def_node in &definition.nodes {
            let Some(projection) = by_id.get(def_node.id.as_str()) else {
                continue;
            };
            if !matches!(
                projection.status,
                WorkflowNodeRunStatus::Pending | WorkflowNodeRunStatus::Ready
            ) {
                continue;
            }
            let deps_ok = def_node.dependencies.iter().all(|dep| {
                by_id
                    .get(dep.as_str())
                    .is_some_and(|n| n.status == WorkflowNodeRunStatus::Succeeded)
            });
            if deps_ok {
                ready.push(def_node.id.clone());
            }
        }
        Ok(ready)
    }

    pub fn all_nodes_succeeded(&self, run_id: Uuid) -> Result<bool> {
        Ok(self
            .list_nodes(run_id)?
            .into_iter()
            .all(|n| n.status == WorkflowNodeRunStatus::Succeeded))
    }

    pub fn any_node_failed(&self, run_id: Uuid) -> Result<bool> {
        Ok(self
            .list_nodes(run_id)?
            .into_iter()
            .any(|n| n.status == WorkflowNodeRunStatus::Failed))
    }

    pub fn topological_order(definition: &WorkflowDefinition) -> Result<Vec<String>> {
        definition.validate()?;
        let mut indegree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for node in &definition.nodes {
            indegree.insert(node.id.as_str(), node.dependencies.len());
            for dep in &node.dependencies {
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(node.id.as_str());
            }
        }
        let mut ready: Vec<&str> = indegree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut order = Vec::with_capacity(definition.nodes.len());
        while !ready.is_empty() {
            ready.sort_unstable();
            let id = ready.remove(0);
            order.push(id.to_string());
            if let Some(children) = dependents.get(id) {
                for child in children {
                    if let Some(degree) = indegree.get_mut(child) {
                        *degree = degree.saturating_sub(1);
                        if *degree == 0 {
                            ready.push(child);
                        }
                    }
                }
            }
        }
        if order.len() != definition.nodes.len() {
            return Err(invalid("workflow topological order incomplete (cycle)"));
        }
        Ok(order)
    }

    pub fn identity_matches(
        run: &WorkflowRun,
        workflow_id: &WorkflowId,
        version: &WorkflowVersion,
    ) -> bool {
        run.workflow_id == workflow_id.as_str() && run.workflow_version == version.as_str()
    }

    pub fn events(&self, run_id: Uuid) -> Result<Vec<WorkflowRunEvent>> {
        let mut statement = self.conn.prepare(
            "SELECT seq,event_id,run_id,kind,created_unix
             FROM workflow_run_events WHERE run_id=?1 ORDER BY seq",
        )?;
        let rows = statement.query_map(params![run_id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, event_id, run_id_s, kind, created_unix) = row?;
            out.push(WorkflowRunEvent {
                seq,
                event_id: Uuid::parse_str(&event_id)
                    .map_err(|_| invalid("persisted event id is invalid"))?,
                run_id: Uuid::parse_str(&run_id_s)
                    .map_err(|_| invalid("persisted run id is invalid"))?,
                kind,
                created_unix: created_unix as u64,
            });
        }
        Ok(out)
    }

    /// Distinct child invocation ids (for cancel fan-out).
    pub fn child_invocation_ids(&self, run_id: Uuid) -> Result<BTreeSet<Uuid>> {
        Ok(self
            .list_children(run_id)?
            .into_iter()
            .map(|c| c.invocation_id)
            .collect())
    }
}

fn append_event(
    tx: &Transaction<'_>,
    run_id: Uuid,
    kind: &str,
    payload: serde_json::Value,
    now: u64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO workflow_run_events(event_id,run_id,kind,payload_json,created_unix)
         VALUES(?1,?2,?3,?4,?5)",
        params![
            Uuid::new_v4().to_string(),
            run_id.to_string(),
            kind,
            serde_json::to_string(&payload)?,
            now as i64
        ],
    )?;
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn invalid(message: impl Into<String>) -> KernelError {
    KernelError::Tool(message.into())
}


