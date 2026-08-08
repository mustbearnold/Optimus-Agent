//! Wire dispatch for `optimus serve` (spec-015 R3/R4/R6/R9): the connection
//! record, the bounded worker pool, and the shared `process_frame` dispatch
//! used by BOTH carriers (WebSocket in `ws.rs`, stdio in `serve.rs`).
//!
//! Dispatch classes (R3): control-plane operations (`hello`, `chat_cancel`,
//! stream-registry operations, disconnect cleanup) execute on the
//! connection's own read/event loop; chat turns and registry/effect methods
//! share the bounded worker pool (production default 4 workers, bounded
//! queue 64 — a blocking call occupies only its worker, never a connection
//! loop). Every stream emits exactly one terminal event; disconnect cancels
//! in-flight streams and tracked campaign effects (R9).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use optimus_kernel::{CancellationToken, StreamControl};
use optimus_runtime::CampaignStore;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::chat::{chat_approval_resolve_cancellable, chat_turn_inner, stream_event_to_json};
use crate::children::run_child_turn;
use crate::contract::{wire_method_class, WireClass, PROTOCOL_VERSION};
use crate::handshake::{self, Carrier, ClientKind, HelloError, HelloParams};
use crate::record;
use crate::router::handle_ipc;
use crate::serve::{ServeState, EXIT_BIND_OR_SECURITY, RATE_LIMIT_PER_MINUTE, WORKER_COUNT};

/// Outbound wire traffic for one connection, ordered by the single channel.
#[derive(Debug)]
pub enum Outbound {
    Reply {
        id: u64,
        result: Value,
    },
    Error {
        id: Option<u64>,
        code: i64,
        message: String,
    },
    Event {
        stream_id: u64,
        event: Value,
    },
    Notify {
        method: &'static str,
        params: Value,
    },
    Pong(Vec<u8>),
    /// Close the carrier (WS close frame / stdio teardown).
    Close {
        code: u16,
        reason: String,
    },
}

/// One client connection (shared between the read loop and workers).
pub struct Connection {
    home: std::path::PathBuf,
    /// Renderer/tui/cli/shell — set by the completed hello.
    kind: Mutex<Option<ClientKind>>,
    /// Stream registry: `stream_id` → cancellation token (per-connection,
    /// the `apps/optimus-tauri/src/main.rs:89-93` pattern).
    streams: Mutex<HashMap<u64, CancellationToken>>,
    /// Binding keys currently resolving (a second
    /// `chat_approval_resolve_start` for one is `-32602`, R6).
    resolving: Mutex<HashSet<String>>,
    /// Campaign ids with an effect in flight on a worker (disconnect →
    /// `CampaignStore::cancel`, R9).
    pending_effects: Mutex<HashSet<String>>,
    outbound: mpsc::SyncSender<Outbound>,
    rate: Mutex<WindowRateLimiter>,
    handshake_done: AtomicBool,
    /// Origin label for the post-hello connections.log line (ws.rs sets it
    /// at upgrade; `"null"`/`"missing"` or the origin value, R8).
    pub origin_label: Mutex<String>,
}

impl Connection {
    pub fn new(home: std::path::PathBuf, outbound: mpsc::SyncSender<Outbound>) -> Self {
        Self {
            home,
            kind: Mutex::new(None),
            streams: Mutex::new(HashMap::new()),
            resolving: Mutex::new(HashSet::new()),
            pending_effects: Mutex::new(HashSet::new()),
            outbound,
            rate: Mutex::new(WindowRateLimiter::new(
                RATE_LIMIT_PER_MINUTE,
                Duration::from_secs(60),
            )),
            handshake_done: AtomicBool::new(false),
            origin_label: Mutex::new("missing".to_string()),
        }
    }

    pub fn kind(&self) -> ClientKind {
        self.kind.lock().unwrap().unwrap_or(ClientKind::Renderer)
    }

