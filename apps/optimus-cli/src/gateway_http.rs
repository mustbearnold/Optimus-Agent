//! HTTP webhook face for the durable gateway (channel adapters POST here).
//!
//! Endpoints (bound to 127.0.0.1 only — no remote exposure by default):
//!   GET  /health
//!   POST /inbound   JSON { text, channel?, provider?, session_id? }
//!   POST /drain     process one inbox message (optional body ignored)
//!   POST /drain_all process entire inbox
//!   GET  /inbox
//!   GET  /outbox

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use optimus_kernel::{enqueue, list_inbox, list_outbox};
use serde::Deserialize;
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const MAX_REQUEST_BODY_BYTES: usize = 256 * 1024;
const REQUESTS_PER_MINUTE: u32 = 120;
const MAX_DRAIN_ALL_MESSAGES: usize = 100;

struct WindowRateLimiter {
    started: Instant,
    used: u32,
}

impl WindowRateLimiter {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            used: 0,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.started) >= Duration::from_secs(60) {
            self.started = now;
            self.used = 0;
        }
        if self.used >= REQUESTS_PER_MINUTE {
            return false;
        }
        self.used += 1;
        true
    }
}

fn read_body_bounded(reader: &mut dyn Read) -> Result<String, u16> {
    let mut bytes = Vec::with_capacity(8192);
    reader
        .take(MAX_REQUEST_BODY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| 400u16)?;
    if bytes.len() > MAX_REQUEST_BODY_BYTES {
        return Err(413);
    }
    String::from_utf8(bytes).map_err(|_| 400)
}

fn public_error(_internal: &str) -> &'static str {
    "request failed"
}

#[derive(Debug, Clone)]
pub struct GatewaySecurity {
    token: String,
    allowed_origins: [String; 2],
}

impl GatewaySecurity {
    pub fn new(port: u16, token: impl Into<String>) -> Result<Self, String> {
        let token = token.into();
        if token.len() < 32 {
            return Err("OPTIMUS_GATEWAY_TOKEN must contain at least 32 characters".into());
        }
        Ok(Self {
            token,
            allowed_origins: [
                format!("http://127.0.0.1:{port}"),
                format!("http://localhost:{port}"),
            ],
        })
    }

    fn authorize(&self, request: &Request) -> Result<(), u16> {
        let bearer = format!("Bearer {}", self.token);
        if header_value(request.headers(), "Authorization") != Some(bearer.as_str()) {
            return Err(401);
        }
        if let Some(origin) = header_value(request.headers(), "Origin") {
            if !self.allowed_origins.iter().any(|allowed| allowed == origin)
                || (*request.method() == Method::Post
                    && header_value(request.headers(), "X-Optimus-CSRF") != Some("1"))
            {
                return Err(403);
            }
        }
        Ok(())
    }
}

fn header_value<'a>(headers: &'a [Header], name: &'static str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

#[derive(Debug, Deserialize)]
struct InboundBody {
    text: String,
    #[serde(default = "default_channel")]
    channel: String,
    #[serde(default = "default_provider")]
    provider: String,
    #[serde(default)]
    session_id: Option<String>,
}

fn default_channel() -> String {
    "webhook".into()
}
fn default_provider() -> String {
    "offline".into()
}

/// Drain one queued message, using the same turn every other surface uses.
///
/// This endpoint used to run its own copy of the turn, and the copy had drifted:
/// it parsed `session_id` as a UUID (silently dropping `telegram:42`), answered
/// with the kernel's session id as the reply address, and knew only two
/// providers. There is one gateway turn now and it lives in `optimus-host`
/// (ADR-0071), so a webhook and a poller cannot disagree about what a message
/// means.
fn drain_once(home: &Path) -> Result<Option<serde_json::Value>, String> {
    let out = optimus_host::drain_gateway_once(&home.to_path_buf())?;
    Ok(out.map(|r| {
        json!({
            "id": r.id,
            "status": r.status,
            "reply_preview": r.reply_preview,
            "session_id": r.session_id,
        })
    }))
}

fn json_response(status: u16, value: serde_json::Value) -> Response<Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
    let len = body.len();
    Response::new(
        StatusCode(status),
        vec![
            Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap(),
        ],
        Cursor::new(body),
        Some(len),
        None,
    )
}

