//! Surface-protocol conformance suite (spec-015 A2/R12).
//!
//! Drives the `optimus serve` wire layer through both carriers (loopback
//! WebSocket and stdio) as a black box: framing, handshake, the full
//! credential-class matrix, error taxonomy, bounds, cancellation, ordering,
//! disconnect cleanup, stdio EOF/exit codes, stdout purity, `host.error`
//! semantics, `chat_cancel` no-op semantics, starvation + saturation, and
//! schema-payload conformance (responses AND events validated against the
//! committed schema — bidirectional).
//!
//! The accepted-method-table test enumerates the actual dispatch: the
//! static gate formula cannot see `serve.rs`'s table, so this file pins it.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use optimus_host::{handle_ipc, read_record, PROTOCOL_VERSION};
use optimus_host::{start_with_io, RunningServer};
use serde_json::{json, Value};
use tungstenite::protocol::{Message, WebSocket};
use tungstenite::stream::MaybeTlsStream;

/// The client socket type: plain TCP (no TLS in this suite).
type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

/// Serializes env-mutating tests (the `cargo test -- --test-threads=1`
/// fallback runs tests in threads of one process).
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the env lock with a deadline (a hung holder would otherwise
/// block every env test in the process forever). A POISONED lock (a
/// test panicked while holding it) is recovered, not spun on: one
/// test's failure must never cascade into every other env test.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        match ENV_LOCK.try_lock() {
            Ok(guard) => return guard,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                return poisoned.into_inner();
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                assert!(Instant::now() < deadline, "ENV_LOCK held >90s");
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

/// The repo root (integration tests run with the crate dir as CWD).
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/architecture/surface-protocol.schema.json"
);

struct TestServer {
    server: RunningServer,
    home: tempfile::TempDir,
    schema: Value,
}

fn start_server() -> TestServer {
    start_server_with(Box::new(std::io::empty()), Box::new(std::io::sink()), false)
}

fn start_server_with(
    stdin: Box<dyn Read + Send>,
    stdout: Box<dyn Write + Send>,
    stdio: bool,
) -> TestServer {
    let home = tempfile::tempdir().unwrap();
    let server = start_with_io(home.path(), 0, stdio, stdin, stdout).expect("serve starts");
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(SCHEMA_PATH).unwrap()).unwrap();
    TestServer {
        server,
        home,
        schema,
    }
}

impl TestServer {
    /// The dial ticket from the record (also proves the record is v2/ws).
    fn ticket(&self) -> String {
        let record = read_record(self.home.path()).expect("record written");
        assert_eq!(record.version, 2, "serve writes record v2");
        assert_eq!(record.transport_label(), "ws", "serve writes transport ws");
        record.token
    }

    fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/ws", self.server.port())
    }
}

/// Connect a WS client with an optional Origin and perform no hello.
fn ws_raw(server: &TestServer, origin: Option<&str>) -> Ws {
    let url = server.ws_url();
    // Pre-built requests must carry the Sec-WebSocket-Key themselves
    // (tungstenite extracts it from the request; the RFC example key is
    // fine — the server derives the accept from whatever we send).
    let request = match origin {
        Some(origin) => tungstenite::http::Request::builder()
            .uri(url)
            .header("Origin", origin)
            .header("Host", "127.0.0.1")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("Sec-WebSocket-Version", "13")
            .body(())
            .unwrap(),
        None => tungstenite::http::Request::builder()
            .uri(url)
            .header("Host", "127.0.0.1")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("Sec-WebSocket-Version", "13")
            .body(())
            .unwrap(),
    };
    let (mut ws, _response) = tungstenite::connect(request).expect("ws upgrade");
    if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
    }
    ws
}

/// A hello'd renderer-kind client.
fn ws_client(server: &TestServer) -> Ws {
    let mut ws = ws_raw(server, None);
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    let (result, ready) = (recv(&mut ws), recv(&mut ws));
    assert_hello_result(&result, server);
    assert_host_ready(&ready);
    ws
}

fn send_hello(ws: &mut Ws, kind: &str, ticket: Option<&str>, version: u64) {
    let mut params = json!({ "protocol_version": version, "client_kind": kind });
    if let Some(ticket) = ticket {
        params["ticket"] = json!(ticket);
    }
    send(
        ws,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "hello", "params": params }),
    );
}

fn send(ws: &mut Ws, value: Value) {
    ws.write(Message::Text(value.to_string().into())).unwrap();
    ws.flush().unwrap();
}

/// Read one message; panics on timeout. Answers server pings like a real
/// client (the server pings every PING_INTERVAL). Returns the parsed JSON.
fn recv(ws: &mut Ws) -> Value {
    loop {
        match ws.read().expect("read") {
            Message::Text(text) => {
                return serde_json::from_str(text.as_str()).expect("valid json frame");
            }
            Message::Ping(payload) => {
                ws.write(Message::Pong(payload)).unwrap();
                let _ = ws.flush();
            }
            other => panic!("expected a text frame, got {other:?}"),
        }
    }
}

fn assert_hello_result(value: &Value, server: &TestServer) {
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 1);
    assert_eq!(value["result"]["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(value["result"]["capabilities"]["streaming"], true);
    assert_eq!(
        value["result"]["capabilities"]["carriers"],
        json!(["stdio", "ws"])
    );
    assert_schema(
        &value["result"],
        &server.schema["methods"]["hello"]["result"],
    );
}

fn assert_host_ready(value: &Value) {
    assert_eq!(value["method"], "host.ready");
    assert_eq!(value["params"]["protocol_version"], PROTOCOL_VERSION);
}

/// ping's actual result: `{"home": ..., "pong": true}`.
fn assert_pong(value: &Value) {
    assert!(value["result"].is_object(), "ping result object: {value}");
    assert_eq!(value["result"]["pong"], true, "ping reply: {value}");
}

fn assert_error(value: &Value, code: i64, id: Value) {
    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], id, "error reply id");
    assert_eq!(value["error"]["code"], code, "error code");
}

