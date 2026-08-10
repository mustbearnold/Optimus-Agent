//! spec-017 R1: the Telegram adapter on the shared transport contract.
//!
//! The legacy `poll_once` path in [`super`] stays byte-for-byte compatible
//! (its conformance suites are the regression pin for the refactor); this
//! module is the same adapter expressed as a `TransportAdapter`, so the
//! generic cycle and the gateway supervisor can drive Telegram exactly like
//! Discord, Slack, and Email.
//!
//! Behavioural notes carried over from the legacy path:
//!
//! - **Fail closed**: live polling refuses to run with an empty allowlist
//!   (`enabled` + no `allowed_chat_ids` is a configuration error), and a chat
//!   outside the allowlist never reaches a turn.
//! - **The offset is durable**: the cursor file (same path the standalone
//!   `optimus gateway telegram run` loop uses) survives restarts, so a
//!   restart never redelivers the same updates. Switching between the
//!   supervisor and the standalone loop therefore never replays a chat.
//! - **The reply route is the routing address**: `telegram:<chat_id>` is the
//!   session derivation, never a session identity (ADR-0071).

use super::{load_telegram_config, TelegramConfig};
use crate::transport::{RawInbound, SendOutcome, TransportAdapter, TransportId};
use std::path::{Path, PathBuf};

/// The durable poll cursor, at the same path the standalone run loop uses.
fn cursor_path(home: &Path) -> PathBuf {
    home.join("gateway").join("telegram-offset")
}

fn load_cursor(home: &Path) -> u64 {
    std::fs::read_to_string(cursor_path(home))
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(0)
}

fn save_cursor(home: &Path, offset: u64) {
    let path = cursor_path(home);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, offset.to_string());
}

/// Telegram on the spec-017 contract.
pub struct TelegramAdapter {
    config: TelegramConfig,
    transport: Box<dyn super::TelegramTransport + Send>,
    offset: u64,
}

impl TelegramAdapter {
    /// Build the adapter for a home: None when Telegram is not configured.
    pub fn open(home: &Path) -> Result<Option<Box<dyn TransportAdapter>>, String> {
        if !home.join("gateway").join("telegram.json").exists() {
            return Ok(None);
        }
        let config = load_telegram_config(home).map_err(|e| e.to_string())?;
        Ok(Some(Self::from_config(config)))
    }

    /// Build the adapter from config with the live transport. A disabled or
    /// token-less config degrades to the mock transport so the supervisor can
    /// report a clean `Stopped (not configured)` state; the cycle itself
    /// never runs when `is_enabled` is false.
    pub fn from_config(config: TelegramConfig) -> Box<dyn TransportAdapter> {
        let poll_hold_secs = 30;
        match config.live_transport(poll_hold_secs) {
            Ok(transport) => Box::new(Self {
                config,
                transport: Box::new(transport),
                offset: 0,
            }),
            Err(_) => Box::new(Self {
                config,
                transport: Box::new(super::MockTelegramTransport::default()),
                offset: 0,
            }),
        }
    }

    /// Test seam: adapter over a scripted transport.
    #[cfg(test)]
    pub(crate) fn with_transport(
        config: TelegramConfig,
        transport: Box<dyn super::TelegramTransport + Send>,
        offset: u64,
    ) -> Self {
        Self {
            config,
            transport,
            offset,
        }
    }
}

impl TransportAdapter for TelegramAdapter {
    fn transport(&self) -> TransportId {
        TransportId::Telegram
    }

    fn is_enabled(&self, _home: &Path) -> bool {
        self.config.enabled
    }

    fn poll_inbound(&mut self, home: &Path) -> Result<Vec<RawInbound>, String> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }
        if self.config.allowed_chat_ids.is_empty() {
            return Err("live telegram requires non-empty allowed_chat_ids (fail closed)".into());
        }
        self.offset = load_cursor(home);
        let updates = self
            .transport
            .get_updates(self.offset)
            .map_err(|e| e.to_string())?;
        let mut inbound = Vec::new();
        for update in updates {
            // Always advance the offset so a redelivered unreadable update does
            // not wedge the poll forever (legacy path contract).
            self.offset = self.offset.max(update.update_id.saturating_add(1));
            if update.text.trim().is_empty() || update.chat_id.is_empty() {
                continue;
            }
            inbound.push(RawInbound {
                from: update.chat_id.clone(),
                text: update.text,
                attachments: Vec::new(),
            });
        }
        save_cursor(home, self.offset);
        Ok(inbound)
    }

    fn is_allowed(&self, from: &str) -> bool {
        self.config.allowed_chat_ids.is_empty()
            || self.config.allowed_chat_ids.iter().any(|id| id == from)
    }

    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        self.transport
            .send_message(target, body)
            .map_err(|e| e.to_string())
    }
}

/// spec-017 adapter convention: Ok(None) when {home}/gateway/telegram.json
/// is absent, Ok(Some(adapter)) when present (disabled or not), Err on
/// malformed config.
pub fn open_adapter(home: &Path) -> Result<Option<Box<dyn TransportAdapter>>, String> {
    TelegramAdapter::open(home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::MockTelegramTransport;
    use tempfile::tempdir;

    fn test_config() -> TelegramConfig {
        TelegramConfig {
            enabled: true,
            bot_token_env: "OPTIMUS_TELEGRAM_BOT_TOKEN".into(),
            allowed_chat_ids: vec!["42".into()],
        }
    }

    #[test]
    fn polls_updates_into_raw_inbound_with_cursor_advance() {
        let dir = tempdir().unwrap();
        let mut mock = MockTelegramTransport::default();
        mock.push_text(1, "42", "hello");
        mock.push_text(2, "99", "outside allowlist");
        mock.push_text(3, "", "blank chat");
        let mut adapter = TelegramAdapter::with_transport(test_config(), Box::new(mock), 0);

        let inbound = adapter.poll_inbound(dir.path()).unwrap();
        // Allowlist filtering is the cycle's job, not the poll's: "42" and
        // "99" both come back raw; the blank-chat update is skipped at the
        // source, before the cursor moves past the messages behind it.
        assert_eq!(inbound.len(), 2);
        assert_eq!(inbound[0].from, "42");
        assert_eq!(inbound[1].from, "99");
        assert_eq!(inbound[0].from, "42");
        assert_eq!(inbound[0].text, "hello");
        // Cursor persisted past the last update seen, at the CLI's path.
        assert_eq!(load_cursor(dir.path()), 4);
        assert!(dir.path().join("gateway").join("telegram-offset").exists());
    }

    #[test]
    fn fail_closed_without_allowlist() {
        let dir = tempdir().unwrap();
        let config = TelegramConfig {
            enabled: true,
            bot_token_env: "X".into(),
            allowed_chat_ids: Vec::new(),
        };
        let mut adapter =
            TelegramAdapter::with_transport(config, Box::new(MockTelegramTransport::default()), 0);
        let error = adapter.poll_inbound(dir.path()).unwrap_err();
        assert!(error.contains("fail closed"));
    }

    #[test]
    fn allowlist_empty_accepts_any_in_mock_mode() {
        let config = TelegramConfig {
            enabled: false,
            bot_token_env: "X".into(),
            allowed_chat_ids: Vec::new(),
        };
        let adapter =
            TelegramAdapter::with_transport(config, Box::new(MockTelegramTransport::default()), 0);
        assert!(adapter.is_allowed("anyone"));
        assert!(!adapter.is_enabled(std::path::Path::new(".")));
    }
}
