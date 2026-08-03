use optimus_memory::{Sensitivity, TrustDomain, WriteContext};
use optimus_packs::PackBudgetConfig;

use crate::CompressionConfig;

#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Model round-trips one turn may take, spanning any approval pause.
    pub max_steps: u32,
    pub max_tool_calls_per_step: usize,
    pub pack_budget: PackBudgetConfig,
    pub memory_ctx: WriteContext,
    pub compression: CompressionConfig,
    /// Reasoning effort: low|medium|high|xhigh|max|ultra (None or "off" = omit).
    pub thinking_level: Option<String>,
    pub fast_mode: bool,
    /// SmartDeny by default; unrestricted is an explicit user/test choice.
    pub effect_policy: optimus_graph::PolicyMode,
    /// Per-turn ADR-0044 profile; ReviewChanges unless the surface asks.
    pub autonomy_profile: optimus_graph::AutonomyProfile,
    /// Overrides product-settings command FS envelope; `None` → settings.json work_isolation.
    pub command_fs_envelope: Option<optimus_graph::CommandFsEnvelope>,
    /// Optional in-memory override for the persisted Developer Full Access
    /// grant. None means load the product setting, not “allow everything”.
    pub developer_access: Option<optimus_policy::DeveloperAccessGrant>,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            max_steps: 32,
            max_tool_calls_per_step: 8,
            pack_budget: PackBudgetConfig::default(),
            memory_ctx: WriteContext {
                tenant: "local".into(),
                user: "user".into(),
                agent: "optimus".into(),
                project: "default".into(),
                principal: "user:local".into(),
                max_trust: TrustDomain::User,
                max_sensitivity: Sensitivity::Personal,
            },
            compression: CompressionConfig::default(),
            thinking_level: None,
            fast_mode: false,
            effect_policy: optimus_graph::PolicyMode::SmartDeny,
            autonomy_profile: optimus_graph::AutonomyProfile::ReviewChanges,
            command_fs_envelope: None,
            developer_access: None,
        }
    }
}
