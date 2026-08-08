//! Skills 2.0: candidate→proven lifecycle, closed permissions, outcome metrics.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("permission denied: missing {missing:?}")]
    PermissionDenied { missing: Vec<Permission> },
    #[error("invariant: {0}")]
    Invariant(String),
    #[error("not eligible for promotion: {0}")]
    NotEligible(String),
}

pub type Result<T> = std::result::Result<T, SkillError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    FsWorkspace,
    Terminal,
    Net,
    Browser,
    MemoryWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Candidate,
    Proven,
    Pinned,
    Deprecated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotePolicy {
    pub min_uses: u32,
    pub min_success_rate: f64,
}

impl Default for PromotePolicy {
    fn default() -> Self {
        Self {
            min_uses: 3,
            min_success_rate: 0.8,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDraft {
    pub name: String,
    pub body: String,
    pub permissions: Vec<Permission>,
    /// If true, created as pinned (human authority); still versioned.
    pub pin: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Outcome {
    pub success: bool,
    pub token_cost: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillView {
    pub id: Uuid,
    pub name: String,
    pub version: u32,
    pub status: SkillStatus,
    pub body: String,
    pub permissions: Vec<Permission>,
    pub uses: u32,
    pub successes: u32,
    pub failures: u32,
    pub total_tokens: u64,
    pub success_rate: f64,
}

pub struct SkillRegistry {
    conn: Connection,
    policy: PromotePolicy,
}

impl SkillRegistry {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_policy(path, PromotePolicy::default())
    }

    pub fn open_with_policy(path: impl AsRef<Path>, policy: PromotePolicy) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        // Concurrent kernel opens (the host's worker pool, one home) must
        // wait for a migrating opener instead of failing "database is
        // locked" (gateway.rs convention).
        conn.busy_timeout(Duration::from_secs(5))?;
        // The journal-mode pragma takes a file-level lock that the busy
        // handler does not cover; concurrent first-opens (the host's worker
        // pool) retry the idempotent batch instead of failing.
        let mut attempts = 0;
        loop {
            match conn.execute_batch(
                "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS skills (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                version INTEGER NOT NULL,
                status TEXT NOT NULL,
                body TEXT NOT NULL,
                permissions_json TEXT NOT NULL,
                uses INTEGER NOT NULL DEFAULT 0,
                successes INTEGER NOT NULL DEFAULT 0,
                failures INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                UNIQUE(name, version)
            );
            CREATE TABLE IF NOT EXISTS skill_events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                skill_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            ",
            ) {
                Ok(()) => break,
                Err(rusqlite::Error::SqliteFailure(failure, _))
                    if failure.code == rusqlite::ErrorCode::DatabaseBusy && attempts < 8 =>
                {
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(Self { conn, policy })
    }

    pub fn create(&self, draft: SkillDraft) -> Result<Uuid> {
        // Normalize the name exactly once: reject empty, then store the
        // trimmed form. Storing the untrimmed name would let "run" and
        // " run " drift into separate rows that resolve() can never
        // reconcile into one skill (regression test: trims_name).
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(SkillError::Invariant("name required".into()));
        }
        if draft.body.trim().is_empty() {
            return Err(SkillError::Invariant("body required".into()));
        }
        let perms = normalize_perms(&draft.permissions);
        let id = Uuid::new_v4();
        let status = if draft.pin {
            SkillStatus::Pinned
        } else {
            SkillStatus::Candidate
        };
        let version = self.next_version(name)?;
        self.conn.execute(
            "INSERT INTO skills(id, name, version, status, body, permissions_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.to_string(),
                name,
                version,
                status_str(status),
                draft.body,
                serde_json::to_string(&perms)?,
            ],
        )?;
        self.event(
            id,
            "created",
            &serde_json::json!({ "status": status, "version": version }),
        )?;
        Ok(id)
    }

    pub fn get(&self, id: Uuid) -> Result<SkillView> {
        self.conn
            .query_row(
                "SELECT id, name, version, status, body, permissions_json, uses, successes, failures, total_tokens
                 FROM skills WHERE id = ?1",
                params![id.to_string()],
                row_to_view,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => SkillError::NotFound(format!("skill {id}")),
                other => SkillError::Sqlite(other),
            })
    }

    pub fn list(&self, include_deprecated: bool) -> Result<Vec<SkillView>> {
        let sql = if include_deprecated {
            "SELECT id, name, version, status, body, permissions_json, uses, successes, failures, total_tokens
             FROM skills ORDER BY name, version DESC"
        } else {
            "SELECT id, name, version, status, body, permissions_json, uses, successes, failures, total_tokens
             FROM skills WHERE status != 'deprecated' ORDER BY name, version DESC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], row_to_view)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Prefer pinned, then proven, then newest candidate for a name.
    pub fn resolve(&self, name: &str) -> Result<Option<SkillView>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, version, status, body, permissions_json, uses, successes, failures, total_tokens
             FROM skills WHERE name = ?1 AND status != 'deprecated'
             ORDER BY
                CASE status
                  WHEN 'pinned' THEN 0
                  WHEN 'proven' THEN 1
                  WHEN 'candidate' THEN 2
                  ELSE 3
                END,
                version DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_view(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn record_outcome(&self, id: Uuid, outcome: Outcome) -> Result<SkillView> {
        let skill = self.get(id)?;
        if matches!(skill.status, SkillStatus::Deprecated) {
            return Err(SkillError::Invariant(
                "cannot record outcome on deprecated skill".into(),
            ));
        }
        let uses = skill.uses + 1;
        let successes = skill.successes + u32::from(outcome.success);
        let failures = skill.failures + u32::from(!outcome.success);
        let total_tokens = skill.total_tokens.saturating_add(outcome.token_cost);
        self.conn.execute(
            "UPDATE skills SET uses = ?1, successes = ?2, failures = ?3, total_tokens = ?4 WHERE id = ?5",
            params![uses, successes, failures, total_tokens as i64, id.to_string()],
        )?;
        self.event(
            id,
            "outcome",
            &serde_json::json!({
                "success": outcome.success,
                "token_cost": outcome.token_cost,
            }),
        )?;
        self.get(id)
    }

    pub fn try_promote(&self, id: Uuid) -> Result<SkillStatus> {
        let skill = self.get(id)?;
        match skill.status {
            SkillStatus::Pinned => return Ok(SkillStatus::Pinned),
            SkillStatus::Proven => return Ok(SkillStatus::Proven),
            SkillStatus::Deprecated => {
                return Err(SkillError::NotEligible("deprecated".into()));
            }
            SkillStatus::Candidate => {}
        }
        if skill.uses < self.policy.min_uses {
            return Err(SkillError::NotEligible(format!(
                "uses {} < min_uses {}",
                skill.uses, self.policy.min_uses
            )));
        }
        if skill.success_rate + f64::EPSILON < self.policy.min_success_rate {
            return Err(SkillError::NotEligible(format!(
                "success_rate {:.3} < min_success_rate {:.3}",
                skill.success_rate, self.policy.min_success_rate
            )));
        }
        self.conn.execute(
            "UPDATE skills SET status = 'proven' WHERE id = ?1",
            params![id.to_string()],
        )?;
        self.event(id, "promoted", &serde_json::json!({ "to": "proven" }))?;
        Ok(SkillStatus::Proven)
    }

    pub fn pin(&self, id: Uuid) -> Result<()> {
        let skill = self.get(id)?;
        if matches!(skill.status, SkillStatus::Deprecated) {
            return Err(SkillError::Invariant("cannot pin deprecated skill".into()));
        }
        self.conn.execute(
            "UPDATE skills SET status = 'pinned' WHERE id = ?1",
            params![id.to_string()],
        )?;
        self.event(id, "pinned", &serde_json::json!({}))?;
        Ok(())
    }

    pub fn deprecate(&self, id: Uuid) -> Result<()> {
        self.get(id)?;
        self.conn.execute(
            "UPDATE skills SET status = 'deprecated' WHERE id = ?1",
            params![id.to_string()],
        )?;
        self.event(id, "deprecated", &serde_json::json!({}))?;
        Ok(())
    }

    /// Update body and/or permissions. Permissions may only shrink (closed set).
    pub fn update_body(
        &self,
        id: Uuid,
        body: Option<String>,
        permissions: Option<Vec<Permission>>,
    ) -> Result<SkillView> {
        let skill = self.get(id)?;
        if matches!(skill.status, SkillStatus::Deprecated) {
            return Err(SkillError::Invariant(
                "cannot update deprecated skill".into(),
            ));
        }
        let new_body = body.unwrap_or(skill.body);
        // Same invariant as create(): a skill must never hold an empty body.
        if new_body.trim().is_empty() {
            return Err(SkillError::Invariant("body required".into()));
        }
        let new_perms = if let Some(p) = permissions {
            let next = normalize_perms(&p);
            let declared: BTreeSet<_> = skill.permissions.iter().copied().collect();
            let next_set: BTreeSet<_> = next.iter().copied().collect();
            if !next_set.is_subset(&declared) {
                let missing: Vec<_> = next_set.difference(&declared).copied().collect();
                return Err(SkillError::PermissionDenied { missing });
            }
            next
        } else {
            skill.permissions
        };
        self.conn.execute(
            "UPDATE skills SET body = ?1, permissions_json = ?2 WHERE id = ?3",
            params![new_body, serde_json::to_string(&new_perms)?, id.to_string()],
        )?;
        self.event(
            id,
            "updated",
            &serde_json::json!({ "permissions": new_perms }),
        )?;
        self.get(id)
    }

    /// Authorize a required permission set against the skill's declared permissions.
    pub fn authorize(&self, id: Uuid, required: &[Permission]) -> Result<()> {
        let skill = self.get(id)?;
        if matches!(skill.status, SkillStatus::Deprecated) {
            return Err(SkillError::Invariant(
                "deprecated skill cannot authorize".into(),
            ));
        }
        let declared: BTreeSet<_> = skill.permissions.iter().copied().collect();
        let need: BTreeSet<_> = required.iter().copied().collect();
        if !need.is_subset(&declared) {
            let missing: Vec<_> = need.difference(&declared).copied().collect();
            return Err(SkillError::PermissionDenied { missing });
        }
        Ok(())
    }

    fn next_version(&self, name: &str) -> Result<u32> {
        let v: Option<i64> = self.conn.query_row(
            "SELECT MAX(version) FROM skills WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        Ok(v.unwrap_or(0) as u32 + 1)
    }

    fn event(&self, id: Uuid, kind: &str, payload: &impl Serialize) -> Result<()> {
        self.conn.execute(
            "INSERT INTO skill_events(skill_id, kind, payload_json) VALUES (?1, ?2, ?3)",
            params![id.to_string(), kind, serde_json::to_string(payload)?],
        )?;
        Ok(())
    }
}

fn normalize_perms(p: &[Permission]) -> Vec<Permission> {
    let set: BTreeSet<_> = p.iter().copied().collect();
    set.into_iter().collect()
}

fn status_str(s: SkillStatus) -> &'static str {
    match s {
        SkillStatus::Candidate => "candidate",
        SkillStatus::Proven => "proven",
        SkillStatus::Pinned => "pinned",
        SkillStatus::Deprecated => "deprecated",
    }
}

fn parse_status(s: &str) -> rusqlite::Result<SkillStatus> {
    match s {
        "candidate" => Ok(SkillStatus::Candidate),
        "proven" => Ok(SkillStatus::Proven),
        "pinned" => Ok(SkillStatus::Pinned),
        "deprecated" => Ok(SkillStatus::Deprecated),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("bad status {other}"),
            )),
        )),
    }
}

