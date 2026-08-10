//! Session-scoped capability consent (spec-014 R7, ADR-0081).
//!
//! The `capability_grants` table keys each consent on the durable transcript
//! session id, the capability, the `CommandClass` discriminator, and the
//! workspace scope sha256. Liveness is bounded by an 8 h TTL (hard cap 24 h)
//! and soft revocation; rows survive revocation so the consent history stays
//! auditable.
//!
//! Split out of `lib.rs` to keep the store within its module-size baseline.

use rusqlite::{params, OptionalExtension};

use crate::{Store, StoreError};

/// One exact-effect audit row (ADR-0044 law 6 evidence).
#[derive(Debug, Clone)]
pub struct ActionApprovalRow {
    pub id: String,
    pub job_id: String,
    pub node_id: String,
    pub effect_hash: String,
    pub actor: String,
    pub decision: String,
    pub created_unix: u64,
    pub expires_unix: u64,
    pub reason: Option<String>,
}

/// A durable session-scoped capability consent (spec-014 R7, ADR-0081).
///
/// Keyed (durable transcript session id, capability, CommandClass
/// discriminator, scope_sha256). Expiry and soft revocation bound liveness;
/// the row survives revocation so the consent history stays auditable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityGrantRow {
    pub session_id: String,
    pub capability: String,
    pub command_class: String,
    pub scope_sha256: String,
    pub created_unix: u64,
    pub expires_unix: u64,
    pub revoked_unix: Option<u64>,
    pub revoked_by: Option<String>,
}

