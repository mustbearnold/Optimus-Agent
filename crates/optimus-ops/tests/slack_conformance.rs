//! spec-017 Slack conformance: the Slack adapter on the shared
//! claim→turn→settle spine, driven by a scripted mock Socket Mode transport.
//!
//! Every test is hermetic — tempdir homes, no real Slack API, no sockets.
//! The mock is a mock of the transport, so the adapter, its allowlist, the
//! inbound filter, and the cycle contract are what these tests exercise.

use std::sync::{Arc, Mutex};

use optimus_ops::adapters::slack::{
    load_slack_config, open_adapter, save_slack_config, MockSlackTransport, SlackAdapter,
    SlackConfig,
};
use optimus_ops::{
    adapter_cycle, list_inbox, list_outbox_receipts, list_transport_events, InboundMessage,
    TransportAdapter,
};
use tempfile::tempdir;

/// Standard reply closure: echo the message back with a `reply:` prefix and
/// keep the routing address so the ledger owes the send to the right channel.
fn echo_reply(message: &InboundMessage) -> Result<(String, Option<String>), String> {
    Ok((
        format!("reply:{}", message.text),
        message.session_id.clone(),
    ))
}

/// An enabled config whose allowlist admits every channel the tests script.
fn enabled_config(channels: &[&str]) -> SlackConfig {
    SlackConfig {
        enabled: true,
        allowed_channel_ids: channels.iter().map(|c| c.to_string()).collect(),
        reconnect_secs: Some(1),
        ..SlackConfig::default()
    }
}

/// A1: two inbound messages run end-to-end — enqueued, turned, replied to via
/// the recorded transport send, settled with terminal receipts, inbox empty.
#[test]
fn full_cycle_enqueues_turns_and_delivers_replies() {
    let dir = tempdir().unwrap();
    let mut mock = MockSlackTransport::new();
    let sent: Arc<Mutex<Vec<(String, String)>>> = mock.sent.clone();
    mock.push("C1", "first");
    mock.push("C2", "second");
    let config = enabled_config(&["C1", "C2"]);
    let mut adapter = SlackAdapter::with_transport(config, Box::new(mock));
    assert_eq!(adapter.transport().as_str(), "slack");

    let result = adapter_cycle(dir.path(), &mut adapter, echo_reply).unwrap();

    assert_eq!(result.enqueued.len(), 2, "both messages enqueued");
    assert_eq!(result.drained.len(), 2, "both turns ran");
    assert_eq!(
        result.receipts.len(),
        2,
        "both replies delivered and settled"
    );
    assert!(result.refused.is_empty());
    assert!(result.ambiguous.is_empty());
    assert!(result.failed_sends.is_empty());

    // Both replies reached the transport, addressed to their channels.
    let mut recorded = sent.lock().unwrap().clone();
    recorded.sort();
    assert_eq!(
        recorded,
        vec![
            ("C1".to_string(), "reply:first".to_string()),
            ("C2".to_string(), "reply:second".to_string()),
        ]
    );

    // Exactly two terminal receipts; nothing left in the inbox.
    assert_eq!(list_outbox_receipts(dir.path(), 10).unwrap().len(), 2);
    assert!(list_inbox(dir.path()).unwrap().is_empty());
}

/// A2: a message from a channel outside the allowlist is refused before any
/// turn and recorded with the named diagnostic; nothing reaches the inbox.
#[test]
fn unauthorized_channel_refused_with_named_diagnostic() {
    let dir = tempdir().unwrap();
    let mut mock = MockSlackTransport::new();
    mock.push("C2", "intruder");
    let config = enabled_config(&["C1"]);
    let mut adapter = SlackAdapter::with_transport(config, Box::new(mock));

    let result = adapter_cycle(dir.path(), &mut adapter, echo_reply).unwrap();

    assert!(result.enqueued.is_empty());
    assert_eq!(result.refused.len(), 1);
    assert_eq!(result.refused[0], "C2");
    let events = list_transport_events(dir.path(), Some("slack"), 20).unwrap();
    assert!(events.iter().any(
        |e| e.kind == "inbound_refused" && e.detail.contains("transport_refused_unauthorized")
    ));
    assert!(list_inbox(dir.path()).unwrap().is_empty());
    assert!(list_outbox_receipts(dir.path(), 10).unwrap().is_empty());
}

