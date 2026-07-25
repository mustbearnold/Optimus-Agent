//! Thin surface adapters (Track Z.4–Z.6): proxy, TUI, ACP.
//!
//! These are contract-level scaffolds with fail-closed bounds — not full product
//! UIs. Tests prove request routing and non-claims.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SurfaceError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("method not allowed: {0}")]
    Method(String),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, SurfaceError>;

/// OpenAI-compatible chat proxy request (Z.4) — offline echo only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyChatRequest {
    pub model: String,
    pub messages: Vec<ProxyMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<ProxyChoice>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyChoice {
    pub index: u32,
    pub message: ProxyMessage,
}

/// Offline OpenAI-compatible chat completion (no network).
pub fn proxy_chat_offline(req: &ProxyChatRequest) -> Result<ProxyChatResponse> {
    if req.messages.is_empty() {
        return Err(SurfaceError::Msg("messages required".into()));
    }
    let last = req.messages.last().unwrap();
    Ok(ProxyChatResponse {
        id: format!("chatcmpl-{}", req.model.chars().take(8).collect::<String>()),
        model: req.model.clone(),
        choices: vec![ProxyChoice {
            index: 0,
            message: ProxyMessage {
                role: "assistant".into(),
                content: format!("[optimus-proxy-offline] {}", last.content),
            },
        }],
        note: "Offline proxy scaffold — not a production OpenAI gateway.".into(),
    })
}

/// Headless TUI snapshot (Z.5) — approvals + chat lines for smoke.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TuiSnapshot {
    pub approvals_pending: usize,
    pub chat_lines: Vec<String>,
    pub mode: String,
}

pub fn tui_smoke_snapshot(approvals: usize, last_user: &str) -> TuiSnapshot {
    TuiSnapshot {
        approvals_pending: approvals,
        chat_lines: vec![
            format!("user: {last_user}"),
            "assistant: [tui-smoke] ready".into(),
        ],
        mode: "headless-smoke".into(),
    }
}

/// ACP/IDE thin adapter envelope (Z.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcpRequest {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AcpResponse {
    pub ok: bool,
    pub result: serde_json::Value,
}

pub fn acp_handle(req: &AcpRequest) -> Result<AcpResponse> {
    match req.method.as_str() {
        "initialize" => Ok(AcpResponse {
            ok: true,
            result: serde_json::json!({
                "name": "optimus-acp",
                "version": "0.1.0",
                "note": "Thin ACP adapter scaffold — not full IDE bridge.",
            }),
        }),
        "session/new" => Ok(AcpResponse {
            ok: true,
            result: serde_json::json!({ "session_id": "acp-session-scaffold" }),
        }),
        other => Err(SurfaceError::Method(other.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_tui_acp_smoke() {
        let resp = proxy_chat_offline(&ProxyChatRequest {
            model: "offline".into(),
            messages: vec![ProxyMessage {
                role: "user".into(),
                content: "hi".into(),
            }],
        })
        .unwrap();
        assert!(resp.choices[0].message.content.contains("hi"));
        let tui = tui_smoke_snapshot(1, "approve?");
        assert_eq!(tui.approvals_pending, 1);
        let acp = acp_handle(&AcpRequest {
            method: "initialize".into(),
            params: serde_json::json!({}),
        })
        .unwrap();
        assert!(acp.ok);
        assert!(acp_handle(&AcpRequest {
            method: "unknown".into(),
            params: serde_json::json!({}),
        })
        .is_err());
    }
}
