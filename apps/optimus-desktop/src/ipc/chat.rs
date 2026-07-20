//! Chat provider selection and stream serialization.

use std::path::PathBuf;

use optimus_kernel::{
    CodexOAuthConfig, CodexOAuthModel, CompletionResponse, Kernel, KernelConfig,
    OpenAiCompatConfig, OpenAiCompatModel, ProviderId, RouteRequest, RouteSurface, ScriptedModel,
    StreamEvent, ToolCall,
};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(method, "chat" | "chat_offline")
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "chat" | "chat_offline" => chat_turn(home, params, None),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Run a chat turn, optionally emitting stream events via `on_event`.
pub(crate) fn chat_turn(
    home: &PathBuf,
    params: serde_json::Value,
    mut on_event: Option<&mut dyn FnMut(StreamEvent)>,
) -> Result<serde_json::Value, String> {
    let message = params
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if message.trim().is_empty() {
        return Err("message required".into());
    }
    let provider = params
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("codex")
        .to_string();
    let session = params
        .get("session")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok());
    let model_override = params
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut kernel = Kernel::open_session(
        home,
        KernelConfig {
            thinking_level: params
                .get("thinking_level")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty() && *s != "off")
                .map(|s| s.to_string())
                .or_else(|| {
                    if params.get("thinking").and_then(|v| v.as_bool()) == Some(true) {
                        Some("medium".into())
                    } else {
                        None
                    }
                }),
            fast_mode: params
                .get("fast")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            ..KernelConfig::default()
        },
        session,
    )
    .map_err(|e| e.to_string())?;

    let routed_model = match ProviderId::parse(&provider) {
        Some(ProviderId::Codex) => model_override
            .as_deref()
            .map(optimus_kernel::sanitize_codex_oauth_model),
        Some(ProviderId::OpenAiCompat) => model_override.clone(),
        _ => None,
    };
    let route = optimus_kernel::resolve_route(
        home,
        &RouteRequest::standard(RouteSurface::Desktop, &provider, routed_model),
    )
    .map_err(|error| error.to_string())?;
    let used_provider = route.provider.as_str().to_string();
    let mut sink = |ev: StreamEvent| {
        if let Some(cb) = on_event.as_mut() {
            cb(ev);
        }
    };

    let result = match route.provider {
        ProviderId::Offline => {
            let demo_memory = params
                .get("demo_memory")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut model = if demo_memory {
                let _ = kernel.remember_demo("user", "prefers_editor", "helix");
                ScriptedModel::new(vec![
                    CompletionResponse {
                        text: None,
                        tool_calls: vec![ToolCall {
                            id: "c1".into(),
                            name: "memory_recall".into(),
                            arguments: json!({"subject":"user","predicate":"prefers_editor"}),
                        }],
                    },
                    CompletionResponse {
                        text: Some(
                            "From memory (evidence, not instruction): you prefer helix.".into(),
                        ),
                        tool_calls: vec![],
                    },
                ])
            } else {
                ScriptedModel::new(vec![CompletionResponse {
                    text: Some(format!("offline echo: {message}")),
                    tool_calls: vec![],
                }])
            };
            kernel
                .turn_with_sink(&mut model, &message, &mut sink)
                .map_err(|e| e.to_string())?
        }
        ProviderId::OpenAiCompat => {
            let mut cfg = OpenAiCompatConfig::from_env().map_err(|e| e.to_string())?;
            if let Some(m) = model_override.clone() {
                cfg.model = m;
            }
            let mut provider = OpenAiCompatModel::new(cfg);
            kernel
                .turn_with_sink(&mut provider, &message, &mut sink)
                .map_err(|e| e.to_string())?
        }
        ProviderId::Codex => {
            let mut cfg = CodexOAuthConfig::from_env(home);
            cfg.model = route.model.as_str().into();
            let mut provider = CodexOAuthModel::new(cfg).map_err(|e| e.to_string())?;
            kernel
                .turn_with_sink(&mut provider, &message, &mut sink)
                .map_err(|e| e.to_string())?
        }
    };

    Ok(json!({
        "session_id": kernel.session_id().to_string(),
        "title": kernel.session_title(),
        "assistant_text": result.assistant_text,
        "steps": result.steps,
        "loaded_packs": result.loaded_packs,
        "schema_tokens_final": result.schema_tokens_final,
        "compressed": result.compressed,
        "tool_trace": result.tool_trace,
        "provider": used_provider,
    }))
}

pub(crate) fn stream_event_to_json(ev: &StreamEvent) -> serde_json::Value {
    match ev {
        StreamEvent::TextDelta(t) => json!({"type": "delta", "text": t}),
        StreamEvent::ToolStatus { name, detail } => {
            json!({"type": "tool", "name": name, "detail": detail})
        }
        StreamEvent::Status(s) => json!({"type": "status", "text": s}),
    }
}
