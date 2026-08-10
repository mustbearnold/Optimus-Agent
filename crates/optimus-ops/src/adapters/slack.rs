//! Slack adapter (spec-017): Socket Mode websocket inbound + REST outbound.
//!
//! Slack never listens on an inbound port: a bot holds an outbound websocket
//! obtained from `apps.connections.open` and receives `events_api` envelopes
//! on it. Every enveloped frame is acked or Slack closes the socket after a
//! few seconds. The ticket URL is one-time, so every reconnect — an
//! unexpected close, a `disconnect` frame, or a `reconnect_url` frame's fresh
//! ticket — re-establishes the socket before polling resumes.
//!
//! Outbound replies go back over REST `chat.postMessage`. The app token
//! (`xapp-`) is used only for Socket Mode; the bot token (`xoxb-`) only for
//! REST. Neither ever lives in config: the config names the environment
//! variables that hold them, read per call so a rotation takes effect on the
//! next request rather than the next restart.
//!
//! The mock transport is the test seam; see tests/slack_conformance.rs.

use std::io;
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect as ws_connect, Error as WsError, Message, WebSocket};

use crate::transport::{RawInbound, SendOutcome, TransportAdapter, TransportId};

/// One REST attempt may take this long before it counts as unknown.
const HTTP_TIMEOUT_SECS: u64 = 30;
/// Socket re-open delay after an unexpected close (`reconnect_secs` default).
const DEFAULT_RECONNECT_SECS: u64 = 5;

/// Slack's public API base. Both Socket Mode ticketing and outbound posting
/// live here; only the websocket host comes from the ticket URL itself.
const SLACK_API_BASE: &str = "https://slack.com/api";

/// Config gate for live Socket Mode (spec-017 R5/R6).
///
/// Tokens are never stored here — only the names of the environment variables
/// that hold them — so this file is safe to print and commit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackConfig {
    /// When false, live poll refuses to run (mock tests ignore this).
    #[serde(default)]
    pub enabled: bool,
    /// Env var holding the `xapp-` Socket Mode token (never the token).
    #[serde(default = "default_app_token_env")]
    pub app_token_env: String,
    /// Env var holding the `xoxb-` REST bot token (never the token).
    #[serde(default = "default_bot_token_env")]
    pub bot_token_env: String,
    /// Inbound channel allowlist; empty fails closed in live mode.
    #[serde(default)]
    pub allowed_channel_ids: Vec<String>,
    /// Seconds to wait before re-opening a dropped socket; defaults to 5.
    #[serde(default)]
    pub reconnect_secs: Option<u64>,
}

impl Default for SlackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            app_token_env: default_app_token_env(),
            bot_token_env: default_bot_token_env(),
            allowed_channel_ids: Vec::new(),
            reconnect_secs: None,
        }
    }
}

fn default_app_token_env() -> String {
    "OPTIMUS_SLACK_APP_TOKEN".into()
}

fn default_bot_token_env() -> String {
    "OPTIMUS_SLACK_BOT_TOKEN".into()
}

