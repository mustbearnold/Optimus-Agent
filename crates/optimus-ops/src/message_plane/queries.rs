//! Query-side of the message plane (spec-025): delivery, policy state
//! transitions, inbox/outbox/thread views, events, and the live registry.
//!
//! Split out of `message_plane.rs` under the module-size ratchet
//! (ADR-0049); `impl MessageStore` in a child module has the same privacy
//! access as the parent (fields are visible to descendant modules).

use rusqlite::params;
use uuid::Uuid;

use super::Result;

use super::{
    now_stamp, parse_uuid, stamp_unix_secs, MessageClassification, MessageError, MessageEvent,
    MessageKind, MessageMode, MessageState, MessageStore, SessionMessage,
};

impl MessageStore {
    /// Deliver every queued message addressed to `session_id`
    /// (`queued -> delivered`), recording one delivered event each.
    /// Called when the session opens or starts a turn (spec-025 R1, A2).
    pub fn deliver_inbox(&self, session_id: Uuid) -> Result<usize> {
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM session_messages
                 WHERE to_session = ?1 AND state = 'queued'",
            )?;
            let rows = stmt.query_map(params![session_id.to_string()], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<String>, _>>()?
        };
        let mut delivered = 0;
        for id in ids {
            let uuid = Uuid::parse_str(&id).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
            })?;
            let Some(mut message) = self.load(uuid)? else {
                continue;
            };
            if message.state == MessageState::Queued {
                self.set_state(&mut message, MessageState::Delivered)?;
                delivered += 1;
            }
        }
        Ok(delivered)
    }

    /// Move a held/queued message to `delivered` (auto-accept path or
    /// hold-approval approval), recording the event. Rejects terminal states.
    pub fn deliver_message(&self, id: Uuid) -> Result<SessionMessage> {
        let Some(mut message) = self.load(id)? else {
            return Err(MessageError::NotFound(id));
        };
        if message.state == MessageState::Delivered {
            return Ok(message);
        }
        if !matches!(
            message.state,
            MessageState::Queued | MessageState::Held | MessageState::Approved
        ) {
            return Err(MessageError::AlreadyTerminal(
                id,
                message.state.as_str().into(),
            ));
        }
        self.set_state(&mut message, MessageState::Delivered)?;
        Ok(message)
    }

    /// Hold a message (hold-approval inbound policy), recording the event.
    pub fn hold(&self, id: Uuid) -> Result<SessionMessage> {
        let Some(mut message) = self.load(id)? else {
            return Err(MessageError::NotFound(id));
        };
        if message.state != MessageState::Queued {
            return Err(MessageError::AlreadyTerminal(
                id,
                message.state.as_str().into(),
            ));
        }
        self.set_state(&mut message, MessageState::Held)?;
        Ok(message)
    }

    /// Record a classification on a message (spec-025 R5). Immutable once
    /// set: the first classification wins and later calls are no-ops.
    pub fn classify(
        &self,
        id: Uuid,
        classification: MessageClassification,
    ) -> Result<SessionMessage> {
        let Some(mut message) = self.load(id)? else {
            return Err(MessageError::NotFound(id));
        };
        if message.classification.is_none() {
            message.classification = Some(classification);
            message.updated_at = now_stamp();
            self.conn.execute(
                "UPDATE session_messages SET classification = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.to_string(), classification.as_str(), message.updated_at],
            )?;
        }
        Ok(message)
    }

    /// Approve a held message: `held -> approved` (spec-025 R3/R5).
    pub fn approve(&self, id: Uuid) -> Result<SessionMessage> {
        let Some(mut message) = self.load(id)? else {
            return Err(MessageError::NotFound(id));
        };
        if message.state != MessageState::Held {
            return Err(MessageError::AlreadyTerminal(
                id,
                message.state.as_str().into(),
            ));
        }
        self.set_state(&mut message, MessageState::Approved)?;
        Ok(message)
    }

    /// Refuse a message (deny policy or operator deny): `queued/held ->
    /// refused`, terminal (spec-025 R3/R4).
    pub fn refuse(&self, id: Uuid) -> Result<SessionMessage> {
        let Some(mut message) = self.load(id)? else {
            return Err(MessageError::NotFound(id));
        };
        if !matches!(message.state, MessageState::Queued | MessageState::Held) {
            return Err(MessageError::AlreadyTerminal(
                id,
                message.state.as_str().into(),
            ));
        }
        self.set_state(&mut message, MessageState::Refused)?;
        Ok(message)
    }

    /// Expire held messages older than `expiry_seconds` (per-session default
    /// 30 min, spec-025 R3): `held -> expired`, terminal. Returns the
    /// expired message ids.
    pub fn expire_held(&self, session_id: Uuid, expiry_seconds: u64) -> Result<Vec<Uuid>> {
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM session_messages WHERE to_session = ?1 AND state = 'held'",
            )?;
            let rows = stmt.query_map(params![session_id.to_string()], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<String>, _>>()?
        };
        let mut expired = Vec::new();
        for id in ids {
            let uuid = Uuid::parse_str(&id).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
            })?;
            let Some(mut message) = self.load(uuid)? else {
                continue;
            };
            if message.state != MessageState::Held {
                continue;
            }
            let created = stamp_unix_secs(&message.created_at).unwrap_or(0);
            let now = stamp_unix_secs(&now_stamp()).unwrap_or(0);
            if now.saturating_sub(created) >= expiry_seconds {
                self.set_state(&mut message, MessageState::Expired)?;
                expired.push(uuid);
            }
        }
        Ok(expired)
    }

    /// Fail a message (store/transport failure): terminal `failed`
    /// (spec-025 R4; the sender's error is `session_send_failed`).
    pub fn fail(&self, id: Uuid) -> Result<SessionMessage> {
        let Some(mut message) = self.load(id)? else {
            return Err(MessageError::NotFound(id));
        };
        if message.state.is_terminal() {
            return Err(MessageError::AlreadyTerminal(
                id,
                message.state.as_str().into(),
            ));
        }
        self.set_state(&mut message, MessageState::Failed)?;
        Ok(message)
    }

    /// Mark a delivered message as surfaced in the receiver's context
    /// (spec-025 R1; a message is injected at most once).
    pub fn mark_surfaced(&self, id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE session_messages SET surfaced_at = ?2 WHERE id = ?1 AND surfaced_at IS NULL",
            params![id.to_string(), now_stamp()],
        )?;
        Ok(())
    }

    /// Inbox: messages addressed to `session_id`, most recent first.
    pub fn inbox(&self, session_id: Uuid, limit: usize) -> Result<Vec<SessionMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_session, to_session, kind, payload, reply_to, mode,
                    machine_id, state, classification, created_at, updated_at,
                    delivered_at, surfaced_at
             FROM session_messages WHERE to_session = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id.to_string(), limit as i64], |row| {
            row_to_message(row)
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MessageError::Sqlite)
    }

    /// Outbox: messages sent by `session_id`, most recent first.
    pub fn outbox(&self, session_id: Uuid, limit: usize) -> Result<Vec<SessionMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_session, to_session, kind, payload, reply_to, mode,
                    machine_id, state, classification, created_at, updated_at,
                    delivered_at, surfaced_at
             FROM session_messages WHERE from_session = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![session_id.to_string(), limit as i64], |row| {
            row_to_message(row)
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MessageError::Sqlite)
    }

    /// Thread view: a message and every message that replies to it,
    /// transitively, in creation order (spec-025 R6).
    pub fn thread(&self, root_id: Uuid) -> Result<Vec<SessionMessage>> {
        let mut out = Vec::new();
        let mut frontier = vec![root_id];
        while let Some(id) = frontier.pop() {
            if let Some(message) = self.load(id)? {
                out.push(message);
            }
            let mut stmt = self.conn.prepare(
                "SELECT id FROM session_messages WHERE reply_to = ?1 ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map(params![id.to_string()], |row| row.get::<_, String>(0))?;
            for id_str in rows.flatten() {
                if let Ok(child) = Uuid::parse_str(&id_str) {
                    frontier.push(child);
                }
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(out)
    }

    /// Ordered lifecycle events for one message (spec-025 R7, A8).
    pub fn events(&self, message_id: Uuid) -> Result<Vec<MessageEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT message_id, event_type, recorded_at, sequence
             FROM session_message_events WHERE message_id = ?1
             ORDER BY sequence",
        )?;
        let rows = stmt.query_map(params![message_id.to_string()], |row| {
            Ok(MessageEvent {
                message_id: parse_uuid(&row.get::<_, String>(0)?, 0)?,
                event_type: row.get(1)?,
                recorded_at: row.get(2)?,
                sequence: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MessageError::Sqlite)
    }

    /// Record a bounded reply-wait outcome (spec-025 R6, A10): either a
    /// reply arrived or the wait expired (`reply_wait_expired`). Idempotent
    /// per (message, outcome).
    pub fn record_reply_wait(&self, message_id: Uuid, outcome: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO session_reply_waits(wait_id, message_id, outcome, recorded_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                message_id.to_string(),
                outcome,
                now_stamp()
            ],
        )?;
        Ok(())
    }

    /// Events for every message of a session (spec-025 R7).
    pub fn events_for_session(&self, session_id: Uuid) -> Result<Vec<MessageEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.message_id, e.event_type, e.recorded_at, e.sequence
             FROM session_message_events e
             JOIN session_messages m ON m.id = e.message_id
             WHERE m.to_session = ?1 OR m.from_session = ?1
             ORDER BY e.sequence",
        )?;
        let rows = stmt.query_map(params![session_id.to_string()], |row| {
            Ok(MessageEvent {
                message_id: parse_uuid(&row.get::<_, String>(0)?, 0)?,
                event_type: row.get(1)?,
                recorded_at: row.get(2)?,
                sequence: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MessageError::Sqlite)
    }
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionMessage> {
    let kind: String = row.get(3)?;
    let mode: String = row.get(6)?;
    let state: String = row.get(8)?;
    let classification: Option<String> = row.get(9)?;
    Ok(SessionMessage {
        id: parse_uuid(&row.get::<_, String>(0)?, 0)?,
        from_session: parse_uuid(&row.get::<_, String>(1)?, 1)?,
        to_session: parse_uuid(&row.get::<_, String>(2)?, 2)?,
        kind: MessageKind::parse(&kind)?,
        payload: row.get(4)?,
        reply_to: row
            .get::<_, Option<String>>(5)?
            .map(|v| parse_uuid(&v, 5))
            .transpose()?,
        mode: MessageMode::parse(&mode)?,
        machine_id: row.get(7)?,
        state: MessageState::parse(&state)?,
        classification: classification
            .as_deref()
            .map(|c| match c {
                "approved" => Ok(MessageClassification::Approved),
                "denied" => Ok(MessageClassification::Denied),
                "pending" => Ok(MessageClassification::Pending),
                other => Err(rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    format!("invalid classification: {other}").into(),
                )),
            })
            .transpose()?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        delivered_at: row.get(12)?,
        surfaced_at: row.get(13)?,
    })
}
