//! Operator services for Optimus: durable gateway, cron, and surface catalogs.
//!
//! These are intentionally outside the turn-loop waist. Surfaces and the kernel
//! may depend on them; they must not depend on `optimus-kernel`.

mod cron;
mod gateway;
mod surface_commands;

pub use cron::{CronAttemptView, CronClaim, CronError, CronJob, CronStore};
pub use gateway::{
    acknowledge_delivery, cancel_claim, claim_one, complete_claim, delivery_state, drain_one,
    enqueue, fail_claim, list_inbox, list_outbox, reconcile, release_claim, renew_claim,
    DrainResult, GatewayClaim, GatewayError, GatewayPaths, InboundMessage, OutboundMessage,
};
pub use surface_commands::{
    builtin_surface_commands, commands_for_surface, CommandSurface, SurfaceCommand,
};
