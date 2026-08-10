//! spec-017 Discord conformance: the adapter contract driven by the scripted
//! mock transport (never the live gateway), plus one loopback REST test that
//! proves the outbound request shape against a real socket.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use optimus_ops::adapters::discord::{
    open_adapter, DiscordAdapter, DiscordConfig, DiscordTransport, LiveDiscordTransport,
    MockDiscordTransport,
};
use optimus_ops::{
    adapter_cycle, list_ambiguous_sends, list_inbox, list_outbox_receipts, list_transport_events,
    InboundMessage, SendOutcome, TransportAdapter,
};
use serde_json::{json, Value};
use tempfile::tempdir;

/// Mirror of `gateway_turn`: the address comes back unchanged and an outbound
/// obligation is written exactly when an address is returned.
fn reply(message: &InboundMessage) -> Result<(String, Option<String>), String> {
    Ok((
        format!("reply:{}", message.text),
        message.session_id.clone(),
    ))
}

fn adapter(config: DiscordConfig, mock: MockDiscordTransport) -> DiscordAdapter {
    DiscordAdapter::with_transport(config, Box::new(mock))
}

fn mock_with(messages: &[(&str, &str, bool)]) -> MockDiscordTransport {
    let mut mock = MockDiscordTransport::new();
    for (channel, text, is_bot) in messages {
        mock.push(channel, text, *is_bot);
    }
    mock
}

fn enabled_config(channels: &[&str]) -> DiscordConfig {
    DiscordConfig {
        enabled: true,
        bot_token_env: "OPTIMUS_DISCORD_BOT_TOKEN".to_string(),
        allowed_channel_ids: channels.iter().map(|channel| channel.to_string()).collect(),
        poll_hold_secs: 25,
        intents: 5120,
    }
}

/// A1: inbound → turn → reply → receipt completes end-to-end; both replies
/// are delivered through the mock and the inbox ends empty.
#[test]
fn full_cycle_delivers_replies_exactly_once() {
    let dir = tempdir().unwrap();
    let mock = mock_with(&[("42", "hello discord", false), ("7", "second", false)]);
    let sent = Arc::clone(&mock.sent);
    let mut adapter = adapter(enabled_config(&["42", "7"]), mock);

    let result = adapter_cycle(dir.path(), &mut adapter, reply).unwrap();

    assert_eq!(result.enqueued.len(), 2, "two inbound enqueued");
    assert_eq!(result.drained.len(), 2, "both turns drained");
    assert_eq!(result.receipts.len(), 2, "two terminal receipts");
    assert!(result.ambiguous.is_empty());
    assert!(result.failed_sends.is_empty());
    assert!(result.refused.is_empty());
    let mut received = sent.lock().unwrap().clone();
    received.sort();
    assert_eq!(
        received,
        vec![
            ("42".to_string(), "reply:hello discord".to_string()),
            ("7".to_string(), "reply:second".to_string()),
        ]
    );
    assert_eq!(list_outbox_receipts(dir.path(), 10).unwrap().len(), 2);
    assert!(list_inbox(dir.path()).unwrap().is_empty());
}

/// A4/R6: a message from a channel outside the allowlist is refused before
/// any turn and recorded with the named diagnostic; nothing reaches the inbox.
#[test]
fn unauthorized_channel_is_refused_with_diagnostic() {
    let dir = tempdir().unwrap();
    let mut adapter = adapter(
        enabled_config(&["42"]),
        mock_with(&[("stranger", "hi", false)]),
    );

    let result = adapter_cycle(dir.path(), &mut adapter, reply).unwrap();

    assert!(result.enqueued.is_empty());
    assert_eq!(result.refused.len(), 1);
    assert!(list_inbox(dir.path()).unwrap().is_empty());
    let events = list_transport_events(dir.path(), Some("discord"), 20).unwrap();
    assert!(events.iter().any(|event| {
        event.kind == "inbound_refused" && event.detail.contains("transport_refused_unauthorized")
    }));
}

/// R5: bot-authored messages are skipped at the adapter, so only the human
/// message drives a turn and a reply.
#[test]
fn bot_messages_are_skipped() {
    let dir = tempdir().unwrap();
    let mock = mock_with(&[("42", "bot noise", true), ("42", "human message", false)]);
    let sent = Arc::clone(&mock.sent);
    let mut adapter = adapter(enabled_config(&["42"]), mock);

    let result = adapter_cycle(dir.path(), &mut adapter, reply).unwrap();

    assert_eq!(result.enqueued.len(), 1);
    assert_eq!(result.drained.len(), 1);
    assert_eq!(
        sent.lock().unwrap().clone(),
        vec![("42".to_string(), "reply:human message".to_string())]
    );
    assert_eq!(list_outbox_receipts(dir.path(), 10).unwrap().len(), 1);
}

/// R5: an enabled adapter with an empty allowlist fails closed at poll time;
/// `is_allowed` itself stays permissive for an empty allowlist (telegram
/// semantics — the poll error is the gate).
#[test]
fn enabled_with_empty_allowlist_fails_closed() {
    let dir = tempdir().unwrap();
    let mut adapter = adapter(enabled_config(&[]), MockDiscordTransport::new());

    let error = adapter.poll_inbound(dir.path()).unwrap_err();
    assert!(error.contains("fail closed"), "unexpected error: {error}");
    assert!(adapter.is_allowed("any-channel"));
}

