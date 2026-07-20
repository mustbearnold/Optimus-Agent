//! HTTP server mode for Playwright / browser testing (incl. SSE chat stream).

use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::{
    mpsc::{self, SyncSender, TrySendError},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::ipc::{
    chat_turn, handle_ipc, stream_delivery_control, stream_event_to_json, IpcEnvelope, IpcReply,
};

const HTTP_STREAM_RESPONSE_WORKERS: usize = 2;
const HTTP_STREAM_RESPONSE_QUEUE_CAPACITY: usize = 8;
const HTTP_STREAM_PRODUCER_WORKERS: usize = 2;
const HTTP_STREAM_PRODUCER_QUEUE_CAPACITY: usize = 8;
const HTTP_STREAM_EVENT_CAPACITY: usize = 64;
const HTTP_MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
const HTTP_REQUESTS_PER_MINUTE: u32 = 600;

struct WindowRateLimiter {
    limit: u32,
    window: Duration,
    started: Instant,
    used: u32,
}

impl WindowRateLimiter {
    fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            started: Instant::now(),
            used: 0,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
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

fn read_body_bounded(reader: &mut dyn Read, max_bytes: usize) -> Result<String, u16> {
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    reader
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| 400u16)?;
    if bytes.len() > max_bytes {
        return Err(413);
    }
    String::from_utf8(bytes).map_err(|_| 400)
}

fn public_error(_internal: &str) -> &'static str {
    "request failed"
}

#[derive(Debug, Clone)]
pub struct HttpSecurity {
    token: String,
    allowed_origins: [String; 2],
}

impl HttpSecurity {
    pub fn new(
        port: u16,
        development_enabled: bool,
        token: impl Into<String>,
    ) -> Result<Self, String> {
        let token = token.into();
        if !development_enabled {
            return Err("HTTP mode requires --development-http".into());
        }
        if token.len() < 32 {
            return Err("OPTIMUS_HTTP_TOKEN must contain at least 32 characters".into());
        }
        Ok(Self {
            token,
            allowed_origins: [
                format!("http://127.0.0.1:{port}"),
                format!("http://localhost:{port}"),
            ],
        })
    }

