use optimus_memory::{Sensitivity, TrustDomain, WriteContext};
use optimus_packs::PackBudgetConfig;
use serde_json::Value;
use std::path::Path;

use crate::{ChildCoordinator, CompressionConfig};

/// Host-owned bridge for the agent-facing self-development tool.
///
/// The kernel never constructs a command or widens a path through this hook;
/// the host callback must route into its already-enforced Developer Full Access
/// supervisor. `None` keeps the affordance out of model tool schemas.
pub type SelfDevelopmentHandler = fn(&Path, &Value) -> Result<String, String>;

#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Model round-trips one turn may take, spanning any approval pause.
    pub max_steps: u32,
    pub max_tool_calls_per_step: usize,
    pub pack_budget: PackBudgetConfig,
    pub memory_ctx: WriteContext,
    pub compression: CompressionConfig,
    /// Reasoning effort: auto/None lets the provider choose; off disables
    /// reasoning where the provider supports an explicit disabled mode.
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
    /// Optional host-owned self-development supervisor bridge.
    pub self_development: Option<SelfDevelopmentHandler>,
    /// Recursive-children config (spec-034). `children_max_depth` is
    /// the depth limit (R3): default 1, so a parent may spawn children
    /// but a child may not spawn its own. `children` is the daemon
    /// bridge; `None` means the kernel is not daemon-backed and spawn
    /// refuses with a diagnostic (R4, A9).
    pub children_max_depth: u32,
    pub children: Option<std::sync::Arc<dyn ChildCoordinator>>,
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
            self_development: None,
            children_max_depth: 1,
            children: None,
        }
    }
}