/// Minimal JSON-Schema subset validator: type, const, enum, required,
/// properties, items (the dialect used by the protocol schema).
fn assert_schema(value: &Value, schema: &Value) {
    if let Some(expected) = schema.get("type") {
        let ok = match expected.as_str().unwrap() {
            "object" => value.is_object(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            _ => panic!("unsupported schema type"),
        };
        assert!(ok, "type mismatch: {value} vs {expected}");
    }
    if let Some(expected) = schema.get("const") {
        assert_eq!(value, expected, "const mismatch");
    }
    if let Some(variants) = schema.get("enum") {
        assert!(
            variants.as_array().unwrap().contains(value),
            "{value} not in enum"
        );
    }
    if let (Some(required), Some(properties)) = (schema.get("required"), schema.get("properties")) {
        let object = value.as_object().expect("required fields imply object");
        for name in required.as_array().unwrap() {
            let name = name.as_str().unwrap();
            assert!(
                object.contains_key(name),
                "missing required field {name} in {value}"
            );
            assert_schema(&object[name], &properties[name]);
        }
        for (name, field) in properties.as_object().unwrap() {
            if let Some(field_value) = object.get(name) {
                assert_schema(field_value, field);
            }
        }
    }
    if let Some(items) = schema.get("items") {
        for item in value.as_array().unwrap() {
            assert_schema(item, items);
        }
    }
}

// ---------------------------------------------------------------------------
// Handshake + hello
// ---------------------------------------------------------------------------

#[test]
fn hello_result_host_ready_and_method_parity() {
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    let result = recv(&mut ws);
    assert_hello_result(&result, &server);
    assert_host_ready(&recv(&mut ws));

    // A1: a non-chat registry method's wire result equals the in-process
    // handle_ipc result for the same call.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "startup_context", "params": {} }),
    );
    let reply = recv(&mut ws);
    assert_eq!(reply["id"], 2);
    let in_process = handle_ipc(
        &server.home.path().to_path_buf(),
        "startup_context",
        json!({}),
    )
    .expect("in-process call");
    assert_eq!(
        reply["result"], in_process,
        "wire result == handle_ipc result"
    );

    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "ping", "params": {} }),
    );
    assert_pong(&recv(&mut ws));
}

#[test]
fn hello_order_violations() {
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    // Method before hello → -32600 with id:null; connection stays open.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 5, "method": "ping", "params": {} }),
    );
    let reply = recv(&mut ws);
    assert_error(&reply, -32600, Value::Null);
    // Second hello → -32600 with id:null.
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    assert_hello_result(&recv(&mut ws), &server);
    assert_host_ready(&recv(&mut ws));
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    let reply = recv(&mut ws);
    assert_error(&reply, -32600, Value::Null);
    // Still healthy: a request works.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 6, "method": "ping", "params": {} }),
    );
    assert_pong(&recv(&mut ws));
}

#[test]
fn unknown_client_kind_is_invalid_request() {
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    send_hello(&mut ws, "wat", Some(&server.ticket()), 1);
    let reply = recv(&mut ws);
    assert_error(&reply, -32600, Value::Null);
    // Connection stays open: a valid hello still works.
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    assert_hello_result(&recv(&mut ws), &server);
}

#[test]
fn unsupported_protocol_version_closes_4002() {
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 2);
    let reply = recv(&mut ws);
    assert_error(&reply, -32001, json!(1));
    match ws.read().unwrap() {
        Message::Close(frame) => {
            assert_eq!(u16::from(frame.unwrap().code), 4002u16);
        }
        other => panic!("expected close, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Credential classes (R5/R7, ADR-0084)
// ---------------------------------------------------------------------------

fn expect_close_code(ws: &mut Ws, code: u16) {
    match ws.read().unwrap() {
        Message::Close(frame) => assert_eq!(u16::from(frame.unwrap().code), code),
        other => panic!("expected close {code}, got {other:?}"),
    }
}

#[test]
fn ticket_credentials_over_ws() {
    let _lock = env_lock();
    let secret = "x".repeat(40);
    std::env::set_var("OPTIMUS_NATIVE_SELECTION_TOKEN", &secret);
    let server = start_server();

    // No ticket → -32000 + close 4001.
    {
        let mut ws = ws_raw(&server, None);
        send_hello(&mut ws, "renderer", None, 1);
        let reply = recv(&mut ws);
        assert_error(&reply, -32000, json!(1));
        expect_close_code(&mut ws, 4001);
    }
    // Wrong ticket → close 4001.
    {
        let mut ws = ws_raw(&server, None);
        send_hello(&mut ws, "renderer", Some("nope-nope-nope-nope-nope"), 1);
        let reply = recv(&mut ws);
        assert_error(&reply, -32000, json!(1));
        expect_close_code(&mut ws, 4001);
    }
    // Shell-kind claim presenting the RECORD token → class violation, 4001.
    {
        let mut ws = ws_raw(&server, None);
        send_hello(&mut ws, "shell", Some(&server.ticket()), 1);
        let reply = recv(&mut ws);
        assert_error(&reply, -32000, json!(1));
        expect_close_code(&mut ws, 4001);
    }
    // Renderer-kind claim presenting the PROCESS SECRET → class violation, 4001.
    {
        let mut ws = ws_raw(&server, None);
        send_hello(&mut ws, "renderer", Some(&secret), 1);
        let reply = recv(&mut ws);
        assert_error(&reply, -32000, json!(1));
        expect_close_code(&mut ws, 4001);
    }
    // Shell-kind with the correct process secret → accepted.
    {
        let mut ws = ws_raw(&server, None);
        send_hello(&mut ws, "shell", Some(&secret), 1);
        assert_hello_result(&recv(&mut ws), &server);
        assert_host_ready(&recv(&mut ws));
    }
    // The server remains healthy for subsequent clients.
    let mut ws = ws_client(&server);
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 9, "method": "ping", "params": {} }),
    );
    assert_pong(&recv(&mut ws));

    std::env::remove_var("OPTIMUS_NATIVE_SELECTION_TOKEN");
}

#[test]
fn shell_gated_method_requires_shell_kind() {
    let _lock = env_lock();
    let secret = "y".repeat(40);
    std::env::set_var("OPTIMUS_NATIVE_SELECTION_TOKEN", &secret);
    let server = start_server();

    // Renderer-kind calling the staging method → -32601 (kind violation).
    let mut ws = ws_client(&server);
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 4, "method": "project_root_stage_native", "params": {"path": "/tmp"} }),
    );
    let reply = recv(&mut ws);
    assert_error(&reply, -32601, json!(4));
    // Connection stays open.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 5, "method": "ping", "params": {} }),
    );
    assert_pong(&recv(&mut ws));

    // Shell-kind calling the staging method: the server injects the secret,
    // so the call succeeds without the client presenting it (R7).
    let mut shell = ws_raw(&server, None);
    send_hello(&mut shell, "shell", Some(&secret), 1);
    assert_hello_result(&recv(&mut shell), &server);
    assert_host_ready(&recv(&mut shell));
    let project = tempfile::tempdir().unwrap();
    send(
        &mut shell,
        json!({ "jsonrpc": "2.0", "id": 6, "method": "project_root_stage_native", "params": {"path": project.path()} }),
    );
    let reply = recv(&mut shell);
    assert_eq!(reply["id"], 6, "staging call dispatched: {reply}");
    assert!(reply["result"].is_object(), "staged selection returned");

    std::env::remove_var("OPTIMUS_NATIVE_SELECTION_TOKEN");
}

// ---------------------------------------------------------------------------
// Error taxonomy (R4/R6)
// ---------------------------------------------------------------------------

