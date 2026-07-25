//! Local operator gateway with a file adapter and SQLite delivery authority.
//!
//! External channels may drop `<uuid>.json` messages into `gateway/inbox/`.
//! SQLite owns ingestion identity, claims, attempts, and terminal outcomes. The
//! outbox and processed/failed directories are reconciled materializations.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("gateway lease ownership was lost for {message_id}")]
    LeaseLost { message_id: String },
    #[error("gateway lease expired for {message_id}")]
    LeaseExpired { message_id: String },
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, GatewayError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboundMessage {
    pub id: String,
    pub channel: String,
    pub text: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub received_unix: u64,
}

fn default_provider() -> String {
    "offline".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundMessage {
    pub id: String,
    pub in_reply_to: String,
    pub channel: String,
    pub text: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub provider: String,
    pub status: String,
    pub sent_unix: u64,
}

#[derive(Debug, Clone)]
pub struct GatewayPaths {
    pub root: PathBuf,
    pub inbox: PathBuf,
    pub outbox: PathBuf,
    pub processed: PathBuf,
    pub failed: PathBuf,
    pub database: PathBuf,
}

impl GatewayPaths {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let root = home.as_ref().join("gateway");
        let paths = Self {
            inbox: root.join("inbox"),
            outbox: root.join("outbox"),
            processed: root.join("processed"),
            failed: root.join("failed"),
            database: root.join("gateway.db"),
            root,
        };
        fs::create_dir_all(&paths.inbox)?;
        fs::create_dir_all(&paths.outbox)?;
        fs::create_dir_all(&paths.processed)?;
        fs::create_dir_all(&paths.failed)?;
        let _ = open_database(&paths)?;
        Ok(paths)
    }
}

#[derive(Debug)]
pub struct GatewayClaim {
    message: InboundMessage,
    owner_id: Uuid,
    generation: u64,
    lease_token: Uuid,
    attempt_id: Uuid,
    deadline_unix: u64,
}

impl GatewayClaim {
    pub fn message(&self) -> &InboundMessage {
        &self.message
    }

    pub fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }

    pub fn deadline_unix(&self) -> u64 {
        self.deadline_unix
    }

    pub fn attempt_number(&self) -> u64 {
        self.generation
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrainResult {
    pub id: String,
    pub status: String,
    pub reply_preview: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn open_database(paths: &GatewayPaths) -> Result<Connection> {
    let connection = Connection::open(&paths.database)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=FULL;
         CREATE TABLE IF NOT EXISTS gateway_messages (
           id TEXT PRIMARY KEY,
           inbound_json TEXT NOT NULL,
           received_unix INTEGER NOT NULL,
           status TEXT NOT NULL CHECK(status IN ('pending','claimed','succeeded','failed')),
           lease_owner_id TEXT,
           lease_generation INTEGER NOT NULL DEFAULT 0,
           lease_token TEXT,
           lease_deadline_unix INTEGER,
           active_attempt_id TEXT,
           outbound_json TEXT,
           completed_unix INTEGER,
           terminal_reason TEXT,
           delivered_unix INTEGER
         );
         CREATE TABLE IF NOT EXISTS gateway_attempts (
           attempt_id TEXT PRIMARY KEY,
           message_id TEXT NOT NULL,
           owner_id TEXT NOT NULL,
           generation INTEGER NOT NULL,
           lease_token TEXT NOT NULL,
           started_unix INTEGER NOT NULL,
           deadline_unix INTEGER NOT NULL,
           status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','expired')),
           completed_unix INTEGER,
           detail TEXT,
           FOREIGN KEY(message_id) REFERENCES gateway_messages(id)
         );
         CREATE INDEX IF NOT EXISTS idx_gateway_claimable
           ON gateway_messages(status, received_unix, lease_deadline_unix);",
    )?;
    ensure_gateway_column(&connection, "terminal_reason", "TEXT")?;
    ensure_gateway_column(&connection, "delivered_unix", "INTEGER")?;
    Ok(connection)
}

fn ensure_gateway_column(connection: &Connection, name: &str, definition: &str) -> Result<()> {
    let columns = {
        let mut statement = connection.prepare("PRAGMA table_info(gateway_messages)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    if !columns.iter().any(|column| column == name) {
        connection.execute_batch(&format!(
            "ALTER TABLE gateway_messages ADD COLUMN {name} {definition};"
        ))?;
    }
    Ok(())
}

fn valid_message_id(id: &str) -> bool {
    Uuid::parse_str(id).is_ok_and(|parsed| parsed.to_string() == id)
}

fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        return if fs::read(path)? == bytes {
            Ok(())
        } else {
            Err(GatewayError::Msg(format!(
                "conflicting gateway materialization at {}",
                path.display()
            )))
        };
    }
    let parent = path
        .parent()
        .ok_or_else(|| GatewayError::Msg("gateway path has no parent".into()))?;
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_error) if path.exists() => {
            let _ = fs::remove_file(&temporary);
            if fs::read(path)? == bytes {
                Ok(())
            } else {
                Err(GatewayError::Msg(format!(
                    "conflicting gateway materialization at {}",
                    path.display()
                )))
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error.into())
        }
    }
}

