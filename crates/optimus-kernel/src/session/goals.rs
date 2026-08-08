//! Durable session goals (spec-026): types, status machine, and the
//! `session_goals` table in the session store.
//!
//! One goal per session. The goal is session state, so it lives in
//! `sessions.db` next to the session it belongs to (ADR-0086), never in the
//! effect ledger. The turn loop enforces budgets through
//! `crate::goal_ops::Kernel` methods; this module owns the record and the
//! closed status machine.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Result;

use super::SessionStore;

/// Additive schema version for the `session_goals` table.
pub const GOALS_SCHEMA_VERSION: u32 = 1;

/// Goal lifecycle statuses (spec-026 R2). Terminal states are absorbing:
/// only `set` rewrites a goal in `complete`, `budget_limited`, or `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Idle,
    Active,
    Paused,
    BudgetLimited,
    Complete,
    Error,
}

impl GoalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::BudgetLimited => "budget_limited",
            Self::Complete => "complete",
            Self::Error => "error",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::BudgetLimited | Self::Complete | Self::Error)
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "idle" => Ok(Self::Idle),
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "budget_limited" => Ok(Self::BudgetLimited),
            "complete" => Ok(Self::Complete),
            "error" => Ok(Self::Error),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid goal status: {other}").into(),
            )),
        }
    }
}

/// Which budget stopped an active goal (spec-026 R3/R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalBudgetReason {
    TokenBudget,
    TimeBudget,
}

impl std::fmt::Display for GoalBudgetReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl GoalBudgetReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TokenBudget => "token_budget",
            Self::TimeBudget => "time_budget",
        }
    }
}

/// One durable, session-scoped goal (spec-026 R1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: Uuid,
    pub session_id: Uuid,
    pub objective: String,
    pub status: GoalStatus,
    pub token_budget: Option<u64>,
    pub time_budget_seconds: Option<u64>,
    /// Provider-reported tokens consumed while `active` (pause freezes it).
    pub tokens_used: u64,
    /// Wall-clock seconds accumulated while `active` (pause freezes it).
    pub active_seconds: u64,
    /// `ts:{nanos}` when the goal most recently became active, if it is
    /// currently `active`; `None` otherwise.
    pub last_resumed_at: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

fn now_stamp() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("ts:{nanos}")
}

/// Parse a `ts:{nanos}` stamp into nanoseconds since the epoch.
pub(crate) fn stamp_nanos(stamp: &str) -> Option<u128> {
    stamp.strip_prefix("ts:").and_then(|n| n.parse().ok())
}

impl Goal {
    /// Effective accumulated active seconds, including the current active
    /// stretch when the goal is `active`.
    pub fn active_seconds_effective(&self) -> u64 {
        let mut seconds = self.active_seconds;
        if self.status == GoalStatus::Active {
            if let Some(stamp) = &self.last_resumed_at {
                if let (Some(started), Some(now)) = (stamp_nanos(stamp), stamp_nanos(&now_stamp()))
                {
                    seconds = seconds.saturating_add(
                        u64::try_from(now.saturating_sub(started) / 1_000_000_000).unwrap_or(0),
                    );
                }
            }
        }
        seconds
    }

    /// Over the token budget, if one is set.
    pub fn over_token_budget(&self) -> bool {
        self.token_budget
            .is_some_and(|budget| self.tokens_used >= budget)
    }

    /// Over the time budget, if one is set.
    pub fn over_time_budget(&self) -> bool {
        self.time_budget_seconds
            .is_some_and(|budget| self.active_seconds_effective() >= budget)
    }

    /// `set`: rewrite objective and budgets. Allowed only while the goal is
    /// not `active` or `paused` (spec-026 R2). Keeps status `idle`.
    pub fn rewrite(
        &mut self,
        objective: String,
        token_budget: Option<u64>,
        time_budget: Option<u64>,
    ) {
        self.objective = objective;
        self.token_budget = token_budget;
        self.time_budget_seconds = time_budget;
        self.status = GoalStatus::Idle;
        self.error = None;
        self.updated_at = now_stamp();
    }

    /// `start`: `idle` to `active`. Any other status is a transition error.
    pub fn start(&mut self) -> Result<()> {
        if self.status != GoalStatus::Idle {
            return Err(crate::KernelError::Tool(format!(
                "goal cannot start from {}",
                self.status.as_str()
            )));
        }
        self.status = GoalStatus::Active;
        self.last_resumed_at = Some(now_stamp());
        self.updated_at = now_stamp();
        Ok(())
    }

