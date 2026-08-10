//! Discord adapter (spec-017): Gateway websocket inbound + REST outbound.
//! Implemented by the spec-017 workstream; see tests/discord_conformance.rs.
//!
//! Inbound rides the Gateway v10 websocket (`wss://gateway.discord.gg/`): the
//! adapter identifies once, heartbeats on the interval the gateway announces,
//! resumes or re-identifies after disconnects, and surfaces `MESSAGE_CREATE`
//! dispatches as [`RawInbound`] keyed by channel id. Outbound posts channel
//! messages over the v10 REST API with `Authorization: Bot <token>`.
//!
//! The bot token lives in the environment variable named by the config; it is
//! never stored in `gateway/discord.json`. Channel authorization is
//! fail-closed: an enabled adapter with an empty allowlist refuses to poll
//! rather than accept anything.

use std::io;
use std::net::TcpStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect as ws_connect, Message, WebSocket};

use crate::transport::{RawInbound, SendOutcome, TransportAdapter, TransportId};

/// Gateway v10 JSON endpoint; `encoding=json` is the only supported wire
/// format (zlib-stream would need a decompression layer on every frame).
const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
/// REST v10 base for channel message delivery.
const API_BASE: &str = "https://discord.com/api/v10";
/// Environment variable read for the bot token. The config stores this name,
/// never the credential itself.
const DEFAULT_TOKEN_ENV: &str = "OPTIMUS_DISCORD_BOT_TOKEN";
/// GUILD_MESSAGES (1 << 9) | DIRECT_MESSAGES (1 << 12): the minimal intent set
/// that delivers server and DM message-create events.
const DEFAULT_INTENTS: u64 = (1 << 9) | (1 << 12);
/// Websocket silence window; a poll returns at least this often, which also
/// bounds how long the heartbeat thread can wait for the socket lock.
const DEFAULT_POLL_HOLD_SECS: u64 = 25;
/// Heartbeat interval Discord announces when Hello omits it (never in
/// practice; kept as a sane fallback).
const DEFAULT_HEARTBEAT_INTERVAL_MS: u64 = 41_250;
/// Cap on the exponential reconnect backoff.
const MAX_RECONNECT_BACKOFF_SECS: u64 = 30;
/// REST timeout: long enough for Discord to answer, short enough that a hung
/// delivery settles as ambiguous instead of blocking the loop forever.
const SEND_TIMEOUT_SECS: u64 = 30;

/// Gateway opcodes (Gateway v10).
const OP_DISPATCH: u64 = 0;
const OP_HEARTBEAT: u64 = 1;
const OP_IDENTIFY: u64 = 2;
const OP_RESUME: u64 = 6;
const OP_RECONNECT: u64 = 7;
const OP_INVALID_SESSION: u64 = 9;
const OP_HELLO: u64 = 10;
const OP_HEARTBEAT_ACK: u64 = 11;

/// The blocking websocket this transport reads and heartbeats.
type GatewaySocket = WebSocket<MaybeTlsStream<TcpStream>>;

/// Discord adapter configuration, read from `{home}/gateway/discord.json`.
///
/// All keys are snake_case on the wire and every field has a default, so a
/// partial config file still parses (unknown keys are ignored by serde).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Master switch; a disabled adapter polls nothing and sends nothing.
    #[serde(default)]
    pub enabled: bool,
    /// Name of the environment variable holding the bot token — never the
    /// token itself.
    #[serde(default = "default_token_env")]
    pub bot_token_env: String,
    /// Inbound allowlist of channel ids; empty + enabled fails closed.
    #[serde(default)]
    pub allowed_channel_ids: Vec<String>,
    /// Websocket silence window (seconds) before a poll returns empty.
    #[serde(default = "default_poll_hold_secs")]
    pub poll_hold_secs: u64,
    /// Gateway intents bitmask.
    #[serde(default = "default_intents")]
    pub intents: u64,
}

fn default_token_env() -> String {
    DEFAULT_TOKEN_ENV.to_string()
}

