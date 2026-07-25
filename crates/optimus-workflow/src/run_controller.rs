//! Multi-agent RunController state machine (design P0).
//!
//! In-memory controller with:
//! - deterministic phase transitions
//! - exactly one terminal outcome per run_id
//! - cancel token integration
//! - token/wall budgets
//! - bounded patch / replan counters (anti-loop)
//! - deterministic QualityGate (code, not LLM)
//!
//! Does not spawn models. Worker/review integration is P1+.

use std::time::{Duration, Instant};

use optimus_runtime::CancellationToken;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orchestrator_envelopes::{
    AttemptCounters, DeliveryPayload, DeliveryTerminal, GateAction, GateDecision, ReviewBallot,
    ReviewLens, ReviewVerdict, SynthesisReport, TaskSpec, GATE_DECISION_SCHEMA_VERSION,
    DELIVERY_PAYLOAD_SCHEMA_VERSION,
};
use crate::{Result, WorkflowError};

fn invalid(msg: impl Into<String>) -> WorkflowError {
    WorkflowError::Msg(msg.into())
}

/// Production defaults from the optimized workflow design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPolicy {
    pub max_patch_attempts: u32,
    pub max_replan_attempts: u32,
    pub max_plan_attempts: u32,
    pub max_budget_tokens: u64,
    pub max_wall_ms: u64,
    /// Reserve fraction for delivery (0–100).
    pub delivery_reserve_pct: u8,
}

impl Default for RunPolicy {
    fn default() -> Self {
        Self {
            max_patch_attempts: 2,
            max_replan_attempts: 2,
            max_plan_attempts: 3,
            max_budget_tokens: 200_000,
            max_wall_ms: 600_000,
            delivery_reserve_pct: 5,
        }
    }
}