pub fn load_slack_config(home: &Path) -> Result<SlackConfig, String> {
    let path = home.join("gateway").join("slack.json");
    if !path.exists() {
        return Ok(SlackConfig::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("slack config: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("slack config: {e}"))
}

pub fn save_slack_config(home: &Path, config: &SlackConfig) -> Result<(), String> {
    let dir = home.join("gateway");
    std::fs::create_dir_all(&dir).map_err(|e| format!("slack config: {e}"))?;
    let raw = serde_json::to_string_pretty(config).map_err(|e| format!("slack config: {e}"))?;
    std::fs::write(dir.join("slack.json"), raw).map_err(|e| format!("slack config: {e}"))
}

/// Open the adapter registry entry: `None` without a config file, `Some`
/// whenever `gateway/slack.json` exists (even disabled), `Err` on malformed
/// config. Live transports are only built when the named tokens exist in the
/// environment; a disabled or token-less config degrades to the mock.
pub fn open_adapter(home: &Path) -> Result<Option<Box<dyn TransportAdapter>>, String> {
    let path = home.join("gateway").join("slack.json");
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(SlackAdapter::from_config(load_slack_config(home)?)))
}

/// Pluggable transport so tests and live Socket Mode share the same
/// claim→turn→receipt path.
pub trait SlackTransport {
    /// Drain whatever inbound is available without blocking.
    fn poll(&mut self) -> Result<Vec<RawInbound>, String>;
    /// Post one outbound message and classify its terminal outcome.
    fn send_message(&mut self, channel: &str, text: &str) -> Result<SendOutcome, String>;
    /// Best-effort liveness, reported through the supervisor.
    fn health(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Shared inbound filter, applied by the live frame parser and the mock alike:
/// bot-authored messages (`bot_id` present or `bot_message` subtype) and empty
/// text never reach the queue.
fn message_to_raw(
    channel: &str,
    text: &str,
    is_bot: bool,
    subtype: Option<&str>,
) -> Option<RawInbound> {
    if is_bot || subtype == Some("bot_message") {
        return None;
    }
    if channel.is_empty() || text.trim().is_empty() {
        return None;
    }
    Some(RawInbound {
        from: channel.to_string(),
        text: text.to_string(),
        attachments: Vec::new(),
    })
}

/// One scripted inbound message for [`MockSlackTransport`].
#[derive(Debug, Clone)]
pub struct MockInbound {
    pub channel: String,
    pub text: String,
    pub is_bot: bool,
    pub subtype: Option<String>,
}

/// In-memory mock Socket Mode transport: a scripted inbound queue and a
/// recorded outbound log. Deterministic — no timing, no network.
#[derive(Debug, Default)]
pub struct MockSlackTransport {
    pending: Vec<MockInbound>,
    /// Every outbound send recorded as (channel, text); shared so tests can
    /// observe sends after the adapter owns the transport.
    pub sent: Arc<Mutex<Vec<(String, String)>>>,
}

impl MockSlackTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Script a human message in `channel`.
    pub fn push(&mut self, channel: &str, text: &str) {
        self.pending.push(MockInbound {
            channel: channel.into(),
            text: text.into(),
            is_bot: false,
            subtype: None,
        });
    }

    /// Script a bot-authored message (the live parser would see `bot_id`).
    pub fn push_bot(&mut self, channel: &str, text: &str) {
        self.pending.push(MockInbound {
            channel: channel.into(),
            text: text.into(),
            is_bot: true,
            subtype: None,
        });
    }

    /// Script a message carrying a subtype (e.g. `bot_message`).
    pub fn push_subtype(&mut self, channel: &str, text: &str, subtype: &str) {
        self.pending.push(MockInbound {
            channel: channel.into(),
            text: text.into(),
            is_bot: false,
            subtype: Some(subtype.into()),
        });
    }

    pub fn sent_records(&self) -> Vec<(String, String)> {
        self.sent.lock().expect("mock send log poisoned").clone()
    }
}

impl SlackTransport for MockSlackTransport {
    fn poll(&mut self) -> Result<Vec<RawInbound>, String> {
        Ok(self
            .pending
            .drain(..)
            .filter_map(|m| message_to_raw(&m.channel, &m.text, m.is_bot, m.subtype.as_deref()))
            .collect())
    }

    fn send_message(&mut self, channel: &str, text: &str) -> Result<SendOutcome, String> {
        let mut sent = self.sent.lock().expect("mock send log poisoned");
        sent.push((channel.to_string(), text.to_string()));
        Ok(SendOutcome::Confirmed {
            provider_message_id: format!("mock-{}", sent.len()),
        })
    }
}

/// Slack on the spec-017 contract.
pub struct SlackAdapter {
    config: SlackConfig,
    transport: Box<dyn SlackTransport + Send>,
}

impl SlackAdapter {
    /// Build the adapter from config with the live transport. A disabled or
    /// token-less config degrades to the mock transport so the supervisor can
    /// report a clean `Stopped (not configured)` state; the cycle itself
    /// never runs when `is_enabled` is false.
    pub fn from_config(config: SlackConfig) -> Box<dyn TransportAdapter> {
        let reconnect_secs = config.reconnect_secs.unwrap_or(DEFAULT_RECONNECT_SECS);
        match LiveSlackTransport::new(&config.app_token_env, &config.bot_token_env, reconnect_secs)
        {
            Ok(transport) => Box::new(Self {
                config,
                transport: Box::new(transport),
            }),
            Err(_) => Box::new(Self {
                config,
                transport: Box::new(MockSlackTransport::new()),
            }),
        }
    }

    /// Build the adapter over a caller-supplied transport (test seam).
    pub fn with_transport(config: SlackConfig, transport: Box<dyn SlackTransport + Send>) -> Self {
        Self { config, transport }
    }
}

impl TransportAdapter for SlackAdapter {
    fn transport(&self) -> TransportId {
        TransportId::Slack
    }

    fn is_enabled(&self, _home: &Path) -> bool {
        self.config.enabled
    }

    fn poll_inbound(&mut self, _home: &Path) -> Result<Vec<RawInbound>, String> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }
        if self.config.allowed_channel_ids.is_empty() {
            return Err("live slack requires non-empty allowed_channel_ids (fail closed)".into());
        }
        self.transport.poll().map_err(|e| e.to_string())
    }

    fn is_allowed(&self, from: &str) -> bool {
        self.config.allowed_channel_ids.is_empty()
            || self.config.allowed_channel_ids.iter().any(|id| id == from)
    }

    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        self.transport
            .send_message(target, body)
            .map_err(|e| e.to_string())
    }

    fn health(&self) -> Result<(), String> {
        self.transport.health()
    }
}

