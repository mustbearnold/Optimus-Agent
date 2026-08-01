//! Agent host: the exact IPC method registry and domain dispatch that every
//! Optimus surface speaks (ADR-0045).
//!
//! Extracted from `crates/optimus-host/src/` so a TUI or CLI can reach the
//! contract without depending on a desktop binary. `handle_ipc` is a pure
//! function of home + method + params, so it is transport-agnostic.

mod chat;
mod consoles;
mod contract;
mod extensibility;
mod files;
mod gateway_turn;
mod home;
mod messaging;
mod os;
mod router;
mod runtime_ops;
mod scheduling;
mod scope;
mod sessions;
mod system;

pub use chat::{
    chat_approval_resolve, chat_approval_resolve_cancellable, chat_turn, chat_turn_cancellable,
    stream_delivery_control, stream_event_to_json,
};
pub use contract::{IpcEnvelope, IpcReply};
pub use gateway_turn::{drain_gateway_once, gateway_turn, session_for_address};
pub use home::resolve_home;
pub use os::pick_folder_dialog;
pub use router::handle_ipc;
pub use sessions::sessions_json;
pub use system::{auth_status_json, doctor_json};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use optimus_kernel::{
        StreamEvent, TimingEvent, TimingEventKind, ToolLifecycleEvent, ToolLifecyclePhase,
    };
    use optimus_packs::{ReplayClass, ToolId, ToolOutcome};
    use serde_json::json;

    use super::{handle_ipc, stream_event_to_json, IpcEnvelope, IpcReply};

    #[test]
    fn envelope_defaults_omitted_params_to_null() {
        let envelope: IpcEnvelope = serde_json::from_value(json!({
            "id": 7,
            "method": "ping"
        }))
        .expect("envelope");
        assert_eq!(envelope.params, serde_json::Value::Null);
    }

    #[test]
    fn reply_omits_absent_result_or_error() {
        let success = serde_json::to_value(IpcReply {
            id: 1,
            ok: true,
            result: Some(json!({"pong": true})),
            error: None,
        })
        .expect("success reply");
        assert!(success.get("error").is_none());
        let failure = serde_json::to_value(IpcReply {
            id: 2,
            ok: false,
            result: None,
            error: Some("nope".into()),
        })
        .expect("failure reply");
        assert!(failure.get("result").is_none());
    }

    #[test]
    fn stream_event_shapes_are_stable() {
        assert_eq!(
            stream_event_to_json(&StreamEvent::TextDelta("x".into())),
            json!({"type":"delta","text":"x"})
        );
        assert_eq!(
            stream_event_to_json(&StreamEvent::ThinkingDelta("plan".into())),
            json!({"type":"thinking","text":"plan"})
        );
        let outcome = ToolOutcome::succeeded(
            "call-1",
            ToolId::new("web_search"),
            "Found 3 sources",
            json!({"result_count": 3}),
            ReplayClass::ExternalNondeterministic,
        );
        let tool = stream_event_to_json(&StreamEvent::Tool(Box::new(ToolLifecycleEvent {
            schema_version: 1,
            event_id: "run-1:call-1:succeeded".into(),
            run_id: "run-1".into(),
            call_id: "call-1".into(),
            tool_id: ToolId::new("web_search"),
            phase: ToolLifecyclePhase::Succeeded,
            summary: "Found 3 sources".into(),
            duration_ms: Some(17),
            outcome: Some(outcome),
            approval: None,
        })));
        assert_eq!(tool["type"], "tool");
        assert_eq!(tool["schema_version"], 1);
        assert_eq!(tool["event_id"], "run-1:call-1:succeeded");
        assert_eq!(tool["run_id"], "run-1");
        assert_eq!(tool["call_id"], "call-1");
        assert_eq!(tool["tool_id"], "web_search");
        assert_eq!(tool["phase"], "succeeded");
        assert_eq!(tool["summary"], "Found 3 sources");
        assert_eq!(tool["duration_ms"], 17);
        assert_eq!(tool["outcome"]["kind"], "succeeded");
        assert_eq!(tool["outcome"]["data"]["result_count"], 3);
        assert_eq!(
            stream_event_to_json(&StreamEvent::Status("working".into())),
            json!({"type":"status","text":"working"})
        );
        assert_eq!(
            stream_event_to_json(&StreamEvent::Timing(TimingEvent {
                kind: TimingEventKind::ToolFinished,
                step: Some(2),
                call_id: Some("call-1".into()),
                name: Some("web_search".into()),
                duration_ms: Some(17),
                elapsed_ms: 29,
                status: Some("succeeded".into()),
                suppressed: false,
            })),
            json!({
                "type":"timing","kind":"tool_finished","step":2,
                "call_id":"call-1","name":"web_search","duration_ms":17,
                "elapsed_ms":29,"status":"succeeded","suppressed":false
            })
        );
    }

    #[test]
    fn unknown_method_error_is_stable() {
        assert_eq!(
            handle_ipc(&PathBuf::from("unused"), "missing", json!({})),
            Err("unknown method: missing".into())
        );
    }
}
