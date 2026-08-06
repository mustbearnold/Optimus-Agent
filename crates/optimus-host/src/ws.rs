//! WebSocket carrier for `optimus serve` (spec-015 R3/R5/R6/R7).
//!
//! The accept loop is a raw loopback TcpListener with a minimal hand-rolled
//! HTTP parser serving exactly two surfaces on the record port:
//! `GET /api/health` (Bearer-gated, the record token IS the Bearer) and the
//! RFC 6455 WebSocket upgrade on `/ws` (Origin allowlisted per R7, framing
//! via tungstenite). Everything else is 404.
//!
//! Owning the socket (rather than tiny_http's opaque `Request::upgrade`
//! stream) is what makes the pinned timeouts real: SO_RCVTIMEO carries the
//! 30 s hello deadline pre-hello and the 100 ms poll tick post-hello, and
//! SO_SNDTIMEO carries the 10 s write-failure bound (R9's delivered=false →
//! Cancel path). See the transport note in `serve.rs`.
//!
//! Per connection: ONE thread does the polling read loop AND the writes —
//! the socket has real timeouts, so the outbound channel and the socket are
//! multiplexed on one loop (no cross-thread stream sharing, no watchdogs).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use serde_json::Value;
use tungstenite::protocol::{Message, Role, WebSocketConfig, WebSocketContext};

use crate::dispatch::{
    disconnect_cleanup, process_frame, serialize_outbound, Connection, Outbound,
};
use crate::handshake::{self, Carrier, MAX_CONNECTIONS};
use crate::serve::{ServeState, MAX_FRAME_BYTES, OUTBOUND_CAPACITY, PING_INTERVAL, WRITE_TIMEOUT};

/// Post-hello read-poll tick (the socket's SO_RCVTIMEO between frames).
const POLL_TICK: Duration = Duration::from_millis(100);

/// The loopback accept loop: health + WS upgrades (see the module docs).
pub fn accept_loop(listener: TcpListener, state: Arc<ServeState>) {
    for stream in listener.incoming() {
        if state.shutdown.load(Ordering::Relaxed) {
            break;
        }
        let Ok(stream) = stream else {
            continue;
        };
        // Per-connection bounds need socket-level timeouts (R7: 30 s hello
        // deadline; R9: 10 s write timeout) — see the module docs.
        let _ = stream.set_read_timeout(Some(handshake::hello_timeout_duration()));
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let mut stream = stream;
        let Ok((method, target, headers)) = read_http_head(&mut stream) else {
            let _ = write_http(&mut stream, 400, "bad request");
            continue;
        };
        if method == "GET" && target == "/api/health" {
            let authorized = headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
                .is_some_and(|(_, value)| bearer_matches(value, &state.ticket));
            let (status, body) = if authorized {
                (200, "{\"ok\":true,\"streaming\":true,\"transport\":\"ws\"}")
            } else {
                (401, "{\"ok\":false}")
            };
            let _ = write_http(&mut stream, status, body);
            continue;
        }
        if method == "GET" && target == "/ws" {
            let Some(ws_key) = headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("Sec-WebSocket-Key"))
                .map(|(_, value)| value.clone())
            else {
                let _ = write_http(&mut stream, 400, "missing Sec-WebSocket-Key");
                continue;
            };
            let origin = headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("Origin"))
                .map(|(_, value)| value.clone());
            if !handshake::origin_allowed(origin.as_deref()) {
                let _ = write_http(&mut stream, 403, "origin not allowed");
                continue;
            }
            let origin_label = match origin.as_deref() {
                None => "missing".to_string(),
                Some(origin) => origin.to_string(),
            };
            if state.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                // Upgrade anyway so the client receives the pinned `4003`
                // close code instead of an HTTP error (R4/R7).
                let _ = write_ws_upgrade(&mut stream, &ws_key);
                let (tx, rx) = mpsc::sync_channel::<Outbound>(OUTBOUND_CAPACITY);
                let conn = Arc::new(Connection::new(state.home.clone(), tx));
                let _ = conn.send(Outbound::Close {
                    code: 4003,
                    reason: "too many connections".into(),
                });
                let state = Arc::clone(&state);
                std::thread::spawn(move || {
                    run_connection(stream, conn, rx, state, true);
                });
                continue;
            }
            let _ = write_ws_upgrade(&mut stream, &ws_key);
            let (tx, rx) = mpsc::sync_channel::<Outbound>(OUTBOUND_CAPACITY);
            let conn = Arc::new(Connection::new(state.home.clone(), tx));
            *conn.origin_label.lock().unwrap() = origin_label;
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                run_connection(stream, conn, rx, state, false);
            });
            continue;
        }
        let _ = write_http(&mut stream, 404, "not found");
    }
}