    pub fn home(&self) -> &std::path::Path {
        &self.home
    }

    pub fn send(&self, outbound: Outbound) -> bool {
        self.outbound.send(outbound).is_ok()
    }

    pub fn reply(&self, id: u64, result: Value) {
        self.send(Outbound::Reply { id, result });
    }

    pub fn error(&self, id: Option<u64>, code: i64, message: impl Into<String>) {
        self.send(Outbound::Error {
            id,
            code,
            message: message.into(),
        });
    }

    /// True once the hello handshake completed.
    pub fn handshake_done(&self) -> bool {
        self.handshake_done.load(Ordering::Relaxed)
    }
}

pub(crate) struct WindowRateLimiter {
    limit: u32,
    window: Duration,
    started: Instant,
    used: u32,
}

impl WindowRateLimiter {
    pub(crate) fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            started: Instant::now(),
            used: 0,
        }
    }

    pub(crate) fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.started) >= self.window {
            self.started = now;
            self.used = 0;
        }
        if self.used >= self.limit {
            return false;
        }
        self.used += 1;
        true
    }
}

pub(crate) enum PoolJob {
    Registry {
        conn: Arc<Connection>,
        id: u64,
        method: String,
        params: Value,
    },
    ChatStart {
        conn: Arc<Connection>,
        stream_id: u64,
        request: Value,
        token: CancellationToken,
        children: Option<Arc<dyn optimus_kernel::ChildCoordinator>>,
    },
    ChildRun {
        spec: crate::children::ChildRunSpec,
        token: CancellationToken,
        live: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
        children: Option<Arc<dyn optimus_kernel::ChildCoordinator>>,
    },
    ResolveStart {
        conn: Arc<Connection>,
        stream_id: u64,
        params: Value,
        token: CancellationToken,
    },
}

enum PoolError {
    /// The bounded queue is full: reject the NEW request with `-32603`
    /// "server busy"; the connection stays healthy.
    Busy,
    /// The pool is gone (all workers died): connection-fatal internal error.
    Dead,
}

pub(crate) struct WorkerPool {
    tx: mpsc::SyncSender<PoolJob>,
}

impl WorkerPool {
    /// Start the pool over a caller-owned channel: `serve` builds the
    /// children runtime over the same channel (spec-034 R4).
    pub(crate) fn start_with_channel(
        tx: mpsc::SyncSender<PoolJob>,
        rx: mpsc::Receiver<PoolJob>,
    ) -> Self {
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..WORKER_COUNT {
            let rx = Arc::clone(&rx);
            std::thread::spawn(move || {
                loop {
                    // Scope the guard so it drops BEFORE the job runs:
                    // `while let Ok(job) = rx.lock().unwrap().recv()`
                    // would hold the mutex across the body and serialize
                    // the whole pool on one worker (regression: a registry
                    // call queued behind a streaming turn would wait for
                    // the turn to finish).
                    let job = rx.lock().unwrap().recv();
                    let Ok(job) = job else { break };
                    run_pool_job(job);
                }
            });
        }
        Self { tx }
    }

    fn dispatch(&self, job: PoolJob) -> Result<(), PoolError> {
        self.tx.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => PoolError::Busy,
            mpsc::TrySendError::Disconnected(_) => PoolError::Dead,
        })
    }
}