#[test]
fn parse_and_invalid_request_errors_continue() {
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    // Malformed JSON → -32700 id:null; the connection continues.
    ws.write(Message::Text("{not json".into())).unwrap();
    ws.flush().unwrap();
    let reply = recv(&mut ws);
    assert_error(&reply, -32700, Value::Null);
    // Non-object JSON → -32600 id:null.
    send(&mut ws, json!(["a", "b"]));
    let reply = recv(&mut ws);
    assert_error(&reply, -32600, Value::Null);
    send(&mut ws, json!(42));
    assert_error(&recv(&mut ws), -32600, Value::Null);
    // Wrong / missing jsonrpc member → -32600 id:null.
    send(
        &mut ws,
        json!({ "jsonrpc": "1.0", "id": 1, "method": "ping", "params": {} }),
    );
    assert_error(&recv(&mut ws), -32600, Value::Null);
    send(&mut ws, json!({ "id": 1, "method": "ping", "params": {} }));
    assert_error(&recv(&mut ws), -32600, Value::Null);
    // Missing method / bad id → -32600 (id:null for bad ids).
    send(&mut ws, json!({ "jsonrpc": "2.0", "id": 1, "params": {} }));
    assert_error(&recv(&mut ws), -32600, json!(1));
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": "str", "method": "ping", "params": {} }),
    );
    assert_error(&recv(&mut ws), -32600, Value::Null);
    // Still healthy.
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    assert_hello_result(&recv(&mut ws), &server);
    assert_host_ready(&recv(&mut ws));
}

#[test]
fn idless_frames_are_dropped_never_answered() {
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    // Id-less frame before hello → dropped (no reply).
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "method": "ping", "params": {} }),
    );
    // Id-less hello with an unknown client_kind → dropped.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "method": "hello", "params": {"protocol_version": 1, "client_kind": "wat"} }),
    );
    // The next id-ful request gets exactly ONE reply — the pre-hello
    // `-32600` (method before hello, id:null) — proving nothing stray was
    // answered for the id-less frames.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 11, "method": "ping", "params": {} }),
    );
    let reply = recv(&mut ws);
    assert_error(&reply, -32600, Value::Null);
    // Post-hello id-less unknown-method frame → dropped.
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    assert_hello_result(&recv(&mut ws), &server);
    assert_host_ready(&recv(&mut ws));
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "method": "event", "params": {} }),
    );
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "method": "nope", "params": {} }),
    );
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 12, "method": "ping", "params": {} }),
    );
    let reply = recv(&mut ws);
    assert_eq!(reply["id"], 12);
}

#[test]
fn unknown_and_server_origin_methods_are_rejected() {
    let server = start_server();
    let mut ws = ws_client(&server);
    // Unknown method → -32601 with the request id; connection stays open.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 21, "method": "frobnicate", "params": {} }),
    );
    assert_error(&recv(&mut ws), -32601, json!(21));
    // Server-origin-only methods as client requests → -32601.
    for method in ["event", "host.ready", "host.error"] {
        send(
            &mut ws,
            json!({ "jsonrpc": "2.0", "id": 22, "method": method, "params": {} }),
        );
        assert_error(&recv(&mut ws), -32601, json!(22));
    }
    // Superseded blocking chat family → -32601.
    for method in ["chat", "chat_offline", "chat_approval_resolve"] {
        send(
            &mut ws,
            json!({ "jsonrpc": "2.0", "id": 23, "method": method, "params": {} }),
        );
        assert_error(&recv(&mut ws), -32601, json!(23));
    }
    // Non-wire channels → -32601.
    for method in ["window_minimize", "pick_folder", "open_path", "open_url"] {
        send(
            &mut ws,
            json!({ "jsonrpc": "2.0", "id": 24, "method": method, "params": {} }),
        );
        assert_error(&recv(&mut ws), -32601, json!(24));
    }
    // Still healthy.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 25, "method": "ping", "params": {} }),
    );
    assert_pong(&recv(&mut ws));
}

#[test]
fn framing_violations_close_4003() {
    let server = start_server();
    // Binary frame → close 4003.
    {
        let mut ws = ws_raw(&server, None);
        ws.write(Message::Binary(vec![1, 2, 3].into())).unwrap();
        ws.flush().unwrap();
        expect_close_code(&mut ws, 4003);
    }
    // Oversized frame (> 1 MiB) → close 4003.
    {
        let mut ws = ws_raw(&server, None);
        let huge = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": {"pad": "x".repeat(MAX_FRAME + 1)} }).to_string();
        ws.write(Message::Text(huge.into())).unwrap();
        ws.flush().unwrap();
        expect_close_code(&mut ws, 4003);
    }
}

const MAX_FRAME: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Chat streaming (R6): one terminal event, ordering, cancel semantics
// ---------------------------------------------------------------------------

const OFFLINE_LATENCY_ENV: &str = "OPTIMUS_OFFLINE_LATENCY_MS";

/// Send a paced offline chat_start and await its ack. The CALLER holds
/// [`env_lock`] for the whole test: the worker reads the latency env
/// asynchronously at model construction, so the env must be stable from
/// the send until the turn terminates. The env is LEFT SET afterwards —
/// the caller removes it when pacing is no longer needed. nextest isolates
/// env per test process; the verify fallback runs serialized.
fn offline_chat(server: &TestServer, ws: &mut Ws, stream_id: u64, latency_ms: u64, message: &str) {
    std::env::set_var(OFFLINE_LATENCY_ENV, latency_ms.to_string());
    send(
        ws,
        json!({
            "jsonrpc": "2.0", "id": stream_id + 100, "method": "chat_start",
            "params": {
                "stream_id": stream_id,
                "request": { "session": "", "message": message, "provider": "offline" }
            }
        }),
    );
    let ack = loop {
        let value = recv(ws);
        if value.get("id").is_some() {
            break value;
        }
    };
    assert_eq!(ack["id"], stream_id + 100, "chat_start ack: {ack}");
    assert_eq!(ack["result"]["stream_id"], stream_id);
    assert_schema(
        &ack["result"],
        &server.schema["methods"]["chat_start"]["result"],
    );
}

/// Drain events until the terminal one; returns the terminal event.
fn drain_to_terminal(ws: &mut Ws, schema: &Value) -> Value {
    let mut terminal = None;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let value = recv(ws);
        if value["method"] == "event" {
            let event = &value["params"]["event"];
            assert_schema(event, schema);
            let event_type = event["type"].as_str().unwrap();
            if matches!(event_type, "done" | "cancelled" | "error") {
                assert!(terminal.is_none(), "exactly one terminal event");
                terminal = Some(event.clone());
                break;
            }
        }
    }
    terminal.expect("a terminal event within the deadline")
}

#[test]
fn chat_round_trip_emits_exactly_one_terminal_event() {
    let server = start_server();
    let _lock = env_lock();
    let mut ws = ws_client(&server);
    offline_chat(&server, &mut ws, 1, 50, "hello wire");
    let terminal = drain_to_terminal(&mut ws, &server.schema["events"]);
    assert_eq!(
        terminal["type"], "done",
        "offline turn completes: {terminal}"
    );
    assert!(terminal["result"].is_object());
    // Ordering: events arrive in one stream, in order (drain already
    // validated every event against the schema).
    let done_event = terminal;
    assert_eq!(
        done_event["result"]["assistant_text"],
        "offline echo: hello wire"
    );
    std::env::remove_var(OFFLINE_LATENCY_ENV);
}

