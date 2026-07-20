//! Evidence-native MetaMemory: ledger, bitemporal claims, EvidencePacket recall.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invariant: {0}")]
    Invariant(String),
    #[error("action authorize unsupported: memory never grants live capability")]
    ActionAuthorizeUnsupported,
    #[error("write denied: {0}")]
    WriteDenied(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

pub trait MemoryClock: Send + Sync {
    fn now(&self) -> String;
}

#[derive(Debug, Default)]
pub struct SystemMemoryClock {
    last_unix: AtomicU64,
}

impl MemoryClock for SystemMemoryClock {
    fn now(&self) -> String {
        let observed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let mut previous = self.last_unix.load(Ordering::Relaxed);
        loop {
            let next = observed.max(previous.saturating_add(1));
            match self.last_unix.compare_exchange_weak(
                previous,
                next,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return format_unix_rfc3339(next),
                Err(current) => previous = current,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustDomain {
    System,
    User,
    Tool,
    Untrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    UserStatement,
    ToolObservation,
    UntrustedContent,
    AgentInference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowedUse {
    Inform,
    Constraint,
    ProcedureLookup,
    /// Never granted by memory write gate in MVP.
    Action,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    #[default]
    Personal,
    Confidential,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallPurpose {
    Inform,
    Constraint,
    ProcedureLookup,
    ActionAuthorize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub tenant: String,
    pub user: String,
    pub agent: String,
    pub project: String,
}

#[derive(Debug, Clone)]
pub struct WriteContext {
    pub tenant: String,
    pub user: String,
    pub agent: String,
    pub project: String,
    pub principal: String,
    pub max_trust: TrustDomain,
    pub max_sensitivity: Sensitivity,
}

impl WriteContext {
    pub fn scope(&self) -> Scope {
        Scope {
            tenant: self.tenant.clone(),
            user: self.user.clone(),
            agent: self.agent.clone(),
            project: self.project.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimDraft {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub origin: Origin,
    /// Knowledge time; defaults to `valid_from` when omitted.
    pub learned_at: Option<String>,
    #[serde(default)]
    pub sensitivity: Sensitivity,
    pub retention_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub supersedes: Uuid,
    pub object: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub confidence: f64,
    pub origin: Origin,
    /// Transaction/knowledge time when the correction was learned.
    pub learned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallQuery {
    pub purpose: RecallPurpose,
    pub subject: Option<String>,
    pub predicate: Option<String>,
    pub as_of_valid: Option<String>,
    pub as_of_tx: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaimView {
    pub id: Uuid,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub tx_from: String,
    pub tx_to: Option<String>,
    pub confidence: f64,
    pub origin: Origin,
    pub trust: TrustDomain,
    pub allowed_uses: Vec<AllowedUse>,
    pub scope: Scope,
    pub supersedes: Option<Uuid>,
    pub in_conflict: bool,
    pub sensitivity: Sensitivity,
    pub retention_until: Option<String>,
    pub tombstoned_at: Option<String>,
    pub erased: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePacket {
    /// Stable fence label: recalled content is DATA, not instruction.
    pub fence: String,
    pub purpose: RecallPurpose,
    pub current: Vec<ClaimView>,
    pub historical: Vec<ClaimView>,
    pub conflicts: Vec<ClaimView>,
    pub citations: Vec<Uuid>,
    pub abstained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEvent {
    pub seq: i64,
    pub event_id: Uuid,
    pub kind: String,
    pub created_at: String,
}

pub struct Memory {
    conn: Connection,
    clock: Arc<dyn MemoryClock>,
}

impl Memory {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_clock(path, Arc::new(SystemMemoryClock::default()))
    }

    pub fn open_with_clock(path: impl AsRef<Path>, clock: Arc<dyn MemoryClock>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ledger (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                tenant TEXT NOT NULL DEFAULT '',
                user_id TEXT NOT NULL DEFAULT '',
                project TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS claims (
                id TEXT PRIMARY KEY NOT NULL,
                tenant TEXT NOT NULL,
                user_id TEXT NOT NULL,
                agent TEXT NOT NULL,
                project TEXT NOT NULL,
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT NOT NULL,
                valid_from TEXT NOT NULL,
                valid_to TEXT,
                tx_from TEXT NOT NULL,
                tx_to TEXT,
                confidence REAL NOT NULL,
                origin TEXT NOT NULL,
                trust TEXT NOT NULL,
                allowed_uses_json TEXT NOT NULL,
                principal TEXT NOT NULL,
                supersedes TEXT,
                in_conflict INTEGER NOT NULL DEFAULT 0,
                sensitivity TEXT NOT NULL DEFAULT 'personal',
                retention_until TEXT,
                tombstoned_at TEXT,
                erased INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_claims_scope_sp
                ON claims(tenant, user_id, project, subject, predicate);
            ",
        )?;
        ensure_column(
            &conn,
            "claims",
            "sensitivity",
            "TEXT NOT NULL DEFAULT 'personal'",
        )?;
        ensure_column(&conn, "claims", "retention_until", "TEXT")?;
        ensure_column(&conn, "claims", "tombstoned_at", "TEXT")?;
        ensure_column(&conn, "claims", "erased", "INTEGER NOT NULL DEFAULT 0")?;
        ensure_column(&conn, "ledger", "tenant", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "ledger", "user_id", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(&conn, "ledger", "project", "TEXT NOT NULL DEFAULT ''")?;
        conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', '2')",
            [],
        )?;
        Ok(Self { conn, clock })
    }

    pub fn remember(&self, ctx: &WriteContext, draft: ClaimDraft) -> Result<Uuid> {
        if draft.sensitivity > ctx.max_sensitivity {
            return Err(MemoryError::WriteDenied(
                "claim sensitivity exceeds principal clearance".into(),
            ));
        }
        let (trust, allowed) = write_gate(ctx, draft.origin)?;
        let id = Uuid::new_v4();
        let tx_from = draft.learned_at.clone().unwrap_or_else(|| self.clock.now());
        self.insert_claim(
            id,
            ctx,
            &draft.subject,
            &draft.predicate,
            &draft.object,
            &draft.valid_from,
            draft.valid_to.as_deref(),
            &tx_from,
            None,
            draft.confidence,
            draft.origin,
            trust,
            &allowed,
            draft.sensitivity,
            draft.retention_until.as_deref(),
            None,
        )?;
        self.append_ledger(
            ctx,
            "claim_remembered",
            &serde_json::json!({ "id": id, "subject": draft.subject, "predicate": draft.predicate }),
        )?;
        self.refresh_conflicts(ctx, &draft.subject, &draft.predicate)?;
        Ok(id)
    }

    pub fn correct(&self, ctx: &WriteContext, corr: Correction) -> Result<Uuid> {
        let prior = self.get_claim(corr.supersedes)?;
        self.ensure_scope(ctx, &prior.scope)?;
        if prior.tombstoned_at.is_some() || prior.erased {
            return Err(MemoryError::WriteDenied(
                "cannot correct tombstoned or erased claim".into(),
            ));
        }
        if prior.sensitivity > ctx.max_sensitivity {
            return Err(MemoryError::WriteDenied(
                "claim sensitivity exceeds principal clearance".into(),
            ));
        }
        let (trust, allowed) = write_gate(ctx, corr.origin)?;

        // Close the prior knowledge version at learned_at (do not rewrite pre-T views).
        self.conn.execute(
            "UPDATE claims SET tx_to = ?1 WHERE id = ?2 AND tx_to IS NULL",
            params![corr.learned_at, corr.supersedes.to_string()],
        )?;

        // Post-T snapshot of the old fact, valid only until correction.valid_from.
        let old_snapshot_id = Uuid::new_v4();
        self.insert_claim(
            old_snapshot_id,
            ctx,
            &prior.subject,
            &prior.predicate,
            &prior.object,
            &prior.valid_from,
            Some(&corr.valid_from),
            &corr.learned_at,
            None,
            prior.confidence,
            prior.origin,
            prior.trust,
            &prior.allowed_uses,
            prior.sensitivity,
            prior.retention_until.as_deref(),
            Some(corr.supersedes),
        )?;

        // New fact from valid_from onward, known from learned_at.
        let id = Uuid::new_v4();
        self.insert_claim(
            id,
            ctx,
            &prior.subject,
            &prior.predicate,
            &corr.object,
            &corr.valid_from,
            corr.valid_to.as_deref(),
            &corr.learned_at,
            None,
            corr.confidence,
            corr.origin,
            trust,
            &allowed,
            prior.sensitivity,
            prior.retention_until.as_deref(),
            Some(corr.supersedes),
        )?;
        self.append_ledger(
            ctx,
            "claim_corrected",
            &serde_json::json!({
                "id": id,
                "old_snapshot": old_snapshot_id,
                "supersedes": corr.supersedes,
                "object": corr.object,
                "valid_from": corr.valid_from,
                "learned_at": corr.learned_at,
            }),
        )?;
        self.refresh_conflicts(ctx, &prior.subject, &prior.predicate)?;
        Ok(id)
    }

    pub fn recall(&self, ctx: &WriteContext, query: RecallQuery) -> Result<EvidencePacket> {
        if matches!(query.purpose, RecallPurpose::ActionAuthorize) {
            return Err(MemoryError::ActionAuthorizeUnsupported);
        }

        let as_of_valid = query
            .as_of_valid
            .clone()
            .unwrap_or_else(|| "9999-12-31T23:59:59Z".into());
        let as_of_tx = query
            .as_of_tx
            .clone()
            .unwrap_or_else(|| "9999-12-31T23:59:59Z".into());
        let limit = query.limit.max(1) as usize;

        // Scope BEFORE candidate cap.
        let mut sql = String::from(
            "SELECT id, tenant, user_id, agent, project, subject, predicate, object,
                    valid_from, valid_to, tx_from, tx_to, confidence, origin, trust,
                    allowed_uses_json, supersedes, in_conflict, sensitivity,
                    retention_until, tombstoned_at, erased
             FROM claims
             WHERE tenant = ?1 AND user_id = ?2 AND project = ?3
               AND tx_from <= ?4
               AND (tx_to IS NULL OR tx_to > ?4)
               AND tombstoned_at IS NULL AND erased = 0",
        );
        let mut bind: Vec<String> = vec![
            ctx.tenant.clone(),
            ctx.user.clone(),
            ctx.project.clone(),
            as_of_tx.clone(),
        ];
        if let Some(ref s) = query.subject {
            sql.push_str(&format!(" AND subject = ?{}", bind.len() + 1));
            bind.push(s.clone());
        }
        if let Some(ref p) = query.predicate {
            sql.push_str(&format!(" AND predicate = ?{}", bind.len() + 1));
            bind.push(p.clone());
        }
        sql.push_str(" ORDER BY tx_from DESC, id DESC");

        let mut stmt = self.conn.prepare(&sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = bind
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt.query_map(params_ref.as_slice(), row_to_claim)?;
        let mut known: Vec<ClaimView> = Vec::new();
        for row in rows {
            let claim = row?;
            if claim.sensitivity <= ctx.max_sensitivity
                && claim
                    .allowed_uses
                    .contains(&purpose_allowed_use(query.purpose))
            {
                known.push(claim);
            }
        }

        let mut current = Vec::new();
        let mut historical = Vec::new();
        let mut conflicts = Vec::new();

        for claim in known {
            let in_valid = claim.valid_from.as_str() <= as_of_valid.as_str()
                && claim
                    .valid_to
                    .as_ref()
                    .map(|t| as_of_valid.as_str() < t.as_str())
                    .unwrap_or(true);
            if in_valid {
                current.push(claim);
            } else {
                historical.push(claim);
            }
        }

        // Prefer the latest knowledge version per (subject,predicate,object) within current.
        current = dedupe_latest(current);
        conflicts = dedupe_latest(conflicts);

        // Detect conflicts only among already authorized rows. Persisted conflict flags may
        // reflect higher-sensitivity claims and must not influence a lower-clearance recall.
        let mut by_key: std::collections::HashMap<(String, String), Vec<ClaimView>> =
            std::collections::HashMap::new();
        for c in current {
            by_key
                .entry((c.subject.clone(), c.predicate.clone()))
                .or_default()
                .push(c);
        }
        current = Vec::new();
        for (_k, group) in by_key {
            let objs: std::collections::HashSet<_> =
                group.iter().map(|c| c.object.clone()).collect();
            if objs.len() > 1 {
                conflicts.extend(group);
            } else {
                current.extend(group);
            }
        }
        conflicts = dedupe_latest(conflicts);

        if current.len() > limit {
            current.truncate(limit);
        }
        if conflicts.len() > limit {
            conflicts.truncate(limit);
        }

        let mut citations: Vec<Uuid> = current.iter().map(|c| c.id).collect();
        citations.extend(conflicts.iter().map(|c| c.id));
        citations.extend(historical.iter().take(limit).map(|c| c.id));

        let abstained = current.is_empty() && conflicts.is_empty();
        Ok(EvidencePacket {
            fence: "EVIDENCE_DATA_NOT_INSTRUCTION_NOT_CAPABILITY".into(),
            purpose: query.purpose,
            current,
            historical,
            conflicts,
            citations,
            abstained,
        })
    }

    pub fn get_claim(&self, id: Uuid) -> Result<ClaimView> {
        self.conn
            .query_row(
                "SELECT id, tenant, user_id, agent, project, subject, predicate, object,
                        valid_from, valid_to, tx_from, tx_to, confidence, origin, trust,
                        allowed_uses_json, supersedes, in_conflict, sensitivity,
                        retention_until, tombstoned_at, erased
                 FROM claims WHERE id = ?1",
                params![id.to_string()],
                row_to_claim,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    MemoryError::NotFound(format!("claim {id}"))
                }
                other => MemoryError::Sqlite(other),
            })
    }

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
            changed += transaction.execute(
                "UPDATE claims SET tombstoned_at=?1,in_conflict=0
                 WHERE id=?2 AND tombstoned_at IS NULL AND erased=0",
                params![as_of, id],
            )?;
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

    pub fn audit_events(&self, ctx: &WriteContext, limit: u32) -> Result<Vec<AuditEvent>> {
        let mut statement = self.conn.prepare(
            "SELECT seq,event_id,kind,created_at FROM ledger
             WHERE tenant=?1 AND user_id=?2 AND project=?3
             ORDER BY seq DESC LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![ctx.tenant, ctx.user, ctx.project, limit.clamp(1, 1000)],
            |row| {
                let raw: String = row.get(1)?;
                let event_id = Uuid::parse_str(&raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(AuditEvent {
                    seq: row.get(0)?,
                    event_id,
                    kind: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemoryError::Sqlite)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_claim(
        &self,
        id: Uuid,
        ctx: &WriteContext,
        subject: &str,
        predicate: &str,
        object: &str,
        valid_from: &str,
        valid_to: Option<&str>,
        tx_from: &str,
        tx_to: Option<&str>,
        confidence: f64,
        origin: Origin,
        trust: TrustDomain,
        allowed: &[AllowedUse],
        sensitivity: Sensitivity,
        retention_until: Option<&str>,
        supersedes: Option<Uuid>,
    ) -> Result<()> {
        let origin_s = enum_str(&origin)?;
        let trust_s = enum_str(&trust)?;
        let allowed_json = serde_json::to_string(allowed)?;
        let sensitivity = enum_str(&sensitivity)?;
        self.conn.execute(
            "INSERT INTO claims(
                id, tenant, user_id, agent, project, subject, predicate, object,
                valid_from, valid_to, tx_from, tx_to, confidence, origin, trust,
                allowed_uses_json, principal, supersedes, in_conflict,
                sensitivity, retention_until, tombstoned_at, erased
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,0,?19,?20,NULL,0)",
            params![
                id.to_string(),
                ctx.tenant,
                ctx.user,
                ctx.agent,
                ctx.project,
                subject,
                predicate,
                object,
                valid_from,
                valid_to,
                tx_from,
                tx_to,
                confidence,
                origin_s,
                trust_s,
                allowed_json,
                ctx.principal,
                supersedes.map(|u| u.to_string()),
                sensitivity,
                retention_until,
            ],
        )?;
        Ok(())
    }

    fn refresh_conflicts(&self, ctx: &WriteContext, subject: &str, predicate: &str) -> Result<()> {
        // Mark open knowledge+valid claims that disagree on object.
        let mut stmt = self.conn.prepare(
            "SELECT id, object FROM claims
             WHERE tenant = ?1 AND user_id = ?2 AND project = ?3
               AND subject = ?4 AND predicate = ?5
               AND tx_to IS NULL AND valid_to IS NULL",
        )?;
        let rows = stmt.query_map(
            params![ctx.tenant, ctx.user, ctx.project, subject, predicate],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        let distinct_objects: std::collections::HashSet<_> =
            items.iter().map(|(_, o)| o.clone()).collect();
        let conflicted = distinct_objects.len() > 1;
        for (id, _) in &items {
            self.conn.execute(
                "UPDATE claims SET in_conflict = ?1 WHERE id = ?2",
                params![if conflicted { 1 } else { 0 }, id],
            )?;
        }
        if conflicted {
            self.append_ledger(
                ctx,
                "claims_conflicted",
                &serde_json::json!({
                    "subject": subject,
                    "predicate": predicate,
                    "objects": distinct_objects.into_iter().collect::<Vec<_>>(),
                }),
            )?;
        }
        Ok(())
    }

    fn append_ledger(
        &self,
        ctx: &WriteContext,
        kind: &str,
        payload: &impl Serialize,
    ) -> Result<()> {
        append_ledger_on(&self.conn, ctx, kind, payload, &self.clock.now())
    }

    fn ensure_scope(&self, ctx: &WriteContext, scope: &Scope) -> Result<()> {
        if scope.tenant != ctx.tenant || scope.user != ctx.user || scope.project != ctx.project {
            return Err(MemoryError::WriteDenied(
                "cannot correct claim outside authenticated scope".into(),
            ));
        }
        Ok(())
    }

    fn ensure_sensitivity(&self, ctx: &WriteContext, sensitivity: Sensitivity) -> Result<()> {
        if sensitivity > ctx.max_sensitivity {
            return Err(MemoryError::WriteDenied(
                "claim sensitivity exceeds principal clearance".into(),
            ));
        }
        Ok(())
    }
}

fn append_ledger_on(
    connection: &Connection,
    ctx: &WriteContext,
    kind: &str,
    payload: &impl Serialize,
    created_at: &str,
) -> Result<()> {
    let event_id = Uuid::new_v4();
    let payload_json = serde_json::to_string(payload)?;
    connection.execute(
        "INSERT INTO ledger(
            event_id, kind, payload_json, created_at, tenant, user_id, project
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            event_id.to_string(),
            kind,
            payload_json,
            created_at,
            ctx.tenant,
            ctx.user,
            ctx.project,
        ],
    )?;
    Ok(())
}

fn write_gate(ctx: &WriteContext, origin: Origin) -> Result<(TrustDomain, Vec<AllowedUse>)> {
    let trust = match origin {
        Origin::UserStatement => TrustDomain::User,
        Origin::ToolObservation => TrustDomain::Tool,
        Origin::AgentInference => TrustDomain::Tool,
        Origin::UntrustedContent => TrustDomain::Untrusted,
    };
    let trust = min_trust(trust, ctx.max_trust);
    let allowed = match trust {
        TrustDomain::Untrusted => vec![AllowedUse::Inform],
        _ => vec![
            AllowedUse::Inform,
            AllowedUse::Constraint,
            AllowedUse::ProcedureLookup,
        ],
    };
    Ok((trust, allowed))
}

fn purpose_allowed_use(purpose: RecallPurpose) -> AllowedUse {
    match purpose {
        RecallPurpose::Inform => AllowedUse::Inform,
        RecallPurpose::Constraint => AllowedUse::Constraint,
        RecallPurpose::ProcedureLookup => AllowedUse::ProcedureLookup,
        RecallPurpose::ActionAuthorize => AllowedUse::Action,
    }
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let valid_identifier = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    };
    if !valid_identifier(table) || !valid_identifier(column) {
        return Err(MemoryError::Invariant(
            "invalid schema migration identifier".into(),
        ));
    }
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(());
        }
    }
    connection.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition}"
    ))?;
    Ok(())
}

fn min_trust(a: TrustDomain, b: TrustDomain) -> TrustDomain {
    use TrustDomain::*;
    let rank = |t: TrustDomain| match t {
        System => 3,
        User => 2,
        Tool => 1,
        Untrusted => 0,
    };
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

fn enum_str<T: Serialize>(v: &T) -> Result<String> {
    Ok(serde_json::to_value(v)?
        .as_str()
        .ok_or_else(|| MemoryError::Invariant("enum str".into()))?
        .to_string())
}

fn dedupe_latest(mut claims: Vec<ClaimView>) -> Vec<ClaimView> {
    // Keep first occurrence per id (already ordered tx_from DESC).
    let mut seen = std::collections::HashSet::new();
    claims.retain(|c| seen.insert(c.id));
    // For same subject/predicate/object keep highest tx_from only.
    let mut best: std::collections::HashMap<(String, String, String), ClaimView> =
        std::collections::HashMap::new();
    for c in claims {
        let key = (c.subject.clone(), c.predicate.clone(), c.object.clone());
        match best.get(&key) {
            Some(existing) if existing.tx_from.as_str() >= c.tx_from.as_str() => {}
            _ => {
                best.insert(key, c);
            }
        }
    }
    best.into_values().collect()
}

fn row_to_claim(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClaimView> {
    let id = Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let origin: Origin =
        serde_json::from_value(serde_json::Value::String(row.get(13)?)).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(13, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let trust: TrustDomain = serde_json::from_value(serde_json::Value::String(row.get(14)?))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let allowed: Vec<AllowedUse> =
        serde_json::from_str(&row.get::<_, String>(15)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(15, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let supersedes = row
        .get::<_, Option<String>>(16)?
        .map(|s| {
            Uuid::parse_str(&s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    16,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
        })
        .transpose()?;
    let sensitivity: Sensitivity = serde_json::from_value(serde_json::Value::String(row.get(18)?))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(18, rusqlite::types::Type::Text, Box::new(e))
        })?;
    Ok(ClaimView {
        id,
        scope: Scope {
            tenant: row.get(1)?,
            user: row.get(2)?,
            agent: row.get(3)?,
            project: row.get(4)?,
        },
        subject: row.get(5)?,
        predicate: row.get(6)?,
        object: row.get(7)?,
        valid_from: row.get(8)?,
        valid_to: row.get(9)?,
        tx_from: row.get(10)?,
        tx_to: row.get(11)?,
        confidence: row.get(12)?,
        origin,
        trust,
        allowed_uses: allowed,
        supersedes,
        in_conflict: row.get::<_, i64>(17)? != 0,
        sensitivity,
        retention_until: row.get(19)?,
        tombstoned_at: row.get(20)?,
        erased: row.get::<_, i64>(21)? != 0,
    })
}

fn format_unix_rfc3339(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod clock_tests {
    use super::format_unix_rfc3339;

    #[test]
    fn unix_time_format_covers_calendar_boundaries() {
        let fixtures = [
            (0, "1970-01-01T00:00:00Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (1_704_067_199, "2023-12-31T23:59:59Z"),
            (1_704_067_200, "2024-01-01T00:00:00Z"),
        ];
        for (seconds, expected) in fixtures {
            assert_eq!(format_unix_rfc3339(seconds), expected);
        }
    }
}
