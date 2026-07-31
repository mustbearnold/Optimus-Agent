//! Chat provider selection and stream serialization.

use std::path::PathBuf;

use optimus_kernel::{
    drain_one, CancellationToken, ChatApprovalDecision, ChatApprovalStatus, CodexOAuthConfig,
    CodexOAuthModel, CompletionResponse, DrainResult, ExecutionManifest, ExecutionStore, Kernel,
    KernelConfig, OpenAiCompatConfig, OpenAiCompatModel, ProviderId, RouteRequest, RouteSurface,
    ScriptedModel, StreamControl, StreamEvent, ToolCall,
};
use serde_json::json;

use crate::scope::optional_project_id;

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
    let manifest = ExecutionStore::open(home.join("execution.db"))
        .map_err(|error| error.to_string())?
        .manifest(run_id)
        .map_err(|error| error.to_string())?;
    if manifest.session_id != session_id {
        return Err("approval continuation route is foreign to the session".into());
    }

    // Provider, model and authority belong to the durable paused turn, not to
    // whichever renderer happens to resolve it later. Old manifests migrate to
    // ReviewChanges. Break-glass also falls closed here: it must not survive a
    // desktop restart as durable authority (ADR-0044 §5).
    let access = resume_access_config(&manifest.autonomy_profile, &manifest.command_fs_envelope)?;
    let config = KernelConfig {
        effect_policy: access.policy,
        autonomy_profile: access.profile,
        command_fs_envelope: access.command_fs_envelope,
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
    let resumed = resume_settled_turn(home, &manifest, &mut kernel, &mut on_event, cancellation);

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
    manifest: &ExecutionManifest,
    kernel: &mut Kernel,
    on_event: &mut Option<&mut dyn FnMut(StreamEvent) -> StreamControl>,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let provider = ProviderId::parse(&manifest.provider).ok_or_else(|| {
        format!(
            "approval continuation has an unknown persisted provider: {}",
            manifest.provider
        )
    })?;

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

    let result = match provider {
        ProviderId::Offline => {
            let mut model = ScriptedModel::new(vec![CompletionResponse {
                text: Some("offline echo: the approved action settled".into()),
                tool_calls: vec![],
            }])
            .paced(offline_pace());
            kernel.resume_pending_turn_with_sink(&mut model, &mut sink, cancellation)
        }
        ProviderId::OpenAiCompat => {
            let mut cfg = OpenAiCompatConfig::from_env().map_err(|error| error.to_string())?;
            cfg.model = manifest.model.clone();
            let mut provider = OpenAiCompatModel::new(cfg);
            kernel.resume_pending_turn_with_sink(&mut provider, &mut sink, cancellation)
        }
        ProviderId::Codex => {
            let mut cfg = CodexOAuthConfig::from_env(home);
            cfg.model = manifest.model.clone();
            let mut provider = CodexOAuthModel::new(cfg).map_err(|error| error.to_string())?;
            kernel.resume_pending_turn_with_sink(&mut provider, &mut sink, cancellation)
        }
    };
    result
        .map(|turn| turn.assistant_text)
        .map_err(|error| error.to_string())
}