fn run_pool_job(job: PoolJob) {
    match job {
        PoolJob::Registry {
            conn,
            id,
            method,
            params,
        } => {
            // Track campaign effects for disconnect-cancellation (R9):
            // CampaignStore::cancel on a completed campaign is a no-op, so
            // removal after completion is race-safe.
            let tracked = (method == "campaign_run")
                .then(|| params.get("id").and_then(Value::as_str).map(str::to_string))
                .flatten();
            if let Some(campaign_id) = &tracked {
                conn.pending_effects
                    .lock()
                    .unwrap()
                    .insert(campaign_id.clone());
            }
            let outcome = handle_ipc(&conn.home, &method, params);
            if let Some(campaign_id) = &tracked {
                conn.pending_effects.lock().unwrap().remove(campaign_id);
            }
            match outcome {
                Ok(result) => conn.reply(id, result),
                // The registry's own unknown-method error resolves to the
                // pinned `-32601`; every other registry error is an internal
                // error carrying the actionable text.
                Err(error) if error.starts_with("unknown method:") => {
                    conn.error(Some(id), -32601, error)
                }
                Err(error) => conn.error(Some(id), -32603, error),
            }
        }
        PoolJob::ChatStart {
            conn,
            stream_id,
            request,
            token,
            children,
        } => {
            let mut on_event = |event| {
                let payload = stream_event_to_json(&event);
                match conn.outbound.try_send(Outbound::Event {
                    stream_id,
                    event: payload,
                }) {
                    Ok(()) => StreamControl::Continue,
                    // The consumer is gone: delivered=false → Cancel (R9).
                    Err(_) => StreamControl::Cancel,
                }
            };
            let outcome = chat_turn_inner(
                &conn.home,
                request,
                Some(&mut on_event),
                &token,
                children,
                false,
            );
            let terminal = match outcome {
                Ok(result) => json!({ "type": "done", "result": result }),
                Err(error) if token.is_cancelled() => {
                    json!({ "type": "cancelled", "error": error })
                }
                Err(error) => json!({ "type": "error", "error": error }),
            };
            // The terminal event MUST arrive even if intermediate events
            // were dropped (exactly-one-terminal invariant, R6): blocking
            // send, bounded by the connection's write timeout.
            let _ = conn.outbound.send(Outbound::Event {
                stream_id,
                event: terminal,
            });
            conn.streams.lock().unwrap().remove(&stream_id);
        }
        PoolJob::ChildRun {
            spec,
            token,
            live,
            children,
        } => {
            run_child_turn(spec, token, live, children);
        }
        PoolJob::ResolveStart {
            conn,
            stream_id,
            params,
            token,
        } => {
            let binding_key = resolve_binding_key(&params);
            let mut on_event = |event| {
                let payload = stream_event_to_json(&event);
                match conn.outbound.try_send(Outbound::Event {
                    stream_id,
                    event: payload,
                }) {
                    Ok(()) => StreamControl::Continue,
                    Err(_) => StreamControl::Cancel,
                }
            };
            let outcome =
                chat_approval_resolve_cancellable(&conn.home, params, Some(&mut on_event), &token);
            // Cancelled-wins (the Tauri path, `main.rs:162-168`): a
            // cancelled resolve is a cancelled resolve even when the
            // settlement itself succeeded.
            let terminal = if token.is_cancelled() {
                let error = outcome
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| "approval continuation cancelled".into());
                json!({ "type": "cancelled", "error": error })
            } else {
                match outcome {
                    Ok(result) => json!({ "type": "done", "result": result }),
                    Err(error) => json!({ "type": "error", "error": error }),
                }
            };
            let _ = conn.outbound.send(Outbound::Event {
                stream_id,
                event: terminal,
            });
            if let Some(key) = binding_key {
                conn.resolving.lock().unwrap().remove(&key);
            }
            conn.streams.lock().unwrap().remove(&stream_id);
        }
    }
}

/// The binding key for the duplicate-resolve rejection (`-32602`, R6).
pub(crate) fn resolve_binding_key(params: &Value) -> Option<String> {
    let session_id = params.get("session_id").and_then(Value::as_str)?;
    let run_id = params.get("run_id").and_then(Value::as_str)?;
    let call_id = params.get("call_id").and_then(Value::as_str)?;
    Some(format!("{session_id}:{run_id}:{call_id}"))
}