impl Store {
    /// Exact-effect audit rows, newest first (ADR-0044 law 6 evidence).
    pub fn list_action_approvals(
        &self,
        limit: usize,
    ) -> Result<Vec<ActionApprovalRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id,job_id,node_id,effect_hash,actor,decision,created_unix,expires_unix,reason
             FROM action_approvals
             ORDER BY created_unix DESC, rowid DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ActionApprovalRow {
                id: row.get(0)?,
                job_id: row.get(1)?,
                node_id: row.get(2)?,
                effect_hash: row.get(3)?,
                actor: row.get(4)?,
                decision: row.get(5)?,
                created_unix: row.get(6)?,
                expires_unix: row.get(7)?,
                reason: row.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Default session-consent TTL: 8 h (spec-014 R7, ADR-0081).
    pub const CONSENT_TTL_DEFAULT_SECS: u64 = 8 * 3600;
    /// Hard cap: 24 h.
    pub const CONSENT_TTL_MAX_SECS: u64 = 24 * 3600;

    /// Grant (or renew) a session-scoped capability consent.
    ///
    /// Upsert semantics: one live row per (session, capability, class,
    /// scope); a revoked row is un-revoked and its expiry renewed. The TTL is
    /// clamped to the [8 h, 24 h] window; `0` means the default 8 h.
    pub fn grant_capability(
        &self,
        session_id: &str,
        capability: &str,
        command_class: &str,
        scope_sha256: &str,
        ttl_secs: u64,
        now_unix: u64,
    ) -> Result<CapabilityGrantRow, StoreError> {
        if session_id.is_empty() || capability.is_empty() || command_class.is_empty() {
            return Err(StoreError::Invariant(format!(
                "consent key parts must be non-empty, got session={session_id:?} capability={capability:?} class={command_class:?}"
            )));
        }
        if !optimus_crypto::is_sha256_hex(scope_sha256) {
            return Err(StoreError::Invariant(format!(
                "consent scope must be a sha256 hex digest, got {scope_sha256:?}"
            )));
        }
        let ttl = if ttl_secs == 0 {
            Self::CONSENT_TTL_DEFAULT_SECS
        } else {
            ttl_secs.clamp(Self::CONSENT_TTL_DEFAULT_SECS, Self::CONSENT_TTL_MAX_SECS)
        };
        let expires_unix = now_unix.saturating_add(ttl);
        self.conn.execute(
            "INSERT INTO capability_grants
                (id, session_id, capability, command_class, scope_sha256, created_unix, expires_unix, revoked_unix, revoked_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)
             ON CONFLICT(session_id, capability, command_class, scope_sha256)
             DO UPDATE SET
                created_unix = ?6,
                expires_unix = ?7,
                revoked_unix = NULL,
                revoked_by = NULL",
            params![
                uuid::Uuid::new_v4().to_string(),
                session_id,
                capability,
                command_class,
                scope_sha256,
                now_unix as i64,
                expires_unix as i64
            ],
        )?;
        Ok(CapabilityGrantRow {
            session_id: session_id.to_string(),
            capability: capability.to_string(),
            command_class: command_class.to_string(),
            scope_sha256: scope_sha256.to_string(),
            created_unix: now_unix,
            expires_unix,
            revoked_unix: None,
            revoked_by: None,
        })
    }

    /// The live consent for a key, if any (unexpired and unrevoked).
    pub fn live_capability_grant(
        &self,
        session_id: &str,
        capability: &str,
        command_class: &str,
        scope_sha256: &str,
        now_unix: u64,
    ) -> Result<Option<CapabilityGrantRow>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT session_id, capability, command_class, scope_sha256,
                        created_unix, expires_unix, revoked_unix, revoked_by
                 FROM capability_grants
                 WHERE session_id = ?1 AND capability = ?2 AND command_class = ?3
                   AND scope_sha256 = ?4 AND expires_unix > ?5 AND revoked_unix IS NULL",
                params![
                    session_id,
                    capability,
                    command_class,
                    scope_sha256,
                    now_unix as i64
                ],
                capability_grant_from_row,
            )
            .optional()?;
        Ok(row)
    }

    /// Soft-revoke one consent key. Returns whether a live row was flipped.
    #[allow(clippy::too_many_arguments)]
    pub fn revoke_capability(
        &self,
        session_id: &str,
        capability: &str,
        command_class: &str,
        scope_sha256: &str,
        revoked_by: &str,
        now_unix: u64,
        reason: &str,
    ) -> Result<bool, StoreError> {
        let changed = self.conn.execute(
            "UPDATE capability_grants
             SET revoked_unix = ?5, revoked_by = ?6, reason = ?7
             WHERE session_id = ?1 AND capability = ?2 AND command_class = ?3
               AND scope_sha256 = ?4 AND revoked_unix IS NULL",
            params![
                session_id,
                capability,
                command_class,
                scope_sha256,
                now_unix as i64,
                revoked_by,
                reason
            ],
        )?;
        Ok(changed > 0)
    }

    /// Soft-revoke every live consent for a session (the settings
    /// affordance). Returns the number of rows flipped.
    pub fn revoke_session_capabilities(
        &self,
        session_id: &str,
        revoked_by: &str,
        now_unix: u64,
        reason: &str,
    ) -> Result<u64, StoreError> {
        let changed = self.conn.execute(
            "UPDATE capability_grants
             SET revoked_unix = ?2, revoked_by = ?3, reason = ?4
             WHERE session_id = ?1 AND revoked_unix IS NULL",
            params![session_id, now_unix as i64, revoked_by, reason],
        )?;
        Ok(changed as u64)
    }

    /// All consent rows for a session (live and revoked), newest first.
    pub fn list_capability_grants(
        &self,
        session_id: &str,
    ) -> Result<Vec<CapabilityGrantRow>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, capability, command_class, scope_sha256,
                    created_unix, expires_unix, revoked_unix, revoked_by
             FROM capability_grants
             WHERE session_id = ?1
             ORDER BY created_unix DESC, rowid DESC",
        )?;
        let rows = stmt.query_map(params![session_id], capability_grant_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn capability_grant_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CapabilityGrantRow> {
    Ok(CapabilityGrantRow {
        session_id: row.get(0)?,
        capability: row.get(1)?,
        command_class: row.get(2)?,
        scope_sha256: row.get(3)?,
        created_unix: row.get::<_, i64>(4)? as u64,
        expires_unix: row.get::<_, i64>(5)? as u64,
        revoked_unix: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
        revoked_by: row.get(7)?,
    })
}
