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

pub fn stream_delivery_control(delivered: bool) -> StreamControl {
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
        "chat_approval_resolve" => chat_approval_resolve(home, params, None),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Resolve an approval and let the paused turn finish.
///
/// Settling is no longer the end of the turn (ADR-0046), so this is a streaming
/// turn like `chat` — surfaces that pass `on_event` see the continuation arrive.
pub fn chat_approval_resolve(
    home: &PathBuf,
    params: serde_json::Value,
    on_event: Option<&mut dyn FnMut(StreamEvent) -> StreamControl>,
) -> Result<serde_json::Value, String> {
    let cancellation = CancellationToken::new();
    chat_approval_resolve_cancellable(home, params, on_event, &cancellation)
}

/// Resolve one previously-emitted exact approval binding.
///
/// The renderer can only present opaque IDs and the persisted effect digest; it
/// never supplies a filesystem root or an executable effect. The kernel
/// re-opens the Rust-authorized project scope and verifies the complete binding
/// again immediately before it mutates the durable job/turn state.
pub fn chat_approval_resolve_cancellable(
    home: &PathBuf,
    params: serde_json::Value,
    mut on_event: Option<&mut dyn FnMut(StreamEvent) -> StreamControl>,
    cancellation: &CancellationToken,
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

    // The continuation may reach further effects, so it runs under the same
    // profile the paused turn did. Absent `access` that is ReviewChanges +
    // SmartDeny, so an omission parks again rather than widening the approval
    // the user just gave into a standing grant.
    let access = access_config(params.get("access").and_then(|value| value.as_str()));
    let config = KernelConfig {
        effect_policy: access.1,
        autonomy_profile: access.0,
        ..KernelConfig::default()
    };
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

    // Close the row the pause left open. The call parked mid-flight, so the last
    // lifecycle event a renderer saw for it was `approval_required`; the
    // continuation starts *after* the recorded tool result and so will never
    // mention this call again. Settlement produced its terminal event — emitting
    // it here is what stops the tool reading as still running forever
    // (ADR-0046).
    if let Some(callback) = on_event.as_mut() {
        if callback(StreamEvent::Tool(Box::new(resolution.event))) == StreamControl::Cancel {
            cancellation.cancel();
        }
    }

    // The decision is durable from here. A continuation that fails must not be
    // reported as a failed approval — the effect already ran and is receipted —
    // so its outcome is carried in the response rather than raised as an error.
    let resumed = resume_settled_turn(home, &params, &mut kernel, &mut on_event, cancellation);

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
        "assistant_text": resumed.as_ref().ok().map(|text| text.as_str()),
        "resume_error": resumed.as_ref().err(),
    }))
}

