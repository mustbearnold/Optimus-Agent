//! Durable outbound obligation ledger.
//!
//! The inbound half of the gateway has owned claims, leases, and attempts since
//! program P28. The outbound half did not: a reply was sent to the external
//! channel *after* the turn had already committed, with nothing durable saying a
//! send was owed. A crash in that window lost the obligation silently, and a
//! crash *during* the send was indistinguishable from never having tried.
//!
//! This module closes that window. An obligation is written in the same
//! transaction that makes the turn terminal, so it cannot be lost; every attempt
//! is recorded *before* the network is touched, so a crash mid-send is visible
//! as attempted-with-unknown-outcome rather than as never-attempted.
//!
//! What is deliberately **not** claimed is external exactly-once. No transport
//! this ledger drives offers a dedupe primitive, so the honest guarantee is
//! at-least-once with a fenced ambiguity window: an attempt whose outcome is
//! unknown never retries on its own, because a silent duplicate is the failure
//! this ledger exists to prevent. Resolving that window is an operator decision,
//! and [`resolve_ambiguous_obligation`] is where they record it.
//!
//! `gateway_messages` remains the terminal authority for the *turn*. Its
//! `delivered_unix` and `terminal_reason` columns are a projection of obligation
//! state, maintained here, so the operator surfaces that already read them stay
//! truthful without knowing this table exists.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{now_unix, open_database, GatewayError, GatewayPaths, OutboundMessage, Result};

/// Definite failures retried before an obligation is abandoned.
///
/// Higher than the inbound limit of 3 because the units differ: an inbound
/// attempt re-runs a whole turn and can re-trigger its effects, while an
/// outbound attempt re-sends a body that is already fixed.
const MAX_SEND_ATTEMPTS: u64 = 5;

/// One owed external send.
///
/// `target` is the routing address *as the owning channel writes it* — the
/// gateway does not parse it. Telegram stores `telegram:<chat id>`; the adapter
/// that created the obligation is the one that knows how to read it back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundObligation {
    pub obligation_id: String,
    pub message_id: String,
    pub outbound_id: String,
    pub channel: String,
    pub target: String,
    /// The full reply text. Not the 200-character preview a `DrainResult` carries.
    pub body: String,
    /// Stable across every retry of this obligation, for transports that can dedupe.
    pub idempotency_key: String,
    pub status: String,
    pub attempts: u64,
    pub created_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_detail: Option<String>,
}

/// A leased obligation. Holding one is permission to attempt exactly one send.
#[derive(Debug)]
pub struct OutboundClaim {
    obligation: OutboundObligation,
    owner_id: Uuid,
    lease_token: Uuid,
    attempt_id: Uuid,
    attempt_no: u64,
    deadline_unix: u64,
}

impl OutboundClaim {
    pub fn obligation(&self) -> &OutboundObligation {
        &self.obligation
    }

    pub fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }

    pub fn attempt_number(&self) -> u64 {
        self.attempt_no
    }

    pub fn deadline_unix(&self) -> u64 {
        self.deadline_unix
    }
}

/// What an adapter learned from one external send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundSettlement {
    /// The platform accepted it and said so.
    Delivered { provider_message_id: String },
    /// Timeout, dropped connection, unreadable response — it may or may not have landed.
    Ambiguous { detail: String },
    /// The platform refused it: bad address, revoked auth, malformed body.
    Failed { detail: String },
}

/// How an operator closed an ambiguous obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmbiguityResolution {
    /// Checked the channel; it did arrive.
    Delivered { provider_message_id: String },
    /// Checked the channel; it did not arrive. Safe to retry.
    NotDelivered,
    /// Stop trying, whatever happened.
    Abandon { detail: String },
}