    /// `pause`: `active` to `paused`. Freezes time accounting.
    pub fn pause(&mut self) -> Result<()> {
        if self.status != GoalStatus::Active {
            return Err(crate::KernelError::Tool(format!(
                "goal cannot pause from {}",
                self.status.as_str()
            )));
        }
        // Freeze the active stretch BEFORE the status leaves `active`
        // (`active_seconds_effective` only counts while active).
        self.active_seconds = self.active_seconds_effective();
        self.status = GoalStatus::Paused;
        self.last_resumed_at = None;
        self.updated_at = now_stamp();
        Ok(())
    }

    /// `resume`: `paused` to `active`. Continues accounting from frozen values.
    pub fn resume(&mut self) -> Result<()> {
        if self.status != GoalStatus::Paused {
            return Err(crate::KernelError::Tool(format!(
                "goal cannot resume from {}",
                self.status.as_str()
            )));
        }
        self.status = GoalStatus::Active;
        self.last_resumed_at = Some(now_stamp());
        self.updated_at = now_stamp();
        Ok(())
    }

    /// `complete`: `active` or `paused` to `complete`.
    pub fn complete(&mut self) -> Result<()> {
        match self.status {
            GoalStatus::Active | GoalStatus::Paused => {}
            other => {
                return Err(crate::KernelError::Tool(format!(
                    "goal cannot complete from {}",
                    other.as_str()
                )))
            }
        }
        if self.status == GoalStatus::Active {
            self.active_seconds = self.active_seconds_effective();
        }
        self.status = GoalStatus::Complete;
        self.last_resumed_at = None;
        self.completed_at = Some(now_stamp());
        self.updated_at = now_stamp();
        Ok(())
    }

    /// Turn-loop stop: `active` to `budget_limited` (spec-026 R3/R4).
    pub fn mark_budget_limited(&mut self, reason: GoalBudgetReason) {
        self.active_seconds = self.active_seconds_effective();
        self.status = GoalStatus::BudgetLimited;
        self.last_resumed_at = None;
        self.error = Some(format!("{} exhausted", reason.as_str()));
        self.updated_at = now_stamp();
    }

    /// Kernel invariant failure: `active` to `error`.
    pub fn mark_error(&mut self, error: &str) {
        self.active_seconds = self.active_seconds_effective();
        self.status = GoalStatus::Error;
        self.last_resumed_at = None;
        self.error = Some(error.to_string());
        self.updated_at = now_stamp();
    }

    /// Add provider-reported tokens while `active`. Paused goals are frozen.
    pub fn accumulate_tokens(&mut self, tokens: u64) {
        if self.status == GoalStatus::Active {
            self.tokens_used = self.tokens_used.saturating_add(tokens);
            self.updated_at = now_stamp();
        }
    }
}