    fn authorize(&self, method: &Method, headers: &[Header]) -> Result<(), u16> {
        let bearer = format!("Bearer {}", self.token);
        if header_value(headers, "Authorization") != Some(bearer.as_str()) {
            return Err(401);
        }
        if *method == Method::Post {
            let origin = header_value(headers, "Origin").ok_or(403u16)?;
            if !self.allowed_origins.iter().any(|allowed| allowed == origin)
                || header_value(headers, "X-Optimus-CSRF") != Some("1")
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

pub fn run_http_server(
    home: PathBuf,
    port: u16,
    html: String,
    security: HttpSecurity,
) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).map_err(|e| e.to_string())?;
    eprintln!("[optimus-desktop] HTTP UI+API on http://{addr}/  (Playwright target)");
    eprintln!("[optimus-desktop] home={}", home.display());

    // Force HTTP IPC mode even if bridge heuristics change.
    let token_json = serde_json::to_string(&security.token).map_err(|e| e.to_string())?;
    let html = html.replace(
        "window.__optimusBridgeInstalled = true;",
        &format!(
            "window.__optimusBridgeInstalled = true; window.__OPTIMUS_HTTP_MODE__ = true; window.__OPTIMUS_HTTP_TOKEN__ = {token_json};"
        ),
    );

    let home = Arc::new(home);
    let html = Arc::new(html);
    let stream_pool = HttpStreamPool::start(Arc::clone(&home)).map_err(|e| e.to_string())?;
    let mut rate_limiter =
        WindowRateLimiter::new(HTTP_REQUESTS_PER_MINUTE, Duration::from_secs(60));

    for request in server.incoming_requests() {
        if !rate_limiter.allow(Instant::now()) {
            let _ = request.respond(json_response(
                429,
                &serde_json::json!({"ok": false, "error": "rate limit exceeded"}),
            ));
            continue;
        }
        let home = Arc::clone(&home);
        let html = Arc::clone(&html);
        let method = request.method().clone();
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or(&url).to_string();

        if path.starts_with("/api/") {
            if let Err(status) = security.authorize(&method, request.headers()) {
                let error = if status == 401 {
                    "unauthorized"
                } else {
                    "forbidden"
                };
                let _ = request.respond(json_response(
                    status,
                    &serde_json::json!({"ok": false, "error": error}),
                ));
                continue;
            }
        }

        match (method, path.as_str()) {
            (Method::Get, "/") | (Method::Get, "/index.html") => {
                let mut resp =
                    Response::from_string(html.as_str()).with_status_code(StatusCode(200));
                if let Ok(h) =
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                {
                    resp.add_header(h);
                }
                add_cors(&mut resp);
                let _ = request.respond(resp);
            }
            (Method::Get, "/api/health") => {
                let body = serde_json::json!({
                    "ok": true,
                    "streaming": true,
                });
                let _ = request.respond(json_response(200, &body));
            }
            (Method::Post, "/api/ipc") => {
                let mut req = request;
                let body = match read_body_bounded(req.as_reader(), HTTP_MAX_REQUEST_BODY_BYTES) {
                    Ok(body) => body,
                    Err(status) => {
                        let _ = req.respond(json_response(
                            status,
                            &serde_json::json!({"ok": false, "error": if status == 413 { "request body too large" } else { "invalid request body" }}),
                        ));
                        continue;
                    }
                };
                let reply = match serde_json::from_str::<IpcEnvelope>(&body) {
                    Ok(env) => match handle_ipc(&home, &env.method, env.params) {
                        Ok(result) => IpcReply {
                            id: env.id,
                            ok: true,
                            result: Some(result),
                            error: None,
                        },
                        Err(e) => {
                            eprintln!("[optimus-desktop] HTTP IPC failed: {e}");
                            IpcReply {
                                id: env.id,
                                ok: false,
                                result: None,
                                error: Some(public_error(&e).into()),
                            }
                        }
                    },
                    Err(_e) => IpcReply {
                        id: 0,
                        ok: false,
                        result: None,
                        error: Some("invalid request envelope".into()),
                    },
                };
                let payload = serde_json::to_value(&reply).unwrap_or_default();
                let _ = req.respond(json_response(200, &payload));
            }
            (Method::Post, "/api/chat/stream") => {
                let mut req = request;
                let body = match read_body_bounded(req.as_reader(), HTTP_MAX_REQUEST_BODY_BYTES) {
                    Ok(body) => body,
                    Err(status) => {
                        let _ = req.respond(json_response(
                            status,
                            &serde_json::json!({"ok": false, "error": if status == 413 { "request body too large" } else { "invalid request body" }}),
                        ));
                        continue;
                    }
                };
                let params: serde_json::Value = match serde_json::from_str(&body) {
                    Ok(v) => v,
                    Err(_e) => {
                        let _ = req.respond(json_response(
                            400,
                            &serde_json::json!({"ok": false, "error": "invalid JSON"}),
                        ));
                        continue;
                    }
                };
                if let Err((error, req)) = stream_pool.enqueue(req, params) {
                    respond_sse_error(*req, format!("HTTP chat worker {error}"));
                }
            }
            (Method::Options, _) => {
                let mut resp = Response::from_data(Vec::new()).with_status_code(StatusCode(204));
                add_cors(&mut resp);
                let _ = request.respond(resp);
            }
            _ => {
                let mut resp = Response::from_string("not found").with_status_code(StatusCode(404));
                add_cors(&mut resp);
                let _ = request.respond(resp);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamEnqueueError {
    Full,
    Disconnected,
}

impl std::fmt::Display for StreamEnqueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => f.write_str("queue full"),
            Self::Disconnected => f.write_str("worker unavailable"),
        }
    }
}

struct HttpStreamWork {
    params: serde_json::Value,
    tx: SyncSender<Vec<u8>>,
}

enum HttpResponseWork {
    Stream {
        request: Request,
        params: serde_json::Value,
    },
    #[cfg(test)]
    Test(Box<dyn FnOnce() + Send>),
}

struct HttpStreamPool {
    response_tx: SyncSender<HttpResponseWork>,
}

impl HttpStreamPool {
    fn start(home: Arc<PathBuf>) -> std::io::Result<Self> {
        let (producer_tx, producer_rx) = mpsc::sync_channel(HTTP_STREAM_PRODUCER_QUEUE_CAPACITY);
        let producer_rx = Arc::new(Mutex::new(producer_rx));
        for index in 0..HTTP_STREAM_PRODUCER_WORKERS {
            let worker_home = Arc::clone(&home);
            let worker_rx = Arc::clone(&producer_rx);
            thread::Builder::new()
                .name(format!("optimus-http-producer-{index}"))
                .spawn(move || run_producer_worker(worker_home, worker_rx))?;
        }

        let (response_tx, response_rx) = mpsc::sync_channel(HTTP_STREAM_RESPONSE_QUEUE_CAPACITY);
        let response_rx = Arc::new(Mutex::new(response_rx));
        for index in 0..HTTP_STREAM_RESPONSE_WORKERS {
            let worker_rx = Arc::clone(&response_rx);
            let worker_producer_tx = producer_tx.clone();
            thread::Builder::new()
                .name(format!("optimus-http-response-{index}"))
                .spawn(move || run_response_worker(worker_rx, worker_producer_tx))?;
        }
        Ok(Self { response_tx })
    }

    fn enqueue(
        &self,
        request: Request,
        params: serde_json::Value,
    ) -> Result<(), (StreamEnqueueError, Box<Request>)> {
        match self
            .response_tx
            .try_send(HttpResponseWork::Stream { request, params })
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(HttpResponseWork::Stream { request, .. })) => {
                Err((StreamEnqueueError::Full, Box::new(request)))
            }
            Err(TrySendError::Disconnected(HttpResponseWork::Stream { request, .. })) => {
                Err((StreamEnqueueError::Disconnected, Box::new(request)))
            }
            #[cfg(test)]
            Err(TrySendError::Full(HttpResponseWork::Test(_)))
            | Err(TrySendError::Disconnected(HttpResponseWork::Test(_))) => unreachable!(),
        }
    }