#[test]
fn chat_cancel_semantics() {
    let server = start_server();
    let _lock = env_lock();
    let mut ws = ws_client(&server);
    // Unknown stream → {"requested": false} no-op (never -32602).
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 7, "method": "chat_cancel", "params": {"stream_id": 999} }),
    );
    let reply = recv(&mut ws);
    assert_eq!(reply["result"], json!({"requested": false}));
    // Malformed stream_id → -32602.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 8, "method": "chat_cancel", "params": {"stream_id": "nope"} }),
    );
    assert_error(&recv(&mut ws), -32602, json!(8));
    // In-flight stream: cancel → {"requested": true} + exactly one
    // terminal (cancelled).
    offline_chat(&server, &mut ws, 2, 1500, "cancel me");
    std::thread::sleep(Duration::from_millis(200));
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 9, "method": "chat_cancel", "params": {"stream_id": 2} }),
    );
    // The in-flight turn's events interleave: read until the reply.
    let reply = loop {
        let value = recv(&mut ws);
        if value.get("id").is_some() {
            break value;
        }
    };
    assert_eq!(reply["result"], json!({"requested": true}));
    let terminal = drain_to_terminal(&mut ws, &server.schema["events"]);
    assert_eq!(terminal["type"], "cancelled");
    // Already-terminal stream → no-op {"requested": false}.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 10, "method": "chat_cancel", "params": {"stream_id": 2} }),
    );
    let reply = recv(&mut ws);
    assert_eq!(reply["result"], json!({"requested": false}));
}

#[test]
fn stream_limit_rejects_the_17th() {
    let server = start_server();
    let _lock = env_lock();
    let mut ws = ws_client(&server);
    // 30 s hold: every admitted stream must still be in flight when the
    // 17th arrives. Streams are registered at dispatch (before their ack),
    // so the hold only has to outlast the 16 sequential round-trips; a
    // 1.5 s hold could be exhausted by those round-trips under full-suite
    // load, letting the earliest streams terminate and admit the 17th.
    // The held streams are cancelled below, so the long pace costs nothing
    // at teardown.
    for stream_id in 0..16 {
        offline_chat(&server, &mut ws, stream_id, 30_000, "hold");
    }
    // The 17th concurrent stream → -32603 "stream limit reached".
    send(
        &mut ws,
        json!({
            "jsonrpc": "2.0", "id": 900, "method": "chat_start",
            "params": { "stream_id": 100, "request": {"session": "", "message": "x", "provider": "offline"} }
        }),
    );
    // In-flight turns emit events; drain until the reply arrives.
    let reply = loop {
        let value = recv(&mut ws);
        if value.get("id").is_some() {
            break value;
        }
    };
    assert_error(&reply, -32603, json!(900));
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("stream limit"),
        "diagnostic: {reply}"
    );
    // Connection stays healthy. `ping` is worker-dispatched (spec-015 R3:
    // only hello/chat_cancel run on the connection loop), so the pong is
    // written only after the pool drains — release the 16 held streams
    // first. Every stream is still registered here (30 s pace), so every
    // cancel is {"requested": true}; a cancelled turn stops within a pace
    // slice, the queued jobs drain, and the pong follows promptly.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 901, "method": "ping", "params": {} }),
    );
    for stream_id in 0..16 {
        send(
            &mut ws,
            json!({
                "jsonrpc": "2.0", "id": 2000 + stream_id, "method": "chat_cancel",
                "params": { "stream_id": stream_id }
            }),
        );
    }
    for _ in 0..16 {
        let reply = loop {
            let value = recv(&mut ws);
            if value.get("id").is_some() {
                break value;
            }
        };
        assert_eq!(
            reply["result"],
            json!({ "requested": true }),
            "cancel reply: {reply}"
        );
    }
    // The only id-bearing reply left is the pong (terminal events carry no
    // id); the pool has drained, so it must arrive promptly.
    let reply = loop {
        let value = recv(&mut ws);
        if value.get("id").is_some() {
            break value;
        }
    };
    assert_pong(&reply);
    std::env::remove_var(OFFLINE_LATENCY_ENV);
}

#[test]
fn disconnect_cancels_in_flight_turns_no_orphan() {
    let server = start_server();
    // The latency env must stay set for the whole test: the worker reads it
    // asynchronously at model construction (removing it early makes the
    // turn run at zero latency and complete before the disconnect).
    let _lock = env_lock();
    std::env::set_var(OFFLINE_LATENCY_ENV, "2500");
    {
        let mut ws = ws_client(&server);
        send(
            &mut ws,
            json!({
                "jsonrpc": "2.0", "id": 101, "method": "chat_start",
                "params": {
                    "stream_id": 1,
                    "request": { "session": "", "message": "will be cut off", "provider": "offline" }
                }
            }),
        );
        let ack = recv(&mut ws);
        assert_eq!(ack["id"], 101, "chat_start ack: {ack}");
        assert_eq!(ack["result"]["stream_id"], 1);
        std::thread::sleep(Duration::from_millis(300));
        // Drop the socket mid-turn.
        ws.close(None).unwrap();
        ws.flush().unwrap();
        drop(ws);
    }
    // Wait past the turn's natural completion window.
    std::thread::sleep(Duration::from_millis(3500));
    // Oracle sanity: a COMPLETED offline turn records its assistant reply.
    std::env::set_var(OFFLINE_LATENCY_ENV, "30");
    {
        let mut control = ws_client(&server);
        send(
            &mut control,
            json!({
                "jsonrpc": "2.0", "id": 102, "method": "chat_start",
                "params": {
                    "stream_id": 2,
                    "request": { "session": "", "message": "control", "provider": "offline" }
                }
            }),
        );
        let ack = recv(&mut control);
        assert_eq!(ack["id"], 102);
        assert_eq!(ack["result"]["stream_id"], 2);
        assert_eq!(
            drain_to_terminal(&mut control, &server.schema["events"])["type"],
            "done"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    std::env::remove_var(OFFLINE_LATENCY_ENV);
    // The disconnected turn must NOT have completed (no orphaned
    // execution, R9): the cancelled session has strictly fewer messages
    // than the completed control session.
    let mut check = ws_client(&server);
    send(
        &mut check,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "sessions", "params": {} }),
    );
    let reply = recv(&mut check);
    assert!(reply.get("error").is_none(), "sessions reply: {reply}");
    // sessions_json wraps the rows: `{"sessions": [...]}`.
    let sessions = reply["result"]["sessions"]
        .as_array()
        .unwrap_or_else(|| panic!("sessions result: {reply}"));
    let counts: Vec<u64> = sessions
        .iter()
        .filter_map(|session| session["message_count"].as_u64())
        .collect();
    // Completed turn = system + user + assistant (3); cancelled at the
    // post-pace checkpoint = system + user only (2) — no assistant reply
    // and no orphaned execution (R9).
    assert!(
        counts.contains(&3),
        "the control turn completed (oracle): {counts:?}"
    );
    assert!(
        counts.contains(&2),
        "the cut-off turn was cancelled, no assistant reply: {counts:?}"
    );
}

