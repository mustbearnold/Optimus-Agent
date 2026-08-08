//! `optimus serve` — the headless agent backend (spec-015, ADR-0083/0084).
//!
//! One core per home: serve owns the SQLite home, sessions, approvals,
//! filesystem scopes, and every durable effect, and a second serve against a
//! healthily served home refuses to start (exit 3, named diagnostic). The
//! wire contract is JSON-RPC 2.0 over two carriers sharing one dispatch —
//! loopback WebSocket (desktop renderer, attached clients; the carrier lives
//! in `ws.rs`) and stdio (spawned children). HTTP `GET /api/health` stays on
//! the record port, Bearer-gated: the record token IS the Bearer.
//!
//! Lifecycle pins (R1/R8): exit 2 = bind, security-validation, or
//! record-write failure (bind-failure exit 2 is a CHANGE from the HTTP
//! mode's exit 1, ADR-0083); exit 3 = refusal (home already served). The
//! record is written only after a successful bind and a post-bind
//! record-write failure is FATAL (the dial ticket lives only in the record).
//! `--stdio` opens the record + listener additively and is the ONLY mode
//! that reads stdin; plain serve never reads stdin at all (a GUI-spawned
//! child's stdin is typically /dev/null and an immediate EOF must not be
//! treated as a carrier disconnect — R4/R9).
//!
//! TRANSPORT NOTE (implementation evidence): the WS accept loop is a raw
//! loopback TcpListener with a minimal hand-rolled HTTP parser (health
//! endpoint + WebSocket upgrade only, `ws.rs`). The spec's R8 mechanism
//! citation — tiny_http `Request::upgrade()` — returns an opaque stream
//! with no socket-level timeout access; the pinned 30 s hello deadline and
//! 10 s write timeout (R7/R9) and the streaming concurrency requirement
//! (R3) cannot be satisfied on that stream, so the listener owns the socket
//! and sets SO_RCVTIMEO/SO_SNDTIMEO directly. tiny_http remains the
//! workspace HTTP substrate (the desktop HTTP mode uses it); this deviation
//! is recorded here and in the A2 landing commit.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::dispatch::{
    disconnect_cleanup, process_frame, serialize_outbound, Connection, Outbound, WorkerPool,
};
use crate::handshake::Carrier;
use crate::record::{self, TRANSPORT_WS};
use crate::ticket;

/// Default loopback port (spec-015 R8; `DEFAULT_HOST_PORT` precedent,
/// `apps/optimus-desktop/src/main.rs:34`).
pub const DEFAULT_HOST_PORT: u16 = 17865;

/// Exit 2: bind, security-validation, or record-write failure (ADR-0083).
pub const EXIT_BIND_OR_SECURITY: i32 = 2;
/// Exit 3: refusal — a healthy host already serves this home.
pub const EXIT_REFUSED: i32 = 3;

/// Bounded worker pool (R3): production default 4 workers; a blocking call
/// occupies only its worker, never a connection's read/event loop. Tunable
/// constants — changing them requires re-running the conformance suite.
pub const WORKER_COUNT: usize = 4;
/// Bounded request queue; queue-full rejects the NEW request with `-32603`
/// "server busy" and the connection stays healthy.
pub const WORKER_QUEUE: usize = 64;
/// Per-connection rate limit (R7): worker-dispatched requests only; the
/// control-plane exempt set is closed-form {`hello`, `chat_cancel`}.
pub const RATE_LIMIT_PER_MINUTE: u32 = 600;
/// Frame-size cap (R7): 1 MiB (`HTTP_MAX_REQUEST_BODY_BYTES` precedent).
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// WS ping keepalive interval (R7).
pub const PING_INTERVAL: Duration = Duration::from_secs(30);
/// WS send failure deadline (R9): a write stuck longer than this maps to the
/// delivered=false → Cancel path.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Per-connection outbound queue (replies + stream events).
pub const OUTBOUND_CAPACITY: usize = 256;

/// Shared server state (one per `start`).
pub struct ServeState {
    pub home: std::path::PathBuf,
    pub ticket: String,
    pub process_secret: Option<String>,
    /// The daemon-owned child coordinator (spec-034 R4), injected into
    /// every chat stream this server opens.
    pub(crate) children: Option<Arc<dyn optimus_kernel::ChildCoordinator>>,
    pub(crate) pool: WorkerPool,
    pub connections: AtomicUsize,
    pub shutdown: AtomicBool,
    pub exit_code: Mutex<Option<i32>>,
}