    #[cfg(test)]
    fn enqueue_test(&self, job: impl FnOnce() + Send + 'static) -> Result<(), StreamEnqueueError> {
        match self
            .response_tx
            .try_send(HttpResponseWork::Test(Box::new(job)))
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(StreamEnqueueError::Full),
            Err(TrySendError::Disconnected(_)) => Err(StreamEnqueueError::Disconnected),
        }
    }
}

fn run_response_worker(
    rx: Arc<Mutex<mpsc::Receiver<HttpResponseWork>>>,
    producer_tx: SyncSender<HttpStreamWork>,
) {
    loop {
        let work = {
            let receiver = rx.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            receiver.recv()
        };
        let Ok(work) = work else {
            break;
        };
        match work {
            HttpResponseWork::Stream { request, params } => {
                respond_stream(request, params, &producer_tx);
            }
            #[cfg(test)]
            HttpResponseWork::Test(job) => job(),
        }
    }
}

fn respond_stream(
    request: Request,
    params: serde_json::Value,
    producer_tx: &SyncSender<HttpStreamWork>,
) {
    let (tx, rx) = mpsc::sync_channel(HTTP_STREAM_EVENT_CAPACITY);
    let admission_tx = tx.clone();
    let admission = match producer_tx.try_send(HttpStreamWork { params, tx }) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(StreamEnqueueError::Full),
        Err(TrySendError::Disconnected(_)) => Err(StreamEnqueueError::Disconnected),
    };
    if let Err(error) = admission {
        let _ = try_send_sse_json(
            &admission_tx,
            serde_json::json!({"type":"error","error": format!("HTTP chat producer {error}")}),
        );
    }
    drop(admission_tx);
    let reader = SseReader {
        rx,
        buf: Vec::new(),
    };
    let _ = request.respond(sse_response(reader));
}