// ---------------------------------------------------------------------------
// Approval resolve over the wire (A5)
// ---------------------------------------------------------------------------

/// Park a real turn on a held effect and hand back its exact binding
/// (the chat.rs test pattern; nothing is faked).
fn parked_turn(home: &std::path::Path) -> (String, optimus_kernel::ToolApprovalBinding) {
    use optimus_kernel::{
        CompletionResponse, Kernel, KernelConfig, ProjectAuthorityStore, ScriptedModel,
        StreamEvent, ToolCall,
    };
    let project = tempfile::tempdir().unwrap();
    let authority = ProjectAuthorityStore::open(home).unwrap();
    let selection = authority.stage_native_selection(project.path()).unwrap();
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
        reasoning_content: None,
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

fn resolve_params(
    binding: &optimus_kernel::ToolApprovalBinding,
    session_id: &str,
    decision: &str,
) -> Value {
    json!({
        "session_id": session_id,
        "run_id": binding.run_id.to_string(),
        "call_id": binding.call_id,
        "job_id": binding.job_id.to_string(),
        "node_id": binding.node_id.to_string(),
        "node_index": binding.node_index,
        "effect_sha256": binding.effect_sha256,
        "decision": decision,
    })
}

#[test]
fn approval_resolve_streams_and_cancel_wins() {
    let server = start_server();
    let (session_id, binding) = parked_turn(server.home.path());

    let mut ws = ws_client(&server);
    // Resolve over the wire: continuation events stream, exactly one
    // terminal (done). The offline latency paces the continuation so the
    // first resolve is still IN FLIGHT when the second arrives.
    let _lock = env_lock();
    std::env::set_var(OFFLINE_LATENCY_ENV, "1200");
    let resolve = resolve_params(&binding, &session_id, "approve");
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 30, "method": "chat_approval_resolve_start", "params": {"stream_id": 5, "params": resolve} }),
    );
    let ack = recv(&mut ws);
    assert_eq!(ack["id"], 30);
    assert_eq!(ack["result"]["stream_id"], 5);
    std::thread::sleep(Duration::from_millis(200));

    // A second resolve for the same binding while the first resolves →
    // -32602 (binding already resolving, R6). The first resolve's events
    // interleave: read until the id-ful reply arrives.
    let resolve2 = resolve_params(&binding, &session_id, "approve");
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 31, "method": "chat_approval_resolve_start", "params": {"stream_id": 6, "params": resolve2} }),
    );
    let reply = loop {
        let value = recv(&mut ws);
        if value.get("id").is_some() {
            break value;
        }
    };
    assert_error(&reply, -32602, json!(31));
    std::env::remove_var(OFFLINE_LATENCY_ENV);
    // The first resolve still streams to exactly one terminal (cancelled
    // after cleanup below is NOT required — let it finish).
    let terminal = drain_to_terminal(&mut ws, &server.schema["events"]);
    assert_eq!(
        terminal["type"], "done",
        "resolve continuation completes: {terminal}"
    );
}

#[test]
fn approval_resolve_cancel_wins() {
    let server = start_server();
    let (session_id, binding) = parked_turn(server.home.path());

    let mut ws = ws_client(&server);
    // The continuation must still be in flight when the cancel lands: pace
    // the offline provider for this test's duration.
    let _lock = env_lock();
    std::env::set_var(OFFLINE_LATENCY_ENV, "1200");
    let resolve = resolve_params(&binding, &session_id, "approve");
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 40, "method": "chat_approval_resolve_start", "params": {"stream_id": 7, "params": resolve} }),
    );
    let ack = recv(&mut ws);
    assert_eq!(ack["id"], 40);
    std::thread::sleep(Duration::from_millis(200));
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 41, "method": "chat_cancel", "params": {"stream_id": 7} }),
    );
    // The continuation's events interleave: read until the id-ful reply.
    let reply = loop {
        let value = recv(&mut ws);
        if value.get("id").is_some() {
            break value;
        }
    };
    assert_eq!(reply["result"], json!({"requested": true}));
    let terminal = drain_to_terminal(&mut ws, &server.schema["events"]);
    assert_eq!(
        terminal["type"], "cancelled",
        "cancelled-wins matches the Tauri path: {terminal}"
    );
    std::env::remove_var(OFFLINE_LATENCY_ENV);
}

// ---------------------------------------------------------------------------
// Bounds: connections, rate limit (R7)
// ---------------------------------------------------------------------------

#[test]
fn ninth_connection_closes_4003() {
    let server = start_server();
    let mut held = Vec::new();
    for _ in 0..8 {
        held.push(ws_client(&server));
    }
    let mut ninth = ws_raw(&server, None);
    send_hello(&mut ninth, "renderer", Some(&server.ticket()), 1);
    // The 9th is upgraded then closed with 4003.
    expect_close_code(&mut ninth, 4003);
}

#[test]
fn hello_deadline_closes_silent_connections() {
    let _lock = env_lock();
    std::env::set_var("OPTIMUS_SERVE_HELLO_TIMEOUT_MS", "300");
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    // No hello: the server closes with 4001 after ~300 ms.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match ws.read() {
            Ok(Message::Close(frame)) => {
                assert_eq!(u16::from(frame.unwrap().code), 4001u16);
                break;
            }
            Ok(_) => continue,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                assert!(Instant::now() < deadline, "close within the deadline");
                continue;
            }
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }
    std::env::remove_var("OPTIMUS_SERVE_HELLO_TIMEOUT_MS");
    // The slot is freed: a fresh connection with a valid hello works.
    let mut after = ws_client(&server);
    send(
        &mut after,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": {} }),
    );
    assert_pong(&recv(&mut after));
}

#[test]
fn rate_limit_exhaustion_and_exempt_chat_cancel() {
    let server = start_server();
    let mut ws = ws_client(&server);
    // 600 worker-dispatched pings fill the per-connection budget.
    for id in 0..600 {
        send(
            &mut ws,
            json!({ "jsonrpc": "2.0", "id": id, "method": "ping", "params": {} }),
        );
    }
    // The pool dispatches CONCURRENTLY (R3, WORKER_COUNT=4): reply order is
    // completion order, never request order — the spec orders only
    // per-stream events (R6), so asserting FIFO here is wrong. Assert the
    // id SET: all 600 accepted, exactly once each.
    let mut ids: Vec<u64> = (0..600)
        .map(|_| {
            let reply = recv(&mut ws);
            reply["id"]
                .as_u64()
                .unwrap_or_else(|| panic!("expected a reply with a numeric id, got {reply}"))
        })
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..600).collect::<Vec<u64>>());
    // The 601st is rejected with -32603 "rate limit exceeded" (request id,
    // connection stays open).
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 600, "method": "ping", "params": {} }),
    );
    let reply = recv(&mut ws);
    assert_error(&reply, -32603, json!(600));
    assert!(reply["error"]["message"]
        .as_str()
        .unwrap()
        .contains("rate limit"));
    // chat_cancel is exempt (closed-form exempt set): it still works.
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 601, "method": "chat_cancel", "params": {"stream_id": 7} }),
    );
    let reply = recv(&mut ws);
    assert_eq!(reply["result"], json!({"requested": false}));
}