/// How long the offline model should take between chunks of its answer, from
/// `OPTIMUS_OFFLINE_LATENCY_MS`. Zero when unset, which is every real run.
///
/// The offline provider is a fake: no credentials, no network, no token spend,
/// and an answer in the same tick the turn starts. That last part is the
/// problem. Everything that only exists *while* a turn is in flight —
/// `Ctrl-C` interrupting it, the spinner turning, text arriving a piece at a
/// time — has no window a test can aim at, so the terminal gate can only prove
/// the idle half of each of those behaviours. A fake that can be told to take
/// its time is a better fake, not a test hook smuggled into production: real
/// providers are unaffected, and the default is the behaviour every existing
/// caller already gets.
///
/// A malformed value is ignored rather than fatal. This is a development
/// affordance, and refusing to launch over a typo in a variable that does not
/// exist in production would be the more surprising behaviour.
fn offline_pace() -> std::time::Duration {
    std::env::var("OPTIMUS_OFFLINE_LATENCY_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map_or(std::time::Duration::ZERO, std::time::Duration::from_millis)
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
/// `UnrestrictedHost` — spelled `unrestricted_host`, `unrestricted`, or `yolo`
/// for the CLI flag — pairs `PolicyMode::Unrestricted` with the explicit host
/// command envelope. Every other profile keeps SmartDeny and the configured
/// containment. `full` used to reach break-glass and deliberately no longer does
/// (#118): an ordinary-sounding word must not hand over the machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AccessConfig {
    profile: optimus_graph::AutonomyProfile,
    policy: optimus_graph::PolicyMode,
    command_fs_envelope: Option<optimus_graph::CommandFsEnvelope>,
}

pub(crate) fn access_config(raw: Option<&str>) -> AccessConfig {
    let profile = raw
        .and_then(optimus_graph::AutonomyProfile::parse)
        .unwrap_or(optimus_graph::AutonomyProfile::ReviewChanges);
    let (policy, command_fs_envelope) =
        if profile == optimus_graph::AutonomyProfile::UnrestrictedHost {
            (
                optimus_graph::PolicyMode::Unrestricted,
                Some(optimus_graph::CommandFsEnvelope::UnrestrictedHost),
            )
        } else {
            (optimus_graph::PolicyMode::SmartDeny, None)
        };
    AccessConfig {
        profile,
        policy,
        command_fs_envelope,
    }
}

fn resume_access_config(
    autonomy_profile: &str,
    command_fs_envelope: &str,
) -> Result<AccessConfig, String> {
    let profile = optimus_graph::AutonomyProfile::parse(autonomy_profile)
        .filter(|profile| profile.as_str() == autonomy_profile)
        .ok_or_else(|| "approval continuation has an invalid autonomy profile".to_string())?;
    let envelope = match command_fs_envelope {
        "confined" => optimus_graph::CommandFsEnvelope::Confined,
        "confined_no_network" => optimus_graph::CommandFsEnvelope::ConfinedNoNetwork,
        "unrestricted_host" => optimus_graph::CommandFsEnvelope::UnrestrictedHost,
        _ => return Err("approval continuation has an invalid command envelope".into()),
    };
    if profile == optimus_graph::AutonomyProfile::UnrestrictedHost {
        let mut access = access_config(Some("review_changes"));
        access.command_fs_envelope = Some(optimus_graph::CommandFsEnvelope::ConfinedNoNetwork);
        return Ok(access);
    }
    if envelope == optimus_graph::CommandFsEnvelope::UnrestrictedHost {
        return Err("approval continuation has inconsistent persisted authority".into());
    }
    let mut access = access_config(Some(profile.as_str()));
    access.command_fs_envelope = Some(envelope);
    Ok(access)
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
        .unwrap_or("auto")
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
        effect_policy: access.policy,
        autonomy_profile: access.profile,
        command_fs_envelope: access.command_fs_envelope,
        ..KernelConfig::default()
    };
    let mut kernel = match params.get("project_id").and_then(|value| value.as_str()) {
        Some(project_id) => Kernel::open_project_session(home, config, session, project_id),
        None => Kernel::open_session(home, config, session),
    }
    .map_err(|e| e.to_string())?;

    let route = optimus_kernel::resolve_route(
        home,
        &RouteRequest::standard(RouteSurface::Desktop, &provider, model_override.clone()),
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
            }
            .paced(offline_pace());
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
            let mut cfg = apply_resolved_openai_model(
                OpenAiCompatConfig::from_env().map_err(|e| e.to_string())?,
                route.model.as_str(),
            );
            if let Some(base_url) = params.get("base_url").and_then(|value| value.as_str()) {
                cfg.base_url = base_url.into();
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
        "model": route.model.as_str(),
    }))
}

/// Drain one gateway message through the host-owned kernel and canonical route.
pub fn drain_gateway_once(home: &PathBuf) -> Result<Option<DrainResult>, String> {
    let home_buf = home.clone();
    drain_one(home, |message| {
        let session = message
            .session_id
            .as_deref()
            .map(uuid::Uuid::parse_str)
            .transpose()
            .map_err(|error| error.to_string())?;
        let mut kernel = Kernel::open_session(&home_buf, KernelConfig::default(), session)
            .map_err(|error| error.to_string())?;
        let route = optimus_kernel::resolve_route(
            &home_buf,
            &RouteRequest::standard(RouteSurface::Gateway, &message.provider, None),
        )
        .map_err(|error| error.to_string())?;
        let result = match route.provider {
            ProviderId::Offline => {
                let mut model = ScriptedModel::new(vec![CompletionResponse {
                    text: Some(format!("[gateway:{}] {}", message.channel, message.text)),
                    tool_calls: vec![],
                }]);
                model.stream_chunks = false;
                kernel.turn(&mut model, &message.text)
            }
            ProviderId::Codex => {
                let mut config = CodexOAuthConfig::from_env(&home_buf);
                config.model = route.model.as_str().into();
                let mut model = CodexOAuthModel::new(config).map_err(|error| error.to_string())?;
                kernel.turn(&mut model, &message.text)
            }
            ProviderId::OpenAiCompat => {
                let config = apply_resolved_openai_model(
                    OpenAiCompatConfig::from_env().map_err(|error| error.to_string())?,
                    route.model.as_str(),
                );
                let mut model = OpenAiCompatModel::new(config);
                kernel.turn(&mut model, &message.text)
            }
        }
        .map_err(|error| error.to_string())?;
        Ok((result.assistant_text, Some(kernel.session_id().to_string())))
    })
    .map_err(|error| error.to_string())
}

