//! What the live transport actually puts on the wire, and what it concludes
//! from what comes back.
//!
//! `MockTelegramTransport` next door proves the adapter's claim→turn→settle
//! spine. It cannot prove anything about the Bot API, because it *is* the
//! contract it is being checked against: a mock that agrees with the code is
//! green whether or not either agrees with Telegram.
//!
//! So these run a real HTTP server on loopback and let the transport talk to it.
//! Every assertion below is about a request Telegram would have received or a
//! response Telegram could have sent — the shape of the poll, where the
//! credential travels, how a long reply is broken up, and which HTTP outcomes
//! mean "nothing happened" versus "nobody knows".

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use optimus_ops::{
    list_ambiguous_sends, list_outbox_receipts, save_telegram_config, telegram_poll_once,
    LiveTelegramTransport, SendOutcome, TelegramConfig, TelegramTransport,
};
use serde_json::{json, Value};
use tempfile::tempdir;

/// Anything in the accepted band; the transport clamps outside it anyway.
const POLL_HOLD_SECS: u64 = 30;

/// One request the fake Bot API received.
#[derive(Debug, Clone)]
struct Call {
    path: String,
    body: Value,
}

/// A Bot API stand-in on loopback.
///
/// Scripted responses are consumed in order; once they run out it answers every
/// call successfully, so a test only has to describe the failure it cares about.
struct FakeBotApi {
    base: String,
    calls: Arc<Mutex<Vec<Call>>>,
    server: Arc<tiny_http::Server>,
}

