//! Surface-protocol client (spec-015 Phase B, B1/B2).
//!
//! The TUI and the CLI are protocol peers of the renderer, not in-process
//! host callers. This module gives them one client:
//!
//! - [`connect`]: spawn-or-attach (spec-015 B1, R11). Spawn
//!   `optimus serve --stdio`; on exit 2/3 or no record after the bounded
//!   wait, attach over WebSocket with the record token; a healthy HTTP
//!   holder is a named diagnostic. Port policy (#148): the desired port
//!   is machine-global but the record is per-home, so when the desired
//!   port is held by another home's serve, the spawn falls back to an
//!   EPHEMERAL port (`serve --port 0`) — the record carries the real
//!   port, so the attach-after-spawn fallback and the named diagnostics
//!   are unchanged when no port at all is available.
//! - [`HostClient`]: newline-delimited JSON-RPC 2.0 over the child's pipes
//!   or the same values in WebSocket text frames.
//!
//! Wire contract pins (serve.rs / handshake.rs):
//! - stdio hello for the `tui` kind OMITS the ticket: pipe ownership is the
//!   credential. An empty string is a REJECTED ticket (class rule R5).
//! - a WebSocket attach MUST present the record token.
//! - `chat_start` / `chat_approval_resolve_start` emit events on the
//!   stream id and end with exactly one terminal event
//!   (`{type:done,result}` / `{type:cancelled,error}` / `{type:error,error}`).
//! - control-plane `chat_cancel` is never rate-limited (R7).
//! - stdio EOF exits 0 (R9).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::contract::PROTOCOL_VERSION;
use crate::record::{healthy_record, HostRuntimeRecord};
use crate::serve::{EXIT_BIND_OR_SECURITY, EXIT_REFUSED};
use crate::spawn_decision::PortState;

/// Bounded wait for the child's record: 5 s at 250 ms probes (spec-015 B1).
pub const RECORD_WAIT: Duration = Duration::from_secs(5);
/// Record-probe interval during the spawn wait (spec-015 B1).
pub const RECORD_PROBE: Duration = Duration::from_millis(250);
/// WS reader poll tick: the socket read timeout that lets the reader thread
/// service the outbound channel while no inbound frame is pending
/// (serve-side POLL_TICK parity, ws.rs:34-35).
const WS_POLL_TICK: Duration = Duration::from_millis(100);

/// Why a client could not reach a host (named diagnostics, spec-015 B1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectDiagnostic {
    /// `optimus` was not found (set `OPTIMUS_SERVE_BIN`).
    BinaryNotFound,
    /// The serve child exited with a non-2/3 code before its record.
    SpawnFailed(i32),
    /// A healthy record exists but its holder serves HTTP only
    /// (`--host-only`); there is no wire to speak.
    HttpHolder,
    /// No record and no live child after the bounded wait.
    NoRecord,
    /// The child exited 2/3, no healthy record appeared after the bounded
    /// re-probe, and the desired port is still occupied by another process
    /// (spec-015 R8: the honest post-spawn settle — a bind failure is not
    /// a stale CLI and not "no record").
    PortOccupied { port: u16 },
}

impl std::fmt::Display for ConnectDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectDiagnostic::BinaryNotFound => {
                write!(
                    f,
                    "the optimus binary was not found (set OPTIMUS_SERVE_BIN)"
                )
            }
            ConnectDiagnostic::SpawnFailed(code) => {
                write!(f, "optimus serve exited {code} before it wrote its record")
            }
            ConnectDiagnostic::HttpHolder => write!(
                f,
                "this home is served by an HTTP-only holder; start `optimus serve` to reach it"
            ),
            ConnectDiagnostic::NoRecord => {
                write!(f, "optimus serve produced no record within the wait")
            }
            ConnectDiagnostic::PortOccupied { port } => {
                write!(f, "serve failed to start: check port {port}")
            }
        }
    }
}

/// The outcome of [`connect`].
#[derive(Debug)]
pub enum ConnectOutcome {
    /// Spawned `optimus serve --stdio`; the client owns the child's pipes.
    Spawned(HostClient),
    /// Attached over WebSocket to a serve that already held the home.
    Attached(HostClient),
    /// No wire reachable; the diagnostic names the cause.
    Diagnostic(ConnectDiagnostic),
}

