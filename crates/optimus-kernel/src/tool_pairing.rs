//! The one structural rule every OpenAI-compatible provider enforces on history.
//!
//! A `tool` message is only meaningful as the answer to a specific `tool_calls`
//! entry on the assistant message before it. Send one without its parent and the
//! provider rejects the whole request — DeepSeek with a bare
//! `status code 400`, which says nothing about which message was wrong.
//!
//! Two things in this crate can break that pairing, and neither is a rare edge:
//!
//! 1. [`crate::compress`] drops a fixed-size middle of the history. The cut is
//!    positional, so it can land between an assistant message carrying
//!    `tool_calls` and the `tool` messages answering it — summarising the parent
//!    and keeping the orphans.
//! 2. [`crate::turn_loop`] pushes one assistant message carrying *all* of a
//!    step's tool calls, then pushes results one at a time. Any early return
//!    between those points — an approval park, a cancellation, a control-plane
//!    error — persists calls whose results never arrive.
//!
//! Both produce a transcript that is saved to disk, so the damage is not
//! confined to the turn that caused it: every later turn reloads the same
//! invalid history and fails the same way. Observed live — a session became
//! permanently unusable, every message answered with `status code 400`.
//!
//! So the repair runs on the way out of compression *and* on the way in from
//! storage, and it is written to be safe to run on anything: it is idempotent,
//! and a transcript that is already valid is left byte-for-byte alone.

use crate::{Message, Role, ToolCall};

/// What a repair pass changed, for callers that report or test it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PairingRepair {
    /// `tool` messages dropped because their `tool_calls` parent was gone.
    pub dropped_orphan_results: usize,
    /// Missing `tool` results synthesised so a surviving parent stays answered.
    pub synthesized_results: usize,
}

impl PairingRepair {
    pub fn changed(self) -> bool {
        self.dropped_orphan_results > 0 || self.synthesized_results > 0
    }
}

/// The tool-call ids an assistant message is waiting on, if it carries any.
///
/// The assistant tool-call message is stored as a JSON array of [`ToolCall`] in
/// `content` and only expanded to `tool_calls` at the provider boundary, so the
/// same parse that the adapter does has to happen here. The `id` check matches
/// the adapter's: an entry without one is not expanded, so it is not a call.
pub fn assistant_call_ids(message: &Message) -> Option<Vec<String>> {
    if message.role != Role::Assistant {
        return None;
    }
    let calls: Vec<ToolCall> = serde_json::from_str(&message.content).ok()?;
    if calls.is_empty() || calls.iter().any(|call| call.id.is_empty()) {
        return None;
    }
    Some(calls.into_iter().map(|call| call.id).collect())
}

/// Whether `messages` is a sequence a provider will accept.
///
/// Cheap enough to assert before every request; used by tests to state the
/// invariant rather than re-deriving it.
pub fn is_well_paired(messages: &[Message]) -> bool {
    let mut awaiting: Vec<String> = Vec::new();
    for message in messages {
        if let Some(ids) = assistant_call_ids(message) {
            if !awaiting.is_empty() {
                return false;
            }
            awaiting = ids;
            continue;
        }
        if message.role == Role::Tool {
            let Some(id) = message.tool_call_id.as_deref() else {
                return false;
            };
            let Some(at) = awaiting.iter().position(|open| open == id) else {
                return false;
            };
            awaiting.remove(at);
            continue;
        }
        if !awaiting.is_empty() {
            return false;
        }
    }
    awaiting.is_empty()
}

/// Drop `tool` messages whose `tool_calls` parent is not in the history.
///
/// Safe on stored history, and the repair the live failure needs: the parent is
/// gone, so nothing can make the result meaningful, and keeping it is exactly
/// what the provider rejects.
///
/// Deliberately *not* paired with synthesis here. An unanswered call is not
/// always an abandoned one — a call parked on approval is waiting for the user,
/// and its result arrives when they answer. Writing "did not complete" into
/// stored history for that call both lies about a live approval and leaves a
/// duplicate behind once the real result lands. Synthesis belongs on the
/// outgoing request only, where nothing is persisted; see
/// [`repair_tool_pairing`].
pub fn drop_orphan_results(messages: &mut Vec<Message>) -> PairingRepair {
    // Every id an assistant message actually asked for. A result is an orphan
    // when no parent anywhere in the history claims its id, which is stricter
    // than "the parent is earlier" on purpose — a result that precedes its own
    // parent is just as invalid, and just as much a compression artifact.
    let mut claimed: Vec<String> = Vec::new();
    for message in messages.iter() {
        if let Some(ids) = assistant_call_ids(message) {
            claimed.extend(ids);
        }
    }

    let before = messages.len();
    messages.retain(|message| {
        if message.role != Role::Tool {
            return true;
        }
        message
            .tool_call_id
            .as_deref()
            .is_some_and(|id| claimed.iter().any(|open| open == id))
    });
    PairingRepair {
        dropped_orphan_results: before - messages.len(),
        synthesized_results: 0,
    }
}