pub(super) fn ensure_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS gateway_outbound (
           obligation_id TEXT PRIMARY KEY,
           message_id TEXT NOT NULL,
           outbound_id TEXT NOT NULL,
           channel TEXT NOT NULL,
           target TEXT NOT NULL,
           body TEXT NOT NULL,
           idempotency_key TEXT NOT NULL UNIQUE,
           status TEXT NOT NULL CHECK(status IN
             ('pending','sending','delivered','ambiguous','abandoned')),
           attempts INTEGER NOT NULL DEFAULT 0,
           created_unix INTEGER NOT NULL,
           settled_unix INTEGER,
           lease_owner_id TEXT,
           lease_token TEXT,
           lease_deadline_unix INTEGER,
           active_attempt_id TEXT,
           provider_message_id TEXT,
           last_detail TEXT,
           UNIQUE(message_id,outbound_id),
           FOREIGN KEY(message_id) REFERENCES gateway_messages(id)
         );
         CREATE TABLE IF NOT EXISTS gateway_outbound_attempts (
           attempt_id TEXT PRIMARY KEY,
           obligation_id TEXT NOT NULL,
           attempt_no INTEGER NOT NULL,
           owner_id TEXT NOT NULL,
           lease_token TEXT NOT NULL,
           started_unix INTEGER NOT NULL,
           deadline_unix INTEGER NOT NULL,
           status TEXT NOT NULL CHECK(status IN
             ('in_flight','delivered','ambiguous','failed','expired')),
           settled_unix INTEGER,
           detail TEXT,
           FOREIGN KEY(obligation_id) REFERENCES gateway_outbound(obligation_id)
         );
         CREATE INDEX IF NOT EXISTS idx_gateway_outbound_claimable
           ON gateway_outbound(status,channel,created_unix);",
    )?;
    Ok(())
}

/// Record the obligation created by a successful turn, inside that turn's transaction.
///
/// Called only from `commit_claim`. Atomicity with the terminal write is the
/// whole point: an obligation written afterwards could be lost by a crash in
/// between, which is the hole this ledger exists to close.
///
/// An outbound with no `session_id` has no routing address, so no external party
/// can be owed anything and no obligation is recorded.
pub(super) fn record_obligation(
    transaction: &Transaction<'_>,
    outbound: &OutboundMessage,
    now: u64,
) -> Result<()> {
    let Some(target) = outbound.session_id.as_deref().filter(|id| !id.is_empty()) else {
        return Ok(());
    };
    let idempotency_key = format!(
        "{}:{}:{}",
        outbound.channel, outbound.in_reply_to, outbound.id
    );
    // INSERT OR IGNORE, not INSERT: replaying a committed transaction must not
    // create a second obligation for the same reply.
    transaction.execute(
        "INSERT OR IGNORE INTO gateway_outbound(
           obligation_id,message_id,outbound_id,channel,target,body,
           idempotency_key,status,attempts,created_unix
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,'pending',0,?8)",
        params![
            Uuid::new_v4().to_string(),
            outbound.in_reply_to,
            outbound.id,
            outbound.channel,
            target,
            outbound.text,
            idempotency_key,
            now as i64
        ],
    )?;
    Ok(())
}

fn read_obligation(
    connection: &Connection,
    obligation_id: &str,
) -> Result<Option<OutboundObligation>> {
    connection
        .query_row(
            "SELECT obligation_id,message_id,outbound_id,channel,target,body,idempotency_key,
                    status,attempts,created_unix,settled_unix,provider_message_id,last_detail
             FROM gateway_outbound WHERE obligation_id=?1",
            params![obligation_id],
            row_to_obligation,
        )
        .optional()
        .map_err(GatewayError::Sqlite)
}