/// Cancel a connection's in-flight streams + tracked effects (R9). Runs on
/// the connection loop (control-plane class; the token flip is a `SeqCst`
/// store and the campaign-store write is bounded — no deadlock).
pub fn disconnect_cleanup(conn: &Connection) {
    let streams = std::mem::take(&mut *conn.streams.lock().unwrap());
    for token in streams.values() {
        token.cancel();
    }
    let effects = std::mem::take(&mut *conn.pending_effects.lock().unwrap());
    if !effects.is_empty() {
        if let Ok(store) = CampaignStore::open(&conn.home) {
            for campaign_id in effects {
                if let Ok(id) = uuid::Uuid::parse_str(&campaign_id) {
                    let _ = store.cancel(id);
                }
            }
        }
    }
}

/// Process one client frame (JSON-RPC 2.0 object): the connection loop's
/// dispatch — control-plane inline, everything else on the worker pool
/// (R3).
pub fn process_frame(state: &ServeState, conn: &Arc<Connection>, carrier: Carrier, value: Value) {
    let Some(object) = value.as_object() else {
        conn.error(None, -32600, "invalid request: JSON value is not an object");
        return;
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        conn.error(
            None,
            -32600,
            "invalid request: missing or wrong jsonrpc member",
        );
        return;
    }
    // Id-less frames are notifications: dropped, never dispatched, never
    // answered (R6). The drop rule governs frames WITHOUT an id member —
    // an id member that is present but not a u64 is a BAD id: `-32600`
    // with `id:null` (R4). Credential-layer closes still apply to
    // id-ful frames: an absent-ticket hello is closed in the hello
    // handler below; id-less frames never reach it (dropped here).
    let id = match object.get("id") {
        None => return,
        Some(Value::Number(number)) => match number.as_u64() {
            Some(id) => id,
            None => {
                conn.error(None, -32600, "invalid request: bad id");
                return;
            }
        },
        Some(_) => {
            conn.error(None, -32600, "invalid request: bad id");
            return;
        }
    };
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        conn.error(Some(id), -32600, "invalid request: method required");
        return;
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);

    let handshake_done = conn.handshake_done();
    if !handshake_done && method != "hello" {
        conn.error(None, -32600, "method before hello");
        return;
    }
    if handshake_done && method == "hello" {
        conn.error(None, -32600, "second hello");
        return;
    }

    if method == "hello" {
        let hello = match handshake::validate_hello(
            &params,
            carrier,
            &state.ticket,
            state.process_secret.as_deref(),
        ) {
            Ok(hello) => hello,
            Err(HelloError::InvalidRequest(message)) => {
                conn.error(None, -32600, message);
                return;
            }
            Err(HelloError::TicketRejected) => {
                if carrier == Carrier::Stdio {
                    // Security-validation class (R5): stderr diagnostic +
                    // exit 2. The exit code is recorded so `wait()` returns
                    // 2; the stderr diagnostic is the shell's signal.
                    eprintln!(
                        "[optimus serve] shell-kind hello rejected over stdio: staging process secret missing or invalid"
                    );
                    state
                        .exit_code
                        .lock()
                        .unwrap()
                        .get_or_insert(EXIT_BIND_OR_SECURITY);
                    state.shutdown.store(true, Ordering::Relaxed);
                    return;
                }
                conn.error(Some(id), -32000, "ticket rejected");
                conn.send(Outbound::Close {
                    code: 4001,
                    reason: "ticket rejected".into(),
                });
                return;
            }
            Err(HelloError::UnsupportedVersion(version)) => {
                conn.error(
                    Some(id),
                    -32001,
                    format!("unsupported protocol version: {version}"),
                );
                conn.send(Outbound::Close {
                    code: 4002,
                    reason: "unsupported protocol version".into(),
                });
                return;
            }
        };
        complete_hello(state, conn, carrier, id, hello);
        return;
    }

    match wire_method_class(method, conn.kind()) {
        WireClass::Control => match method {
            "chat_cancel" => {
                // Closed-form rate-limit exemption (R7): chat_cancel is
                // control-plane and never rate-limited.
                let Some(stream_id) = params.get("stream_id").and_then(Value::as_u64) else {
                    conn.error(Some(id), -32602, "malformed stream_id");
                    return;
                };
                let requested = conn
                    .streams
                    .lock()
                    .unwrap()
                    .get(&stream_id)
                    .is_some_and(|token| {
                        token.cancel();
                        true
                    });
                conn.reply(id, json!({ "requested": requested }));
            }
            _ => unreachable!("hello handled above; only chat_cancel is Control post-hello"),
        },
        WireClass::ChatStart => {
            if !rate_allow(conn) {
                conn.error(Some(id), -32603, "rate limit exceeded");
                return;
            }
            let Some(stream_id) = params.get("stream_id").and_then(Value::as_u64) else {
                conn.error(Some(id), -32602, "malformed stream_id");
                return;
            };
            let request = params.get("request").cloned().unwrap_or(Value::Null);
            let Some(token) = register_stream(conn, stream_id) else {
                conn.error(Some(id), -32603, "stream limit reached");
                return;
            };
            match state.pool.dispatch(PoolJob::ChatStart {
                conn: Arc::clone(conn),
                children: state.children.clone(),
                stream_id,
                request,
                token,
            }) {
                Ok(()) => {
                    conn.reply(id, json!({ "stream_id": stream_id }));
                }
                Err(PoolError::Busy) => {
                    conn.streams.lock().unwrap().remove(&stream_id);
                    conn.error(Some(id), -32603, "server busy");
                }
                Err(PoolError::Dead) => {
                    conn.streams.lock().unwrap().remove(&stream_id);
                    internal_fault(conn);
                }
            }
        }
        WireClass::ResolveStart => {
            if !rate_allow(conn) {
                conn.error(Some(id), -32603, "rate limit exceeded");
                return;
            }
            let Some(stream_id) = params.get("stream_id").and_then(Value::as_u64) else {
                conn.error(Some(id), -32602, "malformed stream_id");
                return;
            };
            let resolve_params = params.get("params").cloned().unwrap_or(Value::Null);
            if let Some(key) = resolve_binding_key(&resolve_params) {
                if !conn.resolving.lock().unwrap().insert(key.clone()) {
                    conn.error(Some(id), -32602, "approval binding is already resolving");
                    return;
                }
            }
            let Some(token) = register_stream(conn, stream_id) else {
                conn.resolving.lock().unwrap().clear();
                conn.error(Some(id), -32603, "stream limit reached");
                return;
            };
            match state.pool.dispatch(PoolJob::ResolveStart {
                conn: Arc::clone(conn),
                stream_id,
                params: resolve_params,
                token,
            }) {
                Ok(()) => conn.reply(id, json!({ "stream_id": stream_id })),
                Err(PoolError::Busy) => {
                    conn.streams.lock().unwrap().remove(&stream_id);
                    conn.resolving.lock().unwrap().clear();
                    conn.error(Some(id), -32603, "server busy");
                }
                Err(PoolError::Dead) => {
                    conn.streams.lock().unwrap().remove(&stream_id);
                    conn.resolving.lock().unwrap().clear();
                    internal_fault(conn);
                }
            }
        }
        WireClass::ShellGated => {
            if !rate_allow(conn) {
                conn.error(Some(id), -32603, "rate limit exceeded");
                return;
            }
            // Server-side secret injection (R7): the injected secret
            // OVERRIDES any client-supplied token, so `os.rs:88-92`'s
            // per-call constant-time check passes unchanged.
            let mut params = params;
            if let (Some(secret), Some(object)) =
                (state.process_secret.as_deref(), params.as_object_mut())
            {
                object.insert(
                    "native_selection_token".into(),
                    Value::String(secret.to_string()),
                );
            }
            dispatch_registry(state, conn, id, method, params);
        }
        WireClass::Registry => {
            if !rate_allow(conn) {
                conn.error(Some(id), -32603, "rate limit exceeded");
                return;
            }
            dispatch_registry(state, conn, id, method, params);
        }
        WireClass::Rejected => {
            conn.error(Some(id), -32601, format!("unknown method: {method}"));
        }
    }
}

