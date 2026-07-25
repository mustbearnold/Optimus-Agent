//! Versioned workflow contracts, durable run ledger, and built-in specialist DAG verticals.
//!
//! Design-P0 orchestration envelopes + in-memory `RunController` live in
//! `orchestrator_envelopes` / `run_controller`. That controller is **not** the
//! durable ADR-0033 `WorkflowRunStore` path and does not execute tools or
//! spawn models.

mod orchestrator_envelopes;
mod run_controller;
mod specialist_vertical;
mod workflow;
mod workflow_run;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("uuid: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("runtime: {0}")]
    Runtime(#[from] optimus_runtime::RuntimeError),
    #[error("agent: {0}")]
    Agent(#[from] optimus_agent::AgentError),
    #[error("artifact: {0}")]
    Artifact(#[from] optimus_artifacts::ArtifactError),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, WorkflowError>;

pub use optimus_agent::{
    AgentArtifactRef, AgentBudget, AgentDescriptor, AgentFailure, AgentId, AgentInvocation,
    AgentInvocationEvent, AgentInvocationStatus, AgentInvocationStore, AgentPermissions,
    AgentRegistry, AgentRequest, AgentResult, AgentResultKind, AgentVersion,
    AGENT_REQUEST_SCHEMA_VERSION, AGENT_RESULT_SCHEMA_VERSION,
};
pub use optimus_artifacts::{ArtifactRecord, ArtifactStore, BulkDeleteFailure, BulkDeleteResult};
pub use orchestrator_envelopes::*;
pub use run_controller::*;
pub use specialist_vertical::*;
pub use workflow::*;
pub use workflow_run::*;