fn run_producer_worker(home: Arc<PathBuf>, rx: Arc<Mutex<mpsc::Receiver<HttpStreamWork>>>) {
    loop {
        let work = {
            let receiver = rx.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            receiver.recv()
        };
        let Ok(HttpStreamWork { params, tx }) = work else {
            break;
        };
        let mut accepting = true;
        let mut on_event = |event| {
            let delivered =
                accepting && try_send_sse_json(&tx, stream_event_to_json(&event)).is_ok();
            accepting = delivered;
            stream_delivery_control(delivered)
        };
        let terminal = match chat_turn(&home, params, Some(&mut on_event)) {
            Ok(result) => serde_json::json!({"type":"done","result": result}),
            Err(error) => {
                eprintln!("[optimus-desktop] HTTP chat failed: {error}");
                serde_json::json!({"type":"error","error": public_error(&error)})
            }
        };
        if accepting {
            let _ = try_send_sse_json(&tx, terminal);
        }
    }
}

fn try_send_sse_json(
    tx: &SyncSender<Vec<u8>>,
    value: serde_json::Value,
) -> Result<(), StreamEnqueueError> {
    let line = format!("data: {value}\n\n");
    match tx.try_send(line.into_bytes()) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(StreamEnqueueError::Full),
        Err(TrySendError::Disconnected(_)) => Err(StreamEnqueueError::Disconnected),
    }
}

fn respond_sse_error(request: Request, error: String) {
    let bytes = format!(
        "data: {}\n\n",
        serde_json::json!({"type":"error","error": error})
    )
    .into_bytes();
    let _ = request.respond(sse_response(Cursor::new(bytes)));
}

fn sse_response<R: Read + Send + 'static>(reader: R) -> Response<R> {
    let mut response = Response::new(StatusCode(200), vec![], reader, None, None);
    if let Ok(header) = Header::from_bytes(
        &b"Content-Type"[..],
        &b"text/event-stream; charset=utf-8"[..],
    ) {
        response.add_header(header);
    }
    if let Ok(header) = Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]) {
        response.add_header(header);
    }
    add_cors(&mut response);
    response
}

struct SseReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
}

impl Read for SseReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.buf.is_empty() {
            match self.rx.recv_timeout(Duration::from_secs(300)) {
                Ok(chunk) => self.buf = chunk,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // heartbeat comment
                    self.buf = b": ping\n\n".to_vec();
                }
            }
        }
        let n = out.len().min(self.buf.len());
        out[..n].copy_from_slice(&self.buf[..n]);
        self.buf.drain(..n);
        Ok(n)
    }
}

fn json_response(status: u16, value: &serde_json::Value) -> Response<Cursor<Vec<u8>>> {
    let bytes = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    let mut resp = Response::from_data(bytes).with_status_code(StatusCode(status));
    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        resp.add_header(h);
    }
    add_cors(&mut resp);
    resp
}

