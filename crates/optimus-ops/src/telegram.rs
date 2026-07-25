//! Telegram home-channel adapter (program P28).
//!
//! Default product path is **mock** or outbound long-poll style client work —
//! this module never opens a public listen port. SQLite gateway remains the
//! local delivery authority (ADR-0021). External exactly-once is not claimed.
//!
//! Flow: poll updates → enqueue inbound → claim/turn → send reply → local
//! delivery receipt on confirmed handoff; leave ambiguous when the transport
//! reports unknown outcome.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::gateway::{
    acknowledge_delivery, claim_one, complete_claim, delivery_state, enqueue, fail_claim,
    mark_external_send_failed, GatewayError, InboundMessage,
};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("gateway: {0}")]
    Gateway(#[from] GatewayError),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, TelegramError>;

/// Outcome of an outbound Bot API style send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// Adapter confirmed the platform accepted the message (local receipt only).
    Confirmed { provider_message_id: String },
    /// Network/timeout left delivery unknown — operator must recover.
    Ambiguous { detail: String },
    /// Definite failure (bad chat, auth, etc.).
    Failed { detail: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramUpdate {
    pub update_id: u64,
    pub chat_id: String,
    pub text: String,
    #[serde(default)]
    pub from_username: Option<String>,
}

/// Pluggable transport so tests and live long-poll share the same claim→turn→receipt path.
pub trait TelegramTransport {
    fn get_updates(&mut self, offset: u64) -> Result<Vec<TelegramUpdate>>;
    fn send_message(&mut self, chat_id: &str, text: &str) -> Result<SendOutcome>;
}

/// In-memory mock Bot API for deterministic adapter tests.
#[derive(Debug, Default)]
pub struct MockTelegramTransport {
    pending: Vec<TelegramUpdate>,
    pub sent: Vec<(String, String)>,
    /// When true, next send returns Ambiguous instead of Confirmed.
    pub next_send_ambiguous: bool,
    /// When true, next send returns Failed.
    pub next_send_failed: bool,
}

impl MockTelegramTransport {
    pub fn push_text(&mut self, update_id: u64, chat_id: &str, text: &str) {
        self.pending.push(TelegramUpdate {
            update_id,
            chat_id: chat_id.into(),
            text: text.into(),
            from_username: Some("mock_user".into()),
        });
    }
}

impl TelegramTransport for MockTelegramTransport {
    fn get_updates(&mut self, offset: u64) -> Result<Vec<TelegramUpdate>> {
        let ready: Vec<_> = self
            .pending
            .iter()
            .filter(|u| u.update_id >= offset)
            .cloned()
            .collect();
        self.pending
            .retain(|u| !ready.iter().any(|r| r.update_id == u.update_id));
        Ok(ready)
    }

    fn send_message(&mut self, chat_id: &str, text: &str) -> Result<SendOutcome> {
        if self.next_send_failed {
            self.next_send_failed = false;
            return Ok(SendOutcome::Failed {
                detail: "mock_failed".into(),
            });
        }
        if self.next_send_ambiguous {
            self.next_send_ambiguous = false;
            return Ok(SendOutcome::Ambiguous {
                detail: "mock_timeout".into(),
            });
        }
        self.sent.push((chat_id.into(), text.into()));
        Ok(SendOutcome::Confirmed {
            provider_message_id: format!("mock-{}", self.sent.len()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramPollResult {
    pub enqueued: Vec<String>,
    pub drained: Vec<String>,
    pub receipts: Vec<String>,
    pub ambiguous: Vec<String>,
    pub failed_sends: Vec<String>,
    pub next_offset: u64,
}

/// Config gate for live long-poll (token never stored in this struct as a secret dump).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramConfig {
    /// When false, live poll refuses to run (mock tests ignore this).
    #[serde(default)]
    pub enabled: bool,
    /// Name of env var holding the bot token (token itself never written here).
    #[serde(default = "default_token_env")]
    pub bot_token_env: String,
    /// Optional fixed chat allowlist; empty = accept any in mock.
    #[serde(default)]
    pub allowed_chat_ids: Vec<String>,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_token_env: default_token_env(),
            allowed_chat_ids: Vec::new(),
        }
    }
}

fn default_token_env() -> String {
    "OPTIMUS_TELEGRAM_BOT_TOKEN".into()
}

pub fn load_telegram_config(home: impl AsRef<Path>) -> Result<TelegramConfig> {
    let path = home.as_ref().join("gateway").join("telegram.json");
    if !path.exists() {
        return Ok(TelegramConfig::default());
    }
    let raw = std::fs::read_to_string(path).map_err(|e| TelegramError::Msg(e.to_string()))?;
    serde_json::from_str(&raw).map_err(|e| TelegramError::Msg(e.to_string()))
}

pub fn save_telegram_config(home: impl AsRef<Path>, config: &TelegramConfig) -> Result<()> {
    let dir = home.as_ref().join("gateway");
    std::fs::create_dir_all(&dir).map_err(|e| TelegramError::Msg(e.to_string()))?;
    let path = dir.join("telegram.json");
    let raw =
        serde_json::to_string_pretty(config).map_err(|e| TelegramError::Msg(e.to_string()))?;
    std::fs::write(path, raw).map_err(|e| TelegramError::Msg(e.to_string()))?;
    Ok(())
}

/// One adapter cycle: ingest updates, process inbox for telegram channel, attempt outbound sends.
pub fn poll_once<T, F>(
    home: impl AsRef<Path>,
    transport: &mut T,
    offset: u64,
    mut turn: F,
) -> Result<TelegramPollResult>
where
    T: TelegramTransport,
    F: FnMut(&InboundMessage) -> std::result::Result<(String, Option<String>), String>,
{
    let home = home.as_ref();
    let config = load_telegram_config(home)?;
    let mut result = TelegramPollResult {
        enqueued: Vec::new(),
        drained: Vec::new(),
        receipts: Vec::new(),
        ambiguous: Vec::new(),
        failed_sends: Vec::new(),
        next_offset: offset,
    };

    if config.enabled && config.allowed_chat_ids.is_empty() {
        return Err(TelegramError::Msg(
            "live telegram requires non-empty allowed_chat_ids (fail closed)".into(),
        ));
    }

    let updates = transport.get_updates(offset)?;
    for update in updates {
        // Always advance offset so live long-poll does not redeliver forever.
        result.next_offset = result.next_offset.max(update.update_id.saturating_add(1));
        if !config.allowed_chat_ids.is_empty()
            && !config.allowed_chat_ids.iter().any(|id| id == &update.chat_id)
        {
            continue;
        }
        // Encode chat_id into session so replies can address the same chat.
        let session = Some(format!("telegram:{}", update.chat_id));
        let text = match update.from_username.as_deref() {
            Some(user) => format!("@{user}: {}", update.text),
            None => update.text.clone(),
        };
        let inbound = enqueue(home, "telegram", &text, "offline", session.as_deref())?;
        result.enqueued.push(inbound.id);
    }

    // Process only messages we just enqueued (channel=telegram), not FIFO global backlog.
    for message_id in result.enqueued.clone() {
        match process_enqueued_telegram(home, transport, &message_id, &mut turn)? {
            Some(step) => {
                result.drained.push(step.drained_id);
                result.receipts.extend(step.receipts);
                result.ambiguous.extend(step.ambiguous);
                result.failed_sends.extend(step.failed_sends);
            }
            None => break,
        }
    }

    Ok(result)
}

struct ProcessStep {
    drained_id: String,
    receipts: Vec<String>,
    ambiguous: Vec<String>,
    failed_sends: Vec<String>,
}

fn process_enqueued_telegram<T, F>(
    home: &Path,
    transport: &mut T,
    message_id: &str,
    turn: &mut F,
) -> Result<Option<ProcessStep>>
where
    T: TelegramTransport,
    F: FnMut(&InboundMessage) -> std::result::Result<(String, Option<String>), String>,
{
    // Skip non-telegram FIFO head by claiming only when the head matches our id.
    // Release foreign claims by never claiming them: claim_one is FIFO, so if head
    // is not our message we stop (operator must drain non-telegram separately).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let Some(claim) = claim_one(home, Uuid::new_v4(), now, 900)? else {
        return Ok(None);
    };
    if claim.message().id != message_id || claim.message().channel != "telegram" {
        // Put foreign claim back for later workers.
        crate::gateway::release_claim(home, &claim, now)?;
        return Ok(None);
    }
    let outcome = turn(claim.message());
    let drained = match outcome {
        Ok(success) => complete_claim(home, &claim, Ok(success), now)?,
        Err(error) => fail_claim(home, &claim, &error, now)?,
    };
    let mut step = ProcessStep {
        drained_id: drained.id.clone(),
        receipts: Vec::new(),
        ambiguous: Vec::new(),
        failed_sends: Vec::new(),
    };
    // Only external-send successful turns.
    if drained.status != "ok" {
        return Ok(Some(step));
    }
    let Some(session_id) = drained.session_id.as_deref() else {
        return Ok(Some(step));
    };
    let Some(chat_id) = session_id.strip_prefix("telegram:") else {
        return Ok(Some(step));
    };
    apply_send_outcome(home, transport, chat_id, &drained.id, &drained.reply_preview, &mut step)?;
    Ok(Some(step))
}

fn apply_send_outcome<T: TelegramTransport>(
    home: &Path,
    transport: &mut T,
    chat_id: &str,
    message_id: &str,
    text: &str,
    step: &mut ProcessStep,
) -> Result<()> {
    match transport.send_message(chat_id, text)? {
        SendOutcome::Confirmed { .. } => {
            if acknowledge_outbound_for_message(home, message_id)? {
                step.receipts.push(message_id.into());
            }
        }
        SendOutcome::Ambiguous { detail } => {
            step.ambiguous.push(format!("{message_id}:{detail}"));
        }
        SendOutcome::Failed { detail } => {
            let _ = mark_external_send_failed(home, message_id, &detail)?;
            step.failed_sends.push(format!("{message_id}:{detail}"));
        }
    }
    Ok(())
}

fn acknowledge_outbound_for_message(home: &Path, message_id: &str) -> Result<bool> {
    // Prefer delivery_state + outbound from exact message id.
    use crate::gateway::list_outbox_receipts;
    let rows = list_outbox_receipts(home, 200)?;
    let Some(row) = rows.into_iter().find(|r| r.message_id == message_id) else {
        return Ok(false);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(acknowledge_delivery(
        home,
        message_id,
        &row.outbound.id,
        now,
    )?)
}

/// Process one inbound telegram message through claim→turn→send without polling.
pub fn process_inbound_reply_path<T, F>(
    home: impl AsRef<Path>,
    transport: &mut T,
    chat_id: &str,
    text: &str,
    mut turn: F,
) -> Result<TelegramPollResult>
where
    T: TelegramTransport,
    F: FnMut(&InboundMessage) -> std::result::Result<(String, Option<String>), String>,
{
    let home = home.as_ref();
    let session = format!("telegram:{chat_id}");
    let inbound = enqueue(home, "telegram", text, "offline", Some(&session))?;
    let mut result = TelegramPollResult {
        enqueued: vec![inbound.id.clone()],
        drained: Vec::new(),
        receipts: Vec::new(),
        ambiguous: Vec::new(),
        failed_sends: Vec::new(),
        next_offset: 0,
    };
    if let Some(step) = process_enqueued_telegram(home, transport, &inbound.id, &mut turn)? {
        result.drained.push(step.drained_id);
        result.receipts = step.receipts;
        result.ambiguous = step.ambiguous;
        result.failed_sends = step.failed_sends;
    }
    let _ = delivery_state(home, &inbound.id);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{gateway_status, list_ambiguous_sends};
    use tempfile::tempdir;

    #[test]
    fn mock_claim_turn_receipt_roundtrip() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport::default();
        transport.push_text(1, "42", "hello telegram");

        let result = poll_once(dir.path(), &mut transport, 0, |inbound| {
            assert_eq!(inbound.channel, "telegram");
            Ok((format!("reply:{}", inbound.text), inbound.session_id.clone()))
        })
        .unwrap();

        assert_eq!(result.enqueued.len(), 1);
        assert_eq!(result.drained.len(), 1);
        assert_eq!(result.receipts.len(), 1);
        assert!(result.ambiguous.is_empty());
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(gateway_status(dir.path()).unwrap().ambiguous_sends, 0);
    }

    #[test]
    fn mock_ambiguous_send_leaves_operator_recovery() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport::default();
        transport.next_send_ambiguous = true;
        let result = process_inbound_reply_path(
            dir.path(),
            &mut transport,
            "99",
            "ping",
            |inbound| Ok(("pong".into(), inbound.session_id.clone())),
        )
        .unwrap();
        assert_eq!(result.receipts.len(), 0);
        assert_eq!(result.ambiguous.len(), 1);
        assert_eq!(list_ambiguous_sends(dir.path(), 10).unwrap().len(), 1);
    }

    #[test]
    fn mock_failed_send_is_not_ambiguous() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport::default();
        transport.next_send_failed = true;
        let result = process_inbound_reply_path(
            dir.path(),
            &mut transport,
            "7",
            "ping",
            |inbound| Ok(("pong".into(), inbound.session_id.clone())),
        )
        .unwrap();
        assert_eq!(result.failed_sends.len(), 1);
        assert!(list_ambiguous_sends(dir.path(), 10).unwrap().is_empty());
        assert_eq!(gateway_status(dir.path()).unwrap().ambiguous_sends, 0);
    }

    #[test]
    fn turn_error_does_not_external_send() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport::default();
        let result = process_inbound_reply_path(dir.path(), &mut transport, "1", "x", |_| {
            Err("provider_unavailable".into())
        })
        .unwrap();
        assert!(result.receipts.is_empty());
        assert!(transport.sent.is_empty());
    }

    #[test]
    fn live_config_defaults_disabled() {
        let dir = tempdir().unwrap();
        let cfg = load_telegram_config(dir.path()).unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.bot_token_env, "OPTIMUS_TELEGRAM_BOT_TOKEN");
    }
}
