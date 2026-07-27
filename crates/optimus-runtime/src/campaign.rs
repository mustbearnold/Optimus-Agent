//! Multi-agent campaign orchestration on the Work Graph.
//!
//! A campaign is an ordered, durable plan of agent steps. Each step maps to a
//! Work Graph job so crash-resume and SmartDeny apply uniformly. Progress is
//! stored in SQLite under the Optimus home — process death does not lose the plan.

use std::path::{Path, PathBuf};

use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{job_id, Effect, JobSpec, JobStatus, NodeSpec, Runtime, RuntimeConfig, RuntimeError};

#[derive(Debug, Error)]
pub enum CampaignError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("runtime: {0}")]
    Runtime(#[from] RuntimeError),
    #[error("work graph store: {0}")]
    Store(#[from] optimus_graph::StoreError),
    #[error("corrupt persisted campaign field {field}: {detail}")]
    Corrupt { field: &'static str, detail: String },
    #[error("campaign {campaign_id} is leased until {deadline_unix}")]
    LeaseHeld {
        campaign_id: Uuid,
        deadline_unix: u64,
    },
    #[error("campaign lease ownership was lost for {campaign_id}")]
    LeaseLost { campaign_id: Uuid },
    #[error("campaign lease expired for {campaign_id}")]
    LeaseExpired { campaign_id: Uuid },
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, CampaignError>;

pub const CAMPAIGN_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    AwaitingApproval,
}

impl CampaignStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::AwaitingApproval => "awaiting_approval",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            value => Err(corrupt(
                "campaign.status",
                format!("unknown status {value:?}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    AwaitingApproval,
    Skipped,
}

impl StepStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Skipped => "skipped",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "skipped" => Ok(Self::Skipped),
            value => Err(corrupt(
                "campaign_step.status",
                format!("unknown status {value:?}"),
            )),
        }
    }
}

fn corrupt(field: &'static str, detail: impl Into<String>) -> CampaignError {
    CampaignError::Corrupt {
        field,
        detail: detail.into(),
    }
}

fn parse_uuid(field: &'static str, value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|error| corrupt(field, error.to_string()))
}

fn parse_optional_uuid(field: &'static str, value: Option<String>) -> Result<Option<Uuid>> {
    value.map(|value| parse_uuid(field, &value)).transpose()
}

fn parse_u64(field: &'static str, value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| corrupt(field, format!("negative value {value}")))
}

fn parse_u32(field: &'static str, value: i64) -> Result<u32> {
    u32::try_from(value).map_err(|_| corrupt(field, format!("out-of-range value {value}")))
}

/// What an agent step does. Extensible without breaking stored rows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepKind {
    /// Durable WriteFile effector (SmartDeny high-risk; may await approval).
    WriteFile {
        relative_path: String,
        contents: String,
    },
    /// SmartDeny-gated RunCommand (high-risk).
    RunCommand { program: String, args: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignStepSpec {
    pub label: String,
    pub kind: StepKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    pub id: Uuid,
    pub name: String,
    pub status: CampaignStatus,
    pub created_unix: u64,
    pub updated_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignStep {
    pub id: Uuid,
    pub campaign_id: Uuid,
    pub idx: u32,
    pub label: String,
    pub kind: StepKind,
    pub status: StepStatus,
    pub job_id: Option<Uuid>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignView {
    pub campaign: Campaign,
    pub steps: Vec<CampaignStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignDiagnostic {
    pub campaign_id: Option<Uuid>,
    pub step_id: Option<Uuid>,
    pub field: String,
    pub detail: String,
    pub repairable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignHealthReport {
    pub schema_version: u32,
    pub diagnostics: Vec<CampaignDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignRepairReport {
    pub repaired: u32,
    pub remaining: Vec<CampaignDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignLeaseView {
    pub campaign_id: Uuid,
    pub owner_id: Uuid,
    pub generation: u64,
    pub acquired_unix: u64,
    pub heartbeat_unix: u64,
    pub deadline_unix: u64,
}

#[derive(Debug)]
pub struct CampaignLeaseCapability {
    campaign_id: Uuid,
    owner_id: Uuid,
    generation: u64,
    lease_token: Uuid,
    deadline_unix: u64,
}

impl CampaignLeaseCapability {
    pub fn owner_id(&self) -> Uuid {
        self.owner_id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn deadline_unix(&self) -> u64 {
        self.deadline_unix
    }
}

pub struct CampaignStore {
    conn: Connection,
    home: PathBuf,
    owner_id: Uuid,
}

#[derive(Debug)]
struct LegacyCampaignRow {
    id: String,
    name: String,
    status: String,
    created_unix: i64,
    updated_unix: i64,
    step_count: i64,
}

#[derive(Debug)]
struct LegacyStepRow {
    id: String,
    campaign_id: String,
    idx: i64,
    label: String,
    kind_json: String,
    status: String,
    job_id: Option<String>,
    detail: String,
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1
         )",
        params![table],
        |row| row.get(0),
    )?)
}

fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for current in columns {
        if current? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn advertised_campaign_version(connection: &Connection) -> Result<Option<u32>> {
    if !table_exists(connection, "campaign_meta")? {
        return Ok(None);
    }
    let raw: Option<String> = connection
        .query_row(
            "SELECT value FROM campaign_meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let raw =
        raw.ok_or_else(|| corrupt("campaign_meta.schema_version", "missing schema version"))?;
    let version = raw
        .parse::<u32>()
        .map_err(|error| corrupt("campaign_meta.schema_version", error.to_string()))?;
    Ok(Some(version))
}

fn set_campaign_version(transaction: &Transaction<'_>, version: u32) -> Result<()> {
    transaction.execute(
        "INSERT INTO campaign_meta(key,value) VALUES ('schema_version',?1)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![version.to_string()],
    )?;
    Ok(())
}

fn migrate_campaign_schema(connection: &mut Connection) -> Result<()> {
    let advertised_version = advertised_campaign_version(connection)?;
    let inferred_version = advertised_version.is_none() && table_exists(connection, "campaigns")?;
    let mut version = match advertised_version {
        Some(version) => version,
        None if table_exists(connection, "campaigns")? => {
            if column_exists(connection, "campaigns", "step_count")? {
                2
            } else {
                1
            }
        }
        None => 0,
    };
    if version > CAMPAIGN_SCHEMA_VERSION {
        return Err(CampaignError::Msg(format!(
            "unsupported campaign schema {version}; maximum is {CAMPAIGN_SCHEMA_VERSION}"
        )));
    }
    if inferred_version {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TABLE campaign_meta (
               key TEXT PRIMARY KEY NOT NULL,
               value TEXT NOT NULL
             );",
        )?;
        set_campaign_version(&transaction, version)?;
        transaction.commit()?;
    }

    while version < CAMPAIGN_SCHEMA_VERSION {
        let transaction = connection.transaction()?;
        match version {
            0 => {
                transaction.execute_batch(
                    "CREATE TABLE campaign_meta (
                       key TEXT PRIMARY KEY NOT NULL,
                       value TEXT NOT NULL
                     );
                     CREATE TABLE campaigns (
                       id TEXT PRIMARY KEY NOT NULL,
                       name TEXT NOT NULL,
                       status TEXT NOT NULL,
                       created_unix INTEGER NOT NULL,
                       updated_unix INTEGER NOT NULL
                     );
                     CREATE TABLE campaign_steps (
                       id TEXT PRIMARY KEY NOT NULL,
                       campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
                       idx INTEGER NOT NULL,
                       label TEXT NOT NULL,
                       kind_json TEXT NOT NULL,
                       status TEXT NOT NULL,
                       job_id TEXT,
                       detail TEXT NOT NULL DEFAULT '',
                       UNIQUE(campaign_id, idx)
                     );",
                )?;
                set_campaign_version(&transaction, 1)?;
                version = 1;
            }
            1 => {
                if !column_exists(&transaction, "campaigns", "step_count")? {
                    transaction
                        .execute("ALTER TABLE campaigns ADD COLUMN step_count INTEGER", [])?;
                }
                transaction.execute(
                    "UPDATE campaigns
                     SET step_count=(SELECT COUNT(*) FROM campaign_steps
                                     WHERE campaign_steps.campaign_id=campaigns.id)
                     WHERE step_count IS NULL",
                    [],
                )?;
                set_campaign_version(&transaction, 2)?;
                version = 2;
            }
            2 => {
                transaction.execute_batch(
                    "CREATE INDEX IF NOT EXISTS campaign_steps_job_id
                     ON campaign_steps(job_id);",
                )?;
                set_campaign_version(&transaction, 3)?;
                version = 3;
            }
            3 => {
                transaction.execute_batch(
                    "CREATE TABLE campaign_lease_attempts (
                       campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
                       generation INTEGER NOT NULL CHECK(generation >= 1),
                       owner_id TEXT NOT NULL,
                       lease_token TEXT NOT NULL UNIQUE,
                       status TEXT NOT NULL CHECK(status IN ('active','released','expired')),
                       acquired_unix INTEGER NOT NULL CHECK(acquired_unix >= 0),
                       heartbeat_unix INTEGER NOT NULL CHECK(heartbeat_unix >= acquired_unix),
                       deadline_unix INTEGER NOT NULL CHECK(deadline_unix > heartbeat_unix),
                       finished_unix INTEGER,
                       PRIMARY KEY(campaign_id, generation),
                       CHECK((status = 'active' AND finished_unix IS NULL)
                          OR (status != 'active' AND finished_unix IS NOT NULL))
                     );
                     CREATE TABLE campaign_leases (
                       campaign_id TEXT PRIMARY KEY NOT NULL
                         REFERENCES campaigns(id) ON DELETE CASCADE,
                       generation INTEGER NOT NULL CHECK(generation >= 1),
                       owner_id TEXT NOT NULL,
                       lease_token TEXT NOT NULL UNIQUE,
                       acquired_unix INTEGER NOT NULL CHECK(acquired_unix >= 0),
                       heartbeat_unix INTEGER NOT NULL CHECK(heartbeat_unix >= acquired_unix),
                       deadline_unix INTEGER NOT NULL CHECK(deadline_unix > heartbeat_unix),
                       FOREIGN KEY(campaign_id, generation)
                         REFERENCES campaign_lease_attempts(campaign_id, generation)
                     );",
                )?;
                set_campaign_version(&transaction, 4)?;
                version = 4;
            }
            _ => unreachable!("version checked above"),
        }
        transaction.commit()?;
    }
    Ok(())
}

fn import_legacy_campaigns(connection: &mut Connection, path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let imported: Option<String> = connection
        .query_row(
            "SELECT value FROM campaign_meta WHERE key='legacy_campaigns_imported'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if imported.as_deref() == Some("1") {
        return Ok(());
    }

    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if !table_exists(&source, "campaigns")? || !table_exists(&source, "campaign_steps")? {
        return Err(corrupt(
            "legacy_campaigns",
            "legacy database is missing campaign tables",
        ));
    }
    let has_step_count = column_exists(&source, "campaigns", "step_count")?;
    let campaign_sql = if has_step_count {
        "SELECT id,name,status,created_unix,updated_unix,step_count FROM campaigns"
    } else {
        "SELECT id,name,status,created_unix,updated_unix,
                (SELECT COUNT(*) FROM campaign_steps
                 WHERE campaign_steps.campaign_id=campaigns.id)
         FROM campaigns"
    };
    let campaigns = {
        let mut statement = source.prepare(campaign_sql)?;
        let rows = statement.query_map([], |row| {
            Ok(LegacyCampaignRow {
                id: row.get(0)?,
                name: row.get(1)?,
                status: row.get(2)?,
                created_unix: row.get(3)?,
                updated_unix: row.get(4)?,
                step_count: row.get(5)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let steps = {
        let mut statement = source.prepare(
            "SELECT id,campaign_id,idx,label,kind_json,status,job_id,detail
             FROM campaign_steps ORDER BY campaign_id,idx",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(LegacyStepRow {
                id: row.get(0)?,
                campaign_id: row.get(1)?,
                idx: row.get(2)?,
                label: row.get(3)?,
                kind_json: row.get(4)?,
                status: row.get(5)?,
                job_id: row.get(6)?,
                detail: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };

    let transaction = connection.transaction()?;
    for campaign in campaigns {
        transaction.execute(
            "INSERT INTO campaigns(id,name,status,created_unix,updated_unix,step_count)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                campaign.id,
                campaign.name,
                campaign.status,
                campaign.created_unix,
                campaign.updated_unix,
                campaign.step_count
            ],
        )?;
    }
    for step in steps {
        transaction.execute(
            "INSERT INTO campaign_steps(
               id,campaign_id,idx,label,kind_json,status,job_id,detail
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                step.id,
                step.campaign_id,
                step.idx,
                step.label,
                step.kind_json,
                step.status,
                step.job_id,
                step.detail
            ],
        )?;
    }
    transaction.execute(
        "INSERT INTO campaign_meta(key,value) VALUES ('legacy_campaigns_imported','1')",
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

impl CampaignStore {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let home = home.as_ref().to_path_buf();
        std::fs::create_dir_all(&home)?;
        let unified_db = home.join("optimus.db");
        if unified_db.exists() {
            let preflight = Connection::open(&unified_db)?;
            if let Some(version) = advertised_campaign_version(&preflight)? {
                if version > CAMPAIGN_SCHEMA_VERSION {
                    return Err(CampaignError::Msg(format!(
                        "unsupported campaign schema {version}; maximum is {CAMPAIGN_SCHEMA_VERSION}"
                    )));
                }
            }
        }
        drop(optimus_graph::Store::open(&unified_db)?);
        let mut conn = Connection::open(&unified_db)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        migrate_campaign_schema(&mut conn)?;
        import_legacy_campaigns(&mut conn, &home.join("campaigns.db"))?;
        Ok(Self {
            conn,
            home,
            owner_id: Uuid::new_v4(),
        })
    }

    pub fn schema_version(&self) -> Result<u32> {
        advertised_campaign_version(&self.conn)?.ok_or_else(|| {
            corrupt(
                "campaign_meta.schema_version",
                "campaign schema is not initialized",
            )
        })
    }

    pub fn lease(&self, campaign_id: Uuid) -> Result<Option<CampaignLeaseView>> {
        let raw: Option<(String, String, i64, i64, i64, i64)> = self
            .conn
            .query_row(
                "SELECT campaign_id,owner_id,generation,acquired_unix,
                        heartbeat_unix,deadline_unix
                 FROM campaign_leases WHERE campaign_id=?1",
                params![campaign_id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?;
        raw.map(
            |(campaign_id, owner_id, generation, acquired, heartbeat, deadline)| {
                Ok(CampaignLeaseView {
                    campaign_id: parse_uuid("campaign_lease.campaign_id", &campaign_id)?,
                    owner_id: parse_uuid("campaign_lease.owner_id", &owner_id)?,
                    generation: parse_u64("campaign_lease.generation", generation)?,
                    acquired_unix: parse_u64("campaign_lease.acquired_unix", acquired)?,
                    heartbeat_unix: parse_u64("campaign_lease.heartbeat_unix", heartbeat)?,
                    deadline_unix: parse_u64("campaign_lease.deadline_unix", deadline)?,
                })
            },
        )
        .transpose()
    }

    pub fn claim(&self, campaign_id: Uuid, lease_seconds: u64) -> Result<CampaignLeaseCapability> {
        if lease_seconds == 0 {
            return Err(CampaignError::Msg(
                "campaign lease duration must be positive".into(),
            ));
        }
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let now = Self::now();
        let deadline = now
            .checked_add(lease_seconds)
            .ok_or_else(|| CampaignError::Msg("campaign lease deadline overflow".into()))?;
        let campaign_status: Option<String> = transaction
            .query_row(
                "SELECT status FROM campaigns WHERE id=?1",
                params![campaign_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let campaign_status = campaign_status
            .ok_or_else(|| CampaignError::Msg(format!("campaign {campaign_id} not found")))?;
        if matches!(
            CampaignStatus::parse(&campaign_status)?,
            CampaignStatus::Succeeded | CampaignStatus::Failed | CampaignStatus::Cancelled
        ) {
            return Err(CampaignError::Msg(format!(
                "terminal campaign {campaign_id} cannot be leased"
            )));
        }
        let current: Option<(String, String, i64, i64)> = transaction
            .query_row(
                "SELECT owner_id,lease_token,generation,deadline_unix
                 FROM campaign_leases WHERE campaign_id=?1",
                params![campaign_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        if let Some((owner, token, generation, current_deadline)) = current {
            let current_deadline = parse_u64("campaign_lease.deadline_unix", current_deadline)?;
            if current_deadline > now {
                return Err(CampaignError::LeaseHeld {
                    campaign_id,
                    deadline_unix: current_deadline,
                });
            }
            let expired = transaction.execute(
                "UPDATE campaign_lease_attempts
                 SET status='expired',finished_unix=?1
                 WHERE campaign_id=?2 AND generation=?3 AND owner_id=?4
                   AND lease_token=?5 AND status='active' AND deadline_unix=?6",
                params![
                    now,
                    campaign_id.to_string(),
                    generation,
                    owner,
                    token,
                    current_deadline
                ],
            )?;
            let removed = transaction.execute(
                "DELETE FROM campaign_leases
                 WHERE campaign_id=?1 AND generation=?2 AND owner_id=?3
                   AND lease_token=?4 AND deadline_unix=?5",
                params![
                    campaign_id.to_string(),
                    generation,
                    owner,
                    token,
                    current_deadline
                ],
            )?;
            if expired != 1 || removed != 1 {
                return Err(CampaignError::LeaseLost { campaign_id });
            }
        }
        let previous_generation: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(generation),0) FROM campaign_lease_attempts
             WHERE campaign_id=?1",
            params![campaign_id.to_string()],
            |row| row.get(0),
        )?;
        let generation = parse_u64("campaign_lease.generation", previous_generation)?
            .checked_add(1)
            .ok_or_else(|| CampaignError::Msg("campaign lease generation overflow".into()))?;
        let lease_token = Uuid::new_v4();
        transaction.execute(
            "INSERT INTO campaign_lease_attempts(
               campaign_id,generation,owner_id,lease_token,status,
               acquired_unix,heartbeat_unix,deadline_unix,finished_unix
             ) VALUES (?1,?2,?3,?4,'active',?5,?5,?6,NULL)",
            params![
                campaign_id.to_string(),
                generation,
                self.owner_id.to_string(),
                lease_token.to_string(),
                now,
                deadline
            ],
        )?;
        transaction.execute(
            "INSERT INTO campaign_leases(
               campaign_id,generation,owner_id,lease_token,
               acquired_unix,heartbeat_unix,deadline_unix
             ) VALUES (?1,?2,?3,?4,?5,?5,?6)",
            params![
                campaign_id.to_string(),
                generation,
                self.owner_id.to_string(),
                lease_token.to_string(),
                now,
                deadline
            ],
        )?;
        transaction.commit()?;
        Ok(CampaignLeaseCapability {
            campaign_id,
            owner_id: self.owner_id,
            generation,
            lease_token,
            deadline_unix: deadline,
        })
    }

    pub fn renew(
        &self,
        capability: &CampaignLeaseCapability,
        lease_seconds: u64,
    ) -> Result<CampaignLeaseCapability> {
        if capability.owner_id != self.owner_id || lease_seconds == 0 {
            return Err(CampaignError::LeaseLost {
                campaign_id: capability.campaign_id,
            });
        }
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let now = Self::now();
        let current_deadline: Option<i64> = transaction
            .query_row(
                "SELECT deadline_unix FROM campaign_leases
                 WHERE campaign_id=?1 AND generation=?2 AND owner_id=?3
                   AND lease_token=?4 AND deadline_unix=?5",
                params![
                    capability.campaign_id.to_string(),
                    capability.generation,
                    capability.owner_id.to_string(),
                    capability.lease_token.to_string(),
                    capability.deadline_unix
                ],
                |row| row.get(0),
            )
            .optional()?;
        let Some(current_deadline) = current_deadline else {
            return Err(CampaignError::LeaseLost {
                campaign_id: capability.campaign_id,
            });
        };
        let current_deadline = parse_u64("campaign_lease.deadline_unix", current_deadline)?;
        if now >= current_deadline {
            let expired = transaction.execute(
                "UPDATE campaign_lease_attempts
                 SET status='expired',finished_unix=?1
                 WHERE campaign_id=?2 AND generation=?3 AND owner_id=?4
                   AND lease_token=?5 AND status='active' AND deadline_unix=?6",
                params![
                    now,
                    capability.campaign_id.to_string(),
                    capability.generation,
                    capability.owner_id.to_string(),
                    capability.lease_token.to_string(),
                    current_deadline
                ],
            )?;
            let removed = transaction.execute(
                "DELETE FROM campaign_leases
                 WHERE campaign_id=?1 AND generation=?2 AND owner_id=?3
                   AND lease_token=?4 AND deadline_unix=?5",
                params![
                    capability.campaign_id.to_string(),
                    capability.generation,
                    capability.owner_id.to_string(),
                    capability.lease_token.to_string(),
                    current_deadline
                ],
            )?;
            if expired != 1 || removed != 1 {
                return Err(CampaignError::LeaseLost {
                    campaign_id: capability.campaign_id,
                });
            }
            transaction.commit()?;
            return Err(CampaignError::LeaseExpired {
                campaign_id: capability.campaign_id,
            });
        }
        let proposed_deadline = now
            .checked_add(lease_seconds)
            .ok_or_else(|| CampaignError::Msg("campaign lease deadline overflow".into()))?;
        if proposed_deadline <= current_deadline {
            transaction.commit()?;
            return Ok(CampaignLeaseCapability {
                campaign_id: capability.campaign_id,
                owner_id: capability.owner_id,
                generation: capability.generation,
                lease_token: capability.lease_token,
                deadline_unix: current_deadline,
            });
        }
        let attempt_updated = transaction.execute(
            "UPDATE campaign_lease_attempts
             SET heartbeat_unix=?1,deadline_unix=?2
             WHERE campaign_id=?3 AND generation=?4 AND owner_id=?5
               AND lease_token=?6 AND status='active' AND deadline_unix=?7",
            params![
                now,
                proposed_deadline,
                capability.campaign_id.to_string(),
                capability.generation,
                capability.owner_id.to_string(),
                capability.lease_token.to_string(),
                current_deadline
            ],
        )?;
        let lease_updated = transaction.execute(
            "UPDATE campaign_leases SET heartbeat_unix=?1,deadline_unix=?2
             WHERE campaign_id=?3 AND generation=?4 AND owner_id=?5
               AND lease_token=?6 AND deadline_unix=?7",
            params![
                now,
                proposed_deadline,
                capability.campaign_id.to_string(),
                capability.generation,
                capability.owner_id.to_string(),
                capability.lease_token.to_string(),
                current_deadline
            ],
        )?;
        if attempt_updated != 1 || lease_updated != 1 {
            return Err(CampaignError::LeaseLost {
                campaign_id: capability.campaign_id,
            });
        }
        transaction.commit()?;
        Ok(CampaignLeaseCapability {
            campaign_id: capability.campaign_id,
            owner_id: capability.owner_id,
            generation: capability.generation,
            lease_token: capability.lease_token,
            deadline_unix: proposed_deadline,
        })
    }

    pub fn release(&self, capability: &CampaignLeaseCapability) -> Result<()> {
        if capability.owner_id != self.owner_id {
            return Err(CampaignError::LeaseLost {
                campaign_id: capability.campaign_id,
            });
        }
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let now = Self::now();
        let attempt_updated = transaction.execute(
            "UPDATE campaign_lease_attempts
             SET status='released',finished_unix=?1
             WHERE campaign_id=?2 AND generation=?3 AND owner_id=?4
               AND lease_token=?5 AND status='active' AND deadline_unix=?6",
            params![
                now,
                capability.campaign_id.to_string(),
                capability.generation,
                capability.owner_id.to_string(),
                capability.lease_token.to_string(),
                capability.deadline_unix
            ],
        )?;
        let removed = transaction.execute(
            "DELETE FROM campaign_leases
             WHERE campaign_id=?1 AND generation=?2 AND owner_id=?3
               AND lease_token=?4 AND deadline_unix=?5",
            params![
                capability.campaign_id.to_string(),
                capability.generation,
                capability.owner_id.to_string(),
                capability.lease_token.to_string(),
                capability.deadline_unix
            ],
        )?;
        if attempt_updated != 1 || removed != 1 {
            return Err(CampaignError::LeaseLost {
                campaign_id: capability.campaign_id,
            });
        }
        transaction.commit()?;
        Ok(())
    }

    /// Inspect all campaign plans without executing, creating a workspace, or
    /// changing persisted state.
    pub fn diagnose(&self) -> Result<CampaignHealthReport> {
        let schema_version = self.schema_version()?;
        let raw_ids = {
            let mut statement = self
                .conn
                .prepare("SELECT id FROM campaigns ORDER BY created_unix,id")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut diagnostics = Vec::new();
        for raw_id in raw_ids {
            let campaign_id = match Uuid::parse_str(&raw_id) {
                Ok(id) => id,
                Err(error) => {
                    diagnostics.push(CampaignDiagnostic {
                        campaign_id: None,
                        step_id: None,
                        field: "campaign.id".into(),
                        detail: error.to_string(),
                        repairable: false,
                    });
                    continue;
                }
            };
            match self.get(campaign_id) {
                Ok(Some(view)) => {
                    let stored_campaign_status: String = self.conn.query_row(
                        "SELECT status FROM campaigns WHERE id=?1",
                        params![campaign_id.to_string()],
                        |row| row.get(0),
                    )?;
                    if stored_campaign_status != view.campaign.status.as_str() {
                        diagnostics.push(CampaignDiagnostic {
                            campaign_id: Some(campaign_id),
                            step_id: None,
                            field: "campaign.status_projection".into(),
                            detail: format!(
                                "stored {stored_campaign_status}, authoritative {}",
                                view.campaign.status.as_str()
                            ),
                            repairable: true,
                        });
                    }
                    for step in &view.steps {
                        let stored_step_status: String = self.conn.query_row(
                            "SELECT status FROM campaign_steps WHERE id=?1",
                            params![step.id.to_string()],
                            |row| row.get(0),
                        )?;
                        if stored_step_status != step.status.as_str() {
                            diagnostics.push(CampaignDiagnostic {
                                campaign_id: Some(campaign_id),
                                step_id: Some(step.id),
                                field: "campaign_step.status_projection".into(),
                                detail: format!(
                                    "stored {stored_step_status}, authoritative {}",
                                    step.status.as_str()
                                ),
                                repairable: true,
                            });
                        }
                    }
                }
                Ok(None) => diagnostics.push(CampaignDiagnostic {
                    campaign_id: Some(campaign_id),
                    step_id: None,
                    field: "campaign.id".into(),
                    detail: "campaign disappeared during diagnostic scan".into(),
                    repairable: false,
                }),
                Err(CampaignError::Corrupt { field, detail }) => {
                    diagnostics.push(CampaignDiagnostic {
                        campaign_id: Some(campaign_id),
                        step_id: None,
                        field: field.into(),
                        detail,
                        repairable: false,
                    });
                }
                Err(CampaignError::Json(error)) => diagnostics.push(CampaignDiagnostic {
                    campaign_id: Some(campaign_id),
                    step_id: None,
                    field: "campaign_step.kind_json".into(),
                    detail: error.to_string(),
                    repairable: false,
                }),
                Err(error) => return Err(error),
            }
        }
        Ok(CampaignHealthReport {
            schema_version,
            diagnostics,
        })
    }

    /// Apply only deterministic repairs. Executable JSON and invalid identities
    /// are never guessed; they remain in the report for operator disposition.
    pub fn repair(&self) -> Result<CampaignRepairReport> {
        let health = self.diagnose()?;
        let campaign_ids: std::collections::BTreeSet<_> = health
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.repairable)
            .filter_map(|diagnostic| diagnostic.campaign_id)
            .collect();
        let mut repaired = 0_u32;
        for campaign_id in campaign_ids {
            let view = self.get(campaign_id)?.ok_or_else(|| {
                CampaignError::Msg(format!("campaign {campaign_id} disappeared during repair"))
            })?;
            let transaction = self.conn.unchecked_transaction()?;
            repaired += u32::try_from(transaction.execute(
                "UPDATE campaigns SET status=?1, updated_unix=?2 WHERE id=?3",
                params![
                    view.campaign.status.as_str(),
                    Self::now() as i64,
                    campaign_id.to_string()
                ],
            )?)
            .map_err(|_| CampaignError::Msg("repair count overflow".into()))?;
            for step in view.steps {
                repaired += u32::try_from(transaction.execute(
                    "UPDATE campaign_steps SET status=?1, detail=?2 WHERE id=?3",
                    params![step.status.as_str(), step.detail, step.id.to_string()],
                )?)
                .map_err(|_| CampaignError::Msg("repair count overflow".into()))?;
            }
            transaction.commit()?;
        }
        let remaining = self.diagnose()?.diagnostics;
        Ok(CampaignRepairReport {
            repaired,
            remaining,
        })
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    pub fn create(&self, name: &str, steps: Vec<CampaignStepSpec>) -> Result<CampaignView> {
        if steps.is_empty() {
            return Err(CampaignError::Msg(
                "campaign needs at least one step".into(),
            ));
        }
        let step_count = u32::try_from(steps.len())
            .map_err(|_| CampaignError::Msg("campaign has too many steps".into()))?;
        let id = Uuid::new_v4();
        let now = Self::now();
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO campaigns(id, name, status, created_unix, updated_unix, step_count)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                id.to_string(),
                name,
                CampaignStatus::Pending.as_str(),
                now,
                now,
                step_count
            ],
        )?;
        for (i, s) in steps.into_iter().enumerate() {
            let sid = Uuid::new_v4();
            let kind_json = serde_json::to_string(&s.kind)?;
            transaction.execute(
                "INSERT INTO campaign_steps(id, campaign_id, idx, label, kind_json, status, job_id, detail)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'')",
                params![
                    sid.to_string(),
                    id.to_string(),
                    i as i64,
                    s.label,
                    kind_json,
                    StepStatus::Pending.as_str(),
                    sid.to_string()
                ],
            )?;
        }
        transaction.commit()?;
        self.get(id)?
            .ok_or_else(|| CampaignError::Msg("missing after create".into()))
    }

    pub fn list(&self) -> Result<Vec<Campaign>> {
        let ids = {
            let mut statement = self
                .conn
                .prepare("SELECT id FROM campaigns ORDER BY created_unix DESC,id")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut out = Vec::new();
        for raw_id in ids {
            let id = parse_uuid("campaign.id", &raw_id)?;
            let view = self.get(id)?.ok_or_else(|| {
                CampaignError::Msg(format!("campaign {id} disappeared during list"))
            })?;
            out.push(view.campaign);
        }
        Ok(out)
    }

    pub fn get(&self, id: Uuid) -> Result<Option<CampaignView>> {
        let transaction = self.conn.unchecked_transaction()?;
        let raw_campaign: Option<(String, String, String, i64, i64, i64)> = transaction
            .query_row(
                "SELECT id, name, status, created_unix, updated_unix, step_count
                 FROM campaigns WHERE id=?1",
                params![id.to_string()],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((campaign_id, name, status, created_unix, updated_unix, step_count)) =
            raw_campaign
        else {
            return Ok(None);
        };
        let expected_step_count = parse_u32("campaign.step_count", step_count)?;
        if expected_step_count == 0 {
            return Err(corrupt("campaign.step_count", "zero steps"));
        }
        let mut campaign = Campaign {
            id: parse_uuid("campaign.id", &campaign_id)?,
            name,
            status: CampaignStatus::parse(&status)?,
            created_unix: parse_u64("campaign.created_unix", created_unix)?,
            updated_unix: parse_u64("campaign.updated_unix", updated_unix)?,
        };
        let mut stmt = transaction.prepare(
            "SELECT id, campaign_id, idx, label, kind_json, status, job_id, detail
             FROM campaign_steps WHERE campaign_id=?1 ORDER BY idx ASC",
        )?;
        let rows = stmt.query_map(params![id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?;
        let mut steps = Vec::new();
        for row in rows {
            let (step_id, step_campaign_id, idx, label, kind_json, status, job_id, detail) = row?;
            let step_campaign_id = parse_uuid("campaign_step.campaign_id", &step_campaign_id)?;
            if step_campaign_id != campaign.id {
                return Err(corrupt(
                    "campaign_step.campaign_id",
                    format!("expected {}, got {step_campaign_id}", campaign.id),
                ));
            }
            steps.push(CampaignStep {
                id: parse_uuid("campaign_step.id", &step_id)?,
                campaign_id: step_campaign_id,
                idx: parse_u32("campaign_step.idx", idx)?,
                label,
                kind: serde_json::from_str(&kind_json)?,
                status: StepStatus::parse(&status)?,
                job_id: parse_optional_uuid("campaign_step.job_id", job_id)?,
                detail,
            });
        }
        drop(stmt);
        if steps.len() != expected_step_count as usize {
            return Err(corrupt(
                "campaign.steps",
                format!(
                    "campaign {} expected {expected_step_count} steps, found {}",
                    campaign.id,
                    steps.len()
                ),
            ));
        }
        for (expected_idx, step) in (0..expected_step_count).zip(&steps) {
            if step.idx != expected_idx {
                return Err(corrupt(
                    "campaign.steps",
                    format!(
                        "campaign {} expected step index {expected_idx}, found {}",
                        campaign.id, step.idx
                    ),
                ));
            }
        }
        for step in &mut steps {
            let Some(job_uuid) = step.job_id else {
                if !matches!(
                    step.status,
                    StepStatus::Pending | StepStatus::Skipped | StepStatus::Cancelled
                ) {
                    return Err(corrupt(
                        "campaign_step.job_id",
                        format!(
                            "step {} has {:?} status without a job",
                            step.id, step.status
                        ),
                    ));
                }
                continue;
            };
            let work_graph_status: Option<String> = transaction
                .query_row(
                    "SELECT status FROM jobs WHERE id=?1",
                    params![job_uuid.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            match work_graph_status.as_deref() {
                Some("pending") => {
                    step.status = StepStatus::Pending;
                    step.detail = "job pending".into();
                }
                Some("running") | Some("interrupted") => {
                    step.status = StepStatus::Running;
                    step.detail = work_graph_status.unwrap_or_default();
                }
                Some("succeeded") => {
                    step.status = StepStatus::Succeeded;
                    step.detail = "ok".into();
                }
                Some("failed") => {
                    step.status = StepStatus::Failed;
                    step.detail = "job failed".into();
                }
                Some("cancelled") => {
                    step.status = StepStatus::Cancelled;
                    step.detail = "job cancelled".into();
                }
                Some("awaiting_approval") => {
                    step.status = StepStatus::AwaitingApproval;
                    step.detail = "needs approval".into();
                }
                Some(other) => {
                    return Err(corrupt("job.status", format!("unknown value {other}")));
                }
                None if job_uuid == step.id
                    && matches!(step.status, StepStatus::Pending | StepStatus::Cancelled) => {}
                None if step.status == StepStatus::Skipped => {}
                None => {
                    return Err(corrupt(
                        "campaign_step.job_id",
                        format!("step {} references missing job {job_uuid}", step.id),
                    ));
                }
            }
        }
        campaign.status = if steps.iter().any(|step| step.status == StepStatus::Failed) {
            CampaignStatus::Failed
        } else if steps
            .iter()
            .any(|step| step.status == StepStatus::Cancelled)
        {
            CampaignStatus::Cancelled
        } else if steps
            .iter()
            .any(|step| step.status == StepStatus::AwaitingApproval)
        {
            CampaignStatus::AwaitingApproval
        } else if steps.iter().any(|step| step.status == StepStatus::Running) {
            CampaignStatus::Running
        } else if steps
            .iter()
            .all(|step| matches!(step.status, StepStatus::Succeeded | StepStatus::Skipped))
        {
            CampaignStatus::Succeeded
        } else {
            CampaignStatus::Pending
        };
        transaction.commit()?;
        Ok(Some(CampaignView { campaign, steps }))
    }

    fn runtime(&self) -> Result<Runtime> {
        let db = self.home.join("optimus.db");
        let ws = self.home.join("workspace");
        std::fs::create_dir_all(&ws)?;
        // Campaigns run under SmartDeny + default Confined unless a future
        // product settings bridge is threaded through CampaignStore (desktop
        // open_runtime loads settings; campaign store is home-local only).
        Ok(Runtime::open_with_config(
            &db,
            &ws,
            RuntimeConfig::default(),
        )?)
    }

    pub fn cancel(&self, id: Uuid) -> Result<CampaignView> {
        let current = self
            .get(id)?
            .ok_or_else(|| CampaignError::Msg(format!("campaign {id} not found")))?;
        if matches!(
            current.campaign.status,
            CampaignStatus::Succeeded | CampaignStatus::Failed | CampaignStatus::Cancelled
        ) {
            return Ok(current);
        }
        let runtime = self.runtime()?;
        for step in &current.steps {
            if matches!(
                step.status,
                StepStatus::Succeeded
                    | StepStatus::Failed
                    | StepStatus::Cancelled
                    | StepStatus::Skipped
            ) {
                continue;
            }
            match step.job_id {
                Some(job_uuid) => match runtime.job_status_optional(job_id(job_uuid))? {
                    Some(JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled) => {}
                    Some(_) => {
                        runtime.cancel_job(job_id(job_uuid))?;
                    }
                    None => self.cancel_uncreated_step(step.id)?,
                },
                None => self.cancel_uncreated_step(step.id)?,
            }
        }
        self.get(id)?
            .ok_or_else(|| CampaignError::Msg(format!("campaign {id} disappeared")))
    }

    fn cancel_uncreated_step(&self, step_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE campaign_steps
             SET status='cancelled',detail='cancelled before job creation'
             WHERE id=?1 AND status NOT IN ('succeeded','failed','cancelled','skipped')",
            params![step_id.to_string()],
        )?;
        Ok(())
    }

    /// Run or resume a campaign under one exact durable lease.
    pub fn run(&self, id: Uuid) -> Result<CampaignView> {
        let current = self
            .get(id)?
            .ok_or_else(|| CampaignError::Msg(format!("campaign {id} not found")))?;
        if matches!(
            current.campaign.status,
            CampaignStatus::Succeeded | CampaignStatus::Failed | CampaignStatus::Cancelled
        ) {
            return Ok(current);
        }
        let mut capability = self.claim(id, 60)?;
        let result = self.run_with_lease(id, &mut capability);
        let released = self.release(&capability);
        match (result, released) {
            (Ok(view), Ok(())) => Ok(view),
            (Ok(_), Err(error)) => Err(error),
            (Err(error), _) => Err(error),
        }
    }

    fn run_with_lease(
        &self,
        id: Uuid,
        capability: &mut CampaignLeaseCapability,
    ) -> Result<CampaignView> {
        let view = self
            .get(id)?
            .ok_or_else(|| CampaignError::Msg(format!("campaign {id} not found")))?;
        if matches!(
            view.campaign.status,
            CampaignStatus::Succeeded | CampaignStatus::Failed | CampaignStatus::Cancelled
        ) {
            return Ok(view);
        }
        let rt = self.runtime()?;

        for step in &view.steps {
            if matches!(
                step.status,
                StepStatus::Succeeded | StepStatus::Skipped | StepStatus::Cancelled
            ) {
                continue;
            }
            *capability = self.renew(capability, 60)?;
            let jid = job_id(step.job_id.unwrap_or(step.id));
            if step.job_id.is_none() {
                self.conn.execute(
                    "UPDATE campaign_steps SET job_id=?1 WHERE id=?2 AND job_id IS NULL",
                    params![jid.0.to_string(), step.id.to_string()],
                )?;
            }
            let effect = match &step.kind {
                StepKind::WriteFile {
                    relative_path,
                    contents,
                } => Effect::WriteFile {
                    relative_path: relative_path.clone(),
                    contents: contents.clone(),
                },
                StepKind::RunCommand { program, args } => Effect::RunCommand {
                    program: program.clone(),
                    args: args.clone(),
                },
            };
            match rt.job_status_optional(jid)? {
                Some(JobStatus::Succeeded) => continue,
                Some(JobStatus::Failed | JobStatus::Cancelled | JobStatus::AwaitingApproval) => {
                    return self
                        .get(id)?
                        .ok_or_else(|| CampaignError::Msg(format!("campaign {id} disappeared")));
                }
                Some(JobStatus::Running) => {
                    rt.recover_crashed_job(jid)?;
                }
                Some(JobStatus::Pending | JobStatus::Interrupted) => {}
                None => {
                    rt.create_job_with_id(
                        jid,
                        JobSpec {
                            label: format!("campaign:{}:{}", view.campaign.name, step.label),
                            budget: Default::default(),
                            nodes: vec![NodeSpec {
                                label: step.label.clone(),
                                effect,
                            }],
                        },
                    )?;
                }
            }
            match rt.resume(jid) {
                Ok(JobStatus::Succeeded) => {}
                Ok(JobStatus::Failed | JobStatus::Cancelled | JobStatus::AwaitingApproval) => {
                    return self
                        .get(id)?
                        .ok_or_else(|| CampaignError::Msg(format!("campaign {id} disappeared")));
                }
                Ok(other) => {
                    return Err(CampaignError::Msg(format!(
                        "job {jid} stopped in unexpected state {other:?}"
                    )));
                }
                Err(RuntimeError::NeedsApproval { .. }) => {
                    return self
                        .get(id)?
                        .ok_or_else(|| CampaignError::Msg(format!("campaign {id} disappeared")));
                }
                Err(_error) if rt.job_status_optional(jid)? == Some(JobStatus::Failed) => {
                    return self
                        .get(id)?
                        .ok_or_else(|| CampaignError::Msg(format!("campaign {id} disappeared")));
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.get(id)?
            .ok_or_else(|| CampaignError::Msg(format!("campaign {id} disappeared")))
    }

    /// After SmartDeny grant on the blocked step's job, continue the campaign.
    pub fn continue_after_grant(&self, id: Uuid) -> Result<CampaignView> {
        self.run(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn assert_corrupt_field(error: CampaignError, expected_field: &str) {
        match error {
            CampaignError::Corrupt { field, .. } => assert_eq!(field, expected_field),
            other => panic!("expected corrupt {expected_field}, got {other:?}"),
        }
    }

    #[cfg(windows)]
    fn successful_command() -> StepKind {
        StepKind::RunCommand {
            program: "cmd".into(),
            args: vec!["/C".into(), "echo done".into()],
        }
    }

    #[cfg(unix)]
    fn successful_command() -> StepKind {
        StepKind::RunCommand {
            program: "sh".into(),
            args: vec!["-c".into(), "printf done".into()],
        }
    }

    /// Run a campaign, granting each SmartDeny-blocked high-risk step until terminal.
    fn run_granting_high_risk(store: &CampaignStore, id: Uuid) -> CampaignView {
        let mut view = store.run(id).unwrap();
        for _ in 0..32 {
            if view.campaign.status != CampaignStatus::AwaitingApproval {
                return view;
            }
            let step = view
                .steps
                .iter()
                .find(|step| step.status == StepStatus::AwaitingApproval)
                .expect("awaiting approval without a blocked step");
            let jid = job_id(step.job_id.expect("blocked step job"));
            store
                .runtime()
                .unwrap()
                .grant_and_resume(jid)
                .expect("grant blocked high-risk step");
            view = store.continue_after_grant(id).unwrap();
        }
        panic!("campaign {id} still awaiting approval after grants");
    }

    #[test]
    fn sequential_write_campaign_succeeds() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "two-writers",
                vec![
                    CampaignStepSpec {
                        label: "a".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "agents/a.txt".into(),
                            contents: "alpha".into(),
                        },
                    },
                    CampaignStepSpec {
                        label: "b".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "agents/b.txt".into(),
                            contents: "beta".into(),
                        },
                    },
                ],
            )
            .unwrap();
        let done = run_granting_high_risk(&store, view.campaign.id);
        assert_eq!(done.campaign.status, CampaignStatus::Succeeded);
        assert!(done.steps.iter().all(|s| s.status == StepStatus::Succeeded));
        let a = std::fs::read_to_string(d.path().join("workspace/agents/a.txt")).unwrap();
        let b = std::fs::read_to_string(d.path().join("workspace/agents/b.txt")).unwrap();
        assert_eq!(a, "alpha");
        assert_eq!(b, "beta");
    }

    #[test]
    fn campaign_schema_v4_has_typed_empty_lease_projection() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "lease-model",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "lease.txt".into(),
                        contents: "ok".into(),
                    },
                }],
            )
            .unwrap();

        assert_eq!(store.schema_version().unwrap(), 4);
        assert!(table_exists(&store.conn, "campaign_leases").unwrap());
        assert!(table_exists(&store.conn, "campaign_lease_attempts").unwrap());
        assert_eq!(store.lease(view.campaign.id).unwrap(), None);
    }

    #[test]
    fn cancelled_campaign_and_step_statuses_are_typed_and_persistable() {
        assert_eq!(
            CampaignStatus::parse("cancelled").unwrap(),
            CampaignStatus::Cancelled
        );
        assert_eq!(
            StepStatus::parse("cancelled").unwrap(),
            StepStatus::Cancelled
        );
        assert_eq!(CampaignStatus::Cancelled.as_str(), "cancelled");
        assert_eq!(StepStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn cancelling_unstarted_campaign_propagates_and_is_idempotent() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let campaign_id = store
            .create(
                "cancel campaign",
                vec![
                    CampaignStepSpec {
                        label: "one".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "one.txt".into(),
                            contents: "one".into(),
                        },
                    },
                    CampaignStepSpec {
                        label: "two".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "two.txt".into(),
                            contents: "two".into(),
                        },
                    },
                ],
            )
            .unwrap()
            .campaign
            .id;

        let first = store.cancel(campaign_id).unwrap();
        let second = store.cancel(campaign_id).unwrap();

        assert_eq!(first.campaign.status, CampaignStatus::Cancelled);
        assert!(first
            .steps
            .iter()
            .all(|step| step.status == StepStatus::Cancelled));
        assert_eq!(second.campaign.status, CampaignStatus::Cancelled);
        assert!(second
            .steps
            .iter()
            .all(|step| step.status == StepStatus::Cancelled));
        assert!(!d.path().join("workspace/one.txt").exists());
        assert!(!d.path().join("workspace/two.txt").exists());
    }

    #[test]
    fn campaign_claim_is_exclusive_across_store_instances() {
        let d = tempdir().unwrap();
        let first = CampaignStore::open(d.path()).unwrap();
        let view = first
            .create(
                "exclusive-claim",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "claimed.txt".into(),
                        contents: "ok".into(),
                    },
                }],
            )
            .unwrap();
        let second = CampaignStore::open(d.path()).unwrap();

        let capability = first.claim(view.campaign.id, 30).unwrap();
        let rejected = second.claim(view.campaign.id, 30);

        assert!(matches!(rejected, Err(CampaignError::LeaseHeld { .. })));
        let lease = first.lease(view.campaign.id).unwrap().unwrap();
        assert_eq!(lease.campaign_id, view.campaign.id);
        assert_eq!(lease.owner_id, capability.owner_id());
        assert_eq!(lease.generation, 1);
        assert!(lease.deadline_unix > lease.acquired_unix);
    }

    #[test]
    fn expired_campaign_lease_is_reclaimed_and_stale_owner_is_fenced() {
        let d = tempdir().unwrap();
        let first = CampaignStore::open(d.path()).unwrap();
        let campaign_id = first
            .create(
                "reclaim",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "reclaim.txt".into(),
                        contents: "ok".into(),
                    },
                }],
            )
            .unwrap()
            .campaign
            .id;
        let stale = first.claim(campaign_id, 1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1_200));
        let second = CampaignStore::open(d.path()).unwrap();

        let replacement = second.claim(campaign_id, 5).unwrap();
        let renewed = second.renew(&replacement, 30).unwrap();
        let stale_release = first.release(&stale);

        assert_eq!(replacement.generation(), 2);
        assert!(renewed.deadline_unix() > replacement.deadline_unix());
        assert!(matches!(
            stale_release,
            Err(CampaignError::LeaseLost { .. })
        ));
        second.release(&renewed).unwrap();
        assert_eq!(second.lease(campaign_id).unwrap(), None);
        let statuses: Vec<String> = second
            .conn
            .prepare(
                "SELECT status FROM campaign_lease_attempts
                 WHERE campaign_id=?1 ORDER BY generation",
            )
            .unwrap()
            .query_map(params![campaign_id.to_string()], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(statuses, vec!["expired", "released"]);
    }

    #[test]
    fn campaign_run_requires_exclusive_live_lease() {
        let d = tempdir().unwrap();
        let owner = CampaignStore::open(d.path()).unwrap();
        let campaign_id = owner
            .create(
                "leased-run",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "leased-run.txt".into(),
                        contents: "owned".into(),
                    },
                }],
            )
            .unwrap()
            .campaign
            .id;
        let capability = owner.claim(campaign_id, 30).unwrap();
        let contender = CampaignStore::open(d.path()).unwrap();

        let rejected = contender.run(campaign_id);

        assert!(matches!(rejected, Err(CampaignError::LeaseHeld { .. })));
        assert!(!d.path().join("workspace/leased-run.txt").exists());
        owner.release(&capability).unwrap();
        let done = run_granting_high_risk(&contender, campaign_id);
        assert_eq!(done.campaign.status, CampaignStatus::Succeeded);
    }

    #[test]
    fn run_command_blocks_then_grant_resumes_campaign() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "needs-grant",
                vec![
                    CampaignStepSpec {
                        label: "prep".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "prep.txt".into(),
                            contents: "ready".into(),
                        },
                    },
                    CampaignStepSpec {
                        label: "cmd".into(),
                        kind: successful_command(),
                    },
                    CampaignStepSpec {
                        label: "tail".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "tail.txt".into(),
                            contents: "after".into(),
                        },
                    },
                ],
            )
            .unwrap();
        // First host-mutating step is the WriteFile prep (now high-risk).
        let blocked = store.run(view.campaign.id).unwrap();
        assert_eq!(blocked.campaign.status, CampaignStatus::AwaitingApproval);
        assert_eq!(blocked.steps[0].status, StepStatus::AwaitingApproval);
        assert_eq!(blocked.steps[1].status, StepStatus::Pending);
        assert_eq!(blocked.steps[2].status, StepStatus::Pending);

        let done = run_granting_high_risk(&store, view.campaign.id);
        assert_eq!(done.campaign.status, CampaignStatus::Succeeded);
        assert!(done.steps.iter().all(|s| s.status == StepStatus::Succeeded));
        assert_eq!(
            std::fs::read_to_string(d.path().join("workspace/tail.txt")).unwrap(),
            "after"
        );
    }

    #[test]
    fn corrupt_step_kind_fails_before_runtime_effects() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "corrupt-kind",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "legitimate.txt".into(),
                        contents: "legitimate".into(),
                    },
                }],
            )
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE campaign_steps SET kind_json='not-json' WHERE campaign_id=?1",
                params![view.campaign.id.to_string()],
            )
            .unwrap();

        let error = store
            .run(view.campaign.id)
            .expect_err("corrupt step kind must fail closed");
        assert!(error.to_string().contains("json"), "{error:?}");
        assert!(d.path().join("optimus.db").exists());
        assert!(!d.path().join("workspace").exists());
        assert!(!d.path().join("workspace/lost.txt").exists());
    }

    #[test]
    fn corrupt_campaign_identity_status_and_time_are_rejected() {
        for (column, value, operation, expected_field) in [
            ("id", "'not-a-uuid'", "list", "campaign.id"),
            ("status", "'unknown'", "get", "campaign.status"),
            ("created_unix", "-1", "get", "campaign.created_unix"),
            ("updated_unix", "-1", "get", "campaign.updated_unix"),
        ] {
            let d = tempdir().unwrap();
            let store = CampaignStore::open(d.path()).unwrap();
            let view = store
                .create(
                    "corrupt-campaign",
                    vec![CampaignStepSpec {
                        label: "write".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "ok.txt".into(),
                            contents: "ok".into(),
                        },
                    }],
                )
                .unwrap();
            if column == "id" {
                store
                    .conn
                    .execute_batch("PRAGMA foreign_keys=OFF;")
                    .unwrap();
            }
            store
                .conn
                .execute(
                    &format!("UPDATE campaigns SET {column}={value} WHERE id=?1"),
                    params![view.campaign.id.to_string()],
                )
                .unwrap();
            if column == "id" {
                store.conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            }

            let error = if operation == "list" {
                store.list().expect_err("corrupt campaign must fail")
            } else {
                store
                    .get(view.campaign.id)
                    .expect_err("corrupt campaign must fail")
            };
            assert_corrupt_field(error, expected_field);
        }
    }

    #[test]
    fn corrupt_step_identity_status_job_and_index_are_rejected() {
        for (column, value, expected_field) in [
            ("id", "'not-a-uuid'", "campaign_step.id"),
            ("status", "'unknown'", "campaign_step.status"),
            ("job_id", "'not-a-uuid'", "campaign_step.job_id"),
            ("idx", "-1", "campaign_step.idx"),
        ] {
            let d = tempdir().unwrap();
            let store = CampaignStore::open(d.path()).unwrap();
            let view = store
                .create(
                    "corrupt-step",
                    vec![CampaignStepSpec {
                        label: "write".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "ok.txt".into(),
                            contents: "ok".into(),
                        },
                    }],
                )
                .unwrap();
            store
                .conn
                .execute(
                    &format!("UPDATE campaign_steps SET {column}={value} WHERE campaign_id=?1"),
                    params![view.campaign.id.to_string()],
                )
                .unwrap();

            let error = store
                .get(view.campaign.id)
                .expect_err("corrupt step must fail");
            assert_corrupt_field(error, expected_field);
        }
    }

    #[test]
    fn orphaned_step_relationship_is_rejected() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "orphaned-step",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "ok.txt".into(),
                        contents: "ok".into(),
                    },
                }],
            )
            .unwrap();
        store
            .conn
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE campaign_steps SET campaign_id=?1 WHERE campaign_id=?2",
                params![Uuid::new_v4().to_string(), view.campaign.id.to_string()],
            )
            .unwrap();
        store.conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        let error = store
            .get(view.campaign.id)
            .expect_err("campaign cannot silently lose all persisted steps");
        assert_corrupt_field(error, "campaign.steps");
    }

    #[test]
    fn partially_reassigned_plan_fails_before_runtime_effects() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "partial-plan",
                vec![
                    CampaignStepSpec {
                        label: "first".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "first.txt".into(),
                            contents: "first".into(),
                        },
                    },
                    CampaignStepSpec {
                        label: "second".into(),
                        kind: StepKind::WriteFile {
                            relative_path: "second.txt".into(),
                            contents: "second".into(),
                        },
                    },
                ],
            )
            .unwrap();
        store
            .conn
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE campaign_steps SET campaign_id=?1 WHERE campaign_id=?2 AND idx=0",
                params![Uuid::new_v4().to_string(), view.campaign.id.to_string()],
            )
            .unwrap();
        store.conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        let error = store
            .run(view.campaign.id)
            .expect_err("partially missing plan must fail closed");
        assert_corrupt_field(error, "campaign.steps");
        assert!(d.path().join("optimus.db").exists());
        assert!(!d.path().join("workspace").exists());
    }

    #[test]
    fn expected_step_count_and_contiguous_indices_are_enforced() {
        for statement in [
            "UPDATE campaigns SET step_count=1 WHERE name='plan-integrity'",
            "UPDATE campaign_steps SET idx=7 WHERE label='second'",
        ] {
            let d = tempdir().unwrap();
            let store = CampaignStore::open(d.path()).unwrap();
            let view = store
                .create(
                    "plan-integrity",
                    vec![
                        CampaignStepSpec {
                            label: "first".into(),
                            kind: StepKind::WriteFile {
                                relative_path: "first.txt".into(),
                                contents: "first".into(),
                            },
                        },
                        CampaignStepSpec {
                            label: "second".into(),
                            kind: StepKind::WriteFile {
                                relative_path: "second.txt".into(),
                                contents: "second".into(),
                            },
                        },
                    ],
                )
                .unwrap();
            store.conn.execute(statement, []).unwrap();

            let error = store
                .get(view.campaign.id)
                .expect_err("plan integrity mismatch must fail closed");
            assert_corrupt_field(error, "campaign.steps");
        }
    }

    #[test]
    fn legacy_schema_migrates_expected_step_count() {
        let d = tempdir().unwrap();
        let campaign_id = Uuid::new_v4();
        let step_id = Uuid::new_v4();
        let connection = rusqlite::Connection::open(d.path().join("campaigns.db")).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE campaigns (
                   id TEXT PRIMARY KEY,
                   name TEXT NOT NULL,
                   status TEXT NOT NULL,
                   created_unix INTEGER NOT NULL,
                   updated_unix INTEGER NOT NULL
                 );
                 CREATE TABLE campaign_steps (
                   id TEXT PRIMARY KEY,
                   campaign_id TEXT NOT NULL,
                   idx INTEGER NOT NULL,
                   label TEXT NOT NULL,
                   kind_json TEXT NOT NULL,
                   status TEXT NOT NULL,
                   job_id TEXT,
                   detail TEXT NOT NULL DEFAULT '',
                   UNIQUE(campaign_id, idx)
                 );",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO campaigns(id,name,status,created_unix,updated_unix)
                 VALUES (?1,'legacy','pending',1,1)",
                params![campaign_id.to_string()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO campaign_steps(
                   id,campaign_id,idx,label,kind_json,status,job_id,detail
                 ) VALUES (?1,?2,0,'write',?3,'pending',NULL,'')",
                params![
                    step_id.to_string(),
                    campaign_id.to_string(),
                    serde_json::to_string(&StepKind::WriteFile {
                        relative_path: "legacy.txt".into(),
                        contents: "legacy".into(),
                    })
                    .unwrap()
                ],
            )
            .unwrap();
        drop(connection);

        let store = CampaignStore::open(d.path()).expect("migrate legacy schema");
        let view = store.get(campaign_id).unwrap().expect("legacy campaign");
        assert_eq!(view.steps.len(), 1);
        let done = run_granting_high_risk(&store, campaign_id);
        assert_eq!(done.campaign.status, CampaignStatus::Succeeded);
        assert_eq!(
            std::fs::read_to_string(d.path().join("workspace/legacy.txt")).unwrap(),
            "legacy"
        );
    }

    #[test]
    fn future_campaign_schema_is_rejected_without_mutation() {
        let d = tempdir().unwrap();
        let db = d.path().join("optimus.db");
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE campaign_meta (
                   key TEXT PRIMARY KEY NOT NULL,
                   value TEXT NOT NULL
                 );
                 INSERT INTO campaign_meta(key,value) VALUES ('schema_version','999');",
            )
            .unwrap();
        drop(connection);

        let error = CampaignStore::open(d.path())
            .err()
            .expect("future schema must fail closed");
        assert!(error
            .to_string()
            .contains("unsupported campaign schema 999"));

        let connection = rusqlite::Connection::open(&db).unwrap();
        let version: String = connection
            .query_row(
                "SELECT value FROM campaign_meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, "999");
    }

    #[test]
    fn unversioned_unified_v2_schema_gains_campaign_meta_and_migrates() {
        let d = tempdir().unwrap();
        let db = d.path().join("optimus.db");
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE campaigns (
                   id TEXT PRIMARY KEY NOT NULL,
                   name TEXT NOT NULL,
                   status TEXT NOT NULL,
                   created_unix INTEGER NOT NULL,
                   updated_unix INTEGER NOT NULL,
                   step_count INTEGER NOT NULL
                 );
                 CREATE TABLE campaign_steps (
                   id TEXT PRIMARY KEY NOT NULL,
                   campaign_id TEXT NOT NULL,
                   idx INTEGER NOT NULL,
                   label TEXT NOT NULL,
                   kind_json TEXT NOT NULL,
                   status TEXT NOT NULL,
                   job_id TEXT,
                   detail TEXT NOT NULL DEFAULT '',
                   UNIQUE(campaign_id, idx)
                 );",
            )
            .unwrap();
        drop(connection);

        let store = CampaignStore::open(d.path()).expect("migrate inferred v2");
        assert_eq!(store.schema_version().unwrap(), CAMPAIGN_SCHEMA_VERSION);
    }

    #[test]
    fn new_campaign_schema_is_versioned_in_the_work_graph_database() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "unified",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "unified.txt".into(),
                        contents: "unified".into(),
                    },
                }],
            )
            .unwrap();
        drop(store);

        assert!(d.path().join("optimus.db").exists());
        assert!(
            !d.path().join("campaigns.db").exists(),
            "a second campaign authority was created"
        );
        let connection = rusqlite::Connection::open(d.path().join("optimus.db")).unwrap();
        let version: String = connection
            .query_row(
                "SELECT value FROM campaign_meta WHERE key='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, CAMPAIGN_SCHEMA_VERSION.to_string());
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM campaigns WHERE id=?1",
                params![view.campaign.id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn diagnostics_report_irreparable_corruption_without_executing() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "diagnostic",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "must-not-run.txt".into(),
                        contents: "no".into(),
                    },
                }],
            )
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE campaigns SET status='invented' WHERE id=?1",
                params![view.campaign.id.to_string()],
            )
            .unwrap();

        let health = store.diagnose().expect("diagnose");
        assert_eq!(health.schema_version, CAMPAIGN_SCHEMA_VERSION);
        assert_eq!(health.diagnostics.len(), 1);
        assert_eq!(health.diagnostics[0].field, "campaign.status");
        assert!(!health.diagnostics[0].repairable);

        let repair = store.repair().expect("repair report");
        assert_eq!(repair.repaired, 0);
        assert_eq!(repair.remaining.len(), 1);
        assert!(!d.path().join("workspace").exists());
        assert!(!d.path().join("workspace/must-not-run.txt").exists());
    }

    #[test]
    fn new_steps_persist_deterministic_job_handoff_identity() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "deterministic-handoff",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "handoff.txt".into(),
                        contents: "handoff".into(),
                    },
                }],
            )
            .unwrap();

        assert_eq!(view.steps[0].job_id, Some(view.steps[0].id));
        let persisted: String = store
            .conn
            .query_row(
                "SELECT job_id FROM campaign_steps WHERE id=?1",
                params![view.steps[0].id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted, view.steps[0].id.to_string());
    }

    fn write_step_job(view: &CampaignView) -> JobSpec {
        let step = &view.steps[0];
        let StepKind::WriteFile {
            relative_path,
            contents,
        } = &step.kind
        else {
            panic!("expected write step")
        };
        JobSpec {
            label: format!("campaign:{}:{}", view.campaign.name, step.label),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: step.label.clone(),
                effect: Effect::WriteFile {
                    relative_path: relative_path.clone(),
                    contents: contents.clone(),
                },
            }],
        }
    }

    #[test]
    fn crash_after_job_creation_resumes_the_same_deterministic_job() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "created-before-crash",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "created-before-crash.txt".into(),
                        contents: "once".into(),
                    },
                }],
            )
            .unwrap();
        let jid = job_id(view.steps[0].id);
        store
            .runtime()
            .unwrap()
            .create_job_with_id(jid, write_step_job(&view))
            .unwrap();
        drop(store);

        let reopened = CampaignStore::open(d.path()).unwrap();
        let done = run_granting_high_risk(&reopened, view.campaign.id);
        assert_eq!(done.campaign.status, CampaignStatus::Succeeded);
        assert_eq!(done.steps[0].status, StepStatus::Succeeded);
        let job_count: i64 = reopened
            .conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE id=?1",
                params![jid.0.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(job_count, 1);
    }

    #[test]
    fn crash_with_running_node_is_recovered_before_campaign_resume() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "running-before-crash",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "running-before-crash.txt".into(),
                        contents: "recovered".into(),
                    },
                }],
            )
            .unwrap();
        let jid = job_id(view.steps[0].id);
        let runtime = store.runtime().unwrap();
        runtime
            .create_job_with_id(jid, write_step_job(&view))
            .unwrap();
        runtime.begin_node_and_crash(jid).unwrap();
        drop(runtime);
        drop(store);

        let reopened = CampaignStore::open(d.path()).unwrap();
        let done = run_granting_high_risk(&reopened, view.campaign.id);
        assert_eq!(done.campaign.status, CampaignStatus::Succeeded);
        assert_eq!(done.steps[0].status, StepStatus::Succeeded);
        assert_eq!(
            std::fs::read_to_string(d.path().join("workspace/running-before-crash.txt")).unwrap(),
            "recovered"
        );
    }

    #[test]
    fn campaign_status_is_derived_from_the_work_graph_authority() {
        let d = tempdir().unwrap();
        let store = CampaignStore::open(d.path()).unwrap();
        let view = store
            .create(
                "derived-status",
                vec![CampaignStepSpec {
                    label: "write".into(),
                    kind: StepKind::WriteFile {
                        relative_path: "derived-status.txt".into(),
                        contents: "derived".into(),
                    },
                }],
            )
            .unwrap();
        let jid = job_id(view.steps[0].id);
        let runtime = store.runtime().unwrap();
        runtime
            .create_job_with_id(jid, write_step_job(&view))
            .unwrap();
        assert_eq!(runtime.run_all(jid).unwrap(), JobStatus::AwaitingApproval);
        assert_eq!(runtime.grant_and_resume(jid).unwrap(), JobStatus::Succeeded);

        let derived = store.get(view.campaign.id).unwrap().unwrap();
        assert_eq!(derived.steps[0].status, StepStatus::Succeeded);
        assert_eq!(derived.campaign.status, CampaignStatus::Succeeded);
        let listed = store.list().unwrap();
        assert_eq!(listed[0].status, CampaignStatus::Succeeded);

        let health = store.diagnose().unwrap();
        assert!(health.diagnostics.iter().any(|diagnostic| {
            diagnostic.repairable && diagnostic.field == "campaign.status_projection"
        }));
        assert!(health.diagnostics.iter().any(|diagnostic| {
            diagnostic.repairable && diagnostic.field == "campaign_step.status_projection"
        }));
        let repaired = store.repair().unwrap();
        assert!(repaired.repaired >= 2);
        assert!(repaired.remaining.is_empty());
    }
}