/// Drop a message into the durable inbox (simulates an external channel webhook).
pub fn enqueue(
    home: impl AsRef<Path>,
    channel: &str,
    text: &str,
    provider: &str,
    session_id: Option<&str>,
) -> Result<InboundMessage> {
    let paths = GatewayPaths::open(home)?;
    let message = InboundMessage {
        id: Uuid::new_v4().to_string(),
        channel: channel.into(),
        text: text.into(),
        session_id: session_id.map(str::to_string),
        provider: provider.into(),
        received_unix: now_unix(),
    };
    let path = paths.inbox.join(format!("{}.json", message.id));
    atomic_write_new(&path, &serde_json::to_vec_pretty(&message)?)?;
    Ok(message)
}

fn ingest_inbox(paths: &GatewayPaths, connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for entry in fs::read_dir(&paths.inbox)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let Ok(message) = serde_json::from_str::<InboundMessage>(&raw) else {
            continue;
        };
        if !valid_message_id(&message.id)
            || path.file_stem().and_then(|stem| stem.to_str()) != Some(message.id.as_str())
        {
            continue;
        }
        transaction.execute(
            "INSERT OR IGNORE INTO gateway_messages(
               id,inbound_json,received_unix,status,lease_generation
             ) VALUES(?1,?2,?3,'pending',0)",
            params![message.id, raw, message.received_unix as i64],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn list_inbox(home: impl AsRef<Path>) -> Result<Vec<InboundMessage>> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    ingest_inbox(&paths, &mut connection)?;
    let mut statement = connection.prepare(
        "SELECT inbound_json FROM gateway_messages
         WHERE status IN ('pending','claimed') ORDER BY received_unix ASC,id ASC",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(serde_json::from_str(&row?)?);
    }
    Ok(messages)
}

pub fn list_outbox(home: impl AsRef<Path>, limit: usize) -> Result<Vec<OutboundMessage>> {
    Ok(list_outbox_receipts(home, limit)?
        .into_iter()
        .map(|row| row.outbound)
        .collect())
}

/// Outbox row with local delivery receipt fields (program P28).
///
/// `delivered_unix` is a **local** receipt that an adapter acknowledged handoff.
/// It is not an external exactly-once proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboxReceipt {
    pub message_id: String,
    pub outbound: OutboundMessage,
    pub terminal_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_unix: Option<u64>,
    /// True when terminal succeeded but no local delivery receipt yet.
    pub ambiguous_send: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_unix: Option<u64>,
}

pub fn list_outbox_receipts(home: impl AsRef<Path>, limit: usize) -> Result<Vec<OutboxReceipt>> {
    let paths = GatewayPaths::open(home)?;
    let connection = open_database(&paths)?;
    let mut statement = connection.prepare(
        "SELECT id,status,outbound_json,terminal_reason,delivered_unix,completed_unix
         FROM gateway_messages
         WHERE status IN ('succeeded','failed') AND outbound_json IS NOT NULL
         ORDER BY completed_unix DESC,id DESC LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (message_id, terminal_status, outbound_json, terminal_reason, delivered, completed) =
            row?;
        let outbound: OutboundMessage = serde_json::from_str(&outbound_json)?;
        let delivered_unix = delivered.and_then(|value| u64::try_from(value).ok());
        let completed_unix = completed.and_then(|value| u64::try_from(value).ok());
        let ambiguous_send = is_ambiguous_receipt(&terminal_status, delivered_unix, &terminal_reason);
        out.push(OutboxReceipt {
            message_id,
            outbound,
            terminal_status,
            terminal_reason,
            delivered_unix,
            ambiguous_send,
            completed_unix,
        });
    }
    Ok(out)
}

fn is_ambiguous_receipt(
    terminal_status: &str,
    delivered_unix: Option<u64>,
    terminal_reason: &Option<String>,
) -> bool {
    if terminal_status != "succeeded" || delivered_unix.is_some() {
        return false;
    }
    match terminal_reason.as_deref() {
        Some(reason)
            if reason == "external_send_failed"
                || reason.starts_with("external_send_failed:")
                || reason == "cancelled"
                || reason == "dead_lettered" =>
        {
            false
        }
        _ => true,
    }
}