fn add_cors<R: std::io::Read>(_resp: &mut Response<R>) {
    // Deliberately empty: development HTTP is same-origin only.
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::{mpsc, Arc, Barrier};
    use std::time::Duration;

    use super::{
        public_error, read_body_bounded, try_send_sse_json, HttpSecurity, HttpStreamPool,
        StreamEnqueueError, WindowRateLimiter, HTTP_STREAM_EVENT_CAPACITY,
        HTTP_STREAM_PRODUCER_QUEUE_CAPACITY, HTTP_STREAM_PRODUCER_WORKERS,
        HTTP_STREAM_RESPONSE_QUEUE_CAPACITY, HTTP_STREAM_RESPONSE_WORKERS,
    };
    use crate::ipc::stream_delivery_control;
    use optimus_kernel::StreamControl;
    use tiny_http::{Header, Method};

    fn header(name: &str, value: &str) -> Header {
        Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
    }

    #[test]
    fn development_http_requires_explicit_mode_and_strong_token() {
        assert!(HttpSecurity::new(8787, false, "a".repeat(32)).is_err());
        assert!(HttpSecurity::new(8787, true, "short").is_err());
        assert!(HttpSecurity::new(8787, true, "a".repeat(32)).is_ok());
    }

    #[test]
    fn api_auth_requires_bearer_exact_origin_and_csrf_for_posts() {
        let security = HttpSecurity::new(8787, true, "a".repeat(32)).unwrap();
        let auth = header("Authorization", &format!("Bearer {}", "a".repeat(32)));
        assert_eq!(security.authorize(&Method::Get, &[]), Err(401));
        assert_eq!(
            security.authorize(&Method::Get, std::slice::from_ref(&auth)),
            Ok(())
        );

        let valid = vec![
            auth.clone(),
            header("Origin", "http://127.0.0.1:8787"),
            header("X-Optimus-CSRF", "1"),
        ];
        assert_eq!(security.authorize(&Method::Post, &valid), Ok(()));
        let foreign = vec![
            auth.clone(),
            header("Origin", "https://evil.example"),
            header("X-Optimus-CSRF", "1"),
        ];
        assert_eq!(security.authorize(&Method::Post, &foreign), Err(403));
        assert_eq!(
            security.authorize(
                &Method::Post,
                &[auth, header("Origin", "http://localhost:8787"),],
            ),
            Err(403)
        );
    }

    #[test]
    fn request_body_and_rate_windows_are_strictly_bounded() {
        assert_eq!(
            read_body_bounded(&mut Cursor::new(b"1234"), 4).unwrap(),
            "1234"
        );
        assert!(read_body_bounded(&mut Cursor::new(b"12345"), 4).is_err());

        let mut limiter = WindowRateLimiter::new(2, Duration::from_secs(60));
        let start = std::time::Instant::now();
        assert!(limiter.allow(start));
        assert!(limiter.allow(start));
        assert!(!limiter.allow(start));
        assert!(limiter.allow(start + Duration::from_secs(60)));
    }

    #[test]
    fn public_errors_never_echo_internal_details() {
        let internal = "C:\\Users\\secret\\auth.json bearer-super-secret";
        let public = public_error(internal);
        assert_eq!(public, "request failed");
        assert!(!public.contains("secret"));
        assert!(!public.contains("auth.json"));
    }

    #[test]
    fn http_stream_workers_and_queue_are_bounded() {
        assert_eq!(HTTP_STREAM_RESPONSE_WORKERS, 2);
        assert_eq!(HTTP_STREAM_RESPONSE_QUEUE_CAPACITY, 8);
        assert_eq!(HTTP_STREAM_PRODUCER_WORKERS, 2);
        assert_eq!(HTTP_STREAM_PRODUCER_QUEUE_CAPACITY, 8);
        assert_eq!(HTTP_STREAM_EVENT_CAPACITY, 64);
    }

    #[test]
    fn response_pool_runs_concurrently_and_rejects_overload() {
        let pool = HttpStreamPool::start(Arc::new(PathBuf::from("unused"))).expect("pool");
        let barrier = Arc::new(Barrier::new(3));
        let (started_tx, started_rx) = mpsc::channel();
        for _ in 0..HTTP_STREAM_RESPONSE_WORKERS {
            let worker_barrier = Arc::clone(&barrier);
            let worker_started = started_tx.clone();
            pool.enqueue_test(move || {
                worker_started.send(()).expect("started");
                worker_barrier.wait();
            })
            .expect("blocking job");
        }
        for _ in 0..HTTP_STREAM_RESPONSE_WORKERS {
            started_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("distinct response worker started");
        }
        for _ in 0..HTTP_STREAM_RESPONSE_QUEUE_CAPACITY {
            pool.enqueue_test(|| {}).expect("queued job");
        }
        assert_eq!(pool.enqueue_test(|| {}), Err(StreamEnqueueError::Full));
        barrier.wait();
    }

    #[test]
    fn stream_event_delivery_cancels_instead_of_blocking() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let delivered = try_send_sse_json(&tx, serde_json::json!({"type":"delta"}));
        assert_eq!(
            stream_delivery_control(delivered.is_ok()),
            StreamControl::Continue
        );
        let full = try_send_sse_json(&tx, serde_json::json!({"type":"delta"}));
        assert_eq!(full, Err(StreamEnqueueError::Full));
        assert_eq!(stream_delivery_control(full.is_ok()), StreamControl::Cancel);
        drop(_rx);
        let disconnected = try_send_sse_json(&tx, serde_json::json!({"type":"delta"}));
        assert_eq!(disconnected, Err(StreamEnqueueError::Disconnected));
        assert_eq!(
            stream_delivery_control(disconnected.is_ok()),
            StreamControl::Cancel
        );
    }
}
