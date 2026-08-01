//! What a remote sender's address means, proven from outside the crate.
//!
//! ADR-0071 split one overloaded field into two ideas. `InboundMessage.session_id`
//! is a **routing address** — `<channel>:<address>`, written by the adapter that
//! knows how to read it back — and the kernel session is *derived* from it, never
//! parsed out of it. Before that split, both gateway turns ran
//! `Uuid::parse_str(session_id)` and then answered with the kernel's own session
//! id as the reply target. A Telegram message therefore either dead-lettered
//! (host: parse error) or silently lost its chat (CLI: `.ok()`), and the durable
//! obligation was addressed to a UUID no adapter can deliver to.
//!
//! These tests run the real `optimus_host::drain_gateway_once` against a real
//! home on disk, so they fail if either half of that split regresses: an address
//! that no longer survives to the outbox, or a conversation that no longer lands
//! on the same session twice.

use std::path::Path;

use optimus_host::{drain_gateway_once, session_for_address};
use optimus_kernel::{enqueue, list_inbox, list_outbox, Role, SessionStore};
use tempfile::tempdir;

/// Enqueue one message from `address` and take it through the host gateway turn.
///
/// `offline` keeps the route deterministic: a fresh home resolves it to the
/// scripted echo model, so these tests assert routing and identity without a
/// network or a key.
fn turn(home: &Path, address: Option<&str>, text: &str) -> optimus_kernel::DrainResult {
    enqueue(home, "telegram", text, "offline", address).expect("enqueue");
    drain_gateway_once(&home.to_path_buf())
        .expect("the gateway turn must not fail")
        .expect("a pending message was available")
}

/// Every message the store holds for one derived session, oldest first.
fn transcript(home: &Path, address: &str) -> Vec<String> {
    let store = SessionStore::open(home.join("sessions.db")).expect("sessions");
    let (_packs, messages, _title) = store
        .load(session_for_address(address))
        .expect("the derived session exists");
    messages
        .into_iter()
        .filter(|message| message.role == Role::User)
        .map(|message| message.content)
        .collect()
}

#[test]
fn a_channel_address_answers_instead_of_dead_lettering() {
    let home = tempdir().unwrap();
    let drained = turn(home.path(), Some("telegram:9001"), "are you there");

    // The regression in one line: `telegram:9001` is not a UUID, and that used to
    // be fatal.
    assert_eq!(
        drained.status, "ok",
        "a channel address must be routable, not a malformed session id"
    );
    assert!(
        list_inbox(home.path()).unwrap().is_empty(),
        "a terminal turn leaves nothing pending (law 10)"
    );
}

#[test]
fn the_reply_is_addressed_to_the_sender_not_to_the_session() {
    let home = tempdir().unwrap();
    turn(home.path(), Some("telegram:9001"), "are you there");

    let outbox = list_outbox(home.path(), 10).unwrap();
    assert_eq!(outbox.len(), 1);
    assert_eq!(
        outbox[0].session_id.as_deref(),
        Some("telegram:9001"),
        "the obligation's target is the address that wrote in; answering with the \
         kernel session id would owe a send to nobody"
    );
    let derived = session_for_address("telegram:9001").to_string();
    assert_ne!(
        outbox[0].session_id.as_deref(),
        Some(derived.as_str()),
        "the derived session must never leak back out as a routing address"
    );
}

#[test]
fn the_same_address_keeps_the_same_conversation() {
    let home = tempdir().unwrap();
    turn(home.path(), Some("telegram:9001"), "first");
    turn(home.path(), Some("telegram:9001"), "second");

    let said = transcript(home.path(), "telegram:9001");
    assert!(
        said.iter().any(|line| line == "first") && said.iter().any(|line| line == "second"),
        "a returning sender continues one thread, got {said:?}"
    );
}

#[test]
fn two_addresses_never_share_a_conversation() {
    let home = tempdir().unwrap();
    turn(home.path(), Some("telegram:9001"), "mine");
    turn(home.path(), Some("telegram:9002"), "theirs");

    assert_ne!(
        session_for_address("telegram:9001"),
        session_for_address("telegram:9002")
    );
    let first = transcript(home.path(), "telegram:9001");
    let second = transcript(home.path(), "telegram:9002");
    assert!(
        first.iter().any(|line| line == "mine") && !first.iter().any(|line| line == "theirs"),
        "one chat must not read another's messages, got {first:?}"
    );
    assert!(
        second.iter().any(|line| line == "theirs") && !second.iter().any(|line| line == "mine"),
        "one chat must not read another's messages, got {second:?}"
    );
}

#[test]
fn an_address_survives_a_process_boundary() {
    let home = tempdir().unwrap();
    turn(home.path(), Some("telegram:9001"), "first");
    // Every public entry point reopens its state from disk, so a second call is
    // what a second process looks like. The address→session mapping is derived,
    // not remembered, so nothing has to be carried across.
    turn(home.path(), Some("telegram:9001"), "second");

    let outbox = list_outbox(home.path(), 10).unwrap();
    assert_eq!(outbox.len(), 2);
    assert!(
        outbox
            .iter()
            .all(|row| row.session_id.as_deref() == Some("telegram:9001")),
        "both replies are owed to the same chat"
    );
}

#[test]
fn a_message_with_no_address_answers_but_owes_no_send() {
    let home = tempdir().unwrap();
    let drained = turn(home.path(), None, "from a surface with no return path");

    assert_eq!(drained.status, "ok");
    let outbox = list_outbox(home.path(), 10).unwrap();
    assert_eq!(outbox.len(), 1);
    assert!(
        outbox[0].session_id.is_none(),
        "an absent address must stay absent rather than being backfilled with a \
         session id that no channel could deliver to"
    );
}