/// Inbound filtering: bot-authored messages (`bot_id`, `bot_message` subtype)
/// and empty text never reach the queue; only the human message turns.
#[test]
fn bot_and_empty_messages_are_skipped() {
    let dir = tempdir().unwrap();
    let mut mock = MockSlackTransport::new();
    mock.push_bot("C1", "bot rambling");
    mock.push_subtype("C1", "channel_join noise", "bot_message");
    mock.push("C1", "   ");
    mock.push("C1", "human text");
    let sent = mock.sent.clone();
    let config = enabled_config(&["C1"]);
    let mut adapter = SlackAdapter::with_transport(config, Box::new(mock));

    let result = adapter_cycle(dir.path(), &mut adapter, echo_reply).unwrap();

    assert_eq!(result.enqueued.len(), 1, "only the human message enqueued");
    assert_eq!(result.drained.len(), 1);
    let recorded = sent.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![("C1".to_string(), "reply:human text".to_string())]
    );
    assert!(list_inbox(dir.path()).unwrap().is_empty());
}

/// R5/R6 config surface: `open_adapter` is `None` without a config file,
/// `Some` (even when disabled) when the file exists, `Err` on malformed JSON.
#[test]
fn open_adapter_respects_config_presence_and_shape() {
    let dir = tempdir().unwrap();

    // Absent config => no adapter.
    assert!(open_adapter(dir.path()).unwrap().is_none());

    // Present but disabled => adapter exists and reports disabled.
    let config = SlackConfig {
        enabled: false,
        ..SlackConfig::default()
    };
    save_slack_config(dir.path(), &config).unwrap();
    let adapter = open_adapter(dir.path())
        .unwrap()
        .expect("adapter present whenever slack.json exists");
    assert!(!adapter.is_enabled(dir.path()));

    // Malformed config => Err, not a silently empty adapter.
    std::fs::write(dir.path().join("gateway/slack.json"), "{not json").unwrap();
    assert!(open_adapter(dir.path()).is_err());
}

/// R5: config survives a JSON round-trip, defaults fill minimal documents,
/// and the on-disk helpers keep the file readable back identically.
#[test]
fn slack_config_round_trips_through_json() {
    let config = enabled_config(&["C1", "C2"]);

    let raw = serde_json::to_string(&config).unwrap();
    let parsed: SlackConfig = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed, config);

    // A hand-written config with only the interesting keys fills defaults
    // under the snake_case JSON keys the contract names.
    let minimal: SlackConfig =
        serde_json::from_str(r#"{"enabled": true, "allowed_channel_ids": ["C9"]}"#).unwrap();
    assert_eq!(minimal.app_token_env, "OPTIMUS_SLACK_APP_TOKEN");
    assert_eq!(minimal.bot_token_env, "OPTIMUS_SLACK_BOT_TOKEN");
    assert_eq!(minimal.reconnect_secs, None);

    let dir = tempdir().unwrap();
    save_slack_config(dir.path(), &config).unwrap();
    assert_eq!(load_slack_config(dir.path()).unwrap(), config);
}

/// R6: an enabled live adapter with an empty allowlist refuses to poll —
/// fail closed — mirroring the Telegram transport.
#[test]
fn enabled_adapter_with_empty_allowlist_fails_closed() {
    let dir = tempdir().unwrap();
    let config = SlackConfig {
        enabled: true,
        ..SlackConfig::default()
    };
    let mut adapter = SlackAdapter::with_transport(config, Box::new(MockSlackTransport::new()));

    let error = adapter.poll_inbound(dir.path()).unwrap_err();
    assert!(
        error.contains("fail closed"),
        "error says fail closed: {error}"
    );
}