/// Locate the `optimus` binary: `OPTIMUS_SERVE_BIN` env, then the sibling
/// of the current executable (`target/debug/optimus` next to
/// `optimus-tui`/`optimus-cli` in the workspace layout), then `PATH`.
pub fn resolve_serve_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("OPTIMUS_SERVE_BIN") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("optimus");
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("optimus"))
            .find(|candidate| candidate.is_file())
    })
}

/// Is the desired port bindable? (A bind check, not a health probe.)
fn port_state(port: u16) -> PortState {
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            PortState::Free
        }
        Err(_) => PortState::Occupied,
    }
}

/// Spawn-or-attach (spec-015 B1, R11).
///
/// 1. Spawn `optimus serve --stdio --home <home> --port <port>` with the
///    parent environment (the offline latency env passes through). When
///    the desired port is held by another home's serve (the port is
///    machine-global, the record is per-home; #148), spawn with
///    `--port 0` instead — the record carries the real port, so the
///    rest of this flow is unchanged.
/// 2. Probe the record for [`RECORD_WAIT`] at [`RECORD_PROBE`] intervals.
/// 3. On a record + live child, speak stdio.
/// 4. On exit 2/3 or no record in time, read the record: a healthy WS
///    holder means attach; a healthy HTTP holder is a named diagnostic.
pub fn connect(home: &Path, port: u16) -> ConnectOutcome {
    let Some(binary) = resolve_serve_binary() else {
        return ConnectOutcome::Diagnostic(ConnectDiagnostic::BinaryNotFound);
    };

    // Port fallback (#148): an ephemeral bind (`--port 0`) keeps the
    // one-core-per-home rule intact (the child still refuses exit 3 on
    // a healthy holder — refusal is record-based) while letting two
    // homes coexist on one machine.
    let spawn_port = if port_state(port) == PortState::Occupied {
        0
    } else {
        port
    };

    let mut child = match Command::new(&binary)
        .args([
            "serve",
            "--stdio",
            "--home",
            &home.to_string_lossy(),
            "--port",
            &spawn_port.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return ConnectOutcome::Diagnostic(ConnectDiagnostic::SpawnFailed(-1)),
    };

    let stdin = child
        .stdin
        .take()
        .expect("stdio client owns the child's stdin");
    let stdout = child
        .stdout
        .take()
        .expect("stdio client owns the child's stdout");

    let deadline = Instant::now() + RECORD_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                if code == EXIT_BIND_OR_SECURITY || code == EXIT_REFUSED {
                    // Spec-015 R8 post-spawn settle: bounded record
                    // re-probe (race recovery), then the branch matrix.
                    return settle_spawn_exit(home, port, spawn_port, code);
                }
                return ConnectOutcome::Diagnostic(ConnectDiagnostic::SpawnFailed(code));
            }
            Ok(None) => {}
            Err(_) => return ConnectOutcome::Diagnostic(ConnectDiagnostic::SpawnFailed(-1)),
        }
        // Only the child WE spawned counts as a successful spawn: the child
        // writes its record with its own pid (record.rs:72), so a healthy
        // record with a DIFFERENT pid is a pre-existing holder — the child
        // is about to refuse it (exit 3) and the client must attach instead
        // of speaking stdio to a dying child (regression: B2 attach).
        if let Some(record) = healthy_record(home) {
            if record.pid == child.id() {
                return ConnectOutcome::Spawned(HostClient::stdio(child, stdin, stdout));
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(RECORD_PROBE);
    }
    // The child never wrote a record in time: kill it and settle.
    let _ = child.kill();
    let _ = child.wait();
    fallback_to_ws(home, -1)
}