/// One WS connection: tungstenite framing over the upgraded socket, the
/// connection loop multiplexing the outbound channel and the socket.
///
/// `reject_only` connections (the 9th) write the queued `4003` close and
/// exit without reading.
fn run_connection(
    stream: TcpStream,
    conn: Arc<Connection>,
    outbound: mpsc::Receiver<Outbound>,
    state: Arc<ServeState>,
    reject_only: bool,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME_BYTES))
        .max_frame_size(Some(MAX_FRAME_BYTES));
    let mut ctx = WebSocketContext::new(Role::Server, Some(config));
    let mut stream = stream;
    let outbound = outbound;
    let mut last_ping = Instant::now();

    let mut close_code: Option<u16> = None;
    let mut done = false;

    while !done {
        // 1. Drain pending outbound first (replies/events/pings/close).
        //    A reject-only connection (the 9th) has exactly one queued item
        //    — the `4003` close — and exits right after the drain.
        while let Ok(item) = outbound.try_recv() {
            match item {
                Outbound::Close { code, reason } => {
                    if let Err(error) = ctx.close(
                        &mut stream,
                        Some(tungstenite::protocol::CloseFrame {
                            code: tungstenite::protocol::frame::coding::CloseCode::from(code),
                            reason: reason.into(),
                        }),
                    ) {
                        let _ = error;
                    }
                    close_code = Some(code);
                    done = true;
                    break;
                }
                Outbound::Pong(payload) => {
                    let _ = ctx.write(&mut stream, Message::Pong(payload.into()));
                }
                other => {
                    let payload = serialize_outbound(&other);
                    if write_text(&mut ctx, &mut stream, &payload).is_err() {
                        // WS send failure: delivered=false → Cancel (R9).
                        disconnect_cleanup(&conn);
                        done = true;
                        break;
                    }
                }
            }
        }
        if done || reject_only {
            break;
        }

        // 2. Keepalive ping when idle (R7: every 30 s).
        if last_ping.elapsed() >= PING_INTERVAL {
            if ctx
                .write(&mut stream, Message::Ping(Vec::new().into()))
                .is_err()
            {
                disconnect_cleanup(&conn);
                break;
            }
            last_ping = Instant::now();
        }

        // 3. Poll the socket (SO_RCVTIMEO = POLL_TICK post-hello; the
        // pre-hello timeout stays at the hello deadline — R7).
        match ctx.read(&mut stream) {
            Ok(Message::Text(text)) => {
                let frame = text.as_str();
                if frame.len() > MAX_FRAME_BYTES {
                    close_with(&mut ctx, &mut stream, 4003, "frame too large");
                    close_code = Some(4003);
                    break;
                }
                let Some(value) = serde_json::from_str::<Value>(frame).ok() else {
                    conn.error(None, -32700, "parse error");
                    continue;
                };
                process_frame(&state, &conn, Carrier::Ws, value);
                if conn.handshake_done() {
                    let _ = stream.set_read_timeout(Some(POLL_TICK));
                }
            }
            Ok(Message::Binary(_)) => {
                close_with(
                    &mut ctx,
                    &mut stream,
                    4003,
                    "binary frames are not supported",
                );
                close_code = Some(4003);
                break;
            }
            Ok(Message::Ping(payload)) => {
                // RFC 6455: pong as soon as practical, via the writer.
                let _ = conn.send(Outbound::Pong(payload.to_vec()));
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => {
                let _ = ctx.close(&mut stream, None);
                done = true;
            }
            Ok(Message::Frame(_)) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if !conn.handshake_done() {
                    // Pre-hello, the socket read timeout IS the 30 s hello
                    // deadline (R7): a silent connection is closed so it
                    // cannot hold an 8-slot bound indefinitely.
                    close_with(&mut ctx, &mut stream, 4001, "hello deadline exceeded");
                    close_code = Some(4001);
                    break;
                }
                // Post-hello: poll tick, keep looping.
            }
            // Framing violations terminate loudly with 4003 (R4): oversized
            // frames (Capacity), non-UTF-8 text / unmasked client frames
            // (Protocol).
            Err(tungstenite::Error::Capacity(_)) => {
                close_with(&mut ctx, &mut stream, 4003, "frame too large");
                close_code = Some(4003);
                break;
            }
            Err(tungstenite::Error::Protocol(_)) => {
                close_with(&mut ctx, &mut stream, 4003, "protocol violation");
                close_code = Some(4003);
                break;
            }
            Err(_) => {
                // Connection closed by the peer: cancel streams + tracked
                // effects (R9) and release the slot.
                disconnect_cleanup(&conn);
                break;
            }
        }
    }

    if close_code.is_none() {
        disconnect_cleanup(&conn);
    }
    drop(conn);
    drop(outbound);
    state.connections.fetch_sub(1, Ordering::SeqCst);
}