// ---------------------------------------------------------------------------
// Dispatch classes (R3): starvation + saturation
// ---------------------------------------------------------------------------

#[test]
fn starvation_control_plane_completes_while_workers_busy() {
    let server = start_server();
    let mut conn1 = ws_client(&server);
    // term_run of a bounded sleep: verifiably in flight on connection 1.
    send(
        &mut conn1,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "term_run", "params": {"line": "sleep 2"} }),
    );
    // In-flight probe: jobs_list shows the running job.
    let mut running = false;
    let mut term_reply: Option<Value> = None;
    for _ in 0..50 {
        send(
            &mut conn1,
            json!({ "jsonrpc": "2.0", "id": 2, "method": "jobs_list", "params": {} }),
        );
        let reply = recv(&mut conn1);
        if reply["id"] == 1 {
            // The term_run result raced the probe: job already done.
            term_reply = Some(reply);
            running = false;
            break;
        }
        running = reply["result"]["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|job| {
                job["status"].as_str().unwrap_or("").contains("Running")
                    || job["status"].as_str().unwrap_or("").contains("running")
            });
        if running {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if term_reply.is_none() {
        assert!(running, "the term_run job is verifiably in flight");
    }

    // Connection 2: hello + chat_start (offline-paced) + chat_cancel must
    // all complete while the job is in flight — control-plane bypass.
    let mut conn2 = ws_raw(&server, None);
    send_hello(&mut conn2, "renderer", Some(&server.ticket()), 1);
    assert_hello_result(&recv(&mut conn2), &server);
    assert_host_ready(&recv(&mut conn2));
    // The env lock serializes against other env tests; the timer starts
    // AFTER acquisition so the bound measures the wire sequence only.
    let _lock = env_lock();
    std::env::set_var(OFFLINE_LATENCY_ENV, "3000");
    let started = Instant::now();
    send(
        &mut conn2,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "chat_start", "params": {"stream_id": 1, "request": {"session": "", "message": "slow", "provider": "offline"}} }),
    );
    let ack = recv(&mut conn2);
    assert_eq!(ack["id"], 3);
    send(
        &mut conn2,
        json!({ "jsonrpc": "2.0", "id": 4, "method": "chat_cancel", "params": {"stream_id": 1} }),
    );
    let reply = recv(&mut conn2);
    assert_eq!(reply["result"], json!({"requested": true}));
    let elapsed = started.elapsed();
    std::env::remove_var(OFFLINE_LATENCY_ENV);
    assert!(
        elapsed < Duration::from_millis(2500),
        "hello + chat_cancel complete well inside the latency oracle bound ({elapsed:?})"
    );
    // The stream emits exactly one terminal event (cancelled).
    let terminal = drain_to_terminal(&mut conn2, &server.schema["events"]);
    assert_eq!(terminal["type"], "cancelled");
    // Teardown waits for the budgeted completion (the exemption: no
    // cancellation of term_run). Its result arrives on connection 1.
    if term_reply.is_none() {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match conn1.read() {
                Ok(Message::Text(text)) => {
                    let value: Value = serde_json::from_str(text.as_str()).unwrap();
                    if value["id"] == 1 {
                        assert!(value["result"]["job_id"].as_str().is_some());
                        break;
                    }
                }
                Ok(_) => continue,
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    assert!(Instant::now() < deadline, "term_run completes");
                    continue;
                }
                Err(other) => panic!("unexpected: {other:?}"),
            }
        }
    }
}

#[test]
fn saturation_fifth_connection_control_plane_bypasses_full_pool() {
    let server = start_server();
    let mut conn1 = ws_client(&server);
    // 4 long turns saturate the production-default pool.
    let _lock = env_lock();
    std::env::set_var(OFFLINE_LATENCY_ENV, "3000");
    for stream_id in 0..4 {
        send(
            &mut conn1,
            json!({ "jsonrpc": "2.0", "id": stream_id, "method": "chat_start", "params": {"stream_id": stream_id, "request": {"session": "", "message": "sat", "provider": "offline"}} }),
        );
        let ack = recv(&mut conn1);
        assert_eq!(ack["id"], stream_id);
    }
    // A 5th connection's hello + chat_cancel still complete.
    let mut conn5 = ws_raw(&server, None);
    let started = Instant::now();
    send_hello(&mut conn5, "renderer", Some(&server.ticket()), 1);
    assert_hello_result(&recv(&mut conn5), &server);
    assert_host_ready(&recv(&mut conn5));
    send(
        &mut conn5,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "chat_cancel", "params": {"stream_id": 777} }),
    );
    let reply = recv(&mut conn5);
    assert_eq!(reply["result"], json!({"requested": false}));
    let elapsed = started.elapsed();
    std::env::remove_var(OFFLINE_LATENCY_ENV);
    assert!(
        elapsed < Duration::from_millis(2500),
        "control plane bypasses the saturated pool ({elapsed:?})"
    );
}

// ---------------------------------------------------------------------------
// Stdio carrier (R4/R5/R9)
// ---------------------------------------------------------------------------

#[test]
fn stdio_hello_method_and_stdout_purity() {
    let (stdin_rx, mut stdin_tx) = std::io::pipe().unwrap();
    let (stdout_rx, stdout_tx) = std::io::pipe().unwrap();
    let mut stdout_rx = std::io::BufReader::new(stdout_rx);
    let server = start_server_with(
        Box::new(stdin_rx.try_clone().unwrap()),
        Box::new(stdout_tx.try_clone().unwrap()),
        true,
    );

    // Hello over stdio: renderer/tui/cli may omit the ticket (pipe
    // ownership). Reply + host.ready on stdout.
    writeln!(stdin_tx, r#"{{"jsonrpc":"2.0","id":1,"method":"hello","params":{{"protocol_version":1,"client_kind":"tui"}}}}"#).unwrap();
    stdin_tx.flush().unwrap();
    let mut line = String::new();
    stdout_rx.read_line(&mut line).unwrap();
    let hello_reply: Value = serde_json::from_str(line.trim()).unwrap();
    assert_hello_result(&hello_reply, &server);
    line.clear();
    stdout_rx.read_line(&mut line).unwrap();
    let ready: Value = serde_json::from_str(line.trim()).unwrap();
    assert_host_ready(&ready);

    // A registry method round trip.
    writeln!(
        stdin_tx,
        r#"{{"jsonrpc":"2.0","id":2,"method":"ping","params":{{}}}}"#
    )
    .unwrap();
    stdin_tx.flush().unwrap();
    line.clear();
    stdout_rx.read_line(&mut line).unwrap();
    let ping: Value = serde_json::from_str(line.trim()).unwrap();
    assert_pong(&ping);

    // EOF: serve cancels and exits 0 (normal teardown).
    drop(stdin_tx);
    assert_eq!(server.server.wait(), 0, "stdio EOF exits 0");

    // Stdout purity: every line parsed as exactly one JSON object; nothing
    // else was written.
    drop(stdout_tx);
    let mut rest = String::new();
    stdout_rx.read_to_string(&mut rest).unwrap();
    for line in rest.lines() {
        assert!(!line.trim().is_empty(), "no stray blank lines");
        let value: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("stdout purity: {line}: {e}"));
        assert_eq!(value["jsonrpc"], "2.0");
    }
}