/// Succeeded terminal turns without a local delivery receipt (operator recovery).
///
/// SQL-filters first so a flood of receipted rows cannot hide older ambiguous ones.
pub fn list_ambiguous_sends(home: impl AsRef<Path>, limit: usize) -> Result<Vec<OutboxReceipt>> {
    let limit = limit.clamp(1, 500);
    let paths = GatewayPaths::open(home)?;
    let connection = open_database(&paths)?;
    let mut statement = connection.prepare(
        "SELECT id,status,outbound_json,terminal_reason,delivered_unix,completed_unix
         FROM gateway_messages
         WHERE status='succeeded'
           AND outbound_json IS NOT NULL
           AND delivered_unix IS NULL
           AND (terminal_reason IS NULL
                OR (terminal_reason NOT IN ('external_send_failed','cancelled','dead_lettered')
                    AND terminal_reason NOT LIKE 'external_send_failed:%'))
         ORDER BY completed_unix DESC,id DESC LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (message_id, terminal_status, outbound_json, terminal_reason, delivered, completed) =
            row?;
        let outbound: OutboundMessage = serde_json::from_str(&outbound_json)?;
        let delivered_unix = delivered.and_then(|value| u64::try_from(value).ok());
        let completed_unix = completed.and_then(|value| u64::try_from(value).ok());
        out.push(OutboxReceipt {
            message_id,
            outbound,
            terminal_status,
            terminal_reason,
            delivered_unix,
            ambiguous_send: true,
            completed_unix,
        });
    }
    Ok(out)
}

/// Mark a succeeded terminal as a definite external send failure (not ambiguous).
pub fn mark_external_send_failed(
    home: impl AsRef<Path>,
    message_id: &str,
    detail: &str,
) -> Result<bool> {
    let paths = GatewayPaths::open(home)?;
    let connection = open_database(&paths)?;
    let detail = if detail.is_empty() {
        "external_send_failed".to_string()
    } else {
        format!("external_send_failed:{detail}")
    };
    let changed = connection.execute(
        "UPDATE gateway_messages
         SET terminal_reason=?1
         WHERE id=?2 AND status='succeeded' AND delivered_unix IS NULL",
        params![detail, message_id],
    )?;
    Ok(changed == 1)
}

/// Gateway summary for doctor / messaging UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayStatus {
    pub inbox_pending: usize,
    pub inbox_claimed: usize,
    pub outbox_total: usize,
    pub ambiguous_sends: usize,
    pub note: String,
}

pub fn gateway_status(home: impl AsRef<Path>) -> Result<GatewayStatus> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    ingest_inbox(&paths, &mut connection)?;
    let inbox_pending: i64 = connection.query_row(
        "SELECT COUNT(*) FROM gateway_messages WHERE status='pending'",
        [],
        |row| row.get(0),
    )?;
    let inbox_claimed: i64 = connection.query_row(
        "SELECT COUNT(*) FROM gateway_messages WHERE status='claimed'",
        [],
        |row| row.get(0),
    )?;
    let outbox_total: i64 = connection.query_row(
        "SELECT COUNT(*) FROM gateway_messages
         WHERE status IN ('succeeded','failed') AND outbound_json IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    let ambiguous_sends: i64 = connection.query_row(
        "SELECT COUNT(*) FROM gateway_messages
         WHERE status='succeeded' AND outbound_json IS NOT NULL AND delivered_unix IS NULL
           AND (terminal_reason IS NULL OR terminal_reason NOT IN (
                'external_send_failed','cancelled','dead_lettered'
           ))
           AND (terminal_reason IS NULL OR terminal_reason NOT LIKE 'external_send_failed:%')",
        [],
        |row| row.get(0),
    )?;
    Ok(GatewayStatus {
        inbox_pending: inbox_pending.max(0) as usize,
        inbox_claimed: inbox_claimed.max(0) as usize,
        outbox_total: outbox_total.max(0) as usize,
        ambiguous_sends: ambiguous_sends.max(0) as usize,
        note: "Local SQLite is delivery authority. External exactly-once is not claimed.".into(),
    })
}