/// Make `messages` a sequence a provider will accept, in place.
///
/// [`drop_orphan_results`], then an answer for every call still open: a
/// `tool_calls` entry with no result gets a **synthesised** one saying the call
/// did not complete. Dropping the parent instead would be the larger lie — the
/// assistant did ask for that call, and a transcript that hides it invites the
/// model to repeat work it already started.
///
/// For the **outgoing request copy only**. Synthesis is correct there because a
/// request must be answerable now, and wrong on stored history because a parked
/// approval is an open call that is still going to be answered.
pub fn repair_tool_pairing(messages: &mut Vec<Message>) -> PairingRepair {
    let mut repair = drop_orphan_results(messages);

    // Answer whatever is still open. Walking backwards keeps the insertion index
    // of every earlier group correct as results are spliced in.
    let mut groups: Vec<(usize, Vec<String>)> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if let Some(ids) = assistant_call_ids(message) {
            groups.push((index, ids));
        }
    }
    for (parent, ids) in groups.into_iter().rev() {
        // The group runs to the next non-`tool` message; results are only ever
        // written directly after their parent.
        let mut end = parent + 1;
        while end < messages.len() && messages[end].role == Role::Tool {
            end += 1;
        }
        let answered: Vec<String> = messages[parent + 1..end]
            .iter()
            .filter_map(|message| message.tool_call_id.clone())
            .collect();
        let missing: Vec<String> = ids
            .into_iter()
            .filter(|id| !answered.iter().any(|done| done == id))
            .collect();
        for (offset, id) in missing.into_iter().enumerate() {
            messages.insert(end + offset, unanswered_result(&id));
            repair.synthesized_results += 1;
        }
    }

    repair
}

