//! Discord / Slack mock adapters (Track Z.11) — parked product residual for live.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::gateway::{enqueue, GatewayError, InboundMessage};

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("gateway: {0}")]
    Gateway(#[from] GatewayError),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, AdapterError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelInbound {
    pub channel: String,
    pub text: String,
    pub external_id: String,
}

/// Enqueue a Discord-shaped message into the durable gateway (mock path).
pub fn discord_enqueue(
    home: impl AsRef<std::path::Path>,
    text: &str,
    external_id: &str,
) -> Result<InboundMessage> {
    if text.trim().is_empty() {
        return Err(AdapterError::Msg("text required".into()));
    }
    let session = format!("discord:{external_id}");
    Ok(enqueue(home, "discord", text, "offline", Some(&session))?)
}

/// Enqueue a Slack-shaped message into the durable gateway (mock path).
pub fn slack_enqueue(
    home: impl AsRef<std::path::Path>,
    text: &str,
    external_id: &str,
) -> Result<InboundMessage> {
    if text.trim().is_empty() {
        return Err(AdapterError::Msg("text required".into()));
    }
    let session = format!("slack:{external_id}");
    Ok(enqueue(home, "slack", text, "offline", Some(&session))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::list_inbox;
    use tempfile::tempdir;

    #[test]
    fn discord_and_slack_enqueue_into_gateway() {
        let dir = tempdir().unwrap();
        let d = discord_enqueue(dir.path(), "hello discord", "ch1").unwrap();
        assert_eq!(d.channel, "discord");
        let s = slack_enqueue(dir.path(), "hello slack", "ch2").unwrap();
        assert_eq!(s.channel, "slack");
        let inbox = list_inbox(dir.path()).unwrap();
        assert_eq!(inbox.len(), 2);
    }

    #[test]
    fn enqueue_rejects_blank_text() {
        let dir = tempdir().unwrap();
        for blank in ["", "   ", "\t\n"] {
            assert!(
                matches!(
                    discord_enqueue(dir.path(), blank, "ch1"),
                    Err(AdapterError::Msg(_))
                ),
                "discord should reject {blank:?}"
            );
            assert!(
                matches!(
                    slack_enqueue(dir.path(), blank, "ch2"),
                    Err(AdapterError::Msg(_))
                ),
                "slack should reject {blank:?}"
            );
        }
        // Rejection must not leave anything in the inbox.
        assert_eq!(list_inbox(dir.path()).unwrap().len(), 0);
    }

    #[test]
    fn enqueue_sets_provider_session_ids() {
        let dir = tempdir().unwrap();
        let d = discord_enqueue(dir.path(), "hi", "ext-1").unwrap();
        assert_eq!(d.session_id.as_deref(), Some("discord:ext-1"));
        let s = slack_enqueue(dir.path(), "hi", "ext-2").unwrap();
        assert_eq!(s.session_id.as_deref(), Some("slack:ext-2"));
    }
}