/// Spec-015 R8 post-spawn settle for a child that exited 2/3 (bind OR
/// security-validation failure, or holder refusal): re-probe the record
/// for the race-recovery window first (the winner writes its record only
/// after bind), attaching if a healthy v2/ws record appears; otherwise
/// settle per the branch matrix — a DESIRED-port spawn whose port is
/// still occupied gets the honest "check port" diagnostic, and everything
/// else (free port, ephemeral spawn) gets the generic spawn-failed state.
fn settle_spawn_exit(
    home: &Path,
    desired_port: u16,
    spawn_port: u16,
    exit_code: i32,
) -> ConnectOutcome {
    let home_for_probe = home.to_path_buf();
    match crate::spawn_decision::re_probe(
        home,
        crate::record::read_record,
        || port_state(desired_port),
        move |record| {
            crate::record::healthy_record(&home_for_probe)
                .is_some_and(|healthy| healthy.pid == record.pid)
        },
    ) {
        crate::spawn_decision::Reprobed::Attach { .. } => {
            // The race resolved: another spawn won and wrote a healthy
            // record — attach over WS with its token.
            match crate::record::healthy_record(home)
                .and_then(|record| HostClient::ws_attach(&record).ok())
            {
                Some(client) => ConnectOutcome::Attached(client),
                None => ConnectOutcome::Diagnostic(ConnectDiagnostic::SpawnFailed(exit_code)),
            }
        }
        crate::spawn_decision::Reprobed::Settled(state) => {
            if spawn_port == desired_port && state == PortState::Occupied {
                ConnectOutcome::Diagnostic(ConnectDiagnostic::PortOccupied { port: desired_port })
            } else {
                ConnectOutcome::Diagnostic(ConnectDiagnostic::SpawnFailed(exit_code))
            }
        }
    }
}

fn fallback_to_ws(home: &Path, exit_code: i32) -> ConnectOutcome {
    match healthy_record(home) {
        Some(record) if record.transport_label() == "ws" => match HostClient::ws_attach(&record) {
            Ok(client) => ConnectOutcome::Attached(client),
            Err(_) => ConnectOutcome::Diagnostic(ConnectDiagnostic::SpawnFailed(exit_code)),
        },
        Some(_) => ConnectOutcome::Diagnostic(ConnectDiagnostic::HttpHolder),
        None => ConnectOutcome::Diagnostic(ConnectDiagnostic::NoRecord),
    }
}

/// One inbound frame the client understands.
#[derive(Debug, Clone, PartialEq)]
enum Inbound {
    Reply { id: u64, result: Value },
    Error { id: u64, code: i64, message: String },
    Event { stream_id: u64, event: Value },
}

