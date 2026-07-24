//! Chat provider selection and stream serialization.

use std::path::PathBuf;

use optimus_kernel::{
    CancellationToken, ChatApprovalDecision, ChatApprovalStatus, CodexOAuthConfig, CodexOAuthModel,
    CompletionResponse, Kernel, KernelConfig, OpenAiCompatConfig, OpenAiCompatModel, ProviderId,
    RouteRequest, RouteSurface, ScriptedModel, StreamControl, StreamEvent, ToolCall,
};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(method, "chat" | "chat_offline" | "chat_approval_resolve")
}

pub(crate) fn stream_delivery_control(delivered: bool) -> StreamControl {
    if delivered {
        StreamControl::Continue
    } else {
        StreamControl::Cancel
    }
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "chat" | "chat_offline" => chat_turn(home, params, None),
        "chat_approval_resolve" => chat_approval_resolve(home, params),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Resolve one previously-emitted exact approval binding.
///
/// The renderer can only present opaque IDs and the persisted effect digest; it
/// never supplies a filesystem root or an executable effect. The kernel
/// re-opens the Rust-authorized project scope and verifies the complete binding
/// again immediately before it mutates the durable job/turn state.
fn chat_approval_resolve(
    home: &PathBuf,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let session_id = required_uuid(&params, "session_id")?;
    let run_id = required_uuid(&params, "run_id")?;
    let job_id = required_uuid(&params, "job_id")?;
    let node_id = required_uuid(&params, "node_id")?;
    let call_id = required_call_id(&params)?;
    let node_index = required_node_index(&params)?;
    let effect_sha256 = required_effect_sha256(&params)?;
    let decision = parse_approval_decision(&params)?;
    let project_id = optional_project_id(&params)?;

    let config = KernelConfig::default();
    let mut kernel = match project_id.as_deref() {
        Some(project_id) => {
            Kernel::open_project_session(home, config, Some(session_id), project_id)
        }
        None => Kernel::open_session(home, config, Some(session_id)),
    }
    .map_err(|error| error.to_string())?;

    let resolution = kernel
        .resolve_chat_approval_exact(
            run_id,
            &call_id,
            optimus_runtime::job_id(job_id),
            node_id,
            node_index,
            &effect_sha256,
            decision,
        )
        .map_err(|error| error.to_string())?;
    let status = match resolution.status {
        ChatApprovalStatus::Approved => "approved",
        ChatApprovalStatus::Denied => "denied",
    };
    let binding = resolution.binding;
    let tool_id = binding.tool_id.as_str().to_string();

    // Deliberately return a small presentation model rather than serializing
    // the durable protocol event, which may grow internal-only fields.
    Ok(json!({
        "session_id": kernel.session_id().to_string(),
        "title": kernel.session_title(),
        "run_id": binding.run_id.to_string(),
        "call_id": binding.call_id,
        "job_id": binding.job_id.to_string(),
        "node_id": binding.node_id.to_string(),
        "node_index": binding.node_index,
        "effect_sha256": binding.effect_sha256,
        "tool_id": tool_id,
        "summary": binding.summary,
        "status": status,
    }))
}

fn required_uuid(params: &serde_json::Value, field: &str) -> Result<uuid::Uuid, String> {
    let value = params
        .get(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("{field} required"))?;
    uuid::Uuid::parse_str(value).map_err(|_| format!("{field} must be a UUID"))
}

fn required_call_id(params: &serde_json::Value) -> Result<String, String> {
    let value = params
        .get("call_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "call_id required".to_string())?;
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err("call_id must be 1-256 ASCII identifier bytes".into());
    }
    Ok(value.to_string())
}

fn required_node_index(params: &serde_json::Value) -> Result<u32, String> {
    let value = params
        .get("node_index")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "node_index required".to_string())?;
    u32::try_from(value).map_err(|_| "node_index is out of range".to_string())
}

fn required_effect_sha256(params: &serde_json::Value) -> Result<String, String> {
    let value = params
        .get("effect_sha256")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "effect_sha256 required".to_string())?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("effect_sha256 must be a 64-character hexadecimal digest".into());
    }
    Ok(value.to_ascii_lowercase())
}

fn optional_project_id(params: &serde_json::Value) -> Result<Option<String>, String> {
    let Some(value) = params.get("project_id") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| "project_id must be a string".to_string())?;
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("project_id must be a 1-128 ASCII identifier".into());
    }
    Ok(Some(value.to_string()))
}

fn parse_approval_decision(params: &serde_json::Value) -> Result<ChatApprovalDecision, String> {
    match params.get("decision").and_then(|value| value.as_str()) {
        Some("approve") => Ok(ChatApprovalDecision::Approve),
        // The presentation layer cannot inject unbounded, user-controlled text
        // into durable authorization records. A later product flow can expose
        // a separately bounded reason field with its own review contract.
        Some("deny") => Ok(ChatApprovalDecision::Deny {
            reason: "user_denied_in_transcript".into(),
        }),
        _ => Err("decision must be approve or deny".into()),
    }
}

/// Run a chat turn, optionally emitting stream events via `on_event`.
pub(crate) fn chat_turn(
    home: &PathBuf,
    params: serde_json::Value,
    on_event: Option<&mut dyn FnMut(StreamEvent) -> StreamControl>,
) -> Result<serde_json::Value, String> {
    let cancellation = CancellationToken::new();
    chat_turn_cancellable(home, params, on_event, &cancellation)
}