fn row_to_view(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillView> {
    let id = Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let status = parse_status(&row.get::<_, String>(3)?)?;
    let permissions: Vec<Permission> =
        serde_json::from_str(&row.get::<_, String>(5)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let uses = row.get::<_, i64>(6)? as u32;
    let successes = row.get::<_, i64>(7)? as u32;
    let failures = row.get::<_, i64>(8)? as u32;
    let total_tokens = row.get::<_, i64>(9)? as u64;
    let success_rate = if uses == 0 {
        0.0
    } else {
        f64::from(successes) / f64::from(uses)
    };
    Ok(SkillView {
        id,
        name: row.get(1)?,
        version: row.get::<_, i64>(2)? as u32,
        status,
        body: row.get(4)?,
        permissions,
        uses,
        successes,
        failures,
        total_tokens,
        success_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_registry() -> (TempDir, SkillRegistry) {
        let dir = tempfile::tempdir().expect("tempdir");
        let reg = SkillRegistry::open(dir.path().join("skills.db")).expect("open registry");
        (dir, reg)
    }

    fn draft(name: &str) -> SkillDraft {
        SkillDraft {
            name: name.into(),
            body: "body".into(),
            permissions: vec![],
            pin: false,
        }
    }

    #[test]
    fn create_trims_surrounding_whitespace_from_name() {
        let (_dir, reg) = open_registry();
        let id = reg.create(draft("  run  ")).unwrap();
        let view = reg.get(id).unwrap();
        assert_eq!(view.name, "run");
        // resolve() must find it under the canonical trimmed name, and must
        // not accidentally match a distinct whitespace-padded spelling.
        assert_eq!(reg.resolve("run").unwrap().unwrap().id, id);
        assert!(reg.resolve("run   ").unwrap().is_none());
    }

    #[test]
    fn create_rejects_whitespace_only_name() {
        let (_dir, reg) = open_registry();
        let err = reg.create(draft("   ")).unwrap_err();
        assert!(matches!(err, SkillError::Invariant(_)));
    }

    #[test]
    fn create_deduplicates_and_sorts_permissions() {
        let (_dir, reg) = open_registry();
        let id = reg
            .create(SkillDraft {
                name: "s".into(),
                body: "body".into(),
                permissions: vec![Permission::Net, Permission::FsWorkspace, Permission::Net],
                pin: false,
            })
            .unwrap();
        let view = reg.get(id).unwrap();
        assert_eq!(
            view.permissions,
            vec![Permission::FsWorkspace, Permission::Net]
        );
    }
}
