//! Kernel goal operations (spec-026): the `goal` tool surface, the turn-loop
//! budget gate, and post-step usage accumulation.
//!
//! Kernel goal operations (spec-026): the `goal` tool surface, the turn-loop
//! budget gate, and post-step usage accumulation.
//!
//! Split out of `lib.rs` under the module-size ratchet and hosted under the
//! `session` module so the kernel file stays under the 800-line law. The
//! record, the status machine, and the `session_goals` table live in
//! `session/goals.rs` (ADR-0086); this file owns how the kernel exposes and
//! enforces them. Private `Kernel` fields are visible here: `session::goals`
//! is a descendant of the crate root where `Kernel` is defined.

use super::goals::GoalBudgetReason;
use crate::{CompletionUsage, Goal, GoalStatus, Kernel, KernelError, Result, ToolCall};

/// Effective provider-reported tokens for one completion: the reported total,
/// falling back to input + output when the provider omitted a total. Optimus
/// never estimates usage (model_usage.rs); absent usage adds zero.
fn effective_tokens(usage: Option<&CompletionUsage>) -> u64 {
    usage
        .and_then(|usage| {
            usage.total_tokens.or_else(|| {
                usage
                    .input_tokens
                    .zip(usage.output_tokens)
                    .map(|(input, output)| input + output)
            })
        })
        .unwrap_or(0)
}

impl Kernel {
    /// Load the session's goal, if any.
    pub fn goal(&self) -> Result<Option<Goal>> {
        self.sessions.goal_load(self.session_id)
    }

    /// `goal` tool action `set` (create or rewrite in `idle`).
    pub fn goal_set(
        &mut self,
        objective: String,
        token_budget: Option<u64>,
        time_budget_seconds: Option<u64>,
    ) -> Result<Goal> {
        let goal = self.sessions.goal_set(
            self.session_id,
            objective,
            token_budget,
            time_budget_seconds,
        )?;
        self.save_session()?;
        Ok(goal)
    }

    /// `goal` tool action `start` (`idle` to `active`).
    pub fn goal_start(&mut self) -> Result<Goal> {
        let goal = self.goal_mutate(|goal| goal.start())?;
        self.save_session()?;
        Ok(goal)
    }

    /// `goal` tool action `pause` (`active` to `paused`; freezes accounting).
    pub fn goal_pause(&mut self) -> Result<Goal> {
        let goal = self.goal_mutate(|goal| goal.pause())?;
        self.save_session()?;
        Ok(goal)
    }

    /// `goal` tool action `resume` (`paused` to `active`).
    pub fn goal_resume(&mut self) -> Result<Goal> {
        let goal = self.goal_mutate(|goal| goal.resume())?;
        self.save_session()?;
        Ok(goal)
    }

    /// `goal` tool action `complete` (`active`/`paused` to `complete`).
    pub fn goal_complete(&mut self) -> Result<Goal> {
        let goal = self.goal_mutate(|goal| goal.complete())?;
        self.save_session()?;
        Ok(goal)
    }

    fn goal_mutate(&mut self, transition: impl FnOnce(&mut Goal) -> Result<()>) -> Result<Goal> {
        let Some(mut goal) = self.sessions.goal_load(self.session_id)? else {
            return Err(KernelError::Tool(
                "no goal set for this session; use goal action set first".into(),
            ));
        };
        transition(&mut goal)?;
        self.sessions.goal_save(&goal)?;
        Ok(goal)
    }

    /// Pre-step gate (spec-026 R3): an `active` goal already over a budget
    /// stops the turn with the distinct `goal_budget_limited` terminal
    /// outcome before any further model call. No-op when no goal is active.
    pub(crate) fn check_active_goal_budget(&mut self) -> Result<()> {
        let Some(mut goal) = self.sessions.goal_load(self.session_id)? else {
            return Ok(());
        };
        if goal.status != GoalStatus::Active {
            return Ok(());
        }
        let reason = if goal.over_token_budget() {
            Some(GoalBudgetReason::TokenBudget)
        } else if goal.over_time_budget() {
            Some(GoalBudgetReason::TimeBudget)
        } else {
            None
        };
        let Some(reason) = reason else {
            return Ok(());
        };
        goal.mark_budget_limited(reason);
        self.sessions.goal_save(&goal)?;
        let _ = self.save_session();
        Err(KernelError::GoalBudgetLimited {
            goal_id: goal.id,
            reason,
        })
    }

    /// Post-step accounting (spec-026 R3): add provider-reported tokens to the
    /// active goal; when the accumulated total reaches the token budget, the
    /// goal transitions to `budget_limited` and the turn stops so the next
    /// model call never starts. No-op when no goal is active.
    pub(crate) fn accumulate_active_goal_usage(
        &mut self,
        usage: Option<&CompletionUsage>,
    ) -> Result<()> {
        let Some(mut goal) = self.sessions.goal_load(self.session_id)? else {
            return Ok(());
        };
        if goal.status != GoalStatus::Active {
            return Ok(());
        }
        let tokens = effective_tokens(usage);
        if tokens == 0 {
            return Ok(());
        }
        goal.accumulate_tokens(tokens);
        let over = goal.over_token_budget();
        self.sessions.goal_save(&goal)?;
        if over {
            goal.mark_budget_limited(GoalBudgetReason::TokenBudget);
            self.sessions.goal_save(&goal)?;
            let _ = self.save_session();
            return Err(KernelError::GoalBudgetLimited {
                goal_id: goal.id,
                reason: GoalBudgetReason::TokenBudget,
            });
        }
        Ok(())
    }
}

impl Kernel {
    /// `goal` tool dispatch (spec-026 R6). Actions are parsed from the call
    /// arguments; every action returns the resulting goal record as JSON.
    pub(crate) fn dispatch_goal(&mut self, call: &ToolCall) -> Result<String> {
        let action = call
            .arguments
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KernelError::Tool("goal requires action".into()))?;
        let goal = match action {
            "set" => {
                let objective = call
                    .arguments
                    .get("objective")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("goal set requires objective".into()))?;
                let token_budget = call.arguments.get("token_budget").and_then(|v| v.as_u64());
                let time_budget = call
                    .arguments
                    .get("time_budget_seconds")
                    .and_then(|v| v.as_u64());
                self.goal_set(objective.to_string(), token_budget, time_budget)?
            }
            "start" => self.goal_start()?,
            "status" => self
                .sessions
                .goal_load(self.session_id)?
                .ok_or_else(|| KernelError::Tool("no goal set for this session".into()))?,
            "pause" => self.goal_pause()?,
            "resume" => self.goal_resume()?,
            "complete" => self.goal_complete()?,
            other => {
                return Err(KernelError::Tool(format!(
                    "goal action must be set|start|status|pause|resume|complete, got {other}"
                )))
            }
        };
        Ok(serde_json::to_string(&goal)?)
    }
}