/// What an unanswered call looks like once the turn that made it is over.
///
/// Shaped like a real failed tool outcome so the model reads it the way it reads
/// any other failure, and marked `repaired` so a transcript reader can tell a
/// call that reported failure from one this module answered on its behalf.
fn unanswered_result(tool_call_id: &str) -> Message {
    let content = serde_json::json!({
        "ok": false,
        "repaired": true,
        "error": {
            "code": "tool_call_not_completed",
            "message": "This tool call did not produce a result before the turn ended. \
                        Treat it as not run and call the tool again if the work is still needed.",
            "retryable": true,
        },
    });
    Message {
        role: Role::Tool,
        content: content.to_string(),
        tool_call_id: Some(tool_call_id.to_string()),
        name: None,
        reasoning_content: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }
    }

    fn calls(ids: &[&str]) -> Message {
        let calls: Vec<_> = ids
            .iter()
            .map(|id| json!({"id": id, "name": "terminal", "arguments": {}}))
            .collect();
        msg(Role::Assistant, &serde_json::to_string(&calls).unwrap())
    }

    fn result(id: &str) -> Message {
        Message {
            role: Role::Tool,
            content: json!({"ok": true}).to_string(),
            tool_call_id: Some(id.into()),
            name: Some("terminal".into()),
            reasoning_content: None,
        }
    }

    #[test]
    fn a_valid_transcript_is_left_exactly_alone() {
        let original = vec![
            msg(Role::System, "SYS"),
            msg(Role::User, "ask"),
            calls(&["a", "b"]),
            result("a"),
            result("b"),
            msg(Role::Assistant, "done"),
        ];
        let mut messages = original.clone();
        let repair = repair_tool_pairing(&mut messages);
        assert!(!repair.changed());
        assert_eq!(messages, original);
        assert!(is_well_paired(&messages));
    }

    #[test]
    fn a_result_whose_parent_was_compressed_away_is_dropped() {
        // The live failure, reduced: compression summarised the assistant message
        // carrying `tool_calls` and kept the tail it opened, so the transcript
        // began with a `tool` message answering a call the provider never saw.
        // Every turn afterwards was rejected `status code 400`.
        let mut messages = vec![
            msg(Role::System, "SYS"),
            msg(Role::User, "[context_compressed 101 messages]"),
            result("call_05_SIGg9EpdLQRluVQTtzbT3018"),
            msg(Role::User, "what's this session id"),
        ];
        assert!(!is_well_paired(&messages));

        let repair = repair_tool_pairing(&mut messages);
        assert_eq!(repair.dropped_orphan_results, 1);
        assert_eq!(repair.synthesized_results, 0);
        assert!(is_well_paired(&messages));
        assert!(!messages.iter().any(|m| m.role == Role::Tool));
    }

    #[test]
    fn a_call_the_turn_never_answered_is_answered_as_not_run() {
        // `turn_loop` writes one assistant message for every call in a step and
        // the results one at a time, so an approval park or a cancellation
        // between them leaves calls open. The parent stays: the model asked.
        let mut messages = vec![msg(Role::User, "ask"), calls(&["a", "b", "c"]), result("a")];
        let repair = repair_tool_pairing(&mut messages);

        assert_eq!(repair.synthesized_results, 2);
        assert!(is_well_paired(&messages));
        assert_eq!(messages.len(), 5);
        assert!(assistant_call_ids(&messages[1]).is_some());
        let ids: Vec<_> = messages[2..]
            .iter()
            .map(|m| m.tool_call_id.clone().unwrap())
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
        assert!(messages[3].content.contains("tool_call_not_completed"));
    }

    #[test]
    fn an_earlier_group_keeps_its_position_when_a_later_one_is_repaired() {
        // Synthesised results are spliced in, so repairing back-to-front is what
        // keeps the earlier group's index from sliding out from under it.
        let mut messages = vec![
            calls(&["a"]),
            result("a"),
            msg(Role::Assistant, "middle"),
            calls(&["b", "c"]),
            result("b"),
        ];
        let repair = repair_tool_pairing(&mut messages);

        assert_eq!(repair.synthesized_results, 1);
        assert!(is_well_paired(&messages));
        assert_eq!(messages[1].tool_call_id.as_deref(), Some("a"));
        assert_eq!(messages[2].content, "middle");
        assert_eq!(messages[5].tool_call_id.as_deref(), Some("c"));
    }

    #[test]
    fn stored_history_never_answers_a_call_that_is_parked_on_an_approval() {
        // Caught by `project_write_emits_exact_approval_lifecycle_before_any_effect`
        // while this module was being written. A turn that parks on SmartDeny
        // saves an assistant `tool_calls` message with no result yet — that is
        // the approval, not damage. Synthesising "did not complete" for it wrote
        // a second result the moment the user approved, and the model was handed
        // the invented failure instead of the effect it authorised.
        let mut messages = vec![msg(Role::User, "write the proof"), calls(&["write-1"])];
        let parked = messages.clone();

        let repair = drop_orphan_results(&mut messages);
        assert!(!repair.changed(), "a parked approval is not damage");
        assert_eq!(messages, parked);

        // The user approves and the real result lands, still the only one.
        messages.push(result("write-1"));
        assert!(!drop_orphan_results(&mut messages).changed());
        assert!(is_well_paired(&messages));
        assert_eq!(
            messages.iter().filter(|m| m.role == Role::Tool).count(),
            1,
            "the approved result must not be shadowed by an invented one"
        );
    }

    #[test]
    fn the_outgoing_request_still_answers_every_open_call() {
        // The request copy is a different contract: it is never stored, and a
        // provider will not accept it with a call left open.
        let mut messages = vec![msg(Role::User, "ask"), calls(&["a"])];
        assert!(!is_well_paired(&messages));
        assert_eq!(repair_tool_pairing(&mut messages).synthesized_results, 1);
        assert!(is_well_paired(&messages));
    }

    #[test]
    fn repairing_twice_changes_nothing_the_second_time() {
        // This runs on every compression pass and every session load. A repair
        // that kept finding work would rewrite stored history forever.
        let mut messages = vec![
            result("orphan"),
            calls(&["a", "b"]),
            result("a"),
            msg(Role::User, "next"),
        ];
        assert!(repair_tool_pairing(&mut messages).changed());
        let settled = messages.clone();

        assert!(!repair_tool_pairing(&mut messages).changed());
        assert_eq!(messages, settled);
        assert!(is_well_paired(&messages));
    }

    #[test]
    fn plain_assistant_prose_is_never_mistaken_for_a_tool_call() {
        // `assistant_call_ids` parses content as JSON. An assistant message that
        // happens to be a JSON array must not be read as calls, or the repair
        // would invent results for a message that asked for nothing.
        for content in ["[]", r#"["a","b"]"#, r#"[{"name":"terminal"}]"#, "not json"] {
            let message = msg(Role::Assistant, content);
            assert!(
                assistant_call_ids(&message).is_none(),
                "must not read {content} as tool calls"
            );
        }
        let mut messages = vec![msg(Role::Assistant, r#"["a","b"]"#)];
        assert!(!repair_tool_pairing(&mut messages).changed());
    }

    #[test]
    fn a_result_without_any_call_id_cannot_be_paired_and_goes() {
        let mut messages = vec![calls(&["a"]), result("a"), msg(Role::Tool, "stray")];
        let repair = repair_tool_pairing(&mut messages);
        assert_eq!(repair.dropped_orphan_results, 1);
        assert!(is_well_paired(&messages));
    }
}