fn default_poll_hold_secs() -> u64 {
    DEFAULT_POLL_HOLD_SECS
}

fn default_intents() -> u64 {
    DEFAULT_INTENTS
}

/// One inbound message in transport-native shape, before authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordMessage {
    pub channel_id: String,
    pub text: String,
    /// Bot-authored messages are dropped by the gateway contract.
    pub is_bot: bool,
}

/// The socket-facing half of the Discord adapter.
///
/// A mock implementation drives the adapter in conformance tests; the live
/// implementation speaks the Gateway protocol. The adapter only sees
/// transport-shaped messages and send outcomes, never wire format.
pub trait DiscordTransport: Send {
    /// Drain whatever inbound the transport has buffered since the last poll.
    fn poll_messages(&mut self) -> Result<Vec<DiscordMessage>, String>;

    /// Deliver one outbound message to a channel.
    fn send_message(&mut self, channel_id: &str, body: &str) -> Result<SendOutcome, String>;

    /// Best-effort health; a mock is always healthy.
    fn health(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Scripted Discord transport for conformance tests.
///
/// `pending` is drained by `poll_messages`; every send is appended to the
/// shared `sent` log (an `Arc` so a test can keep a handle after the mock is
/// moved into an adapter). Bot messages are recorded, not filtered, so the
/// test can prove the adapter skips them.
#[derive(Debug)]
pub struct MockDiscordTransport {
    pending: Vec<DiscordMessage>,
    pub sent: Arc<Mutex<Vec<(String, String)>>>,
    next_send_failed: bool,
    next_send_ambiguous: bool,
}

impl MockDiscordTransport {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            sent: Arc::new(Mutex::new(Vec::new())),
            next_send_failed: false,
            next_send_ambiguous: false,
        }
    }

    /// Queue one inbound message for the next poll.
    pub fn push(&mut self, channel_id: &str, text: &str, is_bot: bool) {
        self.pending.push(DiscordMessage {
            channel_id: channel_id.to_string(),
            text: text.to_string(),
            is_bot,
        });
    }

    /// Snapshot of every `(channel_id, body)` this mock has been asked to send.
    pub fn sent(&self) -> Vec<(String, String)> {
        self.sent.lock().expect("mock send log").clone()
    }

    /// Script the next send as a definite platform refusal.
    pub fn fail_next_send(&mut self) {
        self.next_send_failed = true;
    }

    /// Script the next send as network-level uncertainty.
    pub fn ambiguous_next_send(&mut self) {
        self.next_send_ambiguous = true;
    }
}

impl Default for MockDiscordTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordTransport for MockDiscordTransport {
    fn poll_messages(&mut self) -> Result<Vec<DiscordMessage>, String> {
        Ok(std::mem::take(&mut self.pending))
    }

    fn send_message(&mut self, channel_id: &str, body: &str) -> Result<SendOutcome, String> {
        if self.next_send_failed {
            self.next_send_failed = false;
            return Ok(SendOutcome::Failed {
                detail: "mock: platform refused".to_string(),
            });
        }
        if self.next_send_ambiguous {
            self.next_send_ambiguous = false;
            return Ok(SendOutcome::Ambiguous {
                detail: "mock: network timeout".to_string(),
            });
        }
        let mut log = self.sent.lock().expect("mock send log");
        log.push((channel_id.to_string(), body.to_string()));
        Ok(SendOutcome::Confirmed {
            provider_message_id: format!("mock-{}", log.len()),
        })
    }
}

/// spec-017 Discord adapter: config plus a transport, satisfying
/// [`TransportAdapter`].
pub struct DiscordAdapter {
    config: DiscordConfig,
    transport: Box<dyn DiscordTransport>,
}

impl DiscordAdapter {
    /// Build the adapter for a config: live when enabled, mock when disabled
    /// (a disabled adapter must never touch the network).
    pub fn from_config(config: DiscordConfig) -> Box<dyn TransportAdapter> {
        let transport: Box<dyn DiscordTransport> = if config.enabled {
            Box::new(LiveDiscordTransport::new(
                &config.bot_token_env,
                config.poll_hold_secs,
                config.intents,
            ))
        } else {
            Box::new(MockDiscordTransport::new())
        };
        Box::new(DiscordAdapter { config, transport })
    }