/// A running headless backend (testable: ephemeral ports, injected stdio).
pub struct RunningServer {
    port: u16,
    ticket: String,
    state: Arc<ServeState>,
    _threads: Vec<std::thread::JoinHandle<()>>,
}

impl RunningServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn ticket(&self) -> &str {
        &self.ticket
    }

    pub fn state(&self) -> &Arc<ServeState> {
        &self.state
    }

    /// Block until the server exits (stdio EOF → 0) and return its exit
    /// code. A plain serve (no stdio carrier) runs until killed.
    pub fn wait(&self) -> i32 {
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if let Some(code) = *self.state.exit_code.lock().unwrap() {
                return code;
            }
        }
    }
}

/// Production entry: `optimus serve`. Never returns — exits 2/3 per the
/// pinned codes, then serves until terminated (stdio EOF exits 0).
pub fn run(home: &Path, port: u16, stdio: bool) -> ! {
    if let Some(holder) = record::healthy_record(home) {
        eprintln!("error: {}", record::holder_refusal_diagnostic(&holder));
        std::process::exit(EXIT_REFUSED);
    }
    let server = match start(home, port, stdio) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(EXIT_BIND_OR_SECURITY);
        }
    };
    eprintln!(
        "[optimus serve] home={} on 127.0.0.1:{}",
        home.display(),
        server.port()
    );
    let code = server.wait();
    std::process::exit(code);
}

/// Start the backend: bind loopback, mint the dial ticket, write the record
/// v2/ws (FATAL on failure), spawn the accept loop and — when `stdio` — the
/// stdio carrier. `port` 0 binds an ephemeral port (conformance tests).
pub fn start(home: &Path, port: u16, stdio: bool) -> Result<RunningServer, String> {
    start_with_io(
        home,
        port,
        stdio,
        Box::new(std::io::stdin()),
        Box::new(std::io::stdout()),
    )
}

/// [`start`] with injected stdio streams (tests drive the stdio carrier
/// through pipes).
pub fn start_with_io(
    home: &Path,
    port: u16,
    stdio: bool,
    stdin: Box<dyn std::io::Read + Send>,
    stdout: Box<dyn Write + Send>,
) -> Result<RunningServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot bind 127.0.0.1:{port}: {error}"))?;
    let actual_port = listener
        .local_addr()
        .map_err(|error| format!("cannot read bound address: {error}"))?
        .port();

    let ticket = ticket::dial_ticket();
    let process_secret = ticket::process_secret();
    if let Err(error) = record::write_record(home, actual_port, &ticket, TRANSPORT_WS) {
        return Err(format!("cannot write host-runtime record: {error}"));
    }

    // The daemon-owned children runtime (spec-034 R4): one channel
    // feeds both the pool workers and the coordinator's enqueue path.
    let (tx, rx) = mpsc::sync_channel(WORKER_QUEUE);
    let live = Arc::new(Mutex::new(HashMap::new()));
    let runtime: Arc<dyn optimus_kernel::ChildCoordinator> = Arc::new(
        crate::children::ChildrenRuntime::new(home.to_path_buf(), tx.clone(), Arc::clone(&live)),
    );
    let pool = WorkerPool::start_with_channel(tx.clone(), rx);

    let state = Arc::new(ServeState {
        home: home.to_path_buf(),
        ticket: ticket.clone(),
        process_secret,
        children: Some(Arc::clone(&runtime)),
        pool,
        connections: AtomicUsize::new(0),
        shutdown: AtomicBool::new(false),
        exit_code: Mutex::new(None),
    });

    // Adoption sweep (spec-034 R4): re-run never-started children and
    // settle interrupted ones before the server accepts any client.
    match crate::children::adopt_children(home, &tx, &live, &runtime) {
        Ok(adopted) if adopted > 0 => {
            eprintln!("[optimus serve] adopted {adopted} children");
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!("[optimus serve] child adoption failed: {error}");
        }
    }

    let mut threads = Vec::new();
    let accept_state = Arc::clone(&state);
    threads.push(std::thread::spawn(move || {
        crate::ws::accept_loop(listener, accept_state);
    }));
    if stdio {
        let stdio_state = Arc::clone(&state);
        threads.push(std::thread::spawn(move || {
            stdio_loop(stdio_state, stdin, stdout);
        }));
    }

    Ok(RunningServer {
        port: actual_port,
        ticket,
        state,
        _threads: threads,
    })
}

