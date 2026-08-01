//! Turn-level delivery receipts: the read side of the outbound ledger.
//!
//! These read `gateway_messages` rather than the obligation tables. They predate
//! [`super::outbound_ledger`] and are what the CLI, host messaging, and eval
//! surfaces already call, so they keep that shape; the ledger maintains the two
//! columns they depend on (`delivered_unix`, `terminal_reason`) as a projection
//! of obligation state.
//!
//! The resolution difference matters. [`list_ambiguous_sends`] is the coarse
//! view — every succeeded turn without a receipt — while
//! [`super::outbound_ledger::list_ambiguous_obligations`] distinguishes
//! never-attempted from attempted-outcome-unknown, which is the distinction that
//! decides whether retrying is safe. Both views agree on one point: a turn whose
//! send is still `pending` or `sending` is owed work with a known position, not
//! an unanswerable question, so it is excluded here.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{open_database, GatewayError, GatewayPaths, OutboundMessage, Result};

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
    let still_owed = message_ids_still_owed(&connection)?;
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
        let ambiguous_send = is_ambiguous_receipt(
            &terminal_status,
            delivered_unix,
            &terminal_reason,
            still_owed.contains(&message_id),
        );
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

/// Message ids whose send is still owed and workable (`pending` or `sending`).
///
/// Ambiguity means *nobody can say whether the platform got it*. A send that is
/// still queued or actively leased is not that: someone is going to try, and the
/// ledger knows exactly where it stands. Only turns the ledger has no answer for
/// belong in the operator's ambiguous pile.
fn message_ids_still_owed(connection: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut statement = connection.prepare(
        "SELECT DISTINCT message_id FROM gateway_outbound WHERE status IN ('pending','sending')",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut owed = std::collections::HashSet::new();
    for row in rows {
        owed.insert(row?);
    }
    Ok(owed)
}

fn is_ambiguous_receipt(
    terminal_status: &str,
    delivered_unix: Option<u64>,
    terminal_reason: &Option<String>,
    still_owed: bool,
) -> bool {
    if terminal_status != "succeeded" || delivered_unix.is_some() || still_owed {
        return false;
    }
    !matches!(
        terminal_reason.as_deref(),
        Some(reason)
            if reason == "external_send_failed"
                || reason.starts_with("external_send_failed:")
                || reason == "cancelled"
                || reason == "dead_lettered"
    )
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
           AND NOT EXISTS (SELECT 1 FROM gateway_outbound o
                           WHERE o.message_id=gateway_messages.id
                             AND o.status IN ('pending','sending'))
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