/// Finish the turn the approval paused, and return what the agent said.
///
/// `run_turn_loop` parked without finishing the turn and settlement left it that
/// way, so the accepted turn and its manifest are still Running and this is a
/// continuation of them — not a new turn, and not a new user message.
fn resume_settled_turn(
    home: &PathBuf,
    params: &serde_json::Value,
    kernel: &mut Kernel,
    on_event: &mut Option<&mut dyn FnMut(StreamEvent) -> StreamControl>,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let provider = params
        .get("provider")
        .and_then(|value| value.as_str())
        .unwrap_or("codex")
        .to_string();
    let model_override = params
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
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

    let mut sink = |event: StreamEvent| {
        let control = on_event
            .as_mut()
            .map_or(StreamControl::Continue, |callback| callback(event));
        // A consumer that stopped receiving is a consumer that is gone, and the
        // turn has no one left to finish for. Same rule the controlled sink
        // applies to a fresh turn.
        if control == StreamControl::Cancel {
            cancellation.cancel();
        }
    };

    let result = match route.provider {
        ProviderId::Offline => {
            let mut model = ScriptedModel::new(vec![CompletionResponse {
                text: Some("offline echo: the approved action settled".into()),
                tool_calls: vec![],
            }]);
            kernel.resume_pending_turn_with_sink(&mut model, &mut sink, cancellation)
        }
        ProviderId::OpenAiCompat => {
            let mut cfg = OpenAiCompatConfig::from_env().map_err(|error| error.to_string())?;
            if let Some(model) = model_override.clone() {
                cfg.model = model;
            }
            let mut provider = OpenAiCompatModel::new(cfg);
            kernel.resume_pending_turn_with_sink(&mut provider, &mut sink, cancellation)
        }
        ProviderId::Codex => {
            let mut cfg = CodexOAuthConfig::from_env(home);
            cfg.model = route.model.as_str().into();
            let mut provider = CodexOAuthModel::new(cfg).map_err(|error| error.to_string())?;
            kernel.resume_pending_turn_with_sink(&mut provider, &mut sink, cancellation)
        }
    };
    result
        .map(|turn| turn.assistant_text)
        .map_err(|error| error.to_string())
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
/// Map a surface's `access` string onto the ADR-0044 profile and effect policy.
///
/// Absent/unknown values stay ReviewChanges + SmartDeny (fail closed). Only
/// `UnrestrictedHost` — spelled `full`, `unrestricted`, or `yolo` — pairs with
/// `PolicyMode::Unrestricted`; every other profile keeps SmartDeny and lets the
/// capability broker decide per effect.
pub(crate) fn access_config(
    raw: Option<&str>,
) -> (optimus_graph::AutonomyProfile, optimus_graph::PolicyMode) {
    let profile = raw
        .and_then(optimus_graph::AutonomyProfile::parse)
        .unwrap_or(optimus_graph::AutonomyProfile::ReviewChanges);
    let policy = if profile == optimus_graph::AutonomyProfile::UnrestrictedHost {
        optimus_graph::PolicyMode::Unrestricted
    } else {
        optimus_graph::PolicyMode::SmartDeny
    };
    (profile, policy)
}

pub fn chat_turn(
    home: &PathBuf,
    params: serde_json::Value,
    on_event: Option<&mut dyn FnMut(StreamEvent) -> StreamControl>,
) -> Result<serde_json::Value, String> {
    let cancellation = CancellationToken::new();
    chat_turn_cancellable(home, params, on_event, &cancellation)
}

pub fn chat_turn_cancellable(
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

    let access = access_config(params.get("access").and_then(|value| value.as_str()));
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
        effect_policy: access.1,
        autonomy_profile: access.0,
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

pub fn stream_event_to_json(ev: &StreamEvent) -> serde_json::Value {
    match ev {
        StreamEvent::TextDelta(t) => json!({"type": "delta", "text": t}),
        StreamEvent::ThinkingDelta(t) => json!({"type": "thinking", "text": t}),
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

    #[test]
    fn access_defaults_to_review_changes_and_smart_deny() {
        use optimus_graph::{AutonomyProfile, PolicyMode};
        for raw in [None, Some(""), Some("garbage")] {
            let (profile, policy) = super::access_config(raw);
            assert_eq!(profile, AutonomyProfile::ReviewChanges);
            assert_eq!(policy, PolicyMode::SmartDeny);
        }
    }

    #[test]
    fn only_unrestricted_spellings_lift_smart_deny() {
        use optimus_graph::{AutonomyProfile, PolicyMode};
        for raw in ["full", "unrestricted", "yolo"] {
            let (profile, policy) = super::access_config(Some(raw));
            assert_eq!(profile, AutonomyProfile::UnrestrictedHost, "{raw}");
            assert_eq!(policy, PolicyMode::Unrestricted, "{raw}");
        }
        for raw in ["standard", "ask", "read", "full_project"] {
            let (_, policy) = super::access_config(Some(raw));
            assert_eq!(policy, PolicyMode::SmartDeny, "{raw} must keep SmartDeny");
        }
    }
    use serde_json::json;

    use super::{
        optional_project_id, parse_approval_decision, required_call_id, required_effect_sha256,
        required_node_index, required_uuid,
    };

    /// Park a real turn on a held effect and hand back its exact binding.
    ///
    /// Nothing is faked: the kernel runs a tool call that reaches a project
    /// write, SmartDeny holds it, and the binding is the one the surface would
    /// have been shown.
    fn parked_turn(
        home: &std::path::Path,
        project: &std::path::Path,
    ) -> (String, optimus_kernel::ToolApprovalBinding) {
        use optimus_kernel::{
            CompletionResponse, Kernel, KernelConfig, ProjectAuthorityStore, ScriptedModel,
            StreamEvent, ToolCall,
        };

        let authority = ProjectAuthorityStore::open(home).unwrap();
        let selection = authority.stage_native_selection(project).unwrap();
        authority
            .authorize_project(
                "project-a",
                std::slice::from_ref(&selection.path),
                Some(&selection.path),
                std::slice::from_ref(&selection.grant_token),
            )
            .unwrap();
        let mut kernel =
            Kernel::open_project_session(home, KernelConfig::default(), None, "project-a").unwrap();
        let mut model = ScriptedModel::new(vec![CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "write-1".into(),
                name: "write_file".into(),
                arguments: json!({"path":"src/proof.txt","contents":"safe"}),
            }],
        }]);
        let mut binding = None;
        let _ = kernel.turn_with_sink(&mut model, "write the proof", &mut |event| {
            if let StreamEvent::Tool(tool) = event {
                if let Some(found) = tool.approval {
                    binding = Some(found);
                }
            }
        });
        (
            kernel.session_id().to_string(),
            binding.expect("the held effect must produce a binding"),
        )
    }

    /// The whole point of ADR-0046: approving answers the question that
    /// provoked the approval, over the same call the TUI and desktop make.
    #[test]
    fn approving_runs_the_effect_and_returns_the_agent_s_answer() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let (session_id, binding) = parked_turn(home.path(), project.path());

        let mut streamed = Vec::new();
        let mut settled_phases = Vec::new();
        let settled_call = binding.call_id.clone();
        let mut on_event = |event: super::StreamEvent| {
            match event {
                super::StreamEvent::TextDelta(text) => streamed.push(text),
                super::StreamEvent::Tool(tool) if tool.call_id == settled_call => {
                    settled_phases.push(tool.phase);
                }
                _ => {}
            }
            super::StreamControl::Continue
        };
        let value = super::chat_approval_resolve(
            &home.path().to_path_buf(),
            json!({
                "session_id": session_id,
                "run_id": binding.run_id.to_string(),
                "call_id": binding.call_id,
                "job_id": binding.job_id.to_string(),
                "node_id": binding.node_id.to_string(),
                "node_index": binding.node_index,
                "effect_sha256": binding.effect_sha256,
                "decision": "approve",
                "project_id": "project-a",
                "provider": "offline",
            }),
            Some(&mut on_event),
        )
        .expect("resolving must succeed");

        assert_eq!(value["status"], "approved");
        assert!(
            value["resume_error"].is_null(),
            "the paused turn must finish: {}",
            value["resume_error"]
        );
        assert!(
            value["assistant_text"]
                .as_str()
                .is_some_and(|text| !text.is_empty()),
            "approving must produce an answer, not just a receipt: {value}"
        );
        assert!(
            !streamed.is_empty(),
            "the continuation must reach the surface as it arrives"
        );
        // The call parked mid-flight, and the continuation starts after its
        // recorded result and never mentions it again. Without a terminal
        // event here the row stays "running" for the rest of the session.
        assert_eq!(
            settled_phases,
            vec![optimus_kernel::ToolLifecyclePhase::Succeeded],
            "settling must close the row the pause left open"
        );
        assert_eq!(
            std::fs::read_to_string(project.path().join("src/proof.txt")).unwrap(),
            "safe",
            "the approved effect is the one that ran"
        );
    }

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