impl FakeBotApi {
    fn start(scripted: Vec<(u16, Value)>) -> Self {
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("loopback address");
        let server = Arc::new(tiny_http::Server::http(addr).expect("bind loopback"));
        let base = format!("http://{}", server.server_addr());
        let calls = Arc::new(Mutex::new(Vec::new()));

        let worker = Arc::clone(&server);
        let recorded = Arc::clone(&calls);
        std::thread::spawn(move || {
            let mut scripted = scripted.into_iter();
            let mut sent = 0i64;
            for mut request in worker.incoming_requests() {
                let path = request.url().to_string();
                let mut raw = String::new();
                let _ = request.as_reader().read_to_string(&mut raw);
                let body = serde_json::from_str(&raw).unwrap_or(Value::Null);
                recorded.lock().expect("recording lock").push(Call {
                    path: path.clone(),
                    body,
                });

                let (status, payload) = scripted.next().unwrap_or_else(|| {
                    sent += 1;
                    if path.ends_with("/getUpdates") {
                        (200, json!({ "ok": true, "result": [] }))
                    } else {
                        (200, json!({ "ok": true, "result": { "message_id": sent } }))
                    }
                });
                let response = tiny_http::Response::from_string(payload.to_string())
                    .with_status_code(status)
                    .with_header(
                        "Content-Type: application/json"
                            .parse::<tiny_http::Header>()
                            .expect("header"),
                    );
                let _ = request.respond(response);
            }
        });

        Self {
            base,
            calls,
            server,
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().expect("recording lock").clone()
    }

    fn calls_to(&self, method: &str) -> Vec<Call> {
        let suffix = format!("/{method}");
        self.calls()
            .into_iter()
            .filter(|call| call.path.ends_with(&suffix))
            .collect()
    }
}

impl Drop for FakeBotApi {
    fn drop(&mut self) {
        // Release the worker thread parked on `incoming_requests`.
        self.server.unblock();
    }
}

/// Put `token` in a variable of its own so parallel tests never share one.
fn with_token(name: &'static str, token: &str) -> &'static str {
    std::env::set_var(name, token);
    name
}

fn transport(api: &FakeBotApi, env_name: &'static str, token: &str) -> LiveTelegramTransport {
    let env_name = with_token(env_name, token);
    LiveTelegramTransport::with_api_base(env_name, POLL_HOLD_SECS, &api.base)
        .expect("a token is present")
}

#[test]
fn a_poll_asks_telegram_to_hold_the_connection_rather_than_spin() {
    let api = FakeBotApi::start(vec![]);
    let mut live = transport(&api, "OPTIMUS_TG_TOKEN_POLL_SHAPE", "111:hold");

    live.get_updates(7).expect("poll");

    let call = api.calls_to("getUpdates").pop().expect("one poll");
    assert_eq!(call.body["offset"], json!(7));
    assert_eq!(call.body["allowed_updates"], json!(["message"]));
    // Short polling is what the API documentation warns hammers their servers.
    let hold = call.body["timeout"].as_u64().expect("a hold time");
    assert!(
        (30..=60).contains(&hold),
        "poll hold {hold}s is outside the band long polling is for"
    );
}

#[test]
fn the_credential_travels_in_the_path_and_never_in_the_body() {
    let api = FakeBotApi::start(vec![]);
    let mut live = transport(&api, "OPTIMUS_TG_TOKEN_PATH", "222:where");

    live.get_updates(0).expect("poll");
    live.send_message("42", "hi").expect("send");

    for call in api.calls() {
        assert!(
            call.path.starts_with("/bot222:where/"),
            "token belongs in the path: {}",
            call.path
        );
        assert!(
            !call.body.to_string().contains("222:where"),
            "token leaked into a request body: {}",
            call.body
        );
    }
}

#[test]
fn a_platform_refusal_is_definite_and_a_platform_failure_is_not() {
    let api = FakeBotApi::start(vec![
        (
            400,
            json!({ "ok": false, "error_code": 400, "description": "Bad Request: chat not found" }),
        ),
        (
            500,
            json!({ "ok": false, "description": "Internal Server Error" }),
        ),
    ]);
    let mut live = transport(&api, "OPTIMUS_TG_TOKEN_CLASSIFY", "333:classify");

    // Telegram understood the request and declined it, so nothing was sent and
    // the obligation is safe to retry.
    let refused = live.send_message("42", "one").expect("send");
    assert!(
        matches!(&refused, SendOutcome::Failed { detail } if detail.contains("chat not found")),
        "{refused:?}"
    );

    // Telegram failed partway through its own handling. Whether the message got
    // as far as the chat is not something this process can know.
    let broken = live.send_message("42", "two").expect("send");
    assert!(
        matches!(broken, SendOutcome::Ambiguous { .. }),
        "{broken:?}"
    );
}

#[test]
fn a_reply_past_the_cap_arrives_whole_across_several_messages() {
    let api = FakeBotApi::start(vec![]);
    let mut live = transport(&api, "OPTIMUS_TG_TOKEN_SPLIT", "444:split");

    let reply = "sentence about the thing. ".repeat(400);
    let outcome = live.send_message("42", &reply).expect("send");
    assert!(
        matches!(outcome, SendOutcome::Confirmed { .. }),
        "{outcome:?}"
    );

    let sends = api.calls_to("sendMessage");
    assert!(sends.len() > 1, "expected a split, got {}", sends.len());
    let delivered: String = sends
        .iter()
        .map(|call| call.body["text"].as_str().expect("text").to_string())
        .collect();
    assert_eq!(delivered, reply, "the chat did not receive the whole reply");
    for call in &sends {
        let units: usize = call.body["text"]
            .as_str()
            .expect("text")
            .chars()
            .map(char::len_utf16)
            .sum();
        assert!(
            units <= 4096,
            "part of {units} units exceeds the Bot API cap"
        );
    }
}

#[test]
fn a_refusal_after_the_first_part_landed_is_unknown_rather_than_retryable() {
    // Part one is accepted; part two is declined. Retrying the whole reply would
    // send part one to a real person twice, so the honest answer is that nobody
    // knows what the chat ended up with.
    let api = FakeBotApi::start(vec![
        (200, json!({ "ok": true, "result": { "message_id": 1 } })),
        (
            400,
            json!({ "ok": false, "description": "Bad Request: message is too long" }),
        ),
    ]);
    let mut live = transport(&api, "OPTIMUS_TG_TOKEN_PARTIAL", "555:partial");

    let outcome = live
        .send_message("42", &"paragraph ".repeat(500))
        .expect("send");
    let SendOutcome::Ambiguous { detail } = outcome else {
        panic!("a half-delivered reply must not be retryable: {outcome:?}");
    };
    assert!(
        detail.contains("after 1 sent"),
        "the operator needs to know how much landed: {detail}"
    );
}

#[test]
fn a_transport_failure_reaches_the_operator_without_the_credential() {
    let token = "666:never-log-me";
    // Nothing listens on port 1, so this is a connection failure rather than an
    // answer — the case where `ureq` puts the URL it tried into the error.
    let env_name = with_token("OPTIMUS_TG_TOKEN_REDACT", token);
    let mut live =
        LiveTelegramTransport::with_api_base(env_name, POLL_HOLD_SECS, "http://127.0.0.1:1")
            .expect("a token is present");

    let outcome = live.send_message("42", "hello").expect("send");
    let SendOutcome::Ambiguous { detail } = outcome else {
        panic!("an unreachable platform is unknown, not refused: {outcome:?}");
    };
    assert!(!detail.contains(token), "credential leaked into: {detail}");

    let error = live.get_updates(0).expect_err("poll cannot succeed");
    assert!(
        !error.to_string().contains(token),
        "credential leaked into: {error}"
    );
}

#[test]
fn a_full_cycle_over_the_bot_api_answers_the_chat_that_asked() {
    let api = FakeBotApi::start(vec![(
        200,
        json!({
            "ok": true,
            "result": [{
                "update_id": 90,
                "message": {
                    "message_id": 3,
                    "chat": { "id": 42 },
                    "from": { "username": "ada" },
                    "text": "status?"
                }
            }]
        }),
    )]);
    let mut live = transport(&api, "OPTIMUS_TG_TOKEN_CYCLE", "777:cycle");

    let home = tempdir().expect("temp home");
    save_telegram_config(
        home.path(),
        &TelegramConfig {
            enabled: true,
            bot_token_env: "OPTIMUS_TG_TOKEN_CYCLE".into(),
            allowed_chat_ids: vec!["42".into()],
        },
    )
    .expect("save config");

    let result = telegram_poll_once(home.path(), &mut live, 0, |message| {
        Ok((
            format!("answering: {}", message.text),
            message.session_id.clone(),
        ))
    })
    .expect("cycle");

    // The offset moves past what was just handled, so the next poll asks for
    // what comes after rather than re-reading this message forever.
    assert_eq!(result.next_offset, 91);
    assert_eq!(result.enqueued.len(), 1);
    assert_eq!(result.receipts.len(), 1);
    assert!(result.ambiguous.is_empty() && result.failed_sends.is_empty());

    let send = api.calls_to("sendMessage").pop().expect("one send");
    assert_eq!(send.body["chat_id"], json!("42"));
    assert_eq!(
        send.body["text"].as_str().expect("text"),
        "answering: @ada: status?"
    );

    let receipts = list_outbox_receipts(home.path(), 8).expect("receipts");
    assert_eq!(receipts.len(), 1);
    assert!(
        receipts[0].delivered_unix.is_some(),
        "a confirmed send must show as delivered"
    );
    assert!(list_ambiguous_sends(home.path(), 8)
        .expect("ambiguous")
        .is_empty());
}

#[test]
fn an_update_with_nothing_to_read_advances_the_offset_instead_of_wedging() {
    // A photo with no caption is a `message` update the adapter cannot turn into
    // a turn. Before this was handled, dropping it left the offset where it was
    // and Telegram handed back the same update on every cycle — the adapter
    // never saw anything behind it again.
    let api = FakeBotApi::start(vec![(
        200,
        json!({
            "ok": true,
            "result": [{
                "update_id": 500,
                "message": { "message_id": 4, "chat": { "id": 42 }, "photo": [] }
            }]
        }),
    )]);
    let mut live = transport(&api, "OPTIMUS_TG_TOKEN_BLANK", "888:blank");

    let home = tempdir().expect("temp home");
    save_telegram_config(
        home.path(),
        &TelegramConfig {
            enabled: true,
            bot_token_env: "OPTIMUS_TG_TOKEN_BLANK".into(),
            allowed_chat_ids: vec!["42".into()],
        },
    )
    .expect("save config");

    let result = telegram_poll_once(home.path(), &mut live, 0, |_| {
        panic!("an update with no text must not start a turn")
    })
    .expect("cycle");

    assert_eq!(result.next_offset, 501);
    assert!(result.enqueued.is_empty());
    assert!(api.calls_to("sendMessage").is_empty());
}