fn write_text(
    ctx: &mut WebSocketContext,
    stream: &mut TcpStream,
    payload: &str,
) -> Result<(), tungstenite::Error> {
    ctx.write(stream, Message::Text(payload.into()))?;
    ctx.flush(stream)
}

fn close_with(ctx: &mut WebSocketContext, stream: &mut TcpStream, code: u16, reason: &str) {
    let _ = ctx.close(
        stream,
        Some(tungstenite::protocol::CloseFrame {
            code: tungstenite::protocol::frame::coding::CloseCode::from(code),
            reason: reason.into(),
        }),
    );
    // Polite close handshake: the peer's close echo (or EOF) releases the
    // socket; dropping immediately after the close frame can race the
    // kernel into an RST that swallows the frame on a loaded host.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    while matches!(
        ctx.read(stream),
        Ok(Message::Close(_)) | Ok(Message::Ping(_)) | Ok(Message::Pong(_))
    ) {}
}

type HttpHead = (String, String, Vec<(String, String)>);

/// Minimal HTTP request-head parser (GET only; no body). Bounded head size.
fn read_http_head(stream: &mut TcpStream) -> Result<HttpHead, ()> {
    let mut head = Vec::with_capacity(1024);
    let mut buf = [0u8; 4096];
    loop {
        let read = stream.read(&mut buf).map_err(|_| ())?;
        if read == 0 {
            return Err(());
        }
        head.extend_from_slice(&buf[..read]);
        if head.len() > 16 * 1024 {
            return Err(());
        }
        if head.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    parse_http_head(&head).ok_or(())
}

fn parse_http_head(bytes: &[u8]) -> Option<HttpHead> {
    let text = String::from_utf8(bytes.to_vec()).ok()?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let headers = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    Some((method, target, headers))
}

fn write_http(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// The RFC 6455 upgrade response (Sec-WebSocket-Accept derived from the
/// client key). The WS framing itself is handled by tungstenite.
fn write_ws_upgrade(stream: &mut TcpStream, ws_key: &str) -> std::io::Result<()> {
    let accept = tungstenite::handshake::derive_accept_key(ws_key.as_bytes());
    write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )
}

/// Constant-time bearer check against the dial ticket (the health endpoint
/// is protected by the same credential as the WS handshake, R8).
fn bearer_matches(header: &str, ticket: &str) -> bool {
    let Some(presented) = header.strip_prefix("Bearer ") else {
        return false;
    };
    presented.len() == ticket.len()
        && presented
            .as_bytes()
            .iter()
            .zip(ticket.as_bytes())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_head_parses_method_target_headers() {
        let (method, target, headers) = parse_http_head(
            b"GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer abc\r\n\r\n",
        )
        .unwrap();
        assert_eq!(method, "GET");
        assert_eq!(target, "/api/health");
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| name == "Authorization")
                .map(|(_, value)| value.as_str()),
            Some("Bearer abc")
        );
    }

    #[test]
    fn http_head_rejects_oversized_and_binary() {
        let huge = format!("GET / HTTP/1.1\r\nX-Pad: {}\r\n\r\n", "x".repeat(20 * 1024));
        // The size bound lives in the socket reader; the parser itself is
        // lenient about content but strict about shape.
        assert!(parse_http_head(huge.as_bytes()).is_some());
        assert!(parse_http_head(b"GET \xff\xfe\x00 HTTP/1.1\r\n\r\n").is_none());
        assert!(parse_http_head(b"GET / HTTP/1.1\r\n\r\n").is_some());
        assert!(parse_http_head(b"GET").is_none(), "no target");
        assert!(parse_http_head(b"").is_none());
    }

    #[test]
    fn bearer_matches_constant_time() {
        assert!(bearer_matches("Bearer abc", "abc"));
        assert!(!bearer_matches("Bearer abcd", "abc"));
        assert!(!bearer_matches("abc", "abc"));
        assert!(!bearer_matches("Bearer abc", "abd"));
    }
}