    /// Build the adapter around an explicit transport (test seam).
    pub fn with_transport(config: DiscordConfig, transport: Box<dyn DiscordTransport>) -> Self {
        DiscordAdapter { config, transport }
    }
}

/// spec-017 convention: build the Discord adapter for `home`, or `None` when
/// `{home}/gateway/discord.json` does not exist. A malformed file is an
/// error; a disabled file yields an adapter whose `is_enabled` is false.
pub fn open_adapter(home: &Path) -> Result<Option<Box<dyn TransportAdapter>>, String> {
    let path = home.join("gateway").join("discord.json");
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let config: DiscordConfig = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    Ok(Some(DiscordAdapter::from_config(config)))
}

impl TransportAdapter for DiscordAdapter {
    fn transport(&self) -> TransportId {
        TransportId::Discord
    }

    fn is_enabled(&self, _home: &Path) -> bool {
        self.config.enabled
    }

    fn poll_inbound(&mut self, _home: &Path) -> Result<Vec<RawInbound>, String> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }
        if self.config.allowed_channel_ids.is_empty() {
            return Err(
                "live discord requires non-empty allowed_channel_ids (fail closed)".to_string(),
            );
        }
        let messages = self.transport.poll_messages()?;
        let inbound = messages
            .into_iter()
            .filter(|message| {
                !message.is_bot && !message.text.trim().is_empty() && !message.channel_id.is_empty()
            })
            .map(|message| RawInbound {
                from: message.channel_id,
                text: message.text,
                attachments: Vec::new(),
            })
            .collect();
        Ok(inbound)
    }

    fn is_allowed(&self, from: &str) -> bool {
        self.config.allowed_channel_ids.is_empty()
            || self.config.allowed_channel_ids.iter().any(|id| id == from)
    }

    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        self.transport.send_message(target, body)
    }

    fn health(&self) -> Result<(), String> {
        self.transport.health()
    }
}

/// Gateway protocol client: one blocking websocket plus a heartbeat thread.
///
/// The socket is shared with the heartbeat thread through a mutex. Reads hold
/// the lock for at most one `poll_hold_secs` window, and Discord's heartbeat
/// interval is longer than that window, so the heartbeat always gets through.
pub struct LiveDiscordTransport {
    /// Name of the environment variable holding the bot token. Read per
    /// connection and per send, so a rotated credential applies immediately.
    token_env: String,
    api_base: String,
    poll_hold_secs: u64,
    intents: u64,
    socket: Option<Arc<Mutex<GatewaySocket>>>,
    /// Last dispatch sequence (`s`), kept across reconnects for RESUME.
    seq: Arc<AtomicU64>,
    /// Session id captured from READY, kept across reconnects for RESUME.
    session_id: Option<String>,
    /// Heartbeat interval announced by Hello; 0 until the first Hello.
    heartbeat_interval_ms: Arc<AtomicU64>,
    /// Raised to stop the current heartbeat thread on teardown.
    heartbeat_stop: Arc<AtomicBool>,
    reconnect_attempts: u32,
    last_poll_ok: bool,
    rest: ureq::Agent,
}

impl LiveDiscordTransport {
    /// Build a transport against the real gateway and REST API.
    pub fn new(token_env: &str, poll_hold_secs: u64, intents: u64) -> Self {
        Self::with_api_base(token_env, poll_hold_secs, intents, API_BASE)
    }

