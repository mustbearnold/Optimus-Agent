//! Durable approval bindings (spec-014 R4/R7, ADR-0081).
//!
//! Split out of `lib.rs` to keep the kernel within its module-size baseline.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use optimus_packs::ToolId;

/// Binds a parked approval to the exact effect that produced it (ADR-0046:
/// the approved call is never re-derived).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolApprovalBinding {
    pub run_id: Uuid,
    pub call_id: String,
    pub tool_id: ToolId,
    pub job_id: optimus_runtime::JobId,
    pub node_id: Uuid,
    pub node_index: u32,
    pub effect_sha256: String,
    pub summary: String,
    /// `CommandClass` discriminator of the exact pending effect (spec-014 R7),
    /// derived from the settled effect. `None` for non-command effects; the
    /// UI offers session consent only when this is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_class: Option<String>,
}