fn rate_allow(conn: &Connection) -> bool {
    conn.rate.lock().unwrap().allow(Instant::now())
}

/// Register a stream under the per-connection 16-stream bound; `None` when
/// the bound is reached (the 17th → `-32603` "stream limit reached", R7).
fn register_stream(conn: &Connection, stream_id: u64) -> Option<CancellationToken> {
    let mut streams = conn.streams.lock().unwrap();
    if streams.len() >= handshake::MAX_STREAMS {
        return None;
    }
    let token = CancellationToken::new();
    streams.insert(stream_id, token.clone());
    Some(token)
}

fn dispatch_registry(
    state: &ServeState,
    conn: &Arc<Connection>,
    id: u64,
    method: &str,
    params: Value,
) {
    match state.pool.dispatch(PoolJob::Registry {
        conn: Arc::clone(conn),
        id,
        method: method.to_string(),
        params,
    }) {
        Ok(()) => {}
        Err(PoolError::Busy) => conn.error(Some(id), -32603, "server busy"),
        Err(PoolError::Dead) => internal_fault(conn),
    }
}

/// Connection-fatal internal error: `host.error` fires ONLY here, and only
/// immediately before close (R6) — never for recoverable per-request
/// failures (those are `-326xx` replies) or stream failures (those are
/// stream-terminal `error` events). Pinned by serve_protocol.rs (the
/// never-battery) and this module's unit test (the emission path).
pub(crate) fn internal_fault(conn: &Connection) {
    conn.send(Outbound::Notify {
        method: "host.error",
        params: json!({ "code": -32603, "message": "dispatch unavailable" }),
    });
    conn.send(Outbound::Close {
        code: 1011,
        reason: "internal server error".into(),
    });
    disconnect_cleanup(conn);
}