impl SessionStore {
    /// Create the additive `session_goals` table.
    pub(crate) fn ensure_goals_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS session_goals (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                objective TEXT NOT NULL CHECK(length(objective) > 0),
                status TEXT NOT NULL CHECK(status IN (
                    'idle','active','paused','budget_limited','complete','error'
                )),
                token_budget INTEGER CHECK(token_budget IS NULL OR token_budget > 0),
                time_budget_seconds INTEGER CHECK(time_budget_seconds IS NULL OR time_budget_seconds > 0),
                tokens_used INTEGER NOT NULL DEFAULT 0 CHECK(tokens_used >= 0),
                active_seconds INTEGER NOT NULL DEFAULT 0 CHECK(active_seconds >= 0),
                last_resumed_at TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                UNIQUE(session_id)
            );
            ",
        )?;
        Ok(())
    }

    /// Load the session's goal, if any.
    pub fn goal_load(&self, session_id: Uuid) -> Result<Option<Goal>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, objective, status, token_budget, time_budget_seconds,
                    tokens_used, active_seconds, last_resumed_at, error,
                    created_at, updated_at, completed_at
             FROM session_goals WHERE session_id = ?1",
        )?;
        let row = stmt
            .query_row(params![session_id.to_string()], |row| {
                let status: String = row.get(3)?;
                Ok(Goal {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            e.to_string().into(),
                        )
                    })?,
                    session_id: Uuid::parse_str(&row.get::<_, String>(1)?).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            e.to_string().into(),
                        )
                    })?,
                    objective: row.get(2)?,
                    status: GoalStatus::parse(&status)?,
                    token_budget: row.get(4)?,
                    time_budget_seconds: row.get(5)?,
                    tokens_used: row.get(6)?,
                    active_seconds: row.get(7)?,
                    last_resumed_at: row.get(8)?,
                    error: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                    completed_at: row.get(12)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    /// Persist a goal record (insert or update).
    pub fn goal_save(&self, goal: &Goal) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session_goals(
                 id, session_id, objective, status, token_budget, time_budget_seconds,
                 tokens_used, active_seconds, last_resumed_at, error,
                 created_at, updated_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(session_id) DO UPDATE SET
                 objective = excluded.objective,
                 status = excluded.status,
                 token_budget = excluded.token_budget,
                 time_budget_seconds = excluded.time_budget_seconds,
                 tokens_used = excluded.tokens_used,
                 active_seconds = excluded.active_seconds,
                 last_resumed_at = excluded.last_resumed_at,
                 error = excluded.error,
                 updated_at = excluded.updated_at,
                 completed_at = excluded.completed_at",
            params![
                goal.id.to_string(),
                goal.session_id.to_string(),
                goal.objective,
                goal.status.as_str(),
                goal.token_budget,
                goal.time_budget_seconds,
                goal.tokens_used,
                goal.active_seconds,
                goal.last_resumed_at,
                goal.error,
                goal.created_at,
                goal.updated_at,
                goal.completed_at,
            ],
        )?;
        Ok(())
    }

    /// Create the session's goal in `idle` (spec-026 R1: at most one goal;
    /// `set` on an existing record rewrites it).
    pub fn goal_set(
        &self,
        session_id: Uuid,
        objective: String,
        token_budget: Option<u64>,
        time_budget_seconds: Option<u64>,
    ) -> Result<Goal> {
        let objective = objective.trim().to_string();
        if objective.is_empty() {
            return Err(crate::KernelError::Tool(
                "goal objective must not be empty".into(),
            ));
        }
        if token_budget == Some(0) || time_budget_seconds == Some(0) {
            return Err(crate::KernelError::Tool(
                "goal budgets must be positive when set".into(),
            ));
        }
        let existing = self.goal_load(session_id)?;
        let goal = match existing {
            Some(mut goal) => {
                if matches!(goal.status, GoalStatus::Active | GoalStatus::Paused) {
                    return Err(crate::KernelError::Tool(format!(
                        "goal cannot be rewritten while {}; pause or complete it first",
                        goal.status.as_str()
                    )));
                }
                goal.rewrite(objective, token_budget, time_budget_seconds);
                goal
            }
            None => {
                let now = now_stamp();
                Goal {
                    id: Uuid::new_v4(),
                    session_id,
                    objective,
                    status: GoalStatus::Idle,
                    token_budget,
                    time_budget_seconds,
                    tokens_used: 0,
                    active_seconds: 0,
                    last_resumed_at: None,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                    completed_at: None,
                }
            }
        };
        self.goal_save(&goal)?;
        Ok(goal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(status: GoalStatus) -> Goal {
        let now = now_stamp();
        Goal {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            objective: "finish the report".into(),
            status,
            token_budget: Some(10),
            time_budget_seconds: Some(60),
            tokens_used: 0,
            active_seconds: 0,
            last_resumed_at: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
        }
    }

    #[test]
    fn status_machine_is_closed() {
        let mut g = goal(GoalStatus::Idle);
        g.start().unwrap();
        assert_eq!(g.status, GoalStatus::Active);
        g.pause().unwrap();
        assert_eq!(g.status, GoalStatus::Paused);
        g.resume().unwrap();
        assert_eq!(g.status, GoalStatus::Active);
        g.complete().unwrap();
        assert_eq!(g.status, GoalStatus::Complete);
        // Terminal states are absorbing.
        assert!(g.start().is_err());
        assert!(g.pause().is_err());
        assert!(g.resume().is_err());
        assert!(g.complete().is_err());
    }

    #[test]
    fn invalid_transitions_fail() {
        let mut g = goal(GoalStatus::Idle);
        assert!(g.pause().is_err());
        assert!(g.resume().is_err());
        assert!(g.complete().is_err());
        g.start().unwrap();
        assert!(g.start().is_err());
        g.mark_budget_limited(GoalBudgetReason::TokenBudget);
        assert_eq!(g.status, GoalStatus::BudgetLimited);
        assert!(g.complete().is_err());
    }

    #[test]
    fn pause_freezes_time_accounting() {
        let mut g = goal(GoalStatus::Idle);
        g.start().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        g.pause().unwrap();
        let frozen = g.active_seconds;
        assert_eq!(frozen, 1, "a 1.1 s active stretch must count one second");
        // The pause point froze active_seconds; the next effective reading
        // while paused must equal the frozen value, not grow.
        let reading = g.active_seconds_effective();
        assert_eq!(reading, frozen);
        assert!(g.last_resumed_at.is_none());
        g.resume().unwrap();
        assert!(g.active_seconds_effective() >= frozen);
    }

    #[test]
    fn budget_flags_are_exact() {
        let mut g = goal(GoalStatus::Active);
        g.tokens_used = 9;
        assert!(!g.over_token_budget());
        g.accumulate_tokens(1);
        assert_eq!(g.tokens_used, 10);
        assert!(g.over_token_budget());
        // Paused goals do not accumulate.
        g.pause().unwrap();
        g.accumulate_tokens(5);
        assert_eq!(g.tokens_used, 10);
    }
}
