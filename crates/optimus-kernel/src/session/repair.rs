//! Repairing a stored transcript on the way back in.
//!
//! Two different kinds of damage reach this path, and only one of them is about
//! this crate's own bookkeeping:
//!
//! - An effect committed and its durable link was written, but the transcript
//!   save that should have followed did not land. The link outlives the tool
//!   message, and the message is reconstructed from it so the user sees
//!   provenance rather than a silent gap.
//! - The transcript is structurally invalid: a `tool` message with no
//!   `tool_calls` parent. Compression can cut a history there, and a provider
//!   rejects the whole request when it does. That damage is stored, so it is not
//!   confined to the turn that caused it — every later turn replays it and is
//!   rejected the same way, which is what makes a session permanently unusable
//!   rather than briefly broken. See [`crate::tool_pairing`].
//!
//! Split out of `session.rs` under architectural law 21. Verbatim move: this
//! stays an inherent `SessionStore` method, so no call site changed.

use uuid::Uuid;

use crate::{Message, Result, Role, SessionStore};

impl SessionStore {
    /// Load a session and inject missing tool messages for durable effect links.
    ///
    /// When an effect commits but the transcript save fails, effect links can
    /// outlive tool messages. Reopen reconstructs a deterministic tool message
    /// from the link so users see provenance rather than a silent gap.
    ///
    /// The link-driven repair only reaches calls that committed a durable
    /// effect. A call denied by the broker never had one, and a `tool` message
    /// orphaned by compression has no link either, so this also runs the
    /// structural repair in [`crate::tool_pairing`] — without it a session that
    /// was saved invalid stays invalid, and every turn in it is rejected by the
    /// provider for as long as the session exists.
    pub fn load_repairing_effect_transcript(
        &self,
        id: Uuid,
    ) -> Result<(Vec<String>, Vec<Message>, String, usize)> {
        let (packs, mut messages, title) = self.load(id)?;
        let links = self.effect_links(id)?;
        let mut injected = 0usize;
        for link in &links {
            let present = messages.iter().any(|message| {
                message.role == Role::Tool
                    && message.tool_call_id.as_deref() == Some(link.tool_call_id.as_str())
            });
            if present {
                continue;
            }
            let content = serde_json::json!({
                "repaired": true,
                "ok": link.outcome == "succeeded",
                "data": {
                    "job": link.job_id,
                    "node_id": link.node_id,
                    "effect_attempt_id": link.effect_attempt_id,
                    "effect_hash": link.effect_hash,
                    "outcome": link.outcome,
                    "receipt_hash": link.receipt_hash,
                }
            });
            messages.push(Message {
                role: Role::Tool,
                content: content.to_string(),
                tool_call_id: Some(link.tool_call_id.clone()),
                name: None,
                reasoning_content: None,
            });
            injected += 1;
        }
        // Orphans only. A call parked on approval is an open call whose result
        // is still coming, and answering it here would both contradict a live
        // approval and duplicate the result once the user decides.
        let repaired = crate::tool_pairing::drop_orphan_results(&mut messages);
        if injected > 0 || repaired.changed() {
            self.save(id, &title, &packs, &messages)?;
        }
        Ok((packs, messages, title, injected))
    }
}
