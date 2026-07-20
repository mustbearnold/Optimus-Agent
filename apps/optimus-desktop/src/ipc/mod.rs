//! Shared IPC facade for WebView and HTTP modes.

mod chat;
mod contract;
mod files;
mod os;
mod router;
mod runtime_ops;
mod scheduling;
mod sessions;
mod system;

pub(crate) use chat::{
    chat_turn, chat_turn_cancellable, stream_delivery_control, stream_event_to_json,
};
pub(crate) use contract::{IpcEnvelope, IpcReply};
pub(crate) use os::pick_folder_dialog;
pub(crate) use router::handle_ipc;
pub(crate) use sessions::sessions_json;
pub(crate) use system::{auth_status_json, doctor_json};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use optimus_kernel::StreamEvent;
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
            stream_event_to_json(&StreamEvent::ToolStatus {
                name: "web_search".into(),
                detail: "run".into()
            }),
            json!({"type":"tool","name":"web_search","detail":"run"})
        );
        assert_eq!(
            stream_event_to_json(&StreamEvent::Status("working".into())),
            json!({"type":"status","text":"working"})
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
