//! Spec-014 R6 pairing vertical: synthetic per-node results must reach the
//! provider.
//!
//! After a multi-node re-park, the transcript carries a tool result for
//! `{base}:node{n}` whose exact parent claim never appears in stored history
//! (the assistant only ever claimed `{base}`). Three behaviours are pinned:
//!  1. `drop_orphan_results` exempts `^<base>:node\d+$` results whose base is
//!     claimed anywhere (history-wide lookup); unclaimed bases still drop.
//!  2. `is_well_paired` stays strict, so the request gate fires the repair.
//!  3. `repair_tool_pairing` synthesizes the parent `tool_calls` claim on the
//!     outgoing copy only (stored history stays honest), and the result is a
//!     clone of the base call — never nested inside another call.

use optimus_kernel::tool_pairing::{
    drop_orphan_results, is_well_paired, repair_tool_pairing, PairingRepair,
};
use optimus_kernel::{Message, Role};
use serde_json::json;

fn system() -> Message {
    Message {
        role: Role::System,
        content: "SYS".into(),
        tool_call_id: None,
        name: None,
        reasoning_content: None,
    }
}

fn user() -> Message {
    Message {
        role: Role::User,
        content: "ask".into(),
        tool_call_id: None,
        name: None,
        reasoning_content: None,
    }
}

/// Assistant message claiming exactly `ids`, in order, as `write_file` calls.
fn calls(ids: &[&str]) -> Message {
    let calls: Vec<_> = ids
        .iter()
        .map(|id| json!({"id": id, "name": "write_file", "arguments": {"path": "src/proof.txt"}}))
        .collect();
    Message {
        role: Role::Assistant,
        content: serde_json::to_string(&calls).unwrap(),
        tool_call_id: None,
        name: None,
        reasoning_content: None,
    }
}

fn result(id: &str) -> Message {
    Message {
        role: Role::Tool,
        content: json!({"ok": true}).to_string(),
        tool_call_id: Some(id.into()),
        name: Some("write_file".into()),
        reasoning_content: None,
    }
}

fn proof_transcript() -> Vec<Message> {
    vec![
        system(),
        user(),
        calls(&["write-1"]),
        result("write-1"),
        // The second node's result, whose exact claim never exists in history.
        result("write-1:node1"),
    ]
}

#[test]
fn drop_orphan_results_keeps_node_results_whose_base_is_claimed() {
    let mut transcript = proof_transcript();
    let repair = drop_orphan_results(&mut transcript);
    // The synthetic per-node result survives because `write-1` is claimed
    // somewhere in history — even though `write-1:node1` itself is not.
    assert_eq!(repair.dropped_orphan_results, 0);
    assert_eq!(transcript.len(), 5);
    assert!(transcript
        .iter()
        .any(|m| m.tool_call_id.as_deref() == Some("write-1:node1")));
}

#[test]
fn drop_orphan_results_still_drops_node_results_with_unclaimed_bases() {
    let mut transcript = vec![
        system(),
        user(),
        calls(&["write-1"]),
        result("write-1"),
        // A node result whose base IS claimed: survives.
        result("write-1:node1"),
        // A node result whose base was never claimed anywhere: still an orphan.
        result("ghost:node1"),
    ];
    let repair = drop_orphan_results(&mut transcript);
    assert_eq!(repair.dropped_orphan_results, 1);
    assert!(!transcript
        .iter()
        .any(|m| m.tool_call_id.as_deref() == Some("ghost:node1")));
    // The legitimately-based node result survives.
    assert!(transcript
        .iter()
        .any(|m| m.tool_call_id.as_deref() == Some("write-1:node1")));
}

#[test]
fn is_well_paired_stays_strict_for_unclaimed_node_results() {
    // The exemption lives in drop_orphan_results ONLY; the strict pairing gate
    // must still fail so the request gate fires the repair.
    let transcript = proof_transcript();
    assert!(!is_well_paired(&transcript));
}

#[test]
fn repair_synthesizes_the_parent_claim_on_the_outgoing_copy() {
    let mut outgoing = proof_transcript();
    let repair: PairingRepair = repair_tool_pairing(&mut outgoing);
    assert!(repair.changed());
    assert!(repair.synthesized_claims > 0);
    assert!(
        is_well_paired(&outgoing),
        "the outgoing copy must be answerable"
    );

    // The parent claim is a CLONE of the base call with the node id — the same
    // name and arguments, a sibling entry (never nested inside another call).
    let assistant = outgoing
        .iter()
        .find(|m| m.role == Role::Assistant)
        .expect("assistant message");
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&assistant.content).unwrap();
    assert_eq!(parsed.len(), 2, "base call + synthesized node claim");
    let base = &parsed[0];
    let claim = &parsed[1];
    assert_eq!(base["id"], "write-1");
    assert_eq!(claim["id"], "write-1:node1");
    assert_eq!(claim["name"], base["name"]);
    assert_eq!(claim["arguments"], base["arguments"]);
}

#[test]
fn repair_keeps_stored_history_honest() {
    // drop_orphan_results (the stored-history surface) keeps the node result;
    // it must NOT fabricate the parent claim. Only the outgoing repair does.
    let mut stored = proof_transcript();
    drop_orphan_results(&mut stored);
    let assistant = stored
        .iter()
        .find(|m| m.role == Role::Assistant)
        .expect("assistant message");
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&assistant.content).unwrap();
    assert_eq!(parsed.len(), 1, "stored history stays exactly as written");
    assert_eq!(parsed[0]["id"], "write-1");
    assert!(!is_well_paired(&stored));
}