/// Complete the hello handshake: hello result + `host.ready` notification
/// (R5), the accepted-connection log line post-handshake (R8), and the
/// kind/stream registry state.
fn complete_hello(
    state: &ServeState,
    conn: &Arc<Connection>,
    carrier: Carrier,
    id: u64,
    hello: HelloParams,
) {
    *conn.kind.lock().unwrap() = Some(hello.client_kind);
    conn.handshake_done.store(true, Ordering::Relaxed);
    conn.reply(
        id,
        json!({
            "protocol_version": PROTOCOL_VERSION,
            "capabilities": { "streaming": true, "carriers": ["stdio", "ws"] },
        }),
    );
    conn.send(Outbound::Notify {
        method: "host.ready",
        params: json!({ "protocol_version": PROTOCOL_VERSION }),
    });
    if carrier == Carrier::Ws {
        // Post-credential-validation: a rejected handshake never logs; a
        // line proves dial AND handshake (R8).
        let origin = conn.origin_label.lock().unwrap().clone();
        record::log_connection(&state.home, &origin);
    }
}

/// Serialize an outbound item to one JSON-RPC 2.0 line (stdio framing; the
/// WS carrier wraps the same value in a text frame).
pub fn serialize_outbound(outbound: &Outbound) -> String {
    match outbound {
        Outbound::Reply { id, result } => {
            json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
        }
        Outbound::Error { id, code, message } => {
            let id = id.map(Value::from).unwrap_or(Value::Null);
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
                .to_string()
        }
        Outbound::Event { stream_id, event } => json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": { "stream_id": stream_id, "event": event },
        })
        .to_string(),
        Outbound::Notify { method, params } => {
            json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
        }
        Outbound::Pong(_) | Outbound::Close { .. } => String::new(),
    }
}