/// Run gateway HTTP until `max_requests` (0 = forever) or process killed.
pub fn run_gateway_http(
    home: PathBuf,
    port: u16,
    max_requests: u64,
    security: GatewaySecurity,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).map_err(|e| e.to_string())?;
    eprintln!("[optimus-gateway] http://{addr}/  home={}", home.display());
    let home = Arc::new(home);
    let handled = AtomicU64::new(0);
    let mut rate_limiter = WindowRateLimiter::new();

    for mut request in server.incoming_requests() {
        if !rate_limiter.allow(Instant::now()) {
            let _ = request.respond(json_response(
                429,
                json!({ "error": "rate limit exceeded" }),
            ));
            let n = handled.fetch_add(1, Ordering::SeqCst) + 1;
            if max_requests > 0 && n >= max_requests {
                break;
            }
            continue;
        }
        if let Err(status) = security.authorize(&request) {
            let error = if status == 401 {
                "unauthorized"
            } else {
                "forbidden"
            };
            let _ = request.respond(json_response(status, json!({ "error": error })));
            let n = handled.fetch_add(1, Ordering::SeqCst) + 1;
            if max_requests > 0 && n >= max_requests {
                break;
            }
            continue;
        }
        let method = request.method().clone();
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or(&url);

        let response = match (method, path) {
            (Method::Get, "/health") => json_response(
                200,
                json!({
                    "ok": true,
                    "service": "optimus-gateway",
                }),
            ),
            (Method::Get, "/inbox") => match list_inbox(home.as_path()) {
                Ok(rows) => json_response(
                    200,
                    json!({ "messages": rows.into_iter().take(100).collect::<Vec<_>>() }),
                ),
                Err(e) => {
                    eprintln!("[optimus-gateway] inbox failed: {e}");
                    json_response(500, json!({ "error": public_error(&e.to_string()) }))
                }
            },
            (Method::Get, "/outbox") => match list_outbox(home.as_path(), 50) {
                Ok(rows) => json_response(200, json!({ "messages": rows })),
                Err(e) => {
                    eprintln!("[optimus-gateway] outbox failed: {e}");
                    json_response(500, json!({ "error": public_error(&e.to_string()) }))
                }
            },
            (Method::Post, "/inbound") => match read_body_bounded(request.as_reader()) {
                Err(status) => json_response(
                    status,
                    json!({ "error": if status == 413 { "request body too large" } else { "invalid request body" } }),
                ),
                Ok(body) => match serde_json::from_str::<InboundBody>(&body) {
                    Ok(b) if !b.text.trim().is_empty() => {
                        match enqueue(
                            home.as_path(),
                            &b.channel,
                            &b.text,
                            &b.provider,
                            b.session_id.as_deref(),
                        ) {
                            Ok(m) => json_response(
                                200,
                                json!({
                                    "ok": true,
                                    "id": m.id,
                                    "channel": m.channel,
                                }),
                            ),
                            Err(e) => {
                                eprintln!("[optimus-gateway] enqueue failed: {e}");
                                json_response(500, json!({ "error": public_error(&e.to_string()) }))
                            }
                        }
                    }
                    Ok(_) => json_response(400, json!({ "error": "text required" })),
                    Err(_e) => json_response(400, json!({ "error": "invalid JSON" })),
                },
            },
            (Method::Post, "/drain") => match drain_once(home.as_path()) {
                Ok(None) => json_response(200, json!({ "ok": true, "drained": null })),
                Ok(Some(v)) => json_response(200, json!({ "ok": true, "drained": v })),
                Err(e) => {
                    eprintln!("[optimus-gateway] drain failed: {e}");
                    json_response(500, json!({ "error": public_error(&e) }))
                }
            },
            (Method::Post, "/drain_all") => {
                let mut drained = Vec::new();
                let mut err: Option<String> = None;
                while drained.len() < MAX_DRAIN_ALL_MESSAGES {
                    match drain_once(home.as_path()) {
                        Ok(None) => break,
                        Ok(Some(v)) => drained.push(v),
                        Err(e) => {
                            err = Some(e);
                            break;
                        }
                    }
                }
                if let Some(e) = err {
                    eprintln!("[optimus-gateway] drain_all failed: {e}");
                    json_response(
                        500,
                        json!({ "error": public_error(&e), "drained_count": drained.len() }),
                    )
                } else {
                    json_response(
                        200,
                        json!({
                            "ok": true,
                            "count": drained.len(),
                            "drained": drained,
                            "limit_reached": drained.len() == MAX_DRAIN_ALL_MESSAGES,
                        }),
                    )
                }
            }
            _ => json_response(
                404,
                json!({
                    "error": "not found",
                    "paths": ["/health", "/inbound", "/drain", "/drain_all", "/inbox", "/outbox"]
                }),
            ),
        };

        let _ = request.respond(response);
        let n = handled.fetch_add(1, Ordering::SeqCst) + 1;
        if max_requests > 0 && n >= max_requests {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        public_error, read_body_bounded, WindowRateLimiter, MAX_REQUEST_BODY_BYTES,
        REQUESTS_PER_MINUTE,
    };

    #[test]
    fn gateway_request_body_and_rate_are_bounded() {
        assert_eq!(read_body_bounded(&mut Cursor::new(b"ok")).unwrap(), "ok");
        assert!(
            read_body_bounded(&mut Cursor::new(vec![b'x'; MAX_REQUEST_BODY_BYTES + 1])).is_err()
        );

        let mut limiter = WindowRateLimiter::new();
        let now = std::time::Instant::now();
        for _ in 0..REQUESTS_PER_MINUTE {
            assert!(limiter.allow(now));
        }
        assert!(!limiter.allow(now));
    }

    #[test]
    fn gateway_public_errors_are_redacted() {
        assert_eq!(
            public_error("C:\\Users\\secret\\auth.json token=secret"),
            "request failed"
        );
    }
}
