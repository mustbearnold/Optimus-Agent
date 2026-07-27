//! What one effect attempt did, as the runtime reports it upward.
//!
//! Kept out of `lib.rs` under the module-size law: a caller reasoning about a
//! completed effect wants provenance and result together, and that pairing is
//! worth stating in one place rather than buried among the job machinery.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::JobId;

/// The terminal record of one effect attempt.
///
/// Every field except `receipt_json` is identity: which attempt, against which
/// node, of which effect, and how it ended. Those are what an approval binding
/// is checked against, and none of them says what the effect produced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectOutcome {
    pub attempt_id: Uuid,
    pub job_id: JobId,
    pub node_id: Uuid,
    pub effect_hash: String,
    pub status: String,
    pub receipt_hash: Option<String>,
    /// What the effect actually produced, as recorded in `effect_attempts`.
    ///
    /// The hash above stays the thing identities are bound to. The body is
    /// carried alongside it because a caller that has to report the outcome —
    /// to a person or to a model — cannot do that from a digest, and would
    /// otherwise be describing work it never observed (ADR-0046).
    pub receipt_json: Option<String>,
}