impl RunPolicy {
    pub fn minimal() -> Self {
        Self {
            max_patch_attempts: 1,
            max_replan_attempts: 1,
            max_plan_attempts: 2,
            max_budget_tokens: 50_000,
            max_wall_ms: 120_000,
            delivery_reserve_pct: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Accepted,
    Planning,
    PlanReview,
    Executing,
    Reviewing,
    Synthesizing,
    QualityGate,
    Delivering,
    AwaitingHuman,
    Cancelled,
    Failed,
    Succeeded,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Planning => "planning",
            Self::PlanReview => "plan_review",
            Self::Executing => "executing",
            Self::Reviewing => "reviewing",
            Self::Synthesizing => "synthesizing",
            Self::QualityGate => "quality_gate",
            Self::Delivering => "delivering",
            Self::AwaitingHuman => "awaiting_human",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Succeeded => "succeeded",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Failed | Self::Succeeded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEvent {
    pub seq: u64,
    pub state: RunState,
    pub reason_code: String,
    pub tokens_spent: u64,
}

#[derive(Debug, Clone)]
pub struct RunController {
    pub run_id: Uuid,
    pub task: TaskSpec,
    pub policy: RunPolicy,
    pub state: RunState,
    pub cancel: CancellationToken,
    pub counters: AttemptCounters,
    pub tokens_spent: u64,
    pub started: Instant,
    pub events: Vec<RunEvent>,
    terminal: Option<DeliveryPayload>,
    event_seq: u64,
}

impl RunController {
    pub fn accept(task: TaskSpec, policy: RunPolicy) -> Result<Self> {
        task.validate()?;
        let mut policy = policy;
        // Task caps cannot exceed policy defaults upward without explicit policy;
        // take the min so task cannot inflate budgets.
        policy.max_budget_tokens = policy.max_budget_tokens.min(task.max_budget_tokens);
        policy.max_wall_ms = policy.max_wall_ms.min(task.max_wall_ms);

        let mut ctl = Self {
            run_id: Uuid::new_v4(),
            task,
            policy,
            state: RunState::Accepted,
            cancel: CancellationToken::new(),
            counters: AttemptCounters {
                patch_attempts: 0,
                replan_attempts: 0,
                plan_attempts: 0,
            },
            tokens_spent: 0,
            started: Instant::now(),
            events: Vec::new(),
            terminal: None,
            event_seq: 0,
        };
        ctl.push_event(RunState::Accepted, "run_accepted");
        Ok(ctl)
    }

    pub fn terminal_payload(&self) -> Option<&DeliveryPayload> {
        self.terminal.as_ref()
    }

    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    pub fn cancel(&mut self) {
        self.cancel.cancel();
        if !self.state.is_terminal() {
            let _ = self.force_terminal(
                DeliveryTerminal::Cancelled,
                "cancelled",
                None,
                vec!["run cancelled".into()],
            );
        }
    }

    pub fn record_tokens(&mut self, spent: u64) -> Result<()> {
        self.ensure_not_terminal()?;
        self.tokens_spent = self.tokens_spent.saturating_add(spent);
        self.check_budgets()
    }

    fn budget_remaining(&self) -> u64 {
        let reserve = (self.policy.max_budget_tokens as u128
            * self.policy.delivery_reserve_pct as u128
            / 100) as u64;
        let usable = self.policy.max_budget_tokens.saturating_sub(reserve);
        usable.saturating_sub(self.tokens_spent)
    }

    fn check_budgets(&mut self) -> Result<()> {
        if self.cancel.is_cancelled() {
            self.cancel();
            return Ok(());
        }
        if self.started.elapsed() > Duration::from_millis(self.policy.max_wall_ms) {
            let _ = self.force_terminal(
                DeliveryTerminal::Failed,
                "deadline",
                None,
                vec!["hard wall-clock exceeded".into()],
            );
            return Ok(());
        }
        if self.budget_remaining() == 0 {
            let _ = self.force_terminal(
                DeliveryTerminal::Failed,
                "budget_exhausted",
                None,
                vec!["token budget exhausted".into()],
            );
        }
        Ok(())
    }

    fn ensure_not_terminal(&self) -> Result<()> {
        if self.state.is_terminal() {
            return Err(invalid("run already terminal"));
        }
        Ok(())
    }

    fn push_event(&mut self, state: RunState, reason: &str) {
        self.event_seq += 1;
        self.events.push(RunEvent {
            seq: self.event_seq,
            state,
            reason_code: reason.into(),
            tokens_spent: self.tokens_spent,
        });
    }

    fn transition(&mut self, to: RunState, reason: &str) -> Result<()> {
        self.ensure_not_terminal()?;
        self.check_budgets()?;
        if self.state.is_terminal() {
            return Ok(());
        }
        if !self.is_allowed_transition(self.state, to) {
            return Err(invalid(format!(
                "illegal transition {} -> {}",
                self.state.as_str(),
                to.as_str()
            )));
        }
        self.state = to;
        self.push_event(to, reason);
        Ok(())
    }

    fn is_allowed_transition(&self, from: RunState, to: RunState) -> bool {
        use RunState::*;
        matches!(
            (from, to),
            (Accepted, Planning)
                | (Planning, PlanReview)
                | (PlanReview, Planning)
                | (PlanReview, Executing)
                | (PlanReview, Failed)
                | (Executing, Reviewing)
                | (Executing, AwaitingHuman)
                | (Executing, Cancelled)
                | (Executing, Failed)
                | (AwaitingHuman, Executing)
                | (AwaitingHuman, Cancelled)
                | (AwaitingHuman, Failed)
                | (Reviewing, Synthesizing)
                | (Synthesizing, QualityGate)
                | (QualityGate, Executing)
                | (QualityGate, Planning)
                | (QualityGate, Delivering)
                | (QualityGate, Failed)
                | (Delivering, Succeeded)
                // emergency terminals from non-terminal (handled via force)
                | (_, Cancelled)
                | (_, Failed)
        ) && !(from.is_terminal())
    }

    pub fn begin_planning(&mut self) -> Result<()> {
        self.counters.plan_attempts = self.counters.plan_attempts.saturating_add(1);
        if self.counters.plan_attempts > self.policy.max_plan_attempts {
            return self.force_terminal(
                DeliveryTerminal::Failed,
                "plan_attempts_exhausted",
                None,
                vec!["plan attempt budget exhausted".into()],
            );
        }
        self.transition(RunState::Planning, "begin_planning")
    }

    pub fn enter_plan_review(&mut self) -> Result<()> {
        self.transition(RunState::PlanReview, "enter_plan_review")
    }

    pub fn plan_accepted(&mut self) -> Result<()> {
        self.transition(RunState::Executing, "plan_accepted")
    }

    pub fn plan_revise(&mut self) -> Result<()> {
        self.transition(RunState::Planning, "plan_revise")
    }

    pub fn work_finished(&mut self) -> Result<()> {
        self.transition(RunState::Reviewing, "work_finished")
    }

    pub fn await_human(&mut self) -> Result<()> {
        self.transition(RunState::AwaitingHuman, "awaiting_human")
    }

    pub fn human_resumed(&mut self) -> Result<()> {
        self.transition(RunState::Executing, "human_resumed")
    }

    pub fn reviews_finished(&mut self) -> Result<()> {
        self.transition(RunState::Synthesizing, "reviews_finished")
    }

    pub fn synthesis_finished(&mut self) -> Result<()> {
        self.transition(RunState::QualityGate, "synthesis_finished")
    }

    /// Deterministic quality gate (design §6.2). Not an LLM.
    pub fn apply_quality_gate(&mut self, report: &SynthesisReport) -> Result<GateDecision> {
        if self.state != RunState::QualityGate {
            return Err(invalid("apply_quality_gate requires quality_gate state"));
        }
        report.validate()?;
        self.check_budgets()?;
        if self.state.is_terminal() {
            return self.gate_decision_from_terminal();
        }
        if self.cancel.is_cancelled() {
            self.cancel();
            return self.gate_decision_from_terminal();
        }

        let blocking = report.blocking_count > 0
            || report.merged_findings.iter().any(|f| f.blocking)
            || !report.fail_lenses.is_empty();

        let (action, reason, next) = if !blocking
            && matches!(
                report.recommended_action,
                GateAction::Accept | GateAction::FailClosed
            ) {
            if report.recommended_action == GateAction::FailClosed {
                (GateAction::FailClosed, "synth_fail", RunState::Failed)
            } else {
                (GateAction::Accept, "accept", RunState::Delivering)
            }
        } else if !blocking && report.recommended_action == GateAction::Accept {
            (GateAction::Accept, "accept", RunState::Delivering)
        } else if blocking
            && report.recommended_action == GateAction::PatchWorker
            && self.counters.patch_attempts < self.policy.max_patch_attempts
        {
            self.counters.patch_attempts += 1;
            (GateAction::PatchWorker, "patch_worker", RunState::Executing)
        } else if blocking
            && report.recommended_action == GateAction::Replan
            && self.counters.replan_attempts < self.policy.max_replan_attempts
        {
            self.counters.replan_attempts += 1;
            (GateAction::Replan, "replan", RunState::Planning)
        } else if blocking
            && matches!(
                report.recommended_action,
                GateAction::PatchWorker | GateAction::Accept | GateAction::FailClosed
            )
            && self.counters.patch_attempts < self.policy.max_patch_attempts
            && report.patch_brief.as_ref().is_some_and(|b| !b.trim().is_empty())
        {
            // Patch fallback only when synth asked for patch (or generic fail) with a brief.
            self.counters.patch_attempts += 1;
            (GateAction::PatchWorker, "patch_worker_fallback", RunState::Executing)
        } else if blocking
            && matches!(
                report.recommended_action,
                GateAction::Replan | GateAction::FailClosed | GateAction::Accept
            )
            && self.counters.replan_attempts < self.policy.max_replan_attempts
            && report.replan_brief.as_ref().is_some_and(|b| !b.trim().is_empty())
        {
            // Replan fallback requires an explicit replan_brief (anti free-text loop).
            self.counters.replan_attempts += 1;
            (GateAction::Replan, "replan_fallback", RunState::Planning)
        } else {
            (
                GateAction::FailClosed,
                "attempts_exhausted_or_blocking",
                RunState::Failed,
            )
        };

        let decision = GateDecision {
            schema_version: GATE_DECISION_SCHEMA_VERSION,
            action,
            reason_code: reason.into(),
            attempt_counters: self.counters.clone(),
            next_state: next.as_str().into(),
            user_visible_summary: None,
        };
        decision.validate()?;

        match action {
            GateAction::Accept => {
                self.transition(RunState::Delivering, reason)?;
                let payload = DeliveryPayload {
                    schema_version: DELIVERY_PAYLOAD_SCHEMA_VERSION,
                    run_id: self.run_id,
                    terminal: DeliveryTerminal::Succeeded,
                    answer: Some(report.answer_draft.clone()),
                    warnings: report
                        .merged_findings
                        .iter()
                        .filter(|f| !f.blocking)
                        .map(|f| f.claim.clone())
                        .collect(),
                    evidence_refs: report.evidence_index.clone(),
                    trace_id: None,
                    cost_summary: serde_json::json!({
                        "tokens_spent": self.tokens_spent,
                        "patch_attempts": self.counters.patch_attempts,
                        "replan_attempts": self.counters.replan_attempts,
                    }),
                };
                payload.validate()?;
                self.terminal = Some(payload);
                self.state = RunState::Succeeded;
                self.push_event(RunState::Succeeded, "delivered");
            }
            GateAction::PatchWorker => {
                self.transition(RunState::Executing, reason)?;
            }
            GateAction::Replan => {
                self.transition(RunState::Planning, reason)?;
            }
            GateAction::FailClosed => {
                let _ = self.force_terminal(
                    DeliveryTerminal::Failed,
                    reason,
                    None,
                    vec![reason.into()],
                );
            }
            GateAction::Cancelled => {
                self.cancel();
            }
        }
        Ok(decision)
    }

    /// Aggregate ballots into recommended action (helper for hosts; still not LLM).
    pub fn recommend_from_ballots(ballots: &[ReviewBallot]) -> GateAction {
        let mut blocking_fail = false;
        for b in ballots {
            if b.lens == ReviewLens::StyleOptional {
                continue;
            }
            if b.has_blocking_failure() || matches!(b.verdict, ReviewVerdict::Fail) {
                blocking_fail = true;
            }
            if matches!(b.verdict, ReviewVerdict::Inconclusive)
                && b.lens.is_blocking_by_default()
            {
                blocking_fail = true;
            }
        }
        if blocking_fail {
            GateAction::PatchWorker
        } else {
            GateAction::Accept
        }
    }

    fn gate_decision_from_terminal(&self) -> Result<GateDecision> {
        let action = match self.state {
            RunState::Cancelled => GateAction::Cancelled,
            RunState::Failed => GateAction::FailClosed,
            RunState::Succeeded => GateAction::Accept,
            _ => return Err(invalid("not terminal")),
        };
        let d = GateDecision {
            schema_version: GATE_DECISION_SCHEMA_VERSION,
            action,
            reason_code: "already_terminal".into(),
            attempt_counters: self.counters.clone(),
            next_state: self.state.as_str().into(),
            user_visible_summary: None,
        };
        d.validate()?;
        Ok(d)
    }

    fn force_terminal(
        &mut self,
        terminal: DeliveryTerminal,
        reason: &str,
        answer: Option<String>,
        warnings: Vec<String>,
    ) -> Result<()> {
        if self.state.is_terminal() {
            return Err(invalid("run already terminal"));
        }
        let state = match terminal {
            DeliveryTerminal::Succeeded => RunState::Succeeded,
            DeliveryTerminal::Failed => RunState::Failed,
            DeliveryTerminal::Cancelled => RunState::Cancelled,
        };
        let payload = DeliveryPayload {
            schema_version: DELIVERY_PAYLOAD_SCHEMA_VERSION,
            run_id: self.run_id,
            terminal,
            answer,
            warnings,
            evidence_refs: vec![],
            trace_id: None,
            cost_summary: serde_json::json!({ "tokens_spent": self.tokens_spent }),
        };
        if terminal == DeliveryTerminal::Succeeded {
            payload.validate()?;
        }
        self.terminal = Some(payload);
        self.state = state;
        self.push_event(state, reason);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator_envelopes::{
        SynthesisReport, SYNTHESIS_REPORT_SCHEMA_VERSION, TASK_SPEC_SCHEMA_VERSION,
    };

    fn task() -> TaskSpec {
        TaskSpec {
            schema_version: TASK_SPEC_SCHEMA_VERSION,
            task_id: Uuid::new_v4(),
            user_text: "ship it".into(),
            surface: "cli".into(),
            privacy: "local".into(),
            max_budget_tokens: 100_000,
            max_wall_ms: 60_000,
            required_capabilities: vec![],
            project_scope_ref: None,
            approval_policy: "smart_deny".into(),
        }
    }

    fn accept_report(answer: &str) -> SynthesisReport {
        SynthesisReport {
            schema_version: SYNTHESIS_REPORT_SCHEMA_VERSION,
            work_attempt_id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            merged_findings: vec![],
            blocking_count: 0,
            pass_lenses: vec![ReviewLens::Correctness, ReviewLens::Security],
            fail_lenses: vec![],
            recommended_action: GateAction::Accept,
            patch_brief: None,
            replan_brief: None,
            answer_draft: answer.into(),
            evidence_index: vec!["effect:1".into()],
        }
    }

    fn patch_report() -> SynthesisReport {
        SynthesisReport {
            schema_version: SYNTHESIS_REPORT_SCHEMA_VERSION,
            work_attempt_id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            merged_findings: vec![crate::orchestrator_envelopes::Finding {
                severity: "error".into(),
                claim: "missing test".into(),
                evidence_ref: None,
                blocking: true,
            }],
            blocking_count: 1,
            pass_lenses: vec![],
            fail_lenses: vec![ReviewLens::Completeness],
            recommended_action: GateAction::PatchWorker,
            patch_brief: Some("add regression test for X".into()),
            replan_brief: None,
            answer_draft: String::new(),
            evidence_index: vec![],
        }
    }

    #[test]
    fn happy_path_accept_delivers_once() {
        let mut c = RunController::accept(task(), RunPolicy::default()).unwrap();
        c.begin_planning().unwrap();
        c.enter_plan_review().unwrap();
        c.plan_accepted().unwrap();
        c.work_finished().unwrap();
        c.reviews_finished().unwrap();
        c.synthesis_finished().unwrap();
        let d = c.apply_quality_gate(&accept_report("done")).unwrap();
        assert_eq!(d.action, GateAction::Accept);
        assert_eq!(c.state, RunState::Succeeded);
        let t = c.terminal_payload().unwrap();
        assert_eq!(t.terminal, DeliveryTerminal::Succeeded);
        assert_eq!(t.answer.as_deref(), Some("done"));
        // second terminal rejected
        assert!(c.begin_planning().is_err());
    }

    #[test]
    fn patch_then_accept_respects_pmax() {
        let mut policy = RunPolicy::minimal();
        policy.max_patch_attempts = 2;
        let mut c = RunController::accept(task(), policy).unwrap();
        c.begin_planning().unwrap();
        c.enter_plan_review().unwrap();
        c.plan_accepted().unwrap();
        c.work_finished().unwrap();
        c.reviews_finished().unwrap();
        c.synthesis_finished().unwrap();
        let d1 = c.apply_quality_gate(&patch_report()).unwrap();
        assert_eq!(d1.action, GateAction::PatchWorker);
        assert_eq!(c.state, RunState::Executing);
        assert_eq!(c.counters.patch_attempts, 1);
        // complete another cycle
        c.work_finished().unwrap();
        c.reviews_finished().unwrap();
        c.synthesis_finished().unwrap();
        let d2 = c.apply_quality_gate(&patch_report()).unwrap();
        assert_eq!(d2.action, GateAction::PatchWorker);
        assert_eq!(c.counters.patch_attempts, 2);
        c.work_finished().unwrap();
        c.reviews_finished().unwrap();
        c.synthesis_finished().unwrap();
        // exhausted → fail
        let d3 = c.apply_quality_gate(&patch_report()).unwrap();
        assert_eq!(d3.action, GateAction::FailClosed);
        assert_eq!(c.state, RunState::Failed);
    }

    #[test]
    fn cancel_forces_terminal() {
        let mut c = RunController::accept(task(), RunPolicy::default()).unwrap();
        c.begin_planning().unwrap();
        c.cancel();
        assert_eq!(c.state, RunState::Cancelled);
        assert!(c.terminal_payload().is_some());
        assert!(c.cancel.is_cancelled());
    }

    #[test]
    fn illegal_transition_rejected() {
        let mut c = RunController::accept(task(), RunPolicy::default()).unwrap();
        assert!(c.work_finished().is_err());
    }

    #[test]
    fn token_budget_exhaustion_fails_closed() {
        let mut policy = RunPolicy::default();
        policy.max_budget_tokens = 100;
        policy.delivery_reserve_pct = 10; // usable 90
        let mut task = task();
        task.max_budget_tokens = 100;
        let mut c = RunController::accept(task, policy).unwrap();
        c.begin_planning().unwrap();
        c.record_tokens(90).unwrap();
        assert_eq!(c.state, RunState::Failed);
    }

    #[test]
    fn single_terminal_slot() {
        let mut c = RunController::accept(task(), RunPolicy::default()).unwrap();
        c.cancel();
        let first = c.terminal_payload().unwrap().terminal;
        c.cancel(); // idempotent-ish: already terminal
        assert_eq!(c.terminal_payload().unwrap().terminal, first);
        assert_eq!(
            c.events
                .iter()
                .filter(|e| e.state.is_terminal())
                .count(),
            1
        );
    }
}