pub fn claim_one(
    home: impl AsRef<Path>,
    owner_id: Uuid,
    now: u64,
    lease_secs: u64,
) -> Result<Option<GatewayClaim>> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    ingest_inbox(&paths, &mut connection)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let selected = transaction
        .query_row(
            "SELECT id,inbound_json,status,lease_generation,active_attempt_id
             FROM gateway_messages
             WHERE status='pending' OR (status='claimed' AND lease_deadline_unix<=?1)
             ORDER BY received_unix ASC,id ASC LIMIT 1",
            params![now as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((id, inbound_json, prior_status, prior_generation, prior_attempt)) = selected else {
        transaction.commit()?;
        return Ok(None);
    };
    if prior_status == "claimed" {
        if let Some(attempt_id) = prior_attempt {
            transaction.execute(
                "UPDATE gateway_attempts
                 SET status='expired',completed_unix=?1,detail='lease expired before completion'
                 WHERE attempt_id=?2 AND status='running'",
                params![now as i64, attempt_id],
            )?;
        }
    }
    let generation = u64::try_from(prior_generation)
        .map_err(|_| GatewayError::Msg("negative gateway lease generation".into()))?
        .saturating_add(1);
    let lease_token = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();
    let deadline = now.saturating_add(lease_secs.max(1));
    let changed = transaction.execute(
        "UPDATE gateway_messages
         SET status='claimed',lease_owner_id=?1,lease_generation=?2,lease_token=?3,
             lease_deadline_unix=?4,active_attempt_id=?5
         WHERE id=?6 AND (status='pending' OR (status='claimed' AND lease_deadline_unix<=?7))",
        params![
            owner_id.to_string(),
            generation as i64,
            lease_token.to_string(),
            deadline as i64,
            attempt_id.to_string(),
            id,
            now as i64
        ],
    )?;
    if changed != 1 {
        return Err(GatewayError::LeaseLost { message_id: id });
    }
    transaction.execute(
        "INSERT INTO gateway_attempts(
           attempt_id,message_id,owner_id,generation,lease_token,started_unix,deadline_unix,status
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,'running')",
        params![
            attempt_id.to_string(),
            id,
            owner_id.to_string(),
            generation as i64,
            lease_token.to_string(),
            now as i64,
            deadline as i64
        ],
    )?;
    transaction.commit()?;
    let message: InboundMessage = serde_json::from_str(&inbound_json)?;
    let _ = fs::remove_file(paths.inbox.join(format!("{}.json", message.id)));
    Ok(Some(GatewayClaim {
        message,
        owner_id,
        generation,
        lease_token,
        attempt_id,
        deadline_unix: deadline,
    }))
}

fn lease_error(connection: &Connection, claim: &GatewayClaim, now: u64) -> Result<GatewayError> {
    let current = connection
        .query_row(
            "SELECT lease_owner_id,lease_generation,lease_token,lease_deadline_unix
             FROM gateway_messages WHERE id=?1",
            params![claim.message.id],
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
    let owner = claim.owner_id.to_string();
    let token = claim.lease_token.to_string();
    let exact_expired = current.is_some_and(|(live_owner, generation, live_token, deadline)| {
        live_owner.as_deref() == Some(owner.as_str())
            && generation == claim.generation as i64
            && live_token.as_deref() == Some(token.as_str())
            && deadline.is_some_and(|value| value <= now as i64)
    });
    if exact_expired {
        Ok(GatewayError::LeaseExpired {
            message_id: claim.message.id.clone(),
        })
    } else {
        Ok(GatewayError::LeaseLost {
            message_id: claim.message.id.clone(),
        })
    }
}

pub fn renew_claim(
    home: impl AsRef<Path>,
    claim: &mut GatewayClaim,
    now: u64,
    lease_secs: u64,
) -> Result<()> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let deadline = now.saturating_add(lease_secs.max(1));
    let changed = transaction.execute(
        "UPDATE gateway_messages SET lease_deadline_unix=?1
         WHERE id=?2 AND status='claimed' AND lease_owner_id=?3 AND lease_generation=?4
           AND lease_token=?5 AND active_attempt_id=?6 AND lease_deadline_unix>?7",
        params![
            deadline as i64,
            claim.message.id,
            claim.owner_id.to_string(),
            claim.generation as i64,
            claim.lease_token.to_string(),
            claim.attempt_id.to_string(),
            now as i64
        ],
    )?;
    if changed != 1 {
        return Err(lease_error(&transaction, claim, now)?);
    }
    let attempt_changed = transaction.execute(
        "UPDATE gateway_attempts SET deadline_unix=?1
         WHERE attempt_id=?2 AND message_id=?3 AND owner_id=?4 AND generation=?5
           AND lease_token=?6 AND status='running'",
        params![
            deadline as i64,
            claim.attempt_id.to_string(),
            claim.message.id,
            claim.owner_id.to_string(),
            claim.generation as i64,
            claim.lease_token.to_string()
        ],
    )?;
    if attempt_changed != 1 {
        return Err(GatewayError::LeaseLost {
            message_id: claim.message.id.clone(),
        });
    }
    transaction.commit()?;
    claim.deadline_unix = deadline;
    Ok(())
}

pub fn release_claim(home: impl AsRef<Path>, claim: &GatewayClaim, now: u64) -> Result<()> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = transaction.execute(
        "UPDATE gateway_messages
         SET status='pending',lease_owner_id=NULL,lease_token=NULL,
             lease_deadline_unix=NULL,active_attempt_id=NULL
         WHERE id=?1 AND status='claimed' AND lease_owner_id=?2 AND lease_generation=?3
           AND lease_token=?4 AND active_attempt_id=?5 AND lease_deadline_unix>?6",
        params![
            claim.message.id,
            claim.owner_id.to_string(),
            claim.generation as i64,
            claim.lease_token.to_string(),
            claim.attempt_id.to_string(),
            now as i64
        ],
    )?;
    if changed != 1 {
        return Err(lease_error(&transaction, claim, now)?);
    }
    let attempt_changed = transaction.execute(
        "UPDATE gateway_attempts
         SET status='expired',completed_unix=?1,detail='released without completion'
         WHERE attempt_id=?2 AND status='running'",
        params![now as i64, claim.attempt_id.to_string()],
    )?;
    if attempt_changed != 1 {
        return Err(GatewayError::LeaseLost {
            message_id: claim.message.id.clone(),
        });
    }
    transaction.commit()?;
    Ok(())
}