/// AdapterBuilder convention: absent config opens None, malformed config is an
/// error, and a disabled config still opens an adapter that reports disabled.
#[test]
fn open_adapter_gates_on_config_file() {
    let dir = tempdir().unwrap();
    assert!(open_adapter(dir.path()).unwrap().is_none());

    let gateway = dir.path().join("gateway");
    std::fs::create_dir_all(&gateway).unwrap();

    std::fs::write(gateway.join("discord.json"), "{not json").unwrap();
    assert!(open_adapter(dir.path()).is_err(), "malformed config errors");

    std::fs::write(
        gateway.join("discord.json"),
        json!({ "enabled": false, "allowed_channel_ids": [] }).to_string(),
    )
    .unwrap();
    let adapter = open_adapter(dir.path())
        .unwrap()
        .expect("disabled config still opens an adapter");
    assert!(!adapter.is_enabled(dir.path()));
    assert_eq!(adapter.transport().as_str(), "discord");
}

/// R5/R6: config round-trips through JSON and missing keys take defaults.
#[test]
fn config_round_trips_and_defaults_apply() {
    let config = enabled_config(&["42", "7"]);
    let raw = serde_json::to_string(&config).unwrap();
    let back: DiscordConfig = serde_json::from_str(&raw).unwrap();
    assert_eq!(config, back);

    let minimal: DiscordConfig = serde_json::from_str(r#"{"enabled": true}"#).unwrap();
    assert_eq!(minimal.bot_token_env, "OPTIMUS_DISCORD_BOT_TOKEN");
    assert_eq!(minimal.poll_hold_secs, 25);
    // DIRECT_MESSAGES (1<<12) + GUILD_MESSAGES (1<<9) — the minimal set the
    // adapter needs; reactions/typing etc. are out of scope for v1.
    assert_eq!(minimal.intents, 4608);
    assert!(minimal.allowed_channel_ids.is_empty());
}

/// Terminal-outcome law: a scripted platform refusal settles failed and a
/// scripted network uncertainty settles ambiguous — never both, never a
/// receipt for a failure.
#[test]
fn scripted_send_outcomes_settle_terminal() {
    let dir = tempdir().unwrap();

    let mut failed_mock = MockDiscordTransport::new();
    failed_mock.push("42", "ping", false);
    failed_mock.fail_next_send();
    let result = adapter_cycle(
        dir.path(),
        &mut adapter(enabled_config(&["42"]), failed_mock),
        reply,
    )
    .unwrap();
    assert_eq!(result.failed_sends.len(), 1);
    assert!(result.receipts.is_empty());
    assert!(list_ambiguous_sends(dir.path(), 10).unwrap().is_empty());

    let mut ambiguous_mock = MockDiscordTransport::new();
    ambiguous_mock.push("42", "ping", false);
    ambiguous_mock.ambiguous_next_send();
    let result = adapter_cycle(
        dir.path(),
        &mut adapter(enabled_config(&["42"]), ambiguous_mock),
        reply,
    )
    .unwrap();
    assert_eq!(result.ambiguous.len(), 1);
    assert_eq!(list_ambiguous_sends(dir.path(), 10).unwrap().len(), 1);
}

/// Wire shape of the outbound REST call, checked against a real loopback
/// socket: path, method, `Authorization: Bot <token>`, and JSON body.
#[test]
fn rest_outbound_shape_is_verified_on_a_real_socket() {
    const TOKEN_ENV: &str = "OPTIMUS_DISCORD_CONFORMANCE_TOKEN";
    std::env::set_var(TOKEN_ENV, "test-token");

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server = Arc::new(tiny_http::Server::http(addr).unwrap());
    let base = format!("http://{}", server.server_addr());
    // One recorded REST call: method, url, headers, body.
    type Recorded = (String, String, Vec<(String, String)>, String);
    let recorded: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));

    let worker = Arc::clone(&server);
    let log = Arc::clone(&recorded);
    std::thread::spawn(move || {
        for mut request in worker.incoming_requests() {
            let method = request.method().to_string();
            let url = request.url().to_string();
            let headers: Vec<(String, String)> = request
                .headers()
                .iter()
                .map(|header| {
                    (
                        header.field.as_str().to_string(),
                        header.value.as_str().to_string(),
                    )
                })
                .collect();
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let first = log.lock().unwrap().is_empty();
            let (status, payload) = if first {
                (200, r#"{"id":"12345"}"#)
            } else {
                (404, r#"{"message":"Unknown Channel"}"#)
            };
            log.lock().unwrap().push((method, url, headers, body));
            let response = tiny_http::Response::from_string(payload)
                .with_status_code(status)
                .with_header(
                    "Content-Type: application/json"
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                );
            let _ = request.respond(response);
        }
    });

    let mut transport = LiveDiscordTransport::with_api_base(TOKEN_ENV, 25, 5120, &base);

    let outcome = transport.send_message("42", "hello").unwrap();
    assert_eq!(
        outcome,
        SendOutcome::Confirmed {
            provider_message_id: "12345".to_string()
        }
    );

    // A 4xx is a definite platform refusal, never an Err.
    let refused = transport.send_message("404", "nope").unwrap();
    assert!(matches!(refused, SendOutcome::Failed { .. }));

    let log = recorded.lock().unwrap().clone();
    assert_eq!(log.len(), 2);
    let (method, url, headers, body) = &log[0];
    assert_eq!(method, "POST");
    assert_eq!(url, "/channels/42/messages");
    assert!(headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("authorization") && value == "Bot test-token"
    }));
    assert!(headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("content-type") && value == "application/json"
    }));
    assert_eq!(
        serde_json::from_str::<Value>(body).unwrap(),
        json!({ "content": "hello" })
    );
}
