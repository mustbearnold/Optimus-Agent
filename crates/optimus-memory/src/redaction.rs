//! The three ways a claim stops being readable: forgotten, erased, expired.
//!
//! These are grouped because they share one obligation that the rest of the
//! crate does not have. Every other write *adds* a version; these three take
//! content away, and anything derived from that content has to go with it in
//! the same transaction. `privacy_erase` in particular exists to overwrite a
//! person's words, so a copy of those words surviving in a search index is the
//! exact failure the method was written to prevent (ADR-0072).
//!
//! Split out of `lib.rs` unchanged; the only addition is the `unindex_claim`
//! call each one now makes inside its existing transaction.

use rusqlite::params;
use uuid::Uuid;

use crate::{
    append_ledger_on, text_recall, Memory, MemoryError, Result, Sensitivity, WriteContext,
};

impl Memory {
    pub fn tombstone(&self, ctx: &WriteContext, id: Uuid) -> Result<bool> {
        let prior = self.get_claim(id)?;
        self.ensure_scope(ctx, &prior.scope)?;
        self.ensure_sensitivity(ctx, prior.sensitivity)?;
        if prior.tombstoned_at.is_some() || prior.erased {
            return Ok(false);
        }
        let at = self.clock.now();
        let transaction = self.conn.unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE claims SET tombstoned_at=?1,in_conflict=0
             WHERE id=?2 AND tombstoned_at IS NULL AND erased=0",
            params![at, id.to_string()],
        )?;
        if changed == 1 {
            text_recall::unindex_claim(&transaction, id)?;
            append_ledger_on(
                &transaction,
                ctx,
                "claim_tombstoned",
                &serde_json::json!({"id": id}),
                &self.clock.now(),
            )?;
        }
        transaction.commit()?;
        Ok(changed == 1)
    }

    pub fn privacy_erase(&self, ctx: &WriteContext, id: Uuid) -> Result<bool> {
        let prior = self.get_claim(id)?;
        self.ensure_scope(ctx, &prior.scope)?;
        self.ensure_sensitivity(ctx, prior.sensitivity)?;
        if prior.erased {
            return Ok(false);
        }
        let at = self.clock.now();
        let transaction = self.conn.unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE claims SET
                subject='[erased]',predicate='[erased]',object='[erased]',
                valid_from='[erased]',valid_to=NULL,tx_to=NULL,
                allowed_uses_json='[]',supersedes=NULL,in_conflict=0,
                retention_until=NULL,tombstoned_at=?1,erased=1
             WHERE id=?2 AND erased=0",
            params![at, id.to_string()],
        )?;
        if changed == 1 {
            // The row's own text is now `[erased]`; the index still holds the
            // original. Overwriting one copy and leaving the other is not an
            // erasure.
            text_recall::unindex_claim(&transaction, id)?;
            append_ledger_on(
                &transaction,
                ctx,
                "claim_erased",
                &serde_json::json!({"id": id}),
                &self.clock.now(),
            )?;
        }
        transaction.commit()?;
        Ok(changed == 1)
    }

    pub fn apply_retention(&self, ctx: &WriteContext, as_of: &str) -> Result<usize> {
        if as_of.trim().is_empty() {
            return Err(MemoryError::Invariant(
                "retention evaluation time cannot be empty".into(),
            ));
        }
        let mut statement = self.conn.prepare(
            "SELECT id,sensitivity FROM claims
             WHERE tenant=?1 AND user_id=?2 AND project=?3
               AND retention_until IS NOT NULL AND retention_until <= ?4
               AND tombstoned_at IS NULL AND erased=0
             ORDER BY id",
        )?;
        let rows = statement
            .query_map(params![ctx.tenant, ctx.user, ctx.project, as_of], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
        let mut ids = Vec::new();
        for row in rows {
            let (id, sensitivity) = row?;
            let sensitivity: Sensitivity =
                serde_json::from_value(serde_json::Value::String(sensitivity))?;
            if sensitivity <= ctx.max_sensitivity {
                ids.push(id);
            }
        }
        drop(statement);
        if ids.is_empty() {
            return Ok(0);
        }
        let transaction = self.conn.unchecked_transaction()?;
        let mut changed = 0usize;
        for id in &ids {
            let removed = transaction.execute(
                "UPDATE claims SET tombstoned_at=?1,in_conflict=0
                 WHERE id=?2 AND tombstoned_at IS NULL AND erased=0",
                params![as_of, id],
            )?;
            if removed == 1 {
                text_recall::unindex_claim_raw(&transaction, id)?;
            }
            changed += removed;
        }
        if changed > 0 {
            append_ledger_on(
                &transaction,
                ctx,
                "retention_applied",
                &serde_json::json!({"as_of": as_of, "claim_ids": ids, "count": changed}),
                &self.clock.now(),
            )?;
        }
        transaction.commit()?;
        Ok(changed)
    }
}