/// Slack Socket Mode over a sync tungstenite websocket + ureq REST.
///
/// Every field is safe to print: credentials are never stored, only the names
/// of the environment variables that hold them.
pub struct LiveSlackTransport {
    app_token_env: String,
    bot_token_env: String,
    reconnect_secs: u64,
    agent: ureq::Agent,
    socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
    /// One-time wss ticket from a `reconnect_url` frame; consumed on re-open.
    pending_url: Option<String>,
    /// Earliest unix second a reconnect may be attempted; 0 = now.
    reconnect_at: u64,
    last_poll_ok: bool,
}

impl LiveSlackTransport {
    /// Build a transport against the real Slack API. Fails fast when either
    /// named token is missing so `from_config` can degrade cleanly; a rotated
    /// credential takes effect on the next request, not the next restart.
    pub fn new(
        app_token_env: &str,
        bot_token_env: &str,
        reconnect_secs: u64,
    ) -> Result<Self, String> {
        token(app_token_env)?;
        token(bot_token_env)?;
        Ok(Self {
            app_token_env: app_token_env.to_string(),
            bot_token_env: bot_token_env.to_string(),
            reconnect_secs,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
                .build(),
            socket: None,
            pending_url: None,
            reconnect_at: 0,
            last_poll_ok: false,
        })
    }

    /// Establish the socket: a stored one-time ticket if we have one, else a
    /// fresh `apps.connections.open` ticket. Socket Mode requires no inbound
    /// port — the ticket URL is the connection.
    fn open_socket(&mut self) -> Result<(), String> {
        let url = match self.pending_url.take() {
            Some(url) => url,
            None => {
                let token = token(&self.app_token_env)?;
                Self::request_ticket(&self.agent, &token)?
            }
        };
        self.socket = Some(Self::connect_wss(&url)?);
        Ok(())
    }

    /// `apps.connections.open`: returns the one-time wss ticket URL.
    fn request_ticket(agent: &ureq::Agent, token: &str) -> Result<String, String> {
        let response = agent
            .post(&format!("{SLACK_API_BASE}/apps.connections.open"))
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(json!({ "token": token }))
            .map_err(|e| format!("apps.connections.open: {e}"))?;
        let body = response
            .into_string()
            .map_err(|e| format!("apps.connections.open body: {e}"))?;
        let value: Value =
            serde_json::from_str(&body).map_err(|e| format!("apps.connections.open json: {e}"))?;
        if value.get("ok").and_then(|o| o.as_bool()) != Some(true) {
            let error = value
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown_error");
            return Err(format!("apps.connections.open refused: {error}"));
        }
        let url = value
            .get("url")
            .and_then(|u| u.as_str())
            .ok_or_else(|| "apps.connections.open missing url".to_string())?;
        Ok(url.to_string())
    }

