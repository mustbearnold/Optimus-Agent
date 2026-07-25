//! Versioned multi-agent orchestration envelopes (design P0).
//!
//! These contracts are pure data + validation. They do not execute models or
//! tools, do not grant SmartDeny approvals, and free-form `tool_ids` /
//! `specialist_ids` strings are **not** capability grants (hosts must resolve
//! against registries when wiring workers). Schema versions are independent of
//! `WORKFLOW_SCHEMA_VERSION`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{Result, WorkflowError};

pub const TASK_SPEC_SCHEMA_VERSION: u16 = 1;
pub const PLAN_BUNDLE_SCHEMA_VERSION: u16 = 1;
pub const PLAN_REVIEW_BALLOT_SCHEMA_VERSION: u16 = 1;
pub const WORK_RESULT_SCHEMA_VERSION: u16 = 1;
pub const REVIEW_BALLOT_SCHEMA_VERSION: u16 = 1;
pub const SYNTHESIS_REPORT_SCHEMA_VERSION: u16 = 1;
pub const GATE_DECISION_SCHEMA_VERSION: u16 = 1;
pub const DELIVERY_PAYLOAD_SCHEMA_VERSION: u16 = 1;

const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_SUMMARY_BYTES: usize = 16 * 1024;
const MAX_LIST: usize = 256;

fn invalid(msg: impl Into<String>) -> WorkflowError {
    WorkflowError::Msg(msg.into())
}

fn require_nonempty(label: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{label} must be non-empty")));
    }
    if value.len() > MAX_TEXT_BYTES {
        return Err(invalid(format!("{label} exceeds {MAX_TEXT_BYTES} bytes")));
    }
    Ok(())
}

fn require_list_cap(label: &str, len: usize) -> Result<()> {
    if len > MAX_LIST {
        return Err(invalid(format!("{label} exceeds {MAX_LIST} entries")));
    }
    Ok(())
}

