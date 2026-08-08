//! Stable security-denial codes for operator and UI surfaces.
//!
//! Model/tool errors remain human-readable strings in many paths; this module
//! maps known denial substrings and typed kernel errors to a closed code set
//! without inventing new runtime policy.

use serde::{Deserialize, Serialize};

use crate::KernelError;

/// Closed vocabulary of security / policy denials.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SecurityDenialCode {
    /// Path outside authorized roots or illegal shape.
    FsSandboxDeny,
    /// Secret basename policy (`.env`, keys, …).
    SecretBasenameDeny,
    /// Runtime path confinement / cap-std escape.
    PathEscape,
    /// SmartDeny awaiting exact-effect approval.
    ApprovalRequired,
    /// Browser/HTTP SSRF or scheme policy.
    NetworkSsrfDeny,
    /// Skill closed permission refused the action.
    SkillPermissionDeny,
    /// Pack/tool not available or not advertised.
    ToolUnavailable,
    /// Generic tool policy denial not further classified.
    ToolPolicyDeny,
}

impl SecurityDenialCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FsSandboxDeny => "fs_sandbox_deny",
            Self::SecretBasenameDeny => "secret_basename_deny",
            Self::PathEscape => "path_escape",
            Self::ApprovalRequired => "approval_required",
            Self::NetworkSsrfDeny => "network_ssrf_deny",
            Self::SkillPermissionDeny => "skill_permission_deny",
            Self::ToolUnavailable => "tool_unavailable",
            Self::ToolPolicyDeny => "tool_policy_deny",
        }
    }
}

/// Map a kernel error to a stable denial code when it is a security/policy fence.
pub fn classify_security_denial(error: &KernelError) -> Option<SecurityDenialCode> {
    match error {
        KernelError::Runtime(runtime) => {
            let text = runtime.to_string().to_ascii_lowercase();
            if text.contains("needsapproval") || text.contains("awaiting_approval") {
                return Some(SecurityDenialCode::ApprovalRequired);
            }
            if text.contains("path escape") || text.contains("pathescape") {
                return Some(SecurityDenialCode::PathEscape);
            }
            classify_message(&text)
        }
        KernelError::Tool(message) | KernelError::Model(message) => {
            classify_message(&message.to_ascii_lowercase())
        }
        KernelError::Skills(error) => {
            let text = error.to_string().to_ascii_lowercase();
            if text.contains("permission") || text.contains("authorize") {
                Some(SecurityDenialCode::SkillPermissionDeny)
            } else {
                None
            }
        }
        KernelError::Packs(error) => {
            let text = error.to_string().to_ascii_lowercase();
            if text.contains("unavailable") || text.contains("unknown tool") {
                Some(SecurityDenialCode::ToolUnavailable)
            } else {
                None
            }
        }
        KernelError::Browser(error) => {
            let text = error.to_string().to_ascii_lowercase();
            if text.contains("ssrf")
                || text.contains("loopback")
                || text.contains("private")
                || text.contains("scheme")
                || text.contains("blocked")
            {
                Some(SecurityDenialCode::NetworkSsrfDeny)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn classify_message(message: &str) -> Option<SecurityDenialCode> {
    if message.contains("secret path denied")
        || message.contains("secret basename")
        || (message.contains(".env") && (message.contains("denied") || message.contains("refuse")))
    {
        return Some(SecurityDenialCode::SecretBasenameDeny);
    }
    if message.contains("path not allowed")
        || message.contains("outside root")
        || message.contains("filesystem root")
    {
        return Some(SecurityDenialCode::FsSandboxDeny);
    }
    if message.contains("path escape") || message.contains("pathescape") {
        return Some(SecurityDenialCode::PathEscape);
    }
    if (message.contains("approval")
        && (message.contains("required")
            || message.contains("requires")
            || message.contains("awaiting")))
        || message.contains("awaiting_approval")
        || message.contains("needsapproval")
        || message.contains("needs approval")
        || message.contains("smartdeny")
    {
        return Some(SecurityDenialCode::ApprovalRequired);
    }
    if message.contains("ssrf")
        || message.contains("loopback")
        || message.contains("link-local")
        || message.contains("metadata")
    {
        return Some(SecurityDenialCode::NetworkSsrfDeny);
    }
    if message.contains("unavailable") || message.contains("not advertised") {
        return Some(SecurityDenialCode::ToolUnavailable);
    }
    if message.contains("denied") || message.contains("policy") || message.contains("not allowed") {
        return Some(SecurityDenialCode::ToolPolicyDeny);
    }
    None
}

/// Prefer a security denial code, else the generic kernel error code family.
pub fn kernel_or_security_code(error: &KernelError) -> &'static str {
    if let Some(code) = classify_security_denial(error) {
        return code.as_str();
    }
    match error {
        KernelError::Runtime(_) => "runtime_error",
        KernelError::Memory(_) => "memory_error",
        KernelError::Skills(_) => "skill_error",
        KernelError::Packs(_) => "pack_error",
        KernelError::Agent(_) => "agent_error",
        KernelError::Workflow(_) => "workflow_error",
        KernelError::Artifact(_) => "artifact_error",
        KernelError::Model(_) => "model_error",
        KernelError::Tool(_) => "tool_error",
        KernelError::MaxSteps(_) => "max_steps",
        KernelError::Io(_) => "io_error",
        KernelError::Json(_) => "json_error",
        KernelError::Sqlite(_) => "sqlite_error",
        KernelError::Uuid(_) => "uuid_error",
        KernelError::Browser(_) => "browser_error",
        KernelError::Cancelled => "turn_cancelled",
        KernelError::CronLeaseLost { .. } => "cron_lease_lost",
        KernelError::CronLeaseExpired { .. } => "cron_lease_expired",
        KernelError::Cron(_) => "cron_error",
        KernelError::GoalBudgetLimited { .. } => "goal_budget_limited",
        KernelError::Gateway(_) => "gateway_error",
        KernelError::Message(_) => "message_plane_error",
    }
}