#[test]
fn stdio_shell_kind_without_secret_exits_2() {
    let _lock = env_lock();
    std::env::remove_var("OPTIMUS_NATIVE_SELECTION_TOKEN");
    let (stdin_rx, mut stdin_tx) = std::io::pipe().unwrap();
    // The lock stays held through wait(): the server reads the env
    // asynchronously when the hello is processed.
    let (stdout_rx, stdout_tx) = std::io::pipe().unwrap();
    let server = start_server_with(
        Box::new(stdin_rx.try_clone().unwrap()),
        Box::new(stdout_tx.try_clone().unwrap()),
        true,
    );
    writeln!(stdin_tx, r#"{{"jsonrpc":"2.0","id":1,"method":"hello","params":{{"protocol_version":1,"client_kind":"shell","ticket":"whatever"}}}}"#).unwrap();
    stdin_tx.flush().unwrap();
    // Security-validation class: exit 2 (the stderr diagnostic is the
    // shell's signal; the exit code is the pin).
    assert_eq!(server.server.wait(), 2);
    drop(stdout_rx);
    drop(stdin_tx);
    drop(stdout_tx);
}

#[test]
fn stdio_shell_kind_with_secret_is_accepted() {
    let _lock = env_lock();
    let secret = "z".repeat(40);
    std::env::set_var("OPTIMUS_NATIVE_SELECTION_TOKEN", &secret);
    let (stdin_rx, mut stdin_tx) = std::io::pipe().unwrap();
    let (stdout_rx, stdout_tx) = std::io::pipe().unwrap();
    let mut stdout_rx = std::io::BufReader::new(stdout_rx);
    let server = start_server_with(
        Box::new(stdin_rx.try_clone().unwrap()),
        Box::new(stdout_tx.try_clone().unwrap()),
        true,
    );
    writeln!(
        stdin_tx,
        r#"{{"jsonrpc":"2.0","id":1,"method":"hello","params":{{"protocol_version":1,"client_kind":"shell","ticket":"{secret}"}}}}"#
    )
    .unwrap();
    stdin_tx.flush().unwrap();
    let mut line = String::new();
    stdout_rx.read_line(&mut line).unwrap();
    let reply: Value = serde_json::from_str(line.trim()).unwrap();
    assert_hello_result(&reply, &server);
    drop(stdin_tx);
    assert_eq!(server.server.wait(), 0);
    drop(stdout_rx);
    drop(stdout_tx);
    std::env::remove_var("OPTIMUS_NATIVE_SELECTION_TOKEN");
}

// ---------------------------------------------------------------------------
// Connections.log (R8) + record
// ---------------------------------------------------------------------------

#[test]
fn connections_log_fires_post_hello_and_never_on_rejection() {
    let server = start_server();
    // A rejected handshake never logs.
    {
        let mut ws = ws_raw(&server, Some("http://127.0.0.1:9999"));
        send_hello(&mut ws, "renderer", Some("wrong-wrong-wrong"), 1);
        let _ = recv(&mut ws);
        expect_close_code(&mut ws, 4001);
    }
    // A completed hello logs one line with the origin.
    {
        let mut ws = ws_raw(&server, Some("http://127.0.0.1:5173"));
        send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
        assert_hello_result(&recv(&mut ws), &server);
        assert_host_ready(&recv(&mut ws));
        ws.close(None).unwrap();
        ws.flush().unwrap();
    }
    std::thread::sleep(Duration::from_millis(200));
    let log = std::fs::read_to_string(server.home.path().join("logs/connections.log"))
        .expect("connections.log exists");
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 1, "exactly one accepted connection: {log}");
    // Format pinned in the schema: `YYYY-MM-DDTHH:MM:SSZ origin=<origin>`.
    let (stamp, rest) = lines[0].split_once(' ').expect("timestamp + origin");
    assert_eq!(stamp.len(), 20, "ISO-8601 UTC stamp: {stamp}");
    assert!(stamp.ends_with('Z'), "UTC stamp: {stamp}");
    assert_eq!(
        rest, "origin=http://127.0.0.1:5173",
        "origin label: {}",
        lines[0]
    );
    assert!(!log.contains(&server.ticket()), "never logs the ticket");
}

// ---------------------------------------------------------------------------
// Accepted method table + schema conformance (R10/R12)
// ---------------------------------------------------------------------------

#[test]
fn accepted_method_table_enumerates_the_dispatch() {
    let server = start_server();
    let mut ws = ws_client(&server);

    // Black-box spot checks of the dispatch table.
    for (method, params) in [
        ("ping", json!({})),
        ("startup_context", json!({})),
        ("sessions", json!({})),
        ("term_run", json!({ "line": "true" })),
    ] {
        send(
            &mut ws,
            json!({ "jsonrpc": "2.0", "id": 50, "method": method, "params": params }),
        );
        let reply = recv(&mut ws);
        assert_eq!(reply["id"], 50, "{method} dispatches: {reply}");
        assert!(
            reply["result"].is_object() || reply["result"].is_array(),
            "{method}"
        );
    }

    // The protocol version const matches the schema's declared version.
    assert_eq!(
        PROTOCOL_VERSION,
        server.schema["protocol_version"].as_u64().unwrap()
    );

    // Every schema-declared method is reachable in the wire vocabulary:
    // registry wire methods (minus non-wire channels and the superseded
    // blocking family) + trio + protocol + shell-gated. Non-wire and
    // superseded channels are NOT wire methods and NOT schema methods.
    let registry = parse_registry();
    let excluded: std::collections::HashSet<&str> = optimus_host::NON_WIRE_CHANNELS
        .iter()
        .copied()
        .chain(optimus_host::SUPERSEDED_CHAT_FAMILY.iter().copied())
        .collect();
    let wire = registry
        .iter()
        .map(|name| name.as_str())
        .filter(|name| !excluded.contains(name))
        .chain(optimus_host::STREAMING_TRIO.iter().copied())
        .chain(optimus_host::PROTOCOL_METHODS.iter().copied())
        .chain(optimus_host::SHELL_GATED_METHODS.iter().copied());
    let declared: std::collections::HashSet<&str> = server.schema["methods"]
        .as_object()
        .unwrap()
        .keys()
        .map(|key| key.as_str())
        .collect();
    for method in wire {
        assert!(
            declared.contains(method),
            "schema must declare every wire method: {method}"
        );
    }
    for method in declared {
        let in_registry = registry.contains(method);
        let in_buckets = optimus_host::STREAMING_TRIO.contains(&method)
            || optimus_host::PROTOCOL_METHODS.contains(&method)
            || optimus_host::SHELL_GATED_METHODS.contains(&method);
        assert!(
            in_registry || in_buckets,
            "schema declares no phantom methods: {method}"
        );
    }
}

