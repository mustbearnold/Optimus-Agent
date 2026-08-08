//! Stable request, response, and message contracts shared by model adapters.

use optimus_packs::ToolDesc as ToolSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CancellationToken, KernelError, Result};

/// Fail fast when the shared cancellation token has been set. Every model
/// boundary and tool-loop check observes the same token (ADR-0046).
pub(crate) fn check_cancellation(token: &CancellationToken) -> Result<()> {
    if token.is_cancelled() {
        Err(KernelError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    User,
    System,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// DeepSeek V4 requires assistant reasoning to be replayed with a tool call.
    /// This remains optional so older persisted sessions deserialize unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
    /// Reasoning effort: low | medium | high | xhigh | max | ultra (None = omit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Prefer faster completions when true (may lower effort floor).
    #[serde(default)]
    pub fast_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CompletionResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    /// Provider-private reasoning that must be carried into a follow-up tool turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}