/// The stdio carrier (spawned children): newline-delimited JSON-RPC over
/// stdin/stdout, the SAME dispatch. EOF or a broken-pipe write cancels the
/// connection's streams and exits 0 (normal teardown, R9). A shell-kind
/// hello without the process secret is a security-validation failure: exit
/// 2 (R5 — pipe ownership is not a shell credential).
fn stdio_loop(
    state: Arc<ServeState>,
    stdin: Box<dyn std::io::Read + Send>,
    stdout: Box<dyn Write + Send>,
) {
    let (tx, rx) = mpsc::sync_channel::<Outbound>(OUTBOUND_CAPACITY);
    let conn = Arc::new(Connection::new(state.home.clone(), tx));
    let mut reader = BufReader::new(stdin);
    let mut writer = stdout;

    let writer_state = Arc::clone(&state);
    let writer_thread = std::thread::spawn(move || {
        for outbound in rx {
            match outbound {
                Outbound::Close { .. } => break,
                Outbound::Pong(_) => continue, // no pongs on a line carrier
                other => {
                    let payload = serialize_outbound(&other);
                    if writeln!(writer, "{payload}").is_err() {
                        break; // broken pipe: same teardown as EOF (R9)
                    }
                    let _ = writer.flush();
                }
            }
        }
        // The channel closed (the connection dropped after teardown) or the
        // pipe broke: EOF teardown exits 0 (R9).
        writer_state.exit_code.lock().unwrap().get_or_insert(0);
    });

    let mut process = |conn: &Arc<Connection>| -> bool {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => false, // EOF
            Ok(_) => {
                let frame = line.trim_end_matches(['\r', '\n']);
                if frame.trim().is_empty() {
                    return true;
                }
                if frame.len() > MAX_FRAME_BYTES {
                    eprintln!("[optimus serve] stdio frame too large; dropped");
                    return true;
                }
                let Some(value) = serde_json::from_str::<Value>(frame).ok() else {
                    conn.error(None, -32700, "parse error");
                    return true;
                };
                process_frame(&state, conn, Carrier::Stdio, value);
                true
            }
            Err(_) => false, // stdin read failure: treat as teardown
        }
    };

    while process(&conn) {
        if state.shutdown.load(Ordering::Relaxed) {
            break;
        }
    }
    // EOF / broken pipe: cancel the connection's streams + tracked effects
    // (R9) and exit 0. The writer thread observes the drop and records 0.
    disconnect_cleanup(&conn);
    drop(conn);
    let _ = writer_thread.join();
    state.shutdown.store(true, Ordering::Relaxed);
}