fn parse_registry() -> std::collections::HashSet<String> {
    // Mirror of the gate's `parse_rust_registry` (check-desktop-ipc-matrix
    // pattern): the METHOD_DOMAINS const in router.rs.
    let text =
        std::fs::read_to_string(format!("{ROOT}/crates/optimus-host/src/router.rs")).unwrap();
    let start = text
        .find("const METHOD_DOMAINS:")
        .expect("METHOD_DOMAINS const");
    let open = text[start..].find("= &[").expect("table open") + start + 4;
    let close = text[open..].find("];").expect("table close") + open;
    let block = &text[open..close];
    let mut methods = std::collections::HashSet::new();
    for line in block.lines() {
        // Entries are `("name", Domain::X)` — some with the name on its own
        // line: `("name",` or `"name",` on its own line inside parens.
        let trimmed = line.trim().trim_start_matches('(');
        if let Some(name) = trimmed.strip_prefix('"') {
            if let Some(end) = name.find('"') {
                methods.insert(name[..end].to_string());
            }
        }
    }
    assert!(!methods.is_empty(), "registry parse found methods");
    methods
}

#[test]
fn schema_events_cover_the_runtime_vocabulary() {
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(SCHEMA_PATH).unwrap()).unwrap();
    let declared: std::collections::HashSet<&str> = schema["events"]
        .as_object()
        .unwrap()
        .keys()
        .map(|key| key.as_str())
        .collect();
    for event in optimus_host::STREAM_EVENT_VOCABULARY {
        assert!(declared.contains(event), "schema declares {event}");
    }
    for event in &declared {
        assert!(
            optimus_host::STREAM_EVENT_VOCABULARY.contains(event),
            "schema declares no phantom events: {event}"
        );
    }
}

#[test]
fn host_error_never_fires_for_client_errors() {
    let server = start_server();
    let mut ws = ws_raw(&server, None);
    // A battery of client-caused failures: none may produce host.error.
    ws.write(Message::Text("{broken".into())).unwrap();
    ws.flush().unwrap();
    send(&mut ws, json!(["not", "an", "object"]));
    send(
        &mut ws,
        json!({ "jsonrpc": "1.0", "id": 1, "method": "ping", "params": {} }),
    );
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "nope", "params": {} }),
    );
    send_hello(&mut ws, "wat", Some(&server.ticket()), 1);
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "event", "params": {} }),
    );
    // Drain the replies (mixed id:null and request-id shapes). Five
    // non-close-inducing failures: broken JSON, non-object, wrong jsonrpc,
    // unknown method, unknown-kind hello, server-origin method = six.
    let mut saw_any = 0;
    for _ in 0..6 {
        match ws.read() {
            Ok(Message::Text(text)) => {
                let value: Value = serde_json::from_str(text.as_str()).unwrap();
                assert_ne!(
                    value["method"], "host.error",
                    "client errors never fire host.error: {value}"
                );
                saw_any += 1;
            }
            Ok(_) => continue,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(other) => panic!("unexpected: {other:?}"),
        }
    }
    assert!(saw_any >= 6, "all six replies observed ({saw_any})");
    // The connection remains healthy: a valid hello completes.
    send_hello(&mut ws, "renderer", Some(&server.ticket()), 1);
    assert_hello_result(&recv(&mut ws), &server);
    assert_host_ready(&recv(&mut ws));
    // The ticket-rejection close (4001) is a credential-layer close, not a
    // JSON-RPC response — and never a host.error either.
    let mut rejected = ws_raw(&server, None);
    send_hello(&mut rejected, "renderer", Some("bad-ticket-bad"), 1);
    let _ = recv(&mut rejected);
    expect_close_code(&mut rejected, 4001);
}

#[test]
fn wire_payloads_conform_to_the_schema_bidirectionally() {
    let server = start_server();
    // WebSocket carrier: hello + chat_start params/results + every event
    // against the schema (drain_to_terminal validates events).
    let mut ws = ws_client(&server);
    // chat_start params validated by the schema.
    let params = json!({
        "stream_id": 3,
        "request": { "session": "", "message": "schema", "provider": "offline" }
    });
    assert_schema(&params, &server.schema["methods"]["chat_start"]["params"]);
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 60, "method": "chat_start", "params": params }),
    );
    let ack = recv(&mut ws);
    assert_eq!(ack["id"], 60);
    assert_schema(
        &ack["result"],
        &server.schema["methods"]["chat_start"]["result"],
    );
    let terminal = drain_to_terminal(&mut ws, &server.schema["events"]);
    assert_eq!(terminal["type"], "done");
    // chat_cancel params/result.
    let cancel = json!({ "stream_id": 3 });
    assert_schema(&cancel, &server.schema["methods"]["chat_cancel"]["params"]);
    send(
        &mut ws,
        json!({ "jsonrpc": "2.0", "id": 61, "method": "chat_cancel", "params": cancel }),
    );
    let reply = recv(&mut ws);
    assert_schema(
        &reply["result"],
        &server.schema["methods"]["chat_cancel"]["result"],
    );

    // Stdio carrier: hello result + host.ready + a method reply against the
    // schema.
    let (stdin_rx, mut stdin_tx) = std::io::pipe().unwrap();
    let (stdout_rx, stdout_tx) = std::io::pipe().unwrap();
    let mut stdout_rx = std::io::BufReader::new(stdout_rx);
    let stdio_server = start_server_with(
        Box::new(stdin_rx.try_clone().unwrap()),
        Box::new(stdout_tx.try_clone().unwrap()),
        true,
    );
    writeln!(stdin_tx, r#"{{"jsonrpc":"2.0","id":1,"method":"hello","params":{{"protocol_version":1,"client_kind":"cli"}}}}"#).unwrap();
    stdin_tx.flush().unwrap();
    let mut line = String::new();
    stdout_rx.read_line(&mut line).unwrap();
    let reply: Value = serde_json::from_str(line.trim()).unwrap();
    assert_schema(
        &reply["result"],
        &stdio_server.schema["methods"]["hello"]["result"],
    );
    line.clear();
    stdout_rx.read_line(&mut line).unwrap();
    let ready: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(ready["method"], "host.ready");
    assert_schema(
        &ready["params"],
        &stdio_server.schema["methods"]["host.ready"]["params"],
    );
    drop(stdin_tx);
    assert_eq!(stdio_server.server.wait(), 0);
    drop(stdout_rx);
    drop(stdout_tx);
}

// keep the RunningServer import honest
#[allow(dead_code)]
fn _assert_server_type(_: &RunningServer) {}