    /// Build a transport against `api_base` instead of Discord's REST API.
    ///
    /// Test seam: lets a loopback server stand in for the REST endpoint so the
    /// outbound request shape is asserted against a real socket. The gateway
    /// websocket is always the real one (tests never open it).
    pub fn with_api_base(
        token_env: &str,
        poll_hold_secs: u64,
        intents: u64,
        api_base: &str,
    ) -> Self {
        Self {
            token_env: token_env.to_string(),
            api_base: api_base.trim_end_matches('/').to_string(),
            poll_hold_secs: poll_hold_secs.max(1),
            intents,
            socket: None,
            seq: Arc::new(AtomicU64::new(0)),
            session_id: None,
            heartbeat_interval_ms: Arc::new(AtomicU64::new(0)),
            heartbeat_stop: Arc::new(AtomicBool::new(false)),
            reconnect_attempts: 0,
            last_poll_ok: false,
            rest: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(SEND_TIMEOUT_SECS))
                .build(),
        }
    }

    /// Open the gateway socket and complete the Identify/Resume handshake.
    fn connect(&mut self) -> Result<(), String> {
        let token = read_token(&self.token_env)?;
        let (mut ws, _) = ws_connect(GATEWAY_URL).map_err(|error| error.to_string())?;
        set_socket_timeout(&mut ws, self.poll_hold_secs);

        let socket = Arc::new(Mutex::new(ws));
        let stop = Arc::new(AtomicBool::new(false));
        self.heartbeat_stop = stop.clone();
        spawn_heartbeat(
            Arc::clone(&socket),
            Arc::clone(&self.heartbeat_interval_ms),
            Arc::clone(&self.seq),
            stop,
        );

        // Hello must arrive before anything else; it announces the interval.
        let interval_ms = {
            let mut ws = socket.lock().expect("gateway socket lock");
            loop {
                match ws.read() {
                    Ok(Message::Text(text)) => {
                        let value: Value = serde_json::from_str(text.as_str())
                            .map_err(|error| format!("gateway Hello frame: {error}"))?;
                        if value["op"].as_u64() == Some(OP_HELLO) {
                            break value["d"]["heartbeat_interval"]
                                .as_u64()
                                .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL_MS);
                        }
                        // Anything before Hello is a protocol anomaly.
                        return Err(format!(
                            "gateway: expected Hello, got op {}",
                            value["op"].as_u64().unwrap_or(0)
                        ));
                    }
                    Ok(_) => continue, // ping/pong/binary before Hello: ignore
                    Err(error) => return Err(format!("gateway Hello: {error}")),
                }
            }
        };
        self.heartbeat_interval_ms
            .store(interval_ms, Ordering::Relaxed);

        // Identify, or Resume when a session survives a reconnect.
        let seq = self.seq.load(Ordering::Relaxed);
        let payload = match self.session_id.clone() {
            Some(session_id) if seq > 0 => json!({
                "op": OP_RESUME,
                "d": { "token": token, "session_id": session_id, "seq": seq }
            }),
            _ => json!({
                "op": OP_IDENTIFY,
                "d": {
                    "token": token,
                    "intents": self.intents,
                    "properties": {
                        "os": "linux",
                        "browser": "optimus-agent",
                        "device": "optimus-agent"
                    }
                }
            }),
        };
        {
            let mut ws = socket.lock().expect("gateway socket lock");
            ws.send(Message::text(payload.to_string()))
                .map_err(|error| format!("gateway identify: {error}"))?;
            ws.flush()
                .map_err(|error| format!("gateway identify flush: {error}"))?;
        }

        self.socket = Some(socket);
        Ok(())
    }

    /// Drop the socket and stop its heartbeat thread. Sequence and session
    /// survive, so the next connect can RESUME.
    fn teardown(&mut self) {
        self.heartbeat_stop.store(true, Ordering::Relaxed);
        if let Some(socket) = self.socket.take() {
            if let Ok(mut ws) = socket.lock() {
                let _ = ws.close(None);
                let _ = ws.flush();
            }
        }
    }

    fn poll_messages(&mut self) -> Result<Vec<DiscordMessage>, String> {
        if self.socket.is_none() {
            match self.connect() {
                Ok(()) => self.reconnect_attempts = 0,
                Err(_error) => {
                    self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
                    self.last_poll_ok = false;
                    // Keep the cycle moving; the next poll retries with backoff.
                    std::thread::sleep(reconnect_backoff(self.reconnect_attempts));
                    return Ok(Vec::new());
                }
            }
        }
        let socket = Arc::clone(
            self.socket
                .as_ref()
                .expect("socket connected after connect"),
        );
        let deadline = Instant::now() + Duration::from_secs(self.poll_hold_secs);
        let mut batch = Vec::new();
        loop {
            if Instant::now() >= deadline {
                break;
            }
            let message = socket.lock().expect("gateway socket lock").read();
            match message {
                Ok(Message::Text(text)) => match self.handle_frame(text.as_str()) {
                    FrameEffect::Messages(mut messages) => batch.append(&mut messages),
                    FrameEffect::Reconnect => {
                        self.teardown();
                        break;
                    }
                    FrameEffect::None => {}
                },
                Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => {
                    self.teardown();
                    break;
                }
                Err(tungstenite::Error::Protocol(_)) => {
                    self.teardown();
                    break;
                }
                // Read timeout / would-block: the silence window is over.
                Err(tungstenite::Error::Io(error)) if is_silence(&error) => break,
                Err(tungstenite::Error::Io(_)) => {
                    self.teardown();
                    break;
                }
                Err(_) => break,
                Ok(_) => {} // Ping/Pong/Binary/Frame: nothing to surface
            }
        }
        self.last_poll_ok = true;
        Ok(batch)
    }

    /// Classify one gateway frame and update transport state.
    fn handle_frame(&mut self, text: &str) -> FrameEffect {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            // An undecodable frame means the socket state is suspect.
            Err(_) => return FrameEffect::Reconnect,
        };
        match value["op"].as_u64().unwrap_or(0) {
            OP_DISPATCH => {
                if let Some(seq) = value["s"].as_u64() {
                    self.seq.store(seq, Ordering::Relaxed);
                }
                match value["t"].as_str() {
                    Some("READY") => {
                        self.session_id = value["d"]["session_id"]
                            .as_str()
                            .map(|session| session.to_string());
                        FrameEffect::None
                    }
                    Some("MESSAGE_CREATE") => {
                        let event = &value["d"];
                        let is_bot = event["author"]["bot"].as_bool().unwrap_or(false);
                        let text = event["content"].as_str().unwrap_or("");
                        let channel_id = event["channel_id"].as_str().unwrap_or("");
                        if is_bot || text.trim().is_empty() || channel_id.is_empty() {
                            FrameEffect::None
                        } else {
                            FrameEffect::Messages(vec![DiscordMessage {
                                channel_id: channel_id.to_string(),
                                text: text.to_string(),
                                is_bot: false,
                            }])
                        }
                    }
                    _ => FrameEffect::None,
                }
            }
            OP_HEARTBEAT_ACK => FrameEffect::None,
            OP_RECONNECT => FrameEffect::Reconnect,
            OP_INVALID_SESSION => {
                // `d` false means the session cannot be resumed: start fresh.
                if !value["d"].as_bool().unwrap_or(false) {
                    self.session_id = None;
                }
                FrameEffect::Reconnect
            }
            OP_HELLO => {
                // Re-Hello on an established socket: refresh the interval.
                if let Some(interval) = value["d"]["heartbeat_interval"].as_u64() {
                    self.heartbeat_interval_ms
                        .store(interval, Ordering::Relaxed);
                }
                FrameEffect::None
            }
            _ => FrameEffect::None,
        }
    }

    fn send_message(&mut self, channel_id: &str, body: &str) -> Result<SendOutcome, String> {
        let token = match read_token(&self.token_env) {
            Ok(token) => token,
            Err(error) => {
                // The platform never saw the message: a definite refusal.
                return Ok(SendOutcome::Failed { detail: error });
            }
        };
        let url = format!("{}/channels/{}/messages", self.api_base, channel_id);
        let response = self
            .rest
            .post(&url)
            .set("Authorization", &format!("Bot {token}"))
            .set("Content-Type", "application/json")
            .send_json(json!({ "content": body }));
        match response {
            Ok(response) => {
                // 200: Discord accepted the message and assigned an id.
                let provider_message_id = response
                    .into_json::<Value>()
                    .ok()
                    .and_then(|value| value.get("id").and_then(Value::as_str).map(str::to_string))
                    .unwrap_or_else(|| "unknown".to_string());
                Ok(SendOutcome::Confirmed {
                    provider_message_id,
                })
            }
            Err(ureq::Error::Status(code, response)) => {
                let detail = rest_error_detail(code, response);
                if (400..500).contains(&code) {
                    // Definite refusal: bad token, unknown channel, permissions.
                    Ok(SendOutcome::Failed { detail })
                } else {
                    // 5xx: the platform may have accepted the message; the
                    // generic loop settles adapter errors as Ambiguous.
                    Err(format!("discord REST {code}: {detail}"))
                }
            }
            Err(error) => Err(format!("discord REST: {error}")),
        }
    }

    fn health(&self) -> Result<(), String> {
        if self.last_poll_ok || self.socket.is_some() {
            Ok(())
        } else {
            Err("discord gateway not connected".to_string())
        }
    }
}