pub(crate) fn chat_turn_cancellable(
    home: &PathBuf,
    params: serde_json::Value,
    mut on_event: Option<&mut dyn FnMut(StreamEvent) -> StreamControl>,
    cancellation: &CancellationToken,
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

    let config = KernelConfig {
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
        effect_policy: if params.get("access").and_then(|value| value.as_str()) == Some("full") {
            optimus_graph::PolicyMode::Unrestricted
        } else {
            optimus_graph::PolicyMode::SmartDeny
        },
        ..KernelConfig::default()
    };
    let mut kernel = match params.get("project_id").and_then(|value| value.as_str()) {
        Some(project_id) => Kernel::open_project_session(home, config, session, project_id),
        None => Kernel::open_session(home, config, session),
    }
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
        on_event
            .as_mut()
            .map_or(StreamControl::Continue, |callback| callback(ev))
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
                .turn_with_controlled_sink_cancellable(
                    &mut model,
                    &message,
                    &mut sink,
                    cancellation,
                )
                .map_err(|e| e.to_string())?
        }
        ProviderId::OpenAiCompat => {
            let mut cfg = OpenAiCompatConfig::from_env().map_err(|e| e.to_string())?;
            if let Some(m) = model_override.clone() {
                cfg.model = m;
            }
            let mut provider = OpenAiCompatModel::new(cfg);
            kernel
                .turn_with_controlled_sink_cancellable(
                    &mut provider,
                    &message,
                    &mut sink,
                    cancellation,
                )
                .map_err(|e| e.to_string())?
        }
        ProviderId::Codex => {
            let mut cfg = CodexOAuthConfig::from_env(home);
            cfg.model = route.model.as_str().into();
            let mut provider = CodexOAuthModel::new(cfg).map_err(|e| e.to_string())?;
            kernel
                .turn_with_controlled_sink_cancellable(
                    &mut provider,
                    &message,
                    &mut sink,
                    cancellation,
                )
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
        "timings": result.timings,
        "provider": used_provider,
    }))
}

pub(crate) fn stream_event_to_json(ev: &StreamEvent) -> serde_json::Value {
    match ev {
        StreamEvent::TextDelta(t) => json!({"type": "delta", "text": t}),
        StreamEvent::Tool(tool) => {
            let mut value = serde_json::to_value(tool).unwrap_or_default();
            if let Some(object) = value.as_object_mut() {
                object.insert("type".into(), json!("tool"));
            }
            value
        }
        StreamEvent::Status(s) => json!({"type": "status", "text": s}),
        StreamEvent::Timing(timing) => {
            let mut value = serde_json::to_value(timing).unwrap_or_default();
            if let Some(object) = value.as_object_mut() {
                object.insert("type".into(), json!("timing"));
            }
            value
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        optional_project_id, parse_approval_decision, required_call_id, required_effect_sha256,
        required_node_index, required_uuid,
    };

    #[test]
    fn approval_resolution_identifiers_are_narrow_and_canonical() {
        let id = uuid::Uuid::new_v4();
        let params = json!({
            "session_id": id.to_string(),
            "call_id": "call-01:tool.write",
            "node_id": uuid::Uuid::new_v4().to_string(),
            "node_index": 7,
            "effect_sha256": "A".repeat(64),
            "project_id": "optimus-agent_1",
        });
        assert_eq!(required_uuid(&params, "session_id").unwrap(), id);
        assert!(required_uuid(&params, "node_id").is_ok());
        assert_eq!(required_call_id(&params).unwrap(), "call-01:tool.write");
        assert_eq!(required_node_index(&params).unwrap(), 7);
        assert_eq!(required_effect_sha256(&params).unwrap(), "a".repeat(64));
        assert_eq!(
            optional_project_id(&params).unwrap().as_deref(),
            Some("optimus-agent_1")
        );
    }

    #[test]
    fn approval_resolution_rejects_malformed_or_path_like_authority() {
        for params in [
            json!({"call_id": "../call"}),
            json!({"call_id": "call with space"}),
            json!({"node_index": u64::from(u32::MAX) + 1}),
            json!({"effect_sha256": "not-a-digest"}),
            json!({"project_id": "../workspace"}),
            json!({"project_id": "/tmp/project"}),
        ] {
            if params.get("call_id").is_some() {
                assert!(required_call_id(&params).is_err());
            }
            if params.get("node_index").is_some() {
                assert!(required_node_index(&params).is_err());
            }
            if params.get("effect_sha256").is_some() {
                assert!(required_effect_sha256(&params).is_err());
            }
            if params.get("project_id").is_some() {
                assert!(optional_project_id(&params).is_err());
            }
        }
    }

    #[test]
    fn approval_decision_is_explicit_and_uses_a_bounded_deny_reason() {
        assert!(parse_approval_decision(&json!({"decision": "approve"})).is_ok());
        assert!(parse_approval_decision(&json!({"decision": "deny"})).is_ok());
        for params in [json!({"decision": "later"}), json!({})] {
            assert!(parse_approval_decision(&params).is_err());
        }
    }
}