fn commit_claim(
    paths: &GatewayPaths,
    claim: &GatewayClaim,
    outcome: &std::result::Result<(String, Option<String>), String>,
    now: u64,
) -> Result<OutboundMessage> {
    let mut connection = open_database(paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (terminal_status, text, session_id, attempt_status, detail) = match outcome {
        Ok((text, session_id)) => (
            "succeeded",
            text.clone(),
            session_id.clone(),
            "succeeded",
            "ok".to_string(),
        ),
        Err(error) => (
            "failed",
            format!("error: {error}"),
            claim.message.session_id.clone(),
            "failed",
            error.clone(),
        ),
    };
    let outbound = OutboundMessage {
        id: claim.attempt_id.to_string(),
        in_reply_to: claim.message.id.clone(),
        channel: claim.message.channel.clone(),
        text,
        session_id,
        provider: claim.message.provider.clone(),
        status: if terminal_status == "succeeded" {
            "ok".into()
        } else {
            "error".into()
        },
        sent_unix: now,
    };
    let outbound_json = serde_json::to_string(&outbound)?;
    let changed = transaction.execute(
        "UPDATE gateway_messages
         SET status=?1,outbound_json=?2,completed_unix=?3,
             lease_owner_id=NULL,lease_token=NULL,lease_deadline_unix=NULL,active_attempt_id=NULL
         WHERE id=?4 AND status='claimed' AND lease_owner_id=?5 AND lease_generation=?6
           AND lease_token=?7 AND active_attempt_id=?8 AND lease_deadline_unix>?3",
        params![
            terminal_status,
            outbound_json,
            now as i64,
            claim.message.id,
            claim.owner_id.to_string(),
            claim.generation as i64,
            claim.lease_token.to_string(),
            claim.attempt_id.to_string()
        ],
    )?;
    if changed != 1 {
        let error = lease_error(&transaction, claim, now)?;
        return Err(error);
    }
    transaction.execute(
        "UPDATE gateway_attempts SET status=?1,completed_unix=?2,detail=?3
         WHERE attempt_id=?4 AND status='running'",
        params![
            attempt_status,
            now as i64,
            detail,
            claim.attempt_id.to_string()
        ],
    )?;
    transaction.commit()?;
    Ok(outbound)
}

fn materialize_terminal(
    paths: &GatewayPaths,
    message: &InboundMessage,
    out: &OutboundMessage,
) -> Result<()> {
    atomic_write_new(
        &paths.outbox.join(format!("{}.json", out.id)),
        &serde_json::to_vec_pretty(out)?,
    )?;
    let archive = if out.status == "ok" {
        &paths.processed
    } else {
        &paths.failed
    };
    atomic_write_new(
        &archive.join(format!("{}.json", message.id)),
        &serde_json::to_vec_pretty(message)?,
    )?;
    let _ = fs::remove_file(paths.inbox.join(format!("{}.json", message.id)));
    Ok(())
}

pub fn complete_claim(
    home: impl AsRef<Path>,
    claim: &GatewayClaim,
    outcome: std::result::Result<(String, Option<String>), String>,
    now: u64,
) -> Result<DrainResult> {
    let paths = GatewayPaths::open(home)?;
    let outbound = commit_claim(&paths, claim, &outcome, now)?;
    materialize_terminal(&paths, &claim.message, &outbound)?;
    Ok(DrainResult {
        id: claim.message.id.clone(),
        status: match outcome {
            Ok(_) => "ok".into(),
            Err(error) => format!("error:{error}"),
        },
        reply_preview: outbound.text.chars().take(200).collect(),
        session_id: outbound.session_id,
    })
}

pub fn fail_claim(
    home: impl AsRef<Path>,
    claim: &GatewayClaim,
    error_code: &str,
    now: u64,
) -> Result<DrainResult> {
    const MAX_ATTEMPTS: u64 = 3;
    if claim.attempt_number() >= MAX_ATTEMPTS {
        let result = complete_claim(home.as_ref(), claim, Err(error_code.to_string()), now)?;
        let paths = GatewayPaths::open(home)?;
        let connection = open_database(&paths)?;
        connection.execute(
            "UPDATE gateway_messages SET terminal_reason='dead_lettered'
             WHERE id=?1 AND status='failed'",
            params![claim.message.id],
        )?;
        return Ok(DrainResult {
            status: format!("dead_lettered:{error_code}"),
            ..result
        });
    }
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = transaction.execute(
        "UPDATE gateway_messages
         SET status='pending',lease_owner_id=NULL,lease_token=NULL,
             lease_deadline_unix=NULL,active_attempt_id=NULL
         WHERE id=?1 AND status='claimed' AND lease_owner_id=?2 AND lease_generation=?3
           AND lease_token=?4 AND active_attempt_id=?5 AND lease_deadline_unix>?6",
        params![
            claim.message.id,
            claim.owner_id.to_string(),
            claim.generation as i64,
            claim.lease_token.to_string(),
            claim.attempt_id.to_string(),
            now as i64
        ],
    )?;
    if changed != 1 {
        return Err(lease_error(&transaction, claim, now)?);
    }
    transaction.execute(
        "UPDATE gateway_attempts SET status='failed',completed_unix=?1,detail=?2
         WHERE attempt_id=?3 AND status='running'",
        params![now as i64, error_code, claim.attempt_id.to_string()],
    )?;
    transaction.commit()?;
    Ok(DrainResult {
        id: claim.message.id.clone(),
        status: format!("retry_scheduled:{error_code}"),
        reply_preview: String::new(),
        session_id: claim.message.session_id.clone(),
    })
}

pub fn cancel_claim(home: impl AsRef<Path>, claim: &GatewayClaim, now: u64) -> Result<DrainResult> {
    let result = complete_claim(home.as_ref(), claim, Err("cancelled".into()), now)?;
    let paths = GatewayPaths::open(home)?;
    let connection = open_database(&paths)?;
    connection.execute(
        "UPDATE gateway_messages SET terminal_reason='cancelled'
         WHERE id=?1 AND status='failed'",
        params![claim.message.id],
    )?;
    Ok(DrainResult {
        status: "cancelled".into(),
        ..result
    })
}

pub fn acknowledge_delivery(
    home: impl AsRef<Path>,
    message_id: &str,
    outbound_id: &str,
    now: u64,
) -> Result<bool> {
    let paths = GatewayPaths::open(home)?;
    let connection = open_database(&paths)?;
    let outbound_json: Option<String> = connection
        .query_row(
            "SELECT outbound_json FROM gateway_messages
             WHERE id=?1 AND status IN ('succeeded','failed')",
            params![message_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(outbound_json) = outbound_json else {
        return Ok(false);
    };
    let outbound: OutboundMessage = serde_json::from_str(&outbound_json)?;
    if outbound.id != outbound_id {
        return Ok(false);
    }
    connection.execute(
        "UPDATE gateway_messages SET delivered_unix=COALESCE(delivered_unix,?1) WHERE id=?2",
        params![now as i64, message_id],
    )?;
    Ok(true)
}

pub fn delivery_state(
    home: impl AsRef<Path>,
    message_id: &str,
) -> Result<Option<(Option<String>, Option<u64>)>> {
    let paths = GatewayPaths::open(home)?;
    let connection = open_database(&paths)?;
    connection
        .query_row(
            "SELECT terminal_reason,delivered_unix FROM gateway_messages WHERE id=?1",
            params![message_id],
            |row| {
                let delivered = row.get::<_, Option<i64>>(1)?;
                Ok((
                    row.get(0)?,
                    delivered.and_then(|value| u64::try_from(value).ok()),
                ))
            },
        )
        .optional()
        .map_err(GatewayError::Sqlite)
}

pub fn reconcile(home: impl AsRef<Path>) -> Result<usize> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    ingest_inbox(&paths, &mut connection)?;
    let terminal = {
        let mut statement = connection.prepare(
            "SELECT inbound_json,outbound_json FROM gateway_messages
             WHERE status IN ('succeeded','failed') AND outbound_json IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut materialized = 0;
    for (inbound_json, outbound_json) in terminal {
        let inbound: InboundMessage = serde_json::from_str(&inbound_json)?;
        let outbound: OutboundMessage = serde_json::from_str(&outbound_json)?;
        materialize_terminal(&paths, &inbound, &outbound)?;
        materialized += 1;
    }
    Ok(materialized)
}

/// Claim and process one inbox message via a caller-provided turn function.
pub fn drain_one<F>(home: impl AsRef<Path>, mut turn: F) -> Result<Option<DrainResult>>
where
    F: FnMut(&InboundMessage) -> std::result::Result<(String, Option<String>), String>,
{
    let home = home.as_ref();
    reconcile(home)?;
    let Some(claim) = claim_one(home, Uuid::new_v4(), now_unix(), 900)? else {
        return Ok(None);
    };
    let outcome = turn(claim.message());
    let completed = match outcome {
        Ok(success) => complete_claim(home, &claim, Ok(success), now_unix())?,
        Err(error) => fail_claim(home, &claim, &error, now_unix())?,
    };
    Ok(Some(completed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn enqueue_list_drain_roundtrip() {
        let directory = tempdir().unwrap();
        let message = enqueue(directory.path(), "local", "hello gateway", "offline", None).unwrap();
        assert_eq!(list_inbox(directory.path()).unwrap()[0].id, message.id);

        let drained = drain_one(directory.path(), |inbound| {
            Ok((format!("echo: {}", inbound.text), Some("sess-1".into())))
        })
        .unwrap()
        .unwrap();

        assert_eq!(drained.status, "ok");
        assert!(list_inbox(directory.path()).unwrap().is_empty());
        let outbox = list_outbox(directory.path(), 10).unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].in_reply_to, message.id);
        assert!(acknowledge_delivery(directory.path(), &message.id, &outbox[0].id, 42).unwrap());
        assert_eq!(
            delivery_state(directory.path(), &message.id)
                .unwrap()
                .unwrap()
                .1,
            Some(42)
        );
        assert!(!acknowledge_delivery(directory.path(), &message.id, "wrong", 43).unwrap());
    }

    #[test]
    fn repeated_failure_retries_twice_then_dead_letters() {
        let directory = tempdir().unwrap();
        let message = enqueue(directory.path(), "local", "fail", "offline", None).unwrap();

        let first = drain_one(directory.path(), |_| Err("provider_unavailable".into()))
            .unwrap()
            .unwrap();
        let second = drain_one(directory.path(), |_| Err("provider_unavailable".into()))
            .unwrap()
            .unwrap();
        let third = drain_one(directory.path(), |_| Err("provider_unavailable".into()))
            .unwrap()
            .unwrap();

        assert!(first.status.starts_with("retry_scheduled:"));
        assert!(second.status.starts_with("retry_scheduled:"));
        assert!(third.status.starts_with("dead_lettered:"));
        assert!(list_inbox(directory.path()).unwrap().is_empty());
        let state = delivery_state(directory.path(), &message.id)
            .unwrap()
            .unwrap();
        assert_eq!(state.0.as_deref(), Some("dead_lettered"));
        let outbox = list_outbox(directory.path(), 10).unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].status, "error");
        assert!(GatewayPaths::open(directory.path())
            .unwrap()
            .failed
            .join(format!("{}.json", message.id))
            .exists());
    }

    #[test]
    fn cancellation_is_terminal_and_fences_late_completion() {
        let directory = tempdir().unwrap();
        let message = enqueue(directory.path(), "local", "cancel", "offline", None).unwrap();
        let claim = claim_one(directory.path(), Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();

        let cancelled = cancel_claim(directory.path(), &claim, 11).unwrap();

        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(
            delivery_state(directory.path(), &message.id)
                .unwrap()
                .unwrap()
                .0
                .as_deref(),
            Some("cancelled")
        );
        assert!(matches!(
            complete_claim(directory.path(), &claim, Ok(("late".into(), None)), 12),
            Err(GatewayError::LeaseLost { message_id }) if message_id == message.id
        ));
    }

    #[test]
    fn concurrent_claimers_cannot_own_same_message() {
        let directory = tempdir().unwrap();
        enqueue(directory.path(), "local", "one", "offline", None).unwrap();

        let first = claim_one(directory.path(), Uuid::new_v4(), 10, 30).unwrap();
        let second = claim_one(directory.path(), Uuid::new_v4(), 10, 30).unwrap();

        assert!(first.is_some());
        assert!(second.is_none());
    }

    #[test]
    fn expired_takeover_fences_stale_completion() {
        let directory = tempdir().unwrap();
        let message = enqueue(directory.path(), "local", "one", "offline", None).unwrap();
        let stale = claim_one(directory.path(), Uuid::new_v4(), 10, 5)
            .unwrap()
            .unwrap();
        let live = claim_one(directory.path(), Uuid::new_v4(), 16, 30)
            .unwrap()
            .unwrap();

        assert!(matches!(
            complete_claim(directory.path(), &stale, Ok(("stale".into(), None)), 16),
            Err(GatewayError::LeaseLost { message_id }) if message_id == message.id
        ));
        complete_claim(directory.path(), &live, Ok(("live".into(), None)), 16).unwrap();
        let outbox = list_outbox(directory.path(), 10).unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].text, "live");
    }

    #[test]
    fn reconciliation_materializes_committed_terminal_outcome_without_rerun() {
        let directory = tempdir().unwrap();
        enqueue(directory.path(), "local", "one", "offline", None).unwrap();
        let claim = claim_one(directory.path(), Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();
        let paths = GatewayPaths::open(directory.path()).unwrap();
        let outbound = commit_claim(&paths, &claim, &Ok(("done".into(), None)), 11).unwrap();
        assert!(!paths.outbox.join(format!("{}.json", outbound.id)).exists());

        reconcile(directory.path()).unwrap();

        assert!(paths.outbox.join(format!("{}.json", outbound.id)).exists());
        assert!(paths
            .processed
            .join(format!("{}.json", claim.message.id))
            .exists());
    }

    #[test]
    fn renewal_blocks_takeover_and_release_makes_message_claimable() {
        let directory = tempdir().unwrap();
        enqueue(directory.path(), "local", "one", "offline", None).unwrap();
        let mut claim = claim_one(directory.path(), Uuid::new_v4(), 10, 5)
            .unwrap()
            .unwrap();

        renew_claim(directory.path(), &mut claim, 14, 10).unwrap();
        assert_eq!(claim.deadline_unix(), 24);
        assert!(claim_one(directory.path(), Uuid::new_v4(), 16, 30)
            .unwrap()
            .is_none());
        release_claim(directory.path(), &claim, 17).unwrap();
        assert!(claim_one(directory.path(), Uuid::new_v4(), 17, 30)
            .unwrap()
            .is_some());
    }

    #[test]
    fn reconciliation_rejects_conflicting_outbox_materialization() {
        let directory = tempdir().unwrap();
        enqueue(directory.path(), "local", "one", "offline", None).unwrap();
        let claim = claim_one(directory.path(), Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();
        let paths = GatewayPaths::open(directory.path()).unwrap();
        let outbound = commit_claim(&paths, &claim, &Ok(("trusted".into(), None)), 11).unwrap();
        fs::write(
            paths.outbox.join(format!("{}.json", outbound.id)),
            b"conflicting bytes",
        )
        .unwrap();

        assert!(matches!(
            reconcile(directory.path()),
            Err(GatewayError::Msg(message)) if message.contains("conflicting gateway materialization")
        ));
    }

    #[test]
    fn ambiguous_sends_list_until_delivery_receipt() {
        let directory = tempdir().unwrap();
        let message = enqueue(directory.path(), "local", "hello", "offline", None).unwrap();
        drain_one(directory.path(), |inbound| {
            Ok((format!("echo: {}", inbound.text), None))
        })
        .unwrap()
        .unwrap();

        let status = gateway_status(directory.path()).unwrap();
        assert_eq!(status.ambiguous_sends, 1);
        assert_eq!(status.outbox_total, 1);
        let ambiguous = list_ambiguous_sends(directory.path(), 10).unwrap();
        assert_eq!(ambiguous.len(), 1);
        assert!(ambiguous[0].ambiguous_send);
        assert_eq!(ambiguous[0].message_id, message.id);
        assert!(ambiguous[0].delivered_unix.is_none());

        let outbound_id = ambiguous[0].outbound.id.clone();
        assert!(acknowledge_delivery(directory.path(), &message.id, &outbound_id, 99).unwrap());
        assert!(list_ambiguous_sends(directory.path(), 10).unwrap().is_empty());
        assert_eq!(gateway_status(directory.path()).unwrap().ambiguous_sends, 0);
        let receipts = list_outbox_receipts(directory.path(), 10).unwrap();
        assert_eq!(receipts[0].delivered_unix, Some(99));
        assert!(!receipts[0].ambiguous_send);
    }

    #[test]
    fn ambiguous_list_is_not_blinded_by_newer_receipted_rows() {
        let directory = tempdir().unwrap();
        // Older message stays ambiguous.
        let old = enqueue(directory.path(), "local", "old", "offline", None).unwrap();
        drain_one(directory.path(), |inbound| Ok((format!("echo:{}", inbound.text), None)))
            .unwrap()
            .unwrap();
        // Newer messages get receipts so they would occupy a naive "limit then filter" window.
        for i in 0..5 {
            let m = enqueue(
                directory.path(),
                "local",
                &format!("new-{i}"),
                "offline",
                None,
            )
            .unwrap();
            drain_one(directory.path(), |inbound| {
                Ok((format!("echo:{}", inbound.text), None))
            })
            .unwrap()
            .unwrap();
            let receipts = list_outbox_receipts(directory.path(), 20).unwrap();
            let row = receipts
                .iter()
                .find(|r| r.message_id == m.id)
                .expect("receipt row");
            acknowledge_delivery(directory.path(), &m.id, &row.outbound.id, 100 + i as u64)
                .unwrap();
        }
        // limit=3 would hide `old` if we filtered after LIMIT on all terminals.
        let ambiguous = list_ambiguous_sends(directory.path(), 3).unwrap();
        assert_eq!(ambiguous.len(), 1);
        assert_eq!(ambiguous[0].message_id, old.id);
        assert_eq!(gateway_status(directory.path()).unwrap().ambiguous_sends, 1);
    }

    #[test]
    fn external_send_failed_is_not_ambiguous() {
        let directory = tempdir().unwrap();
        let message = enqueue(directory.path(), "local", "x", "offline", None).unwrap();
        drain_one(directory.path(), |_| Ok(("reply".into(), None)))
            .unwrap()
            .unwrap();
        assert!(mark_external_send_failed(directory.path(), &message.id, "mock_failed").unwrap());
        assert!(list_ambiguous_sends(directory.path(), 10).unwrap().is_empty());
        assert_eq!(gateway_status(directory.path()).unwrap().ambiguous_sends, 0);
    }
}