/// Append an accepted-connection line to `<home>/logs/connections.log`
/// (R8). See `record::log_connection` for the format pin.
pub fn append_connection_log(home: &Path, origin: &str) {
    record::log_connection(home, origin);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        wire_method_class, WireClass, NON_WIRE_CHANNELS, PROTOCOL_METHODS, SERVER_ORIGIN_METHODS,
        SHELL_GATED_METHODS, STREAMING_TRIO, SUPERSEDED_CHAT_FAMILY,
    };
    use crate::dispatch::{internal_fault, resolve_binding_key, WindowRateLimiter};
    use crate::handshake::ClientKind;
    use serde_json::{json, Value};
    use std::time::Instant;

    #[test]
    fn wire_class_table_matches_the_pinned_buckets() {
        // Control plane: hello + chat_cancel.
        assert_eq!(
            wire_method_class("hello", ClientKind::Renderer),
            WireClass::Control
        );
        assert_eq!(
            wire_method_class("chat_cancel", ClientKind::Renderer),
            WireClass::Control
        );
        // Trio.
        for method in STREAMING_TRIO {
            let expected = match *method {
                "chat_start" => WireClass::ChatStart,
                "chat_approval_resolve_start" => WireClass::ResolveStart,
                _ => WireClass::Control,
            };
            assert_eq!(wire_method_class(method, ClientKind::Renderer), expected);
        }
        // Superseded blocking family: rejected on the wire.
        for method in SUPERSEDED_CHAT_FAMILY {
            assert_eq!(
                wire_method_class(method, ClientKind::Renderer),
                WireClass::Rejected,
                "{method} must not be wire-reachable"
            );
        }
        // Non-wire channels: rejected.
        for method in NON_WIRE_CHANNELS {
            assert_eq!(
                wire_method_class(method, ClientKind::Renderer),
                WireClass::Rejected,
                "{method} must not be wire-reachable"
            );
        }
        // Server-origin-only: rejected as client requests.
        for method in SERVER_ORIGIN_METHODS {
            assert_eq!(
                wire_method_class(method, ClientKind::Renderer),
                WireClass::Rejected,
                "{method} is server-origin-only"
            );
        }
        // Protocol methods include exactly the handshake + server-origin set.
        assert_eq!(PROTOCOL_METHODS.len(), 4);
        // Shell-gated: shell-kind dispatches, everyone else is rejected.
        for method in SHELL_GATED_METHODS {
            assert_eq!(
                wire_method_class(method, ClientKind::Shell),
                WireClass::ShellGated
            );
            assert_eq!(
                wire_method_class(method, ClientKind::Renderer),
                WireClass::Rejected
            );
        }
        // Ordinary registry methods dispatch.
        assert_eq!(
            wire_method_class("ping", ClientKind::Renderer),
            WireClass::Registry
        );
        assert_eq!(
            wire_method_class("term_run", ClientKind::Renderer),
            WireClass::Registry
        );
        assert_eq!(
            wire_method_class("campaign_run", ClientKind::Renderer),
            WireClass::Registry
        );
        assert_eq!(
            wire_method_class("browser_navigate", ClientKind::Renderer),
            WireClass::Registry
        );
        // Methods outside every named bucket classify as Registry; the
        // registry's own "unknown method" error resolves to the pinned
        // `-32601` at dispatch (see run_pool_job).
        assert_eq!(
            wire_method_class("nope", ClientKind::Renderer),
            WireClass::Registry
        );
    }

    #[test]
    fn rate_limiter_bounds_and_resets() {
        let mut limiter = WindowRateLimiter::new(3, Duration::from_secs(60));
        let now = Instant::now();
        assert!(limiter.allow(now));
        assert!(limiter.allow(now));
        assert!(limiter.allow(now));
        assert!(!limiter.allow(now), "limit reached");
        assert!(
            limiter.allow(now + Duration::from_secs(61)),
            "window resets"
        );
    }

    #[test]
    fn resolve_binding_key_is_exact() {
        let params = json!({
            "session_id": "s", "run_id": "r", "call_id": "c",
            "node_id": "n", "node_index": 0, "effect_sha256": "e", "decision": "approve"
        });
        assert_eq!(resolve_binding_key(&params).as_deref(), Some("s:r:c"));
        assert_eq!(resolve_binding_key(&json!({})), None);
    }

    #[test]
    fn serialize_outbound_is_one_json_rpc_line() {
        let reply = serialize_outbound(&Outbound::Reply {
            id: 7,
            result: json!({"ok": true}),
        });
        let parsed: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 7);
        assert_eq!(parsed["result"], json!({"ok": true}));
        assert!(parsed.get("error").is_none());
        let event = serialize_outbound(&Outbound::Event {
            stream_id: 3,
            event: json!({"type": "delta", "text": "hi"}),
        });
        let parsed: Value = serde_json::from_str(&event).unwrap();
        assert_eq!(parsed["method"], "event");
        assert_eq!(parsed["params"]["stream_id"], 3);
        assert_eq!(
            parsed["params"]["event"],
            json!({"type": "delta", "text": "hi"})
        );
    }

    #[test]
    fn internal_fault_emits_host_error_then_close() {
        // The connection-fatal path: host.error fires ONLY immediately
        // before close (R6). Drive the emission function directly and
        // observe the ordered outbound stream.
        let (tx, rx) = mpsc::sync_channel::<Outbound>(16);
        let home = tempfile::tempdir().unwrap();
        let conn = Arc::new(Connection::new(home.path().to_path_buf(), tx));
        internal_fault(&conn);
        let messages: Vec<Outbound> = rx.try_iter().collect();
        assert!(matches!(&messages[0], Outbound::Notify { method, .. } if *method == "host.error"));
        assert!(matches!(&messages[1], Outbound::Close { code: 1011, .. }));
    }
}