/// Content hash for envelope integrity (hex sha256 of canonical JSON bytes).
pub fn content_sha256_json(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(WorkflowError::Json)?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProducerRole {
    Ingress,
    Planner,
    PlanReviewer,
    Worker,
    Reviewer,
    Synthesizer,
    RunController,
    Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanReviewVerdict {
    Accept,
    Revise,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLens {
    Correctness,
    Security,
    Completeness,
    Evidence,
    StyleOptional,
    Feasibility,
    SecurityPolicy,
}

impl ReviewLens {
    pub fn is_blocking_by_default(self) -> bool {
        !matches!(self, Self::StyleOptional)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateAction {
    Accept,
    PatchWorker,
    Replan,
    FailClosed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryTerminal {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub claim: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
    #[serde(default)]
    pub blocking: bool,
}

impl Finding {
    pub fn validate(&self) -> Result<()> {
        require_nonempty("finding.severity", &self.severity)?;
        require_nonempty("finding.claim", &self.claim)?;
        if self.claim.len() > MAX_SUMMARY_BYTES {
            return Err(invalid("finding.claim too large"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSpec {
    pub schema_version: u16,
    pub task_id: Uuid,
    pub user_text: String,
    pub surface: String,
    #[serde(default)]
    pub privacy: String,
    pub max_budget_tokens: u64,
    pub max_wall_ms: u64,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_scope_ref: Option<String>,
    #[serde(default)]
    pub approval_policy: String,
}

impl TaskSpec {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != TASK_SPEC_SCHEMA_VERSION {
            return Err(invalid("TaskSpec schema_version mismatch"));
        }
        require_nonempty("user_text", &self.user_text)?;
        require_nonempty("surface", &self.surface)?;
        require_list_cap("required_capabilities", self.required_capabilities.len())?;
        if self.max_budget_tokens == 0 {
            return Err(invalid("max_budget_tokens must be > 0"));
        }
        if self.max_wall_ms == 0 {
            return Err(invalid("max_wall_ms must be > 0"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanStep {
    pub step_id: String,
    pub intent: String,
    #[serde(default)]
    pub tool_ids: Vec<String>,
    #[serde(default)]
    pub specialist_ids: Vec<String>,
    pub success_criteria: String,
    pub risk_class: RiskClass,
    #[serde(default)]
    pub estimated_tokens: u64,
}

impl PlanStep {
    pub fn validate(&self) -> Result<()> {
        require_nonempty("step_id", &self.step_id)?;
        require_nonempty("intent", &self.intent)?;
        require_nonempty("success_criteria", &self.success_criteria)?;
        if self.tool_ids.is_empty() && self.specialist_ids.is_empty() {
            return Err(invalid("plan step needs tool_ids or specialist_ids"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanBundle {
    pub schema_version: u16,
    pub plan_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_plan_id: Option<Uuid>,
    pub goals: Vec<String>,
    pub steps: Vec<PlanStep>,
    #[serde(default)]
    pub stop_conditions: Vec<String>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(default)]
    pub evidence_needs: Vec<String>,
}

impl PlanBundle {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PLAN_BUNDLE_SCHEMA_VERSION {
            return Err(invalid("PlanBundle schema_version mismatch"));
        }
        if self.goals.is_empty() {
            return Err(invalid("PlanBundle.goals must be non-empty"));
        }
        if self.steps.is_empty() {
            return Err(invalid("PlanBundle.steps must be non-empty"));
        }
        require_list_cap("steps", self.steps.len())?;
        for step in &self.steps {
            step.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanReviewBallot {
    pub schema_version: u16,
    pub plan_id: Uuid,
    pub reviewer_id: String,
    pub lens: ReviewLens,
    pub verdict: PlanReviewVerdict,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub required_revisions: Vec<String>,
}

impl PlanReviewBallot {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PLAN_REVIEW_BALLOT_SCHEMA_VERSION {
            return Err(invalid("PlanReviewBallot schema_version mismatch"));
        }
        require_nonempty("reviewer_id", &self.reviewer_id)?;
        require_list_cap("findings", self.findings.len())?;
        for f in &self.findings {
            f.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub status: String,
    #[serde(default)]
    pub tool_outcome_refs: Vec<String>,
    #[serde(default)]
    pub effect_links: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkResult {
    pub schema_version: u16,
    pub plan_id: Uuid,
    pub work_attempt_id: Uuid,
    pub step_results: Vec<StepResult>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    #[serde(default)]
    pub self_check: String,
}

impl WorkResult {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != WORK_RESULT_SCHEMA_VERSION {
            return Err(invalid("WorkResult schema_version mismatch"));
        }
        if self.step_results.is_empty() {
            return Err(invalid("WorkResult.step_results must be non-empty"));
        }
        require_list_cap("step_results", self.step_results.len())?;
        for s in &self.step_results {
            require_nonempty("step_id", &s.step_id)?;
            require_nonempty("status", &s.status)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewBallot {
    pub schema_version: u16,
    pub work_attempt_id: Uuid,
    pub reviewer_id: String,
    pub lens: ReviewLens,
    pub verdict: ReviewVerdict,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

impl ReviewBallot {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != REVIEW_BALLOT_SCHEMA_VERSION {
            return Err(invalid("ReviewBallot schema_version mismatch"));
        }
        require_nonempty("reviewer_id", &self.reviewer_id)?;
        for f in &self.findings {
            f.validate()?;
        }
        Ok(())
    }

    pub fn has_blocking_failure(&self) -> bool {
        matches!(self.verdict, ReviewVerdict::Fail | ReviewVerdict::Inconclusive)
            && self.lens.is_blocking_by_default()
            || self.findings.iter().any(|f| f.blocking)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthesisReport {
    pub schema_version: u16,
    pub work_attempt_id: Uuid,
    pub plan_id: Uuid,
    #[serde(default)]
    pub merged_findings: Vec<Finding>,
    pub blocking_count: u32,
    #[serde(default)]
    pub pass_lenses: Vec<ReviewLens>,
    #[serde(default)]
    pub fail_lenses: Vec<ReviewLens>,
    pub recommended_action: GateAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_brief: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replan_brief: Option<String>,
    pub answer_draft: String,
    #[serde(default)]
    pub evidence_index: Vec<String>,
}

impl SynthesisReport {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SYNTHESIS_REPORT_SCHEMA_VERSION {
            return Err(invalid("SynthesisReport schema_version mismatch"));
        }
        // accept/fail_closed/cancelled allowed as recommendations; patch/replan need briefs
        match self.recommended_action {
            GateAction::PatchWorker => {
                let b = self
                    .patch_brief
                    .as_deref()
                    .ok_or_else(|| invalid("patch_worker requires patch_brief"))?;
                require_nonempty("patch_brief", b)?;
            }
            GateAction::Replan => {
                let b = self
                    .replan_brief
                    .as_deref()
                    .ok_or_else(|| invalid("replan requires replan_brief"))?;
                require_nonempty("replan_brief", b)?;
            }
            GateAction::Accept | GateAction::FailClosed | GateAction::Cancelled => {}
        }
        if self.recommended_action == GateAction::Accept {
            require_nonempty("answer_draft", &self.answer_draft)?;
        }
        for f in &self.merged_findings {
            f.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptCounters {
    pub patch_attempts: u32,
    pub replan_attempts: u32,
    pub plan_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateDecision {
    pub schema_version: u16,
    pub action: GateAction,
    pub reason_code: String,
    pub attempt_counters: AttemptCounters,
    pub next_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_visible_summary: Option<String>,
}

impl GateDecision {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != GATE_DECISION_SCHEMA_VERSION {
            return Err(invalid("GateDecision schema_version mismatch"));
        }
        require_nonempty("reason_code", &self.reason_code)?;
        require_nonempty("next_state", &self.next_state)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryPayload {
    pub schema_version: u16,
    pub run_id: Uuid,
    pub terminal: DeliveryTerminal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub cost_summary: Value,
}

impl DeliveryPayload {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != DELIVERY_PAYLOAD_SCHEMA_VERSION {
            return Err(invalid("DeliveryPayload schema_version mismatch"));
        }
        if self.terminal == DeliveryTerminal::Succeeded {
            let a = self
                .answer
                .as_deref()
                .ok_or_else(|| invalid("succeeded delivery requires answer"))?;
            require_nonempty("answer", a)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_task() -> TaskSpec {
        TaskSpec {
            schema_version: TASK_SPEC_SCHEMA_VERSION,
            task_id: Uuid::new_v4(),
            user_text: "do the thing".into(),
            surface: "cli".into(),
            privacy: "local".into(),
            max_budget_tokens: 100_000,
            max_wall_ms: 60_000,
            required_capabilities: vec![],
            project_scope_ref: None,
            approval_policy: "smart_deny".into(),
        }
    }

    #[test]
    fn task_spec_rejects_zero_budget() {
        let mut t = sample_task();
        t.max_budget_tokens = 0;
        assert!(t.validate().is_err());
    }

    #[test]
    fn plan_bundle_requires_steps() {
        let p = PlanBundle {
            schema_version: PLAN_BUNDLE_SCHEMA_VERSION,
            plan_id: Uuid::new_v4(),
            parent_plan_id: None,
            goals: vec!["g".into()],
            steps: vec![],
            stop_conditions: vec![],
            open_questions: vec![],
            evidence_needs: vec![],
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn synthesis_patch_requires_brief() {
        let s = SynthesisReport {
            schema_version: SYNTHESIS_REPORT_SCHEMA_VERSION,
            work_attempt_id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            merged_findings: vec![],
            blocking_count: 1,
            pass_lenses: vec![],
            fail_lenses: vec![ReviewLens::Correctness],
            recommended_action: GateAction::PatchWorker,
            patch_brief: None,
            replan_brief: None,
            answer_draft: String::new(),
            evidence_index: vec![],
        };
        assert!(s.validate().is_err());
    }

    #[test]
    fn delivery_succeeded_requires_answer() {
        let d = DeliveryPayload {
            schema_version: DELIVERY_PAYLOAD_SCHEMA_VERSION,
            run_id: Uuid::new_v4(),
            terminal: DeliveryTerminal::Succeeded,
            answer: None,
            warnings: vec![],
            evidence_refs: vec![],
            trace_id: None,
            cost_summary: json!({}),
        };
        assert!(d.validate().is_err());
    }

    #[test]
    fn content_hash_is_stable() {
        let v = json!({"a":1,"b":"x"});
        let h1 = content_sha256_json(&v).unwrap();
        let h2 = content_sha256_json(&v).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }
}
