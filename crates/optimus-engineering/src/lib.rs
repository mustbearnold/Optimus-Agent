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

pub mod catalogue;
pub mod command;
pub mod controller;
pub mod differential;
pub mod phase;
pub mod repository;
pub mod roles;
pub mod run;
pub mod triage;
pub mod worktree;

pub use catalogue::{plan_for, plans_for_run, PhasePlan, UnresolvedStep};
pub use command::{
    CommandError, CommandOutcome, CommandRunner, ProcessRunner, MAX_CAPTURE_BYTES, SIGNAL_STATUS,
    TIMEOUT_STATUS,
};
pub use controller::{
    ControllerError, DriveOutcome, PhaseStep, RunDriver, StepOutcome, DEFAULT_STEP_TIMEOUT,
};
pub use differential::{
    DifferentialError, DifferentialProof, DifferentialProver, DifferentialRequest,
    DifferentialVerdict,
};
pub use phase::{DevPhase, EvidenceKind, PhaseAuthority, PhaseContract, TransitionError};
pub use repository::{
    instruction_files_for, resolve_profile, BranchProtection, DeclaredPolicy, RepositoryError,
    RepositoryPolicyProfile, VerificationCommands,
};
pub use roles::{check_assertion, routing_for, Effort, Role, RoleError, RoleIdentity};
pub use run::{
    digest, DevTaskRun, EvidenceDraft, EvidenceItem, RunBudget, RunError, StopKind, StopReason,
    TaskOrigin, TaskPlan, RECORD_VERSION,
};
pub use triage::{
    check, AcceptanceCriterion, ChangeScope, RefusalReason, RiskClass, TriageContext, TriageLimits,
    TriageOutput, TriageResult, TriageVerdict, Ungrounded,
};
pub use worktree::{PreparedWorktree, RemovalOutcome, WorktreeError, WorktreeManager};