fn row_to_obligation(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutboundObligation> {
    Ok(OutboundObligation {
        obligation_id: row.get(0)?,
        message_id: row.get(1)?,
        outbound_id: row.get(2)?,
        channel: row.get(3)?,
        target: row.get(4)?,
        body: row.get(5)?,
        idempotency_key: row.get(6)?,
        status: row.get(7)?,
        attempts: row.get::<_, i64>(8)?.max(0) as u64,
        created_unix: row.get::<_, i64>(9)?.max(0) as u64,
        settled_unix: row
            .get::<_, Option<i64>>(10)?
            .and_then(|v| u64::try_from(v).ok()),
        provider_message_id: row.get(11)?,
        last_detail: row.get(12)?,
    })
}

/// Promote sends whose lease died mid-flight to `ambiguous`.
///
/// The process holding the lease may have reached the platform before it went
/// away. Returning these to `pending` would be the duplicate this ledger
/// refuses to create, so they stop here and wait for a human.
fn expire_stale_sends(transaction: &Transaction<'_>, now: u64) -> Result<usize> {
    let stale: Vec<(String, Option<String>)> = {
        let mut statement = transaction.prepare(
            "SELECT obligation_id,active_attempt_id FROM gateway_outbound
             WHERE status='sending' AND lease_deadline_unix<=?1",
        )?;
        let rows = statement.query_map(params![now as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (obligation_id, attempt_id) in &stale {
        transaction.execute(
            "UPDATE gateway_outbound
             SET status='ambiguous',settled_unix=?1,
                 last_detail='lease expired while sending; outcome unknown',
                 lease_owner_id=NULL,lease_token=NULL,lease_deadline_unix=NULL,
                 active_attempt_id=NULL
             WHERE obligation_id=?2 AND status='sending'",
            params![now as i64, obligation_id],
        )?;
        if let Some(attempt_id) = attempt_id {
            transaction.execute(
                "UPDATE gateway_outbound_attempts
                 SET status='expired',settled_unix=?1,
                     detail='lease expired before the adapter settled it'
                 WHERE attempt_id=?2 AND status='in_flight'",
                params![now as i64, attempt_id],
            )?;
        }
    }
    Ok(stale.len())
}

/// Sweep dead in-flight sends into the ambiguity window without claiming anything.
///
/// Callers that only report state need this; [`claim_outbound`] does it too.
pub fn sweep_stale_sends(home: impl AsRef<Path>, now: u64) -> Result<usize> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let swept = expire_stale_sends(&transaction, now)?;
    transaction.commit()?;
    Ok(swept)
}

/// Lease the oldest owed send, recording the attempt before any network call.
///
/// `channel` restricts the claim to one adapter's obligations; `None` takes the
/// oldest of any channel.
pub fn claim_outbound(
    home: impl AsRef<Path>,
    channel: Option<&str>,
    owner_id: Uuid,
    now: u64,
    lease_secs: u64,
) -> Result<Option<OutboundClaim>> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    expire_stale_sends(&transaction, now)?;
    let selected = transaction
        .query_row(
            "SELECT obligation_id,message_id,outbound_id,channel,target,body,idempotency_key,
                    status,attempts,created_unix,settled_unix,provider_message_id,last_detail
             FROM gateway_outbound
             WHERE status='pending' AND (?1 IS NULL OR channel=?1)
             ORDER BY created_unix ASC,obligation_id ASC LIMIT 1",
            params![channel],
            row_to_obligation,
        )
        .optional()?;
    let Some(mut obligation) = selected else {
        transaction.commit()?;
        return Ok(None);
    };
    let attempt_no = obligation.attempts.saturating_add(1);
    let lease_token = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();
    let deadline = now.saturating_add(lease_secs.max(1));
    let changed = transaction.execute(
        "UPDATE gateway_outbound
         SET status='sending',attempts=?1,lease_owner_id=?2,lease_token=?3,
             lease_deadline_unix=?4,active_attempt_id=?5
         WHERE obligation_id=?6 AND status='pending'",
        params![
            attempt_no as i64,
            owner_id.to_string(),
            lease_token.to_string(),
            deadline as i64,
            attempt_id.to_string(),
            obligation.obligation_id
        ],
    )?;
    if changed != 1 {
        return Err(GatewayError::LeaseLost {
            message_id: obligation.message_id,
        });
    }
    transaction.execute(
        "INSERT INTO gateway_outbound_attempts(
           attempt_id,obligation_id,attempt_no,owner_id,lease_token,
           started_unix,deadline_unix,status
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,'in_flight')",
        params![
            attempt_id.to_string(),
            obligation.obligation_id,
            attempt_no as i64,
            owner_id.to_string(),
            lease_token.to_string(),
            now as i64,
            deadline as i64
        ],
    )?;
    transaction.commit()?;
    obligation.attempts = attempt_no;
    obligation.status = "sending".into();
    Ok(Some(OutboundClaim {
        obligation,
        owner_id,
        lease_token,
        attempt_id,
        attempt_no,
        deadline_unix: deadline,
    }))
}

/// Record what the platform said, and decide whether anything is still owed.
///
/// A settlement that no longer owns the lease is refused rather than applied:
/// the obligation has already moved on, and overwriting it would erase whatever
/// the sweep or another worker recorded.
pub fn settle_outbound(
    home: impl AsRef<Path>,
    claim: &OutboundClaim,
    settlement: OutboundSettlement,
    now: u64,
) -> Result<OutboundObligation> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (status, attempt_status, detail, provider_message_id) = match &settlement {
        OutboundSettlement::Delivered {
            provider_message_id,
        } => (
            "delivered",
            "delivered",
            "delivered".to_string(),
            Some(provider_message_id.clone()),
        ),
        OutboundSettlement::Ambiguous { detail } => {
            ("ambiguous", "ambiguous", detail.clone(), None)
        }
        OutboundSettlement::Failed { detail } => {
            // Retries are only safe because the platform refused this send.
            if claim.attempt_no >= MAX_SEND_ATTEMPTS {
                ("abandoned", "failed", detail.clone(), None)
            } else {
                ("pending", "failed", detail.clone(), None)
            }
        }
    };
    let settled_unix = (status != "pending").then_some(now as i64);
    let changed = transaction.execute(
        "UPDATE gateway_outbound
         SET status=?1,settled_unix=?2,provider_message_id=?3,last_detail=?4,
             lease_owner_id=NULL,lease_token=NULL,lease_deadline_unix=NULL,
             active_attempt_id=NULL
         WHERE obligation_id=?5 AND status='sending' AND lease_owner_id=?6
           AND lease_token=?7 AND active_attempt_id=?8",
        params![
            status,
            settled_unix,
            provider_message_id,
            detail,
            claim.obligation.obligation_id,
            claim.owner_id.to_string(),
            claim.lease_token.to_string(),
            claim.attempt_id.to_string()
        ],
    )?;
    if changed != 1 {
        return Err(GatewayError::LeaseLost {
            message_id: claim.obligation.message_id.clone(),
        });
    }
    transaction.execute(
        "UPDATE gateway_outbound_attempts
         SET status=?1,settled_unix=?2,detail=?3
         WHERE attempt_id=?4 AND status='in_flight'",
        params![
            attempt_status,
            now as i64,
            detail,
            claim.attempt_id.to_string()
        ],
    )?;
    project_terminal(
        &transaction,
        &claim.obligation.message_id,
        status,
        &detail,
        now,
    )?;
    transaction.commit()?;
    read_obligation(&connection, &claim.obligation.obligation_id)?
        .ok_or_else(|| GatewayError::Msg("settled obligation disappeared".into()))
}

