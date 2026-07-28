//! Durable, phased engineering runs in isolated git worktrees (ADR-0052).
//!
//! A development task is not a conversation. It is an object that owns a
//! branch, a base commit, a checkout of its own, a position in a fixed phase
//! table, and an append-only record of what it actually observed.
//!
//! Three rules hold this crate together:
//!
//! 1. **The model does not choose the next phase.** Transitions live in
//!    [`phase`], as a table in Rust.
//! 2. **Evidence, not assertion, advances a phase.** A phase exits when the
//!    facts its contract demands are already in the record — and evidence
//!    never carries over from another phase or an earlier attempt.
//! 3. **A run never writes the main checkout.** Its worktree is its boundary.
//!
//! Authority is still resolved through the capability broker in
//! `optimus-policy`; a phase contract is a ceiling, not a grant. This crate
//! deliberately knows nothing about GitHub — push, draft PRs and merge arrive
//! in later program P40 phases and stay behind explicit approval.

pub mod command;
pub mod controller;
pub mod phase;
pub mod run;
pub mod worktree;

pub use command::{
    CommandError, CommandOutcome, CommandRunner, ProcessRunner, MAX_CAPTURE_BYTES, SIGNAL_STATUS,
    TIMEOUT_STATUS,
};
pub use controller::{
    ControllerError, DriveOutcome, PhaseStep, RunDriver, StepOutcome, DEFAULT_STEP_TIMEOUT,
};
pub use phase::{DevPhase, EvidenceKind, PhaseAuthority, PhaseContract, TransitionError};
pub use run::{
    digest, DevTaskRun, EvidenceDraft, EvidenceItem, RunBudget, RunError, StopKind, StopReason,
    TaskOrigin, TaskPlan, RECORD_VERSION,
};
pub use worktree::{PreparedWorktree, RemovalOutcome, WorktreeError, WorktreeManager};