impl DiscordTransport for LiveDiscordTransport {
    fn poll_messages(&mut self) -> Result<Vec<DiscordMessage>, String> {
        self.poll_messages()
    }

    fn send_message(&mut self, channel_id: &str, body: &str) -> Result<SendOutcome, String> {
        self.send_message(channel_id, body)
    }

    fn health(&self) -> Result<(), String> {
        self.health()
    }
}

/// What one gateway frame means for the transport.
enum FrameEffect {
    None,
    Messages(Vec<DiscordMessage>),
    /// Gateway requested a reconnect (op 7/9) or the frame was undecodable.
    Reconnect,
}

fn read_token(token_env: &str) -> Result<String, String> {
    std::env::var(token_env)
        .map_err(|_| format!("live discord needs a bot token; set ${token_env}"))
}

/// Bound how long a socket read can block so a poll always returns.
fn set_socket_timeout(socket: &mut GatewaySocket, secs: u64) {
    let timeout = Duration::from_secs(secs);
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(Some(timeout));
        }
        MaybeTlsStream::Rustls(stream) => {
            let _ = stream.sock.set_read_timeout(Some(timeout));
        }
        // NativeTls is not enabled in this build; nothing to configure.
        _ => {}
    }
}

/// Send op 1 on the announced interval until told to stop. The thread sleeps
/// in short slices and never holds the socket lock while sleeping, so a
/// blocked read only delays a heartbeat, never deadlocks it.
fn spawn_heartbeat(
    socket: Arc<Mutex<GatewaySocket>>,
    interval_ms: Arc<AtomicU64>,
    seq: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut last = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
            let interval = interval_ms.load(Ordering::Relaxed);
            if interval == 0 || last.elapsed() < Duration::from_millis(interval) {
                continue;
            }
            last = Instant::now();
            let seq_value = seq.load(Ordering::Relaxed);
            let payload = if seq_value == 0 {
                json!({"op": OP_HEARTBEAT, "d": Value::Null})
            } else {
                json!({"op": OP_HEARTBEAT, "d": seq_value})
            };
            if let Ok(mut ws) = socket.lock() {
                if ws.send(Message::text(payload.to_string())).is_ok() {
                    let _ = ws.flush();
                }
            }
        }
    });
}

/// Exponential backoff capped at `MAX_RECONNECT_BACKOFF_SECS`.
fn reconnect_backoff(attempts: u32) -> Duration {
    let secs = (1u64 << attempts.min(5)).min(MAX_RECONNECT_BACKOFF_SECS);
    Duration::from_secs(secs)
}

/// A read that hit the socket timeout is silence, not a dead connection.
fn is_silence(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// Extract a short, credential-free detail from a REST error response.
fn rest_error_detail(code: u16, response: ureq::Response) -> String {
    let body = response.into_string().unwrap_or_default();
    let detail = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or(body);
    format!(
        "HTTP {code}: {}",
        detail.chars().take(200).collect::<String>()
    )
}
