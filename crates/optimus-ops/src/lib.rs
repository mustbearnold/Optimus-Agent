//! Operator services for Optimus: durable gateway and cron schedule store.
//!
//! These are intentionally outside the turn-loop waist. Surfaces and the kernel
//! may depend on them; they must not depend on `optimus-kernel`.

mod cron;
mod gateway;

pub use cron::{CronAttemptView, CronClaim, CronError, CronJob, CronStore};
pub use gateway::{
    acknowledge_delivery, cancel_claim, claim_one, complete_claim, delivery_state, drain_one,
    enqueue, fail_claim, list_inbox, list_outbox, reconcile, release_claim, renew_claim,
    DrainResult, GatewayClaim, GatewayError, GatewayPaths, InboundMessage, OutboundMessage,
};