    /// Connect to a wss ticket: TLS with webpki-roots, HTTP upgrade, then the
    /// socket is polled non-blocking so one cycle never waits on Slack.
    fn connect_wss(url: &str) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
        let (mut socket, _) = ws_connect(url).map_err(|e| format!("slack socket connect: {e}"))?;
        set_nonblocking(&mut socket, true).map_err(|e| format!("slack socket nonblocking: {e}"))?;
        Ok(socket)
    }

    /// What one Socket Mode frame asked for, decided before the caller acts.
    fn handle_frame(
        &mut self,
        socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
        text: &str,
    ) -> FrameAction {
        let Ok(frame) = serde_json::from_str::<Value>(text) else {
            return FrameAction::None;
        };
        match frame.get("type").and_then(|t| t.as_str()) {
            Some("hello") => FrameAction::None,
            Some("reconnect_url") => match frame.get("url").and_then(|u| u.as_str()) {
                Some(url) => FrameAction::ReconnectUrl(url.to_string()),
                None => FrameAction::None,
            },
            Some("disconnect") => {
                // Slack is retiring this socket; re-ticket via apps.connections.open.
                self.pending_url = None;
                FrameAction::Disconnect
            }
            Some("events_api") => {
                // Ack every enveloped frame or Slack closes the socket (~3s).
                let Some(envelope_id) = frame.get("envelope_id").and_then(|e| e.as_str()) else {
                    return FrameAction::None;
                };
                let ack = Message::text(json!({ "envelope_id": envelope_id }).to_string());
                if socket.write(ack).is_err() || !flush_ok(socket) {
                    return FrameAction::Broken;
                }
                let Some(event) = frame.pointer("/payload/event") else {
                    return FrameAction::None;
                };
                if event.get("type").and_then(|t| t.as_str()) != Some("message") {
                    return FrameAction::None;
                }
                let channel = event
                    .get("channel")
                    .and_then(|c| c.as_str())
                    .unwrap_or_default();
                let text = event
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default();
                let subtype = event.get("subtype").and_then(|s| s.as_str());
                match message_to_raw(channel, text, event.get("bot_id").is_some(), subtype) {
                    Some(raw) => FrameAction::Inbound(raw),
                    None => FrameAction::None,
                }
            }
            _ => FrameAction::None,
        }
    }
}

/// What one Socket Mode frame asked for.
enum FrameAction {
    /// A message a turn may run.
    Inbound(RawInbound),
    /// A fresh one-time ticket; use it on the next socket open.
    ReconnectUrl(String),
    /// Slack asked us to go away; drop the socket and re-ticket.
    Disconnect,
    /// The socket is unusable (ack write failed); drop and reconnect.
    Broken,
    /// Nothing to do (hello, acked event, unrecognized frame).
    None,
}

/// True if all buffered bytes reached the wire. A `WouldBlock` only defers
/// them — tungstenite retries on the next read/write/flush — so it is not a
/// failure; any other flush error means the socket is gone.
fn flush_ok(socket: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> bool {
    match socket.flush() {
        Ok(()) => true,
        Err(WsError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => true,
        Err(_) => false,
    }
}

fn set_nonblocking(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    nonblocking: bool,
) -> io::Result<()> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(tcp) => tcp.set_nonblocking(nonblocking),
        MaybeTlsStream::Rustls(tls) => tls.get_ref().set_nonblocking(nonblocking),
        // `MaybeTlsStream` is non-exhaustive; any other variant is left as-is.
        _ => Ok(()),
    }
}