fn apply_resolved_openai_model(
    mut config: OpenAiCompatConfig,
    resolved_model: &str,
) -> OpenAiCompatConfig {
    config.model = resolved_model.into();
    config
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
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct RemovedEnvVar {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl RemovedEnvVar {
        fn new(key: &'static str) -> Self {
            let original = std::env::var_os(key);
            std::env::remove_var(key);
            Self { key, original }
        }
    }

    impl Drop for RemovedEnvVar {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn access_defaults_to_review_changes_and_smart_deny() {
        use optimus_graph::{AutonomyProfile, PolicyMode};
        for raw in [None, Some(""), Some("garbage")] {
            let access = super::access_config(raw);
            assert_eq!(access.profile, AutonomyProfile::ReviewChanges);
            assert_eq!(access.policy, PolicyMode::SmartDeny);
            assert_eq!(access.command_fs_envelope, None);
        }
    }

    #[test]
    fn approval_resume_uses_the_persisted_envelope_and_fails_closed() {
        use optimus_graph::{AutonomyProfile, CommandFsEnvelope, PolicyMode};
        let isolated = super::resume_access_config("standard", "confined_no_network").unwrap();
        assert_eq!(isolated.profile, AutonomyProfile::Standard);
        assert_eq!(isolated.policy, PolicyMode::SmartDeny);
        assert_eq!(
            isolated.command_fs_envelope,
            Some(CommandFsEnvelope::ConfinedNoNetwork),
            "current product settings must not add network access to a paused turn"
        );

        let break_glass =
            super::resume_access_config("unrestricted_host", "unrestricted_host").unwrap();
        assert_eq!(break_glass.profile, AutonomyProfile::ReviewChanges);
        assert_eq!(break_glass.policy, PolicyMode::SmartDeny);
        assert_eq!(
            break_glass.command_fs_envelope,
            Some(CommandFsEnvelope::ConfinedNoNetwork),
            "break-glass authority must not survive a restart"
        );

        assert!(super::resume_access_config("standard", "unrestricted_host").is_err());
        assert!(super::resume_access_config("standard", "corrupt").is_err());
        assert!(super::resume_access_config("corrupt", "confined").is_err());
    }

    #[test]
    fn only_unrestricted_spellings_lift_smart_deny() {
        use optimus_graph::{AutonomyProfile, CommandFsEnvelope, PolicyMode};
        for raw in ["unrestricted_host", "unrestricted", "yolo"] {
            let access = super::access_config(Some(raw));
            assert_eq!(access.profile, AutonomyProfile::UnrestrictedHost, "{raw}");
            assert_eq!(access.policy, PolicyMode::Unrestricted, "{raw}");
            assert_eq!(
                access.command_fs_envelope,
                Some(CommandFsEnvelope::UnrestrictedHost),
                "{raw} must make the host-wide command reach explicit"
            );
        }
        for raw in ["standard", "ask", "read", "full_project"] {
            let access = super::access_config(Some(raw));
            assert_eq!(
                access.policy,
                PolicyMode::SmartDeny,
                "{raw} must keep SmartDeny"
            );
            assert_eq!(
                access.command_fs_envelope, None,
                "{raw} must keep the configured containment"
            );
        }
    }

    /// Every value the composer can send, and what it means here. The menu is
    /// checked against this vocabulary by
    /// `scripts/check-autonomy-profiles.py`; this pins what each one *does*,
    /// so a relabelled menu item cannot quietly change the authority behind it
    /// (#118).
    #[test]
    fn each_composer_profile_maps_to_its_own_authority() {
        use optimus_graph::{AutonomyProfile, CommandFsEnvelope, PolicyMode};
        let expected = [
            (
                "standard",
                AutonomyProfile::Standard,
                PolicyMode::SmartDeny,
                None,
            ),
            (
                "review_changes",
                AutonomyProfile::ReviewChanges,
                PolicyMode::SmartDeny,
                None,
            ),
            (
                "read_only",
                AutonomyProfile::ReadOnly,
                PolicyMode::SmartDeny,
                None,
            ),
            (
                "full_project",
                AutonomyProfile::FullProject,
                PolicyMode::SmartDeny,
                None,
            ),
            (
                "unrestricted_host",
                AutonomyProfile::UnrestrictedHost,
                PolicyMode::Unrestricted,
                Some(CommandFsEnvelope::UnrestrictedHost),
            ),
        ];
        for (raw, profile, policy, command_fs_envelope) in expected {
            assert_eq!(
                super::access_config(Some(raw)),
                super::AccessConfig {
                    profile,
                    policy,
                    command_fs_envelope,
                },
                "{raw}"
            );
        }
        // The word the old menu offered first is no longer a profile at all.
        assert_eq!(
            super::access_config(Some("full")),
            super::AccessConfig {
                profile: AutonomyProfile::ReviewChanges,
                policy: PolicyMode::SmartDeny,
                command_fs_envelope: None,
            },
            "a stale 'full' sender must fall closed, not receive the host"
        );
    }
    use serde_json::json;

    use super::{
        optional_project_id, parse_approval_decision, required_call_id, required_effect_sha256,
        required_node_index, required_uuid,
    };

    #[test]
    fn omitted_provider_uses_auto_and_fresh_home_stays_offline() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _key = RemovedEnvVar::new("OPTIMUS_API_KEY");
        let home = tempfile::tempdir().unwrap();

        let value = super::chat_turn(
            &home.path().to_path_buf(),
            json!({"message": "deterministic default"}),
            None,
        )
        .unwrap();

        assert_eq!(value["provider"], "offline");
        assert_eq!(
            value["assistant_text"],
            "offline echo: deterministic default"
        );
    }

    #[test]
    fn explicit_offline_provider_behavior_is_unchanged() {
        let home = tempfile::tempdir().unwrap();
        let value = super::chat_turn(
            &home.path().to_path_buf(),
            json!({"message": "explicit", "provider": "offline"}),
            None,
        )
        .unwrap();
        assert_eq!(value["provider"], "offline");
        assert_eq!(value["assistant_text"], "offline echo: explicit");
    }

    #[test]
    fn explicit_model_reaches_the_router_without_alias_sanitizing() {
        let home = tempfile::tempdir().unwrap();
        let error = super::chat_turn(
            &home.path().to_path_buf(),
            json!({
                "message": "must fail before a model call",
                "provider": "codex",
                // The adapter accepts this convenience alias, but it is not a
                // canonical model identity owned by the routing catalog.
                "model": "sol"
            }),
            None,
        )
        .unwrap_err();

        assert!(
            error.contains("model_not_owned_by_provider"),
            "the canonical router must reject the caller's exact value: {error}"
        );
        assert_eq!(
            optimus_kernel::route_decision_count(home.path()).unwrap(),
            0,
            "a rejected explicit model must not become a defaulted route"
        );
    }

    #[test]
    fn gateway_openai_adapter_uses_the_resolved_route_model() {
        let config = optimus_kernel::OpenAiCompatConfig {
            base_url: "https://example.invalid/v1".into(),
            api_key: "test-key".into(),
            model: "ambient-model".into(),
            organization: None,
            timeout_secs: 1,
        };

        assert_eq!(
            super::apply_resolved_openai_model(config, "routed-model").model,
            "routed-model"
        );
    }

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
    /// provoked the approval, over the same call the TUI and desktop make. The
    /// renderer cannot reroute or widen the process-reopened continuation.
    #[test]
    fn approval_resume_uses_the_durable_route_and_returns_the_agent_s_answer() {
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
                // Hostile/stale renderer routing is ignored. `parked_turn`
                // persisted an offline + ReviewChanges execution manifest.
                "provider": "codex",
                "model": "gpt-5.6-terra",
                "access": "unrestricted_host",
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
        assert_eq!(
            value["assistant_text"].as_str(),
            Some("offline echo: the approved action settled"),
            "resume must use the durable offline route, not renderer params: {value}"
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
