//! Telegram home-channel adapter (program P28).
//!
//! Default product path is **mock** or outbound long-poll style client work —
//! this module never opens a public listen port. SQLite gateway remains the
//! local delivery authority (ADR-0021). External exactly-once is not claimed.
//!
//! Flow: poll updates → enqueue inbound → claim/turn → the turn commits the send
//! it owes to [`crate::gateway::outbound_ledger`] → this adapter claims that
//! obligation, sends, and settles what the platform said.
//!
//! The adapter never decides *whether* a reply is owed; the turn's commit does.
//! That is what makes a crash between the two survivable: the debt is already
//! durable, so the next poll picks it up instead of losing it.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::gateway::outbound_ledger::{claim_outbound, settle_outbound, OutboundSettlement};
use crate::gateway::{
    claim_one, complete_claim, enqueue, fail_claim, GatewayError, InboundMessage,
};
use uuid::Uuid;

mod live;

pub use live::LiveTelegramTransport;

mod adapter;

pub use adapter::TelegramAdapter;

/// How long one outbound send may hold its obligation before the sweep calls it
/// unknown. A Bot API call that has not returned in two minutes has already
/// stopped being a question this process can answer.
const SEND_LEASE_SECS: u64 = 120;

pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Error)]
pub enum TelegramError {
    #[error("gateway: {0}")]
    Gateway(#[from] GatewayError),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, TelegramError>;

/// Outcome of an outbound Bot API style send. Defined once on the spec-017
/// transport contract so every adapter shares the same terminal outcomes.
pub use crate::transport::SendOutcome;

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

impl TelegramConfig {
    /// Build the live Bot API transport this config describes.
    ///
    /// Returned as an `impl Trait` deliberately. A caller that only wants to run
    /// a poll cycle never has to name the concrete type, so adding a live
    /// transport does not drag a new name through every re-export layer between
    /// this crate and the command that runs it.
    pub fn live_transport(&self, poll_hold_secs: u64) -> Result<impl TelegramTransport> {
        if !self.enabled {
            return Err(TelegramError::Msg(
                "live telegram is disabled; set enabled in gateway/telegram.json".into(),
            ));
        }
        LiveTelegramTransport::new(&self.bot_token_env, poll_hold_secs)
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
        // A photo, a sticker, or a join carries no text and no turn to run. It
        // is skipped *after* the offset moves: dropping it before would leave
        // Telegram handing back the same unreadable update on every cycle, and
        // the adapter would never see anything behind it again.
        if update.text.trim().is_empty() || update.chat_id.is_empty() {
            continue;
        }
        if !config.allowed_chat_ids.is_empty()
            && !config
                .allowed_chat_ids
                .iter()
                .any(|id| id == &update.chat_id)
        {
            continue;
        }
        // The routing address, not a session id: `<channel>:<address>` is what the
        // reply is sent back to, and the kernel session is derived from it (ADR-0071).
        let session = Some(format!("telegram:{}", update.chat_id));
        let text = match update.from_username.as_deref() {
            Some(user) => format!("@{user}: {}", update.text),
            None => update.text.clone(),
        };
        // "auto" so a remote message reaches whatever provider this machine is
        // configured for. Hard-coding "offline" pinned every inbound message to the
        // scripted echo model, which emits no tool calls — a bot that can never do
        // anything is also a bot whose approval spine can never fire.
        let inbound = enqueue(home, "telegram", &text, "auto", session.as_deref())?;
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
    let now = now_unix();
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
    // Whether this particular turn owes a reply was decided inside its commit.
    // Draining the whole telegram backlog rather than just this turn's send is
    // what makes an obligation stranded by an earlier crash recoverable.
    deliver_owed_sends(home, transport, &mut step)?;
    Ok(Some(step))
}

/// Send every telegram reply the ledger says is owed, one attempt each.
///
/// Stops at the first send that is not confirmed. A transport that just refused
/// or went dark will not answer differently a millisecond later, and the failed
/// obligation is already back in the pending pool — the next poll cycle is the
/// retry, which gives the bound in [`crate::gateway::outbound_ledger`] a real
/// interval to count rather than five attempts inside one loop.
fn deliver_owed_sends<T: TelegramTransport>(
    home: &Path,
    transport: &mut T,
    step: &mut ProcessStep,
) -> Result<()> {
    while let Some(claim) = claim_outbound(
        home,
        Some("telegram"),
        Uuid::new_v4(),
        now_unix(),
        SEND_LEASE_SECS,
    )? {
        let owed = claim.obligation().clone();
        let Some(chat_id) = owed.target.strip_prefix("telegram:") else {
            // The ledger keeps routing addresses opaque, so an address this
            // adapter cannot read is a definite failure to send, not an unknown.
            let detail = format!("unroutable telegram target {}", owed.target);
            settle(home, &claim, OutboundSettlement::Failed { detail }, step)?;
            break;
        };

        let outcome = match transport.send_message(chat_id, &owed.body) {
            Ok(outcome) => outcome,
            Err(error) => {
                // The adapter broke around the call, so whether the platform saw
                // it is exactly the question this ledger refuses to guess at.
                let detail = format!("transport error: {error}");
                settle(home, &claim, OutboundSettlement::Ambiguous { detail }, step)?;
                return Err(error);
            }
        };

        let confirmed = matches!(outcome, SendOutcome::Confirmed { .. });
        settle(
            home,
            &claim,
            crate::transport::settlement_for_outcome(outcome),
            step,
        )?;
        if !confirmed {
            break;
        }
    }
    Ok(())
}

/// Record the settlement and report it in the adapter's per-cycle accounting.
///
/// A settlement whose lease is gone is not re-applied: the sweep already called
/// that send unknown, and overwriting it with this worker's late opinion would
/// erase the only honest thing the ledger knows about it.
fn settle(
    home: &Path,
    claim: &crate::gateway::outbound_ledger::OutboundClaim,
    settlement: OutboundSettlement,
    step: &mut ProcessStep,
) -> Result<()> {
    let message_id = claim.obligation().message_id.clone();
    let note = match &settlement {
        OutboundSettlement::Delivered { .. } => None,
        OutboundSettlement::Ambiguous { detail } | OutboundSettlement::Failed { detail } => {
            Some(format!("{message_id}:{detail}"))
        }
    };
    let bucket = match &settlement {
        OutboundSettlement::Delivered { .. } => &mut step.receipts,
        OutboundSettlement::Ambiguous { .. } => &mut step.ambiguous,
        OutboundSettlement::Failed { .. } => &mut step.failed_sends,
    };
    match settle_outbound(home, claim, settlement, now_unix()) {
        Ok(_) => {
            bucket.push(note.unwrap_or(message_id));
            Ok(())
        }
        Err(GatewayError::LeaseLost { message_id }) => {
            step.ambiguous.push(format!("{message_id}:lease expired"));
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
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
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::gateway_status;
    use crate::gateway::outbound_ledger::{
        list_ambiguous_obligations, list_pending_obligations, outbound_ledger_status,
    };
    use crate::gateway::outbound_receipts::list_ambiguous_sends;
    use tempfile::tempdir;

    #[test]
    fn mock_claim_turn_receipt_roundtrip() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport::default();
        transport.push_text(1, "42", "hello telegram");

        let result = poll_once(dir.path(), &mut transport, 0, |inbound| {
            assert_eq!(inbound.channel, "telegram");
            Ok((
                format!("reply:{}", inbound.text),
                inbound.session_id.clone(),
            ))
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
        let mut transport = MockTelegramTransport {
            next_send_ambiguous: true,
            ..Default::default()
        };
        let result =
            process_inbound_reply_path(dir.path(), &mut transport, "99", "ping", |inbound| {
                Ok(("pong".into(), inbound.session_id.clone()))
            })
            .unwrap();
        assert_eq!(result.receipts.len(), 0);
        assert_eq!(result.ambiguous.len(), 1);
        assert_eq!(list_ambiguous_sends(dir.path(), 10).unwrap().len(), 1);
    }

    #[test]
    fn mock_failed_send_is_not_ambiguous() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport {
            next_send_failed: true,
            ..Default::default()
        };
        let result =
            process_inbound_reply_path(dir.path(), &mut transport, "7", "ping", |inbound| {
                Ok(("pong".into(), inbound.session_id.clone()))
            })
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

    /// Regression: the adapter used to send `drained.reply_preview`, which
    /// `complete_claim` builds with `.take(200)`. Every reply longer than that
    /// arrived at the chat silently cut off, with the gateway recording a
    /// delivery receipt for a message the user never fully received.
    #[test]
    fn the_chat_receives_the_whole_reply_not_the_preview() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport::default();
        let long_reply = "x".repeat(5_000);
        let expected = long_reply.clone();

        let result =
            process_inbound_reply_path(dir.path(), &mut transport, "42", "ping", |inbound| {
                Ok((expected.clone(), inbound.session_id.clone()))
            })
            .unwrap();

        assert_eq!(result.receipts.len(), 1);
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(transport.sent[0].0, "42");
        assert_eq!(transport.sent[0].1, long_reply);
    }

    #[test]
    fn a_send_stranded_by_a_crash_goes_out_on_the_next_cycle() {
        let dir = tempdir().unwrap();
        // A turn that commits its obligation, then a process that dies before
        // any transport exists to send it.
        let mut dead = MockTelegramTransport {
            next_send_failed: true,
            ..Default::default()
        };
        process_inbound_reply_path(dir.path(), &mut dead, "7", "ping", |inbound| {
            Ok(("pong".into(), inbound.session_id.clone()))
        })
        .unwrap();
        assert!(dead.sent.is_empty());
        assert_eq!(list_pending_obligations(dir.path(), 10).unwrap().len(), 1);

        // A later cycle finds the debt without needing the original turn.
        let mut revived = MockTelegramTransport::default();
        let mut step = ProcessStep {
            drained_id: String::new(),
            receipts: Vec::new(),
            ambiguous: Vec::new(),
            failed_sends: Vec::new(),
        };
        deliver_owed_sends(dir.path(), &mut revived, &mut step).unwrap();

        assert_eq!(revived.sent.len(), 1);
        assert_eq!(revived.sent[0], ("7".to_string(), "pong".to_string()));
        assert_eq!(step.receipts.len(), 1);
        assert!(list_pending_obligations(dir.path(), 10).unwrap().is_empty());
        assert_eq!(outbound_ledger_status(dir.path()).unwrap().delivered, 1);
    }

    #[test]
    fn a_refused_send_is_not_retried_inside_the_same_cycle() {
        let dir = tempdir().unwrap();
        // Only the first send is refused; a loop that retried immediately would
        // succeed on the second call and hide the refusal entirely.
        let mut transport = MockTelegramTransport {
            next_send_failed: true,
            ..Default::default()
        };
        let result =
            process_inbound_reply_path(dir.path(), &mut transport, "7", "ping", |inbound| {
                Ok(("pong".into(), inbound.session_id.clone()))
            })
            .unwrap();

        assert_eq!(result.failed_sends.len(), 1);
        assert!(transport.sent.is_empty());
        let owed = list_pending_obligations(dir.path(), 10).unwrap();
        assert_eq!(owed.len(), 1);
        assert_eq!(owed[0].attempts, 1);
    }

    #[test]
    fn an_unknown_send_is_never_re_sent_by_the_adapter() {
        let dir = tempdir().unwrap();
        let mut transport = MockTelegramTransport {
            next_send_ambiguous: true,
            ..Default::default()
        };
        process_inbound_reply_path(dir.path(), &mut transport, "99", "ping", |inbound| {
            Ok(("pong".into(), inbound.session_id.clone()))
        })
        .unwrap();
        assert_eq!(list_ambiguous_obligations(dir.path(), 10).unwrap().len(), 1);

        // Every later cycle leaves it alone: only an operator resolution can
        // decide whether the platform already has this message.
        let mut later = MockTelegramTransport::default();
        let mut step = ProcessStep {
            drained_id: String::new(),
            receipts: Vec::new(),
            ambiguous: Vec::new(),
            failed_sends: Vec::new(),
        };
        deliver_owed_sends(dir.path(), &mut later, &mut step).unwrap();
        assert!(later.sent.is_empty());
        assert_eq!(list_ambiguous_obligations(dir.path(), 10).unwrap().len(), 1);
    }

    #[test]
    fn live_config_defaults_disabled() {
        let dir = tempdir().unwrap();
        let cfg = load_telegram_config(dir.path()).unwrap();
        assert!(!cfg.enabled);
        assert_eq!(cfg.bot_token_env, "OPTIMUS_TELEGRAM_BOT_TOKEN");
    }
}