/// Parse one JSON-RPC 2.0 line into the client-relevant subset.
fn parse_inbound(line: &str) -> Option<Inbound> {
    let object: Value = serde_json::from_str(line).ok()?;
    // Event notifications carry no id (JSON-RPC 2.0 notifications).
    if object.get("method").and_then(Value::as_str) == Some("event") {
        let params = object.get("params")?;
        let stream_id = params.get("stream_id")?.as_u64()?;
        let event = params.get("event")?.clone();
        return Some(Inbound::Event { stream_id, event });
    }
    let id = object.get("id")?.as_u64()?;
    if let Some(error) = object.get("error") {
        return Some(Inbound::Error {
            id,
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    if let Some(result) = object.get("result") {
        return Some(Inbound::Reply {
            id,
            result: result.clone(),
        });
    }
    None
}

/// A chat/approval stream: events until the exactly-one terminal event.
#[derive(Debug)]
pub struct Stream {
    rx: Receiver<Value>,
}

impl Stream {
    /// Block for the next event. `None` means the connection closed.
    pub fn next(&self) -> Option<Value> {
        self.rx.recv().ok()
    }

    /// Drain events until the terminal event and return it.
    pub fn wait_terminal(&self) -> Result<Value, String> {
        while let Some(event) = self.next() {
            match event.get("type").and_then(Value::as_str) {
                Some("done") | Some("cancelled") | Some("error") => return Ok(event),
                _ => continue,
            }
        }
        Err("connection closed before the terminal event".to_string())
    }
}

/// Shared client-side tables: pending request ids → reply senders, and
/// open streams → event senders (aliased to keep clippy's complexity gate).
type PendingTable = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;
type StreamTable = Arc<Mutex<HashMap<u64, Sender<Value>>>>;

/// A client of one `optimus serve` connection (stdio or WebSocket).
pub struct HostClient {
    /// Pending request ids → reply senders.
    pending: PendingTable,
    /// Open streams → event senders.
    streams: StreamTable,
    /// The child, when this client spawned it (stdio carrier).
    child: Option<Child>,
    /// The stdio write half.
    stdin: Option<Mutex<Box<dyn Write + Send>>>,
    /// The WebSocket write half, when attached: serialized frames sent to
    /// the reader thread's outbound channel (one thread owns the socket for
    /// reads AND writes — see `ws_attach`).
    ws: Option<mpsc::Sender<WsOutbound>>,
    /// The record token, when attached over WebSocket (hello credential).
    ws_ticket: Option<String>,
    next_id: AtomicU64,
}

/// Frames the caller hands to the WS reader thread. `Close` mirrors the
/// serve's `Outbound::Close`: the reader performs the polite close
/// handshake and exits.
enum WsOutbound {
    Frame(String),
    Close,
}

impl std::fmt::Debug for HostClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostClient")
            .field("pending", &self.pending.lock().unwrap().len())
            .field("streams", &self.streams.lock().unwrap().len())
            .field("has_child", &self.child.is_some())
            .field("has_ws", &self.ws.is_some())
            .finish()
    }
}

impl Drop for HostClient {
    fn drop(&mut self) {
        self.close();
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }
}

impl HostClient {
    /// Client over the child's pipes. The reader thread consumes the
    /// child's stdout; the child lives until [`HostClient::close`] drops
    /// its stdin (stdio EOF exits 0, R9).
    fn stdio(child: Child, stdin: ChildStdin, stdout: std::process::ChildStdout) -> Self {
        Self::from_io(Some(child), Box::new(stdin), Box::new(stdout))
    }

    /// Client over caller-owned byte streams, with no child to reap.
    ///
    /// This is the in-process test seam: a `start_with_io` serve in the same
    /// process speaks the identical stdio carrier, so unit tests exercise the
    /// same wire a spawned child would (spec-015 B1). The write half doubles
    /// as the teardown signal — [`HostClient::close`] dropping it is the EOF
    /// the serve reads as exit 0 (R9).
    pub fn pipes(stdin: Box<dyn Write + Send>, stdout: Box<dyn Read + Send>) -> Self {
        Self::from_io(None, stdin, stdout)
    }

    /// Shared constructor: spawn the reader thread over `stdout`, keep the
    /// write half in `stdin`, and remember the child only when this client
    /// owns one.
    fn from_io(
        child: Option<Child>,
        stdin: Box<dyn Write + Send>,
        stdout: Box<dyn Read + Send>,
    ) -> Self {
        let pending: PendingTable = Arc::new(Mutex::new(HashMap::new()));
        let streams: StreamTable = Arc::new(Mutex::new(HashMap::new()));
        let reader_pending = Arc::clone(&pending);
        let reader_streams = Arc::clone(&streams);
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                let Some(frame) = parse_inbound(&line) else {
                    continue;
                };
                match frame {
                    Inbound::Reply { id, result } => {
                        if let Some(tx) = reader_pending.lock().unwrap().remove(&id) {
                            let _ = tx.send(Ok(result));
                        }
                    }
                    Inbound::Error { id, code, message } => {
                        if let Some(tx) = reader_pending.lock().unwrap().remove(&id) {
                            let _ = tx.send(Err(format!("{message} (code {code})")));
                        }
                    }
                    Inbound::Event { stream_id, event } => {
                        if let Some(tx) = reader_streams.lock().unwrap().get(&stream_id) {
                            let _ = tx.send(event.clone());
                        }
                    }
                }
            }
            // The pipe closed: fail every pending request and stream so no
            // caller waits forever (R9 teardown on the client side too).
            for (_, tx) in reader_pending.lock().unwrap().drain() {
                let _ = tx.send(Err("connection lost".to_string()));
            }
        });
        Self {
            pending,
            streams,
            child,
            stdin: Some(Mutex::new(stdin)),
            ws: None,
            ws_ticket: None,
            next_id: AtomicU64::new(1),
        }
    }

    /// Attach to a running serve over WebSocket with the record token.
    ///
    /// No hello is sent here: the caller's `hello`/`hello_as` is the
    /// handshake on BOTH carriers, so one code path presents the credential
    /// and the serve's second-hello rejection (dispatch.rs:441) can never
    /// fire. The ticket rides on the client for `hello_as` to present.
    ///
    /// ONE thread owns the socket and multiplexes reads and writes (the
    /// serve-side pattern, ws.rs:15-17): the reader thread drains the
    /// outbound channel into the socket and dispatches inbound frames.
    /// A second thread sharing the socket through a mutex would deadlock —
    /// a reader blocked in `read()` holds the socket while `write()` waits
    /// for it (regression: the pre-B2 attach path hung until the serve's
    /// 30 s hello deadline closed the connection).
    fn ws_attach(record: &HostRuntimeRecord) -> Result<Self, String> {
        let url = format!("ws://127.0.0.1:{}/ws", record.port);
        let (mut socket, _) = tungstenite::connect(url)
            .map_err(|error| format!("websocket attach failed: {error}"))?;
        // The reader thread polls with a short socket timeout so outbound
        // frames are serviced while no inbound frame is pending (serve-side
        // POLL_TICK, ws.rs:34-35).
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
            let _ = stream.set_read_timeout(Some(WS_POLL_TICK));
        }

        let pending: PendingTable = Arc::new(Mutex::new(HashMap::new()));
        let streams: StreamTable = Arc::new(Mutex::new(HashMap::new()));
        let reader_pending = Arc::clone(&pending);
        let reader_streams = Arc::clone(&streams);
        let (out_tx, out_rx) = mpsc::channel::<WsOutbound>();
        let reader_out = out_rx;
        std::thread::spawn(move || {
            loop {
                // Drain outbound first: requests, events, and the close
                // handshake are serviced even while the socket is idle.
                while let Ok(item) = reader_out.try_recv() {
                    match item {
                        WsOutbound::Frame(line) => {
                            if socket
                                .send(tungstenite::Message::Text(line.into()))
                                .is_err()
                            {
                                break;
                            }
                        }
                        WsOutbound::Close => {
                            let _ = socket.close(None);
                            return;
                        }
                    }
                }
                match socket.read() {
                    Ok(tungstenite::Message::Text(text)) => {
                        let Some(frame) = parse_inbound(&text) else {
                            continue;
                        };
                        match frame {
                            Inbound::Reply { id, result } => {
                                if let Some(tx) = reader_pending.lock().unwrap().remove(&id) {
                                    let _ = tx.send(Ok(result));
                                }
                            }
                            Inbound::Error { id, code, message } => {
                                if let Some(tx) = reader_pending.lock().unwrap().remove(&id) {
                                    let _ = tx.send(Err(format!("{message} (code {code})")));
                                }
                            }
                            Inbound::Event { stream_id, event } => {
                                if let Some(tx) = reader_streams.lock().unwrap().get(&stream_id) {
                                    let _ = tx.send(event.clone());
                                }
                            }
                        }
                    }
                    Ok(tungstenite::Message::Ping(payload)) => {
                        let _ = socket.send(tungstenite::Message::Pong(payload));
                    }
                    Ok(tungstenite::Message::Close(_)) => {
                        for (_, tx) in reader_pending.lock().unwrap().drain() {
                            let _ = tx.send(Err("connection lost".to_string()));
                        }
                        return;
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => {
                        for (_, tx) in reader_pending.lock().unwrap().drain() {
                            let _ = tx.send(Err("connection lost".to_string()));
                        }
                        return;
                    }
                }
            }
        });
        Ok(Self {
            pending,
            streams,
            child: None,
            stdin: None,
            ws: Some(out_tx),
            ws_ticket: Some(record.token.clone()),
            next_id: AtomicU64::new(1),
        })
    }

    fn write(&self, frame: &Value) -> Result<(), String> {
        let line = frame.to_string();
        if let Some(stdin) = &self.stdin {
            let mut stdin = stdin.lock().unwrap();
            writeln!(stdin, "{line}")
                .and_then(|_| stdin.flush())
                .map_err(|error| format!("stdio write failed: {error}"))
        } else if let Some(ws) = &self.ws {
            ws.send(WsOutbound::Frame(line))
                .map_err(|_| "websocket write failed: connection closed".to_string())
        } else {
            Err("client has no carrier".to_string())
        }
    }

    /// One request/reply round trip (registry methods).
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        rx.recv().map_err(|_| "connection lost".to_string())?
    }

    /// Open a chat stream. Events arrive on the returned [`Stream`] until
    /// its exactly-one terminal event.
    pub fn start_turn(&self, stream_id: u64, request: Value) -> Result<Stream, String> {
        let (tx, rx) = mpsc::channel();
        self.streams.lock().unwrap().insert(stream_id, tx);
        self.call(
            "chat_start",
            json!({ "stream_id": stream_id, "request": request }),
        )?;
        Ok(Stream { rx })
    }

    /// Open an approval-resolve stream (chat_approval_resolve_start).
    pub fn resolve(&self, stream_id: u64, params: Value) -> Result<Stream, String> {
        let (tx, rx) = mpsc::channel();
        self.streams.lock().unwrap().insert(stream_id, tx);
        self.call(
            "chat_approval_resolve_start",
            json!({ "stream_id": stream_id, "params": params }),
        )?;
        Ok(Stream { rx })
    }

    /// Control-plane cancel: never rate-limited (R7).
    pub fn cancel(&self, stream_id: u64) -> Result<(), String> {
        self.call("chat_cancel", json!({ "stream_id": stream_id }))?;
        Ok(())
    }

    /// A fresh stream id for this connection. Stream ids are a separate id
    /// space from request ids: the serve keys its stream registry by them,
    /// and nothing else on the wire shares that namespace.
    pub fn fresh_stream_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Hello with the `tui` kind. On stdio the ticket is OMITTED (pipe
    /// ownership is the credential; an empty string is a rejected ticket,
    /// `handshake.rs:133-147`). On a WebSocket attach the record token is
    /// presented.
    pub fn hello(&self) -> Result<Value, String> {
        self.hello_as("tui")
    }

    /// Hello presenting an explicit `client_kind` (spec-015 B2: the CLI
    /// speaks the wire as `cli`, the TUI as `tui`). Credential rules are
    /// identical for renderer/tui/cli (handshake.rs:133-147); the kind only
    /// labels the surface.
    pub fn hello_as(&self, kind: &str) -> Result<Value, String> {
        let mut params = json!({
            "protocol_version": PROTOCOL_VERSION,
            "client_kind": kind,
        });
        if let Some(ticket) = &self.ws_ticket {
            params["ticket"] = json!(ticket);
        }
        self.call("hello", params)
    }

    /// Close the connection: drop stdin (stdio EOF exits 0, R9) or ask the
    /// WS reader thread to perform the polite close handshake.
    pub fn close(&mut self) {
        self.stdin.take();
        if let Some(ws) = &self.ws {
            let _ = ws.send(WsOutbound::Close);
        }
    }

    /// Reap the stdio child and return its exit code (stdio EOF exits 0).
    pub fn wait(&mut self) -> Option<i32> {
        self.child
            .take()
            .and_then(|mut child| child.wait().ok())
            .and_then(|status| status.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::record_path;
    use serde_json::json;
    use std::sync::Mutex as StdMutex;

    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    /// Locate a real `optimus serve` binary (build optimus-cli first).
    /// Callers hold [`ENV_LOCK`] (this helper must not re-lock it).
    fn serve_binary() -> Option<PathBuf> {
        std::env::remove_var("OPTIMUS_SERVE_BIN");
        resolve_serve_binary()
    }

    #[test]
    fn resolve_prefers_env_then_sibling() {
        let _guard = ENV_LOCK.lock().unwrap();
        let Some(binary) = serve_binary() else {
            eprintln!("skipping: optimus binary not built");
            return;
        };
        std::env::set_var("OPTIMUS_SERVE_BIN", &binary);
        assert_eq!(resolve_serve_binary().as_deref(), Some(binary.as_path()));
    }

    #[test]
    fn hello_round_trip_over_real_stdio() {
        let _guard = ENV_LOCK.lock().unwrap();
        let Some(binary) = serve_binary() else {
            eprintln!("skipping: optimus binary not built");
            return;
        };
        std::env::set_var("OPTIMUS_SERVE_BIN", &binary);
        let home = tempfile::tempdir().expect("temp home");

        // The public B1 entry point: spawn-or-attach over stdio.
        let mut client = match connect(home.path(), 0) {
            ConnectOutcome::Spawned(client) => client,
            ConnectOutcome::Attached(_) => panic!("nothing held the home; must spawn"),
            ConnectOutcome::Diagnostic(diagnostic) => {
                panic!("connect failed: {diagnostic}")
            }
        };

        let reply = client.hello().expect("hello reply");
        assert_eq!(reply["protocol_version"], PROTOCOL_VERSION);
        assert_eq!(reply["capabilities"]["streaming"], true);

        // A registry call proves the same dispatch path.
        let doctor = client.call("doctor", json!({})).expect("doctor reply");
        assert!(doctor.is_object());

        client.close(); // Drop stdin; the child must exit 0 (R9).
        let code = client.wait().expect("reap serve");
        assert_eq!(code, 0, "stdio EOF must exit 0 (R9)");
    }

    #[test]
    fn record_path_is_under_the_home() {
        let home = tempfile::tempdir().expect("temp home");
        let path = record_path(home.path());
        assert!(path.starts_with(home.path()));
    }

    #[test]
    fn port_state_reflects_a_bind_probe() {
        // A held port probes Occupied; a free one probes Free (bind port
        // 0, take the port, drop the listener — the state the probe sees
        // is the state at probe time).
        let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let held = holder.local_addr().unwrap().port();
        assert_eq!(port_state(held), PortState::Occupied);
        let free = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(port_state(free), PortState::Free);
    }

    #[test]
    fn second_home_spawns_ephemerally_when_the_desired_port_is_held() {
        // #148: the desired port is machine-global but the record is
        // per-home. Home A's serve holds the port (here: a plain
        // listener — port state is what matters); connecting a fresh
        // home must spawn on an EPHEMERAL port, reach the record, and
        // speak the wire — not die at launch.
        let _guard = ENV_LOCK.lock().unwrap();
        let Some(binary) = serve_binary() else {
            eprintln!("skipping: optimus binary not built");
            return;
        };
        std::env::set_var("OPTIMUS_SERVE_BIN", &binary);
        let holder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let desired = holder.local_addr().unwrap().port();
        let home = tempfile::tempdir().expect("temp home");

        let mut client = match connect(home.path(), desired) {
            ConnectOutcome::Spawned(client) => client,
            ConnectOutcome::Attached(_) => {
                panic!("nothing held the home; must spawn")
            }
            ConnectOutcome::Diagnostic(diagnostic) => {
                panic!("connect failed: {diagnostic}")
            }
        };

        let record = healthy_record(home.path()).expect("record after spawn");
        assert_ne!(
            record.port, desired,
            "the spawn must land on a free port, not the held desired one"
        );
        assert!(record.port > 0);

        // The carrier is ready: hello round-trips and a registry call
        // proves the same dispatch path (the TUI's "· ready" state).
        let reply = client.hello().expect("hello reply");
        assert_eq!(reply["protocol_version"], PROTOCOL_VERSION);
        let doctor = client.call("doctor", json!({})).expect("doctor reply");
        assert!(doctor.is_object());

        client.close(); // Drop stdin; the child must exit 0 (R9).
        let code = client.wait().expect("reap serve");
        assert_eq!(code, 0, "stdio EOF must exit 0 (R9)");
    }

    #[test]
    fn parse_inbound_understands_all_frames() {
        let reply = parse_inbound(r#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#).unwrap();
        match reply {
            Inbound::Reply { id, result } => {
                assert_eq!(id, 7);
                assert_eq!(result["ok"], true);
            }
            _ => panic!("expected reply"),
        }
        let error = parse_inbound(
            r#"{"jsonrpc":"2.0","id":8,"error":{"code":-32602,"message":"malformed"}}"#,
        )
        .unwrap();
        match error {
            Inbound::Error { id, code, message } => {
                assert_eq!(id, 8);
                assert_eq!(code, -32602);
                assert_eq!(message, "malformed");
            }
            _ => panic!("expected error"),
        }
        let event = parse_inbound(
            r#"{"jsonrpc":"2.0","method":"event","params":{"stream_id":9,"event":{"type":"delta","text":"hi"}}}"#,
        )
        .unwrap();
        match event {
            Inbound::Event { stream_id, event } => {
                assert_eq!(stream_id, 9);
                assert_eq!(event["type"], "delta");
            }
            _ => panic!("expected event"),
        }
        assert!(parse_inbound("not json").is_none());
        assert!(parse_inbound(r#"{"jsonrpc":"2.0","method":"host.ready","params":{}}"#).is_none());
    }

    #[test]
    fn stream_wait_terminal_skips_intermediate_events() {
        let (tx, rx) = mpsc::channel();
        let stream = Stream { rx };
        tx.send(json!({"type":"delta","text":"a"})).unwrap();
        tx.send(json!({"type":"tool","phase":"executing"})).unwrap();
        tx.send(json!({"type":"done","result":{"session_id":"s"}}))
            .unwrap();
        let terminal = stream.wait_terminal().expect("terminal");
        assert_eq!(terminal["type"], "done");
    }

    #[test]
    fn hello_params_follow_the_ticket_rules() {
        // stdio client: hello must NOT carry a ticket key at all.
        let stdio = HostClient {
            pending: Arc::new(Mutex::new(HashMap::new())),
            streams: Arc::new(Mutex::new(HashMap::new())),
            child: None,
            stdin: None,
            ws: None,
            ws_ticket: None,
            next_id: AtomicU64::new(1),
        };
        let params = hello_params_for_test(&stdio);
        assert!(params.get("ticket").is_none(), "stdio omits the ticket");

        // WS attach client: the record token IS the hello credential.
        let ws = HostClient {
            pending: Arc::new(Mutex::new(HashMap::new())),
            streams: Arc::new(Mutex::new(HashMap::new())),
            child: None,
            stdin: None,
            ws: None,
            ws_ticket: Some("dial-token".to_string()),
            next_id: AtomicU64::new(1),
        };
        let params = hello_params_for_test(&ws);
        assert_eq!(params["ticket"], "dial-token");
    }

    /// The hello params as the client would send them (ticket rule probe).
    fn hello_params_for_test(client: &HostClient) -> Value {
        let mut params = json!({
            "protocol_version": PROTOCOL_VERSION,
            "client_kind": "tui",
        });
        if let Some(ticket) = &client.ws_ticket {
            params["ticket"] = json!(ticket);
        }
        params
    }

    #[test]
    fn check_port_diagnostic_display_names_the_port() {
        // Spec-015 R8: the post-spawn settle message must name the port,
        // not hint at a stale CLI or "no record".
        let text = ConnectDiagnostic::PortOccupied { port: 17865 }.to_string();
        assert!(text.contains("check port 17865"), "message: {text}");
        assert!(text.contains("serve failed to start"), "message: {text}");
        assert!(!text.contains("no record"), "message: {text}");
    }

    /// Write an executable fake `optimus serve` (shebang script) that
    /// behaves per `script_body` (exit-2 bind-failure stand-in).
    fn fake_serve_binary(dir: &std::path::Path, script_body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("fake-optimus");
        std::fs::write(&path, script_body).expect("write fake binary");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake binary");
        path
    }

    #[test]
    fn exit_two_with_occupied_desired_port_surfaces_check_port_diagnostic() {
        // Spec-015 R8: a DESIRED-port spawn that raced into a bind
        // failure (child exit 2) with the port still occupied after the
        // bounded re-probe must settle as "check port N" — never
        // "produced no record within the wait".
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("temp dir");
        let fake = fake_serve_binary(dir.path(), "#!/bin/sh\nsleep 0.5\nexit 2\n");
        std::env::set_var("OPTIMUS_SERVE_BIN", &fake);

        // A free port now; the test binds it ~250ms after the spawn (after
        // the client's pre-spawn free-check, before the settle's probe) to
        // reproduce the check-vs-bind race.
        let desired = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            probe.local_addr().unwrap().port()
        };
        let home = tempfile::tempdir().expect("temp home");
        let holder = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let listener = std::net::TcpListener::bind(("127.0.0.1", desired)).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(7));
            drop(listener);
        });

        let outcome = connect(home.path(), desired);
        holder.join().unwrap();
        match outcome {
            ConnectOutcome::Diagnostic(ConnectDiagnostic::PortOccupied { port }) => {
                assert_eq!(port, desired, "the diagnostic names the desired port");
            }
            other => panic!("expected PortOccupied settle, got {other:?}"),
        }
    }

    #[test]
    fn exit_two_with_free_port_is_generic_spawn_failure() {
        // Spec-015 R8: exit 2 with a free port (e.g. security-validation
        // failure) settles as the generic spawn-failed state — no port
        // hint, no "no record".
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().expect("temp dir");
        let fake = fake_serve_binary(dir.path(), "#!/bin/sh\nexit 2\n");
        std::env::set_var("OPTIMUS_SERVE_BIN", &fake);
        let desired = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            probe.local_addr().unwrap().port()
        };
        let home = tempfile::tempdir().expect("temp home");

        match connect(home.path(), desired) {
            ConnectOutcome::Diagnostic(ConnectDiagnostic::SpawnFailed(code)) => {
                assert_eq!(code, 2);
            }
            other => panic!("expected generic SpawnFailed(2), got {other:?}"),
        }
    }
}