impl SlackTransport for LiveSlackTransport {
    fn poll(&mut self) -> Result<Vec<RawInbound>, String> {
        let mut inbound = Vec::new();
        if self.socket.is_none() {
            if now_unix() < self.reconnect_at {
                // Inside the backoff window: stay quiet while reconnecting.
                return Ok(inbound);
            }
            if let Err(error) = self.open_socket() {
                self.reconnect_at = now_unix() + self.reconnect_secs;
                self.last_poll_ok = false;
                return Err(error);
            }
        }
        let mut socket = self.socket.take().expect("socket open above");
        let mut broken = false;
        let mut reconnect_now = false;
        loop {
            match socket.read() {
                Ok(Message::Text(text)) => match self.handle_frame(&mut socket, text.as_str()) {
                    FrameAction::Inbound(raw) => inbound.push(raw),
                    FrameAction::ReconnectUrl(url) => self.pending_url = Some(url),
                    FrameAction::Disconnect => {
                        reconnect_now = true;
                        broken = true;
                        break;
                    }
                    FrameAction::Broken => {
                        broken = true;
                        break;
                    }
                    FrameAction::None => {}
                },
                // tungstenite answers pings itself; queued pongs flush below.
                Ok(Message::Ping(_)) => {}
                Ok(Message::Pong(_)) => {}
                // Not produced by Slack, and never by `read`; defensive.
                Ok(Message::Binary(_)) | Ok(Message::Frame(_)) => {}
                Ok(Message::Close(_)) => {
                    broken = true;
                    break;
                }
                Err(WsError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(WsError::ConnectionClosed | WsError::AlreadyClosed) => {
                    broken = true;
                    break;
                }
                Err(_) => {
                    broken = true;
                    break;
                }
            }
        }
        if broken {
            self.socket = None;
            self.reconnect_at = if reconnect_now {
                now_unix()
            } else {
                now_unix() + self.reconnect_secs
            };
        } else if flush_ok(&mut socket) {
            self.socket = Some(socket);
        } else {
            self.socket = None;
            self.reconnect_at = now_unix() + self.reconnect_secs;
        }
        self.last_poll_ok = true;
        Ok(inbound)
    }

    fn send_message(&mut self, channel: &str, text: &str) -> Result<SendOutcome, String> {
        if text.trim().is_empty() {
            // Definite, and it stays definite however many times it is retried.
            return Ok(SendOutcome::Failed {
                detail: "refusing to send an empty message".into(),
            });
        }
        let token = token(&self.bot_token_env)?;
        let response = self
            .agent
            .post(&format!("{SLACK_API_BASE}/chat.postMessage"))
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(json!({ "channel": channel, "text": text }))
            .map_err(|e| format!("chat.postMessage: {e}"))?;
        let body = response
            .into_string()
            .map_err(|e| format!("chat.postMessage body: {e}"))?;
        // Slack answers every chat.postMessage with HTTP 200, saying `ok:false`
        // on refusal — so a non-200 or unparseable body is a transport-level
        // unknown (Err), never a fake confirmation.
        let Ok(value) = serde_json::from_str::<Value>(&body) else {
            return Err(format!("chat.postMessage unparseable response: {body}"));
        };
        if value.get("ok").and_then(|o| o.as_bool()) == Some(true) {
            let ts = value.get("ts").and_then(|t| t.as_str()).unwrap_or_default();
            return Ok(SendOutcome::Confirmed {
                provider_message_id: ts.to_string(),
            });
        }
        let error = value
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("unknown_error");
        Ok(SendOutcome::Failed {
            detail: error.to_string(),
        })
    }

    fn health(&self) -> Result<(), String> {
        if self.socket.is_some() || self.last_poll_ok {
            Ok(())
        } else {
            Err("slack socket not connected".into())
        }
    }
}

/// The token named by `env_name`, read from the environment per call.
fn token(env_name: &str) -> Result<String, String> {
    let value = std::env::var(env_name).unwrap_or_default();
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("live slack needs a token in ${env_name}"));
    }
    Ok(value.to_string())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