/// Close an ambiguous obligation with what an operator actually observed.
pub fn resolve_ambiguous_obligation(
    home: impl AsRef<Path>,
    obligation_id: &str,
    resolution: AmbiguityResolution,
    now: u64,
) -> Result<Option<OutboundObligation>> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let message_id: Option<String> = transaction
        .query_row(
            "SELECT message_id FROM gateway_outbound
             WHERE obligation_id=?1 AND status='ambiguous'",
            params![obligation_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(message_id) = message_id else {
        transaction.commit()?;
        return Ok(None);
    };
    let (status, detail, provider_message_id) = match &resolution {
        AmbiguityResolution::Delivered {
            provider_message_id,
        } => (
            "delivered",
            "operator confirmed delivery".to_string(),
            Some(provider_message_id.clone()),
        ),
        // Re-armed rather than resent here: a human has established there is no
        // duplicate to create, so the ordinary claim path is safe again.
        AmbiguityResolution::NotDelivered => (
            "pending",
            "operator confirmed non-delivery; re-armed".to_string(),
            None,
        ),
        AmbiguityResolution::Abandon { detail } => ("abandoned", detail.clone(), None),
    };
    transaction.execute(
        "UPDATE gateway_outbound
         SET status=?1,settled_unix=?2,provider_message_id=?3,last_detail=?4
         WHERE obligation_id=?5 AND status='ambiguous'",
        params![
            status,
            (status != "pending").then_some(now as i64),
            provider_message_id,
            detail,
            obligation_id
        ],
    )?;
    project_terminal(&transaction, &message_id, status, &detail, now)?;
    transaction.commit()?;
    read_obligation(&connection, obligation_id)
}

/// Mirror obligation state onto the turn row the operator surfaces already read.
///
/// `gateway_messages` is authority for the turn, not for the send, so this only
/// ever writes the two columns that describe external delivery.
fn project_terminal(
    transaction: &Transaction<'_>,
    message_id: &str,
    status: &str,
    detail: &str,
    now: u64,
) -> Result<()> {
    match status {
        "delivered" => {
            transaction.execute(
                "UPDATE gateway_messages
                 SET delivered_unix=COALESCE(delivered_unix,?1) WHERE id=?2",
                params![now as i64, message_id],
            )?;
        }
        "abandoned" => {
            transaction.execute(
                "UPDATE gateway_messages SET terminal_reason=?1
                 WHERE id=?2 AND status='succeeded' AND delivered_unix IS NULL",
                params![format!("external_send_failed:{detail}"), message_id],
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn list_by_status(
    home: impl AsRef<Path>,
    predicate: &str,
    limit: usize,
) -> Result<Vec<OutboundObligation>> {
    let limit = limit.clamp(1, 500);
    let paths = GatewayPaths::open(home)?;
    let connection = open_database(&paths)?;
    let mut statement = connection.prepare(&format!(
        "SELECT obligation_id,message_id,outbound_id,channel,target,body,idempotency_key,
                status,attempts,created_unix,settled_unix,provider_message_id,last_detail
         FROM gateway_outbound WHERE {predicate}
         ORDER BY created_unix ASC,obligation_id ASC LIMIT ?1"
    ))?;
    let rows = statement.query_map(params![limit as i64], row_to_obligation)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Sends that are owed and safe to attempt.
pub fn list_pending_obligations(
    home: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<OutboundObligation>> {
    list_by_status(home, "status='pending'", limit)
}

/// Sends whose outcome nobody knows. These never retry without a human.
pub fn list_ambiguous_obligations(
    home: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<OutboundObligation>> {
    list_by_status(home, "status='ambiguous'", limit)
}

/// Everything still owed or unresolved, for a doctor-style summary.
pub fn list_unsettled_obligations(
    home: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<OutboundObligation>> {
    list_by_status(home, "status IN ('pending','sending','ambiguous')", limit)
}

/// Per-status counts for the messaging/doctor surfaces.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundLedgerStatus {
    pub pending: usize,
    pub sending: usize,
    pub delivered: usize,
    pub ambiguous: usize,
    pub abandoned: usize,
}

pub fn outbound_ledger_status(home: impl AsRef<Path>) -> Result<OutboundLedgerStatus> {
    let paths = GatewayPaths::open(home)?;
    let mut connection = open_database(&paths)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    expire_stale_sends(&transaction, now_unix())?;
    transaction.commit()?;
    let mut status = OutboundLedgerStatus::default();
    let mut statement =
        connection.prepare("SELECT status,COUNT(*) FROM gateway_outbound GROUP BY status")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (name, count) = row?;
        let count = count.max(0) as usize;
        match name.as_str() {
            "pending" => status.pending = count,
            "sending" => status.sending = count,
            "delivered" => status.delivered = count,
            "ambiguous" => status.ambiguous = count,
            "abandoned" => status.abandoned = count,
            _ => {}
        }
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::outbound_receipts::{delivery_state, list_ambiguous_sends};
    use crate::gateway::{drain_one, enqueue};
    use tempfile::tempdir;

    /// Run one successful turn that has a routing address, yielding one obligation.
    fn owed_reply(home: &Path, text: &str, session: &str) -> OutboundObligation {
        enqueue(home, "telegram", text, "offline", Some(session)).unwrap();
        drain_one(home, |inbound| {
            Ok((
                format!("reply:{}", inbound.text),
                inbound.session_id.clone(),
            ))
        })
        .unwrap()
        .unwrap();
        list_pending_obligations(home, 10).unwrap().remove(0)
    }

    #[test]
    fn a_successful_turn_owes_a_send_before_anyone_tries_to_send_it() {
        let dir = tempdir().unwrap();
        let obligation = owed_reply(dir.path(), "hello", "telegram:42");

        assert_eq!(obligation.status, "pending");
        assert_eq!(obligation.attempts, 0);
        assert_eq!(obligation.channel, "telegram");
        assert_eq!(obligation.target, "telegram:42");
        assert_eq!(obligation.body, "reply:hello");
        assert_eq!(
            obligation.idempotency_key,
            format!(
                "telegram:{}:{}",
                obligation.message_id, obligation.outbound_id
            )
        );
    }

    #[test]
    fn a_turn_with_no_routing_address_owes_nothing() {
        let dir = tempdir().unwrap();
        enqueue(dir.path(), "local", "hello", "offline", None).unwrap();
        drain_one(dir.path(), |_| Ok(("reply".into(), None)))
            .unwrap()
            .unwrap();

        assert!(list_unsettled_obligations(dir.path(), 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_failed_turn_owes_nothing() {
        let dir = tempdir().unwrap();
        enqueue(dir.path(), "telegram", "x", "offline", Some("telegram:1")).unwrap();
        drain_one(dir.path(), |_| Err("provider_unavailable".into()))
            .unwrap()
            .unwrap();

        assert!(list_unsettled_obligations(dir.path(), 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_obligation_carries_the_whole_reply_not_the_preview() {
        let dir = tempdir().unwrap();
        let long = "x".repeat(5_000);
        enqueue(
            dir.path(),
            "telegram",
            "long",
            "offline",
            Some("telegram:7"),
        )
        .unwrap();
        let drained = drain_one(dir.path(), |inbound| {
            Ok((long.clone(), inbound.session_id.clone()))
        })
        .unwrap()
        .unwrap();

        // The DrainResult preview is lossy by design; the obligation is not.
        assert_eq!(drained.reply_preview.chars().count(), 200);
        assert_eq!(
            list_pending_obligations(dir.path(), 10).unwrap()[0].body,
            long
        );
    }

    #[test]
    fn claiming_records_the_attempt_and_only_one_worker_wins() {
        let dir = tempdir().unwrap();
        owed_reply(dir.path(), "hello", "telegram:42");

        let first = claim_outbound(dir.path(), Some("telegram"), Uuid::new_v4(), 10, 30).unwrap();
        let second = claim_outbound(dir.path(), Some("telegram"), Uuid::new_v4(), 10, 30).unwrap();

        let first = first.expect("first worker claims");
        assert!(second.is_none());
        assert_eq!(first.attempt_number(), 1);
        assert_eq!(first.obligation().status, "sending");
        assert_eq!(first.deadline_unix(), 40);
        assert!(list_pending_obligations(dir.path(), 10).unwrap().is_empty());
    }

    #[test]
    fn a_channel_filter_leaves_other_channels_owed() {
        let dir = tempdir().unwrap();
        owed_reply(dir.path(), "hello", "telegram:42");

        assert!(
            claim_outbound(dir.path(), Some("slack"), Uuid::new_v4(), 10, 30)
                .unwrap()
                .is_none()
        );
        assert_eq!(list_pending_obligations(dir.path(), 10).unwrap().len(), 1);
    }

    #[test]
    fn delivery_settles_the_obligation_and_the_turn_receipt() {
        let dir = tempdir().unwrap();
        let owed = owed_reply(dir.path(), "hello", "telegram:42");
        let claim = claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();

        let settled = settle_outbound(
            dir.path(),
            &claim,
            OutboundSettlement::Delivered {
                provider_message_id: "tg-9".into(),
            },
            11,
        )
        .unwrap();

        assert_eq!(settled.status, "delivered");
        assert_eq!(settled.provider_message_id.as_deref(), Some("tg-9"));
        assert_eq!(settled.settled_unix, Some(11));
        // The legacy operator view is a projection of this, so it must agree.
        assert_eq!(
            delivery_state(dir.path(), &owed.message_id)
                .unwrap()
                .unwrap()
                .1,
            Some(11)
        );
        assert!(list_ambiguous_sends(dir.path(), 10).unwrap().is_empty());
    }

    #[test]
    fn a_definite_failure_retries_to_a_bound_then_abandons() {
        let dir = tempdir().unwrap();
        let owed = owed_reply(dir.path(), "hello", "telegram:42");

        for attempt in 1..=MAX_SEND_ATTEMPTS {
            let claim = claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 30)
                .unwrap()
                .expect("still owed");
            assert_eq!(claim.attempt_number(), attempt);
            let settled = settle_outbound(
                dir.path(),
                &claim,
                OutboundSettlement::Failed {
                    detail: "chat_not_found".into(),
                },
                11,
            )
            .unwrap();
            let expected = if attempt == MAX_SEND_ATTEMPTS {
                "abandoned"
            } else {
                "pending"
            };
            assert_eq!(settled.status, expected);
        }

        assert!(claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 30)
            .unwrap()
            .is_none());
        // An abandoned send is a definite non-delivery, so it is not ambiguous.
        assert!(list_ambiguous_sends(dir.path(), 10).unwrap().is_empty());
        assert_eq!(
            delivery_state(dir.path(), &owed.message_id)
                .unwrap()
                .unwrap()
                .0
                .as_deref(),
            Some("external_send_failed:chat_not_found")
        );
    }

    #[test]
    fn an_ambiguous_send_never_retries_on_its_own() {
        let dir = tempdir().unwrap();
        owed_reply(dir.path(), "hello", "telegram:42");
        let claim = claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();

        let settled = settle_outbound(
            dir.path(),
            &claim,
            OutboundSettlement::Ambiguous {
                detail: "read timeout".into(),
            },
            11,
        )
        .unwrap();

        assert_eq!(settled.status, "ambiguous");
        assert!(claim_outbound(dir.path(), None, Uuid::new_v4(), 12, 30)
            .unwrap()
            .is_none());
        assert_eq!(list_ambiguous_obligations(dir.path(), 10).unwrap().len(), 1);
    }

    #[test]
    fn a_send_whose_worker_died_becomes_ambiguous_not_pending() {
        let dir = tempdir().unwrap();
        owed_reply(dir.path(), "hello", "telegram:42");
        let claim = claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 5)
            .unwrap()
            .unwrap();
        // The worker never settles: it reached the platform, or it did not.

        assert_eq!(sweep_stale_sends(dir.path(), 16).unwrap(), 1);

        let stranded = list_ambiguous_obligations(dir.path(), 10).unwrap();
        assert_eq!(stranded.len(), 1);
        assert!(stranded[0]
            .last_detail
            .as_deref()
            .unwrap()
            .contains("outcome unknown"));
        assert!(claim_outbound(dir.path(), None, Uuid::new_v4(), 17, 30)
            .unwrap()
            .is_none());
        // The dead worker cannot settle it afterwards.
        assert!(matches!(
            settle_outbound(
                dir.path(),
                &claim,
                OutboundSettlement::Delivered {
                    provider_message_id: "tg-late".into()
                },
                18
            ),
            Err(GatewayError::LeaseLost { .. })
        ));
    }

    #[test]
    fn an_operator_can_confirm_an_ambiguous_send_arrived() {
        let dir = tempdir().unwrap();
        let owed = owed_reply(dir.path(), "hello", "telegram:42");
        let claim = claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();
        settle_outbound(
            dir.path(),
            &claim,
            OutboundSettlement::Ambiguous {
                detail: "timeout".into(),
            },
            11,
        )
        .unwrap();

        let resolved = resolve_ambiguous_obligation(
            dir.path(),
            &claim.obligation().obligation_id,
            AmbiguityResolution::Delivered {
                provider_message_id: "tg-4".into(),
            },
            20,
        )
        .unwrap()
        .unwrap();

        assert_eq!(resolved.status, "delivered");
        assert_eq!(
            delivery_state(dir.path(), &owed.message_id)
                .unwrap()
                .unwrap()
                .1,
            Some(20)
        );
    }

    #[test]
    fn an_operator_can_re_arm_a_send_that_never_arrived() {
        let dir = tempdir().unwrap();
        owed_reply(dir.path(), "hello", "telegram:42");
        let claim = claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();
        settle_outbound(
            dir.path(),
            &claim,
            OutboundSettlement::Ambiguous {
                detail: "timeout".into(),
            },
            11,
        )
        .unwrap();

        let resolved = resolve_ambiguous_obligation(
            dir.path(),
            &claim.obligation().obligation_id,
            AmbiguityResolution::NotDelivered,
            20,
        )
        .unwrap()
        .unwrap();

        assert_eq!(resolved.status, "pending");
        let retry = claim_outbound(dir.path(), None, Uuid::new_v4(), 21, 30)
            .unwrap()
            .expect("re-armed obligation is claimable");
        // The retry is the same obligation, so a dedupe-capable transport sees one key.
        assert_eq!(
            retry.obligation().idempotency_key,
            claim.obligation().idempotency_key
        );
        assert_eq!(retry.attempt_number(), 2);
    }

    #[test]
    fn resolving_something_that_is_not_ambiguous_changes_nothing() {
        let dir = tempdir().unwrap();
        let owed = owed_reply(dir.path(), "hello", "telegram:42");

        assert!(resolve_ambiguous_obligation(
            dir.path(),
            &owed.obligation_id,
            AmbiguityResolution::NotDelivered,
            20
        )
        .unwrap()
        .is_none());
        assert_eq!(
            list_pending_obligations(dir.path(), 10).unwrap()[0].status,
            "pending"
        );
    }

    #[test]
    fn status_counts_every_state_and_sweeps_first() {
        let dir = tempdir().unwrap();
        owed_reply(dir.path(), "one", "telegram:1");
        owed_reply(dir.path(), "two", "telegram:2");
        let claim = claim_outbound(dir.path(), None, Uuid::new_v4(), 10, 30)
            .unwrap()
            .unwrap();
        settle_outbound(
            dir.path(),
            &claim,
            OutboundSettlement::Delivered {
                provider_message_id: "tg-1".into(),
            },
            11,
        )
        .unwrap();
        // Leave the second one stranded in flight with a lease that has expired.
        claim_outbound(dir.path(), None, Uuid::new_v4(), 12, 1).unwrap();

        let status = outbound_ledger_status(dir.path()).unwrap();

        assert_eq!(status.delivered, 1);
        assert_eq!(status.ambiguous, 1);
        assert_eq!(status.sending, 0);
        assert_eq!(status.pending, 0);
    }
}
