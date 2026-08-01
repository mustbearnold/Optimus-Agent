//! The durable delivery spine, exercised the way a consumer outside the crate
//! sees it: only `optimus_ops::` re-exports, never a crate internal.
//!
//! The inline `#[cfg(test)]` suites reach private helpers and can set up state
//! by writing rows directly. These cannot, and that restriction is the point.
//! Every public entry point reopens the database from its path, so a sequence of
//! public calls is what a sequence of process lifetimes looks like — a crash
//! between two of these calls is exactly the hole ADR-0070 closes, and a test
//! written only against the public surface walks through that hole on every
//! line.
//!
//! What is proven here: a terminal turn and the send it owes commit together, an
//! unknown outcome never retries on its own, a definite refusal retries to a
//! bound, and the turn-level receipt columns stay a truthful projection of
//! ledger state for surfaces that never heard of the ledger.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use optimus_ops::{
    claim_outbound, delivery_state, drain_one, enqueue, gateway_status, list_ambiguous_obligations,
    list_ambiguous_sends, list_inbox, list_outbox_receipts, list_pending_obligations,
    list_unsettled_obligations, outbound_ledger_status, reconcile, resolve_ambiguous_obligation,
    settle_outbound, sweep_stale_sends, AmbiguityResolution, OutboundSettlement,
};
use tempfile::tempdir;
use uuid::Uuid;

/// A test clock anchored to the real one.
///
/// `outbound_ledger_status` expires stale leases against `SystemTime`, so a
/// lease minted at an arbitrary fixed epoch would look decades dead to it and
/// every `sending` row would silently turn `ambiguous` mid-test.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_secs()
}

/// Enqueue one message and take it through a turn that succeeds, returning the
/// inbound id the obligation will point back at.
fn succeeded_turn(home: &Path, channel: &str, session: Option<&str>, reply: &str) -> String {
    let inbound = enqueue(home, channel, "ping", "offline", session).expect("enqueue");
    let session = session.map(str::to_string);
    let drained = drain_one(home, |_message| Ok((reply.to_string(), session.clone())))
        .expect("drain")
        .expect("a pending message was available");
    assert_eq!(drained.id, inbound.id);
    assert_eq!(drained.status, "ok");
    inbound.id
}

#[test]
fn a_terminal_turn_and_the_send_it_owes_commit_together() {
    let home = tempdir().unwrap();
    let inbound = enqueue(home.path(), "cli", "ping", "offline", Some("cli:operator")).unwrap();
    assert_eq!(list_inbox(home.path()).unwrap().len(), 1);

    let drained = drain_one(home.path(), |message| {
        Ok((format!("pong:{}", message.text), message.session_id.clone()))
    })
    .unwrap()
    .expect("a pending message was available");
    assert_eq!(drained.id, inbound.id);

    // The turn is terminal and the debt is already recorded. There is no
    // intermediate observation in which one is true and the other is not.
    let owed = list_pending_obligations(home.path(), 10).unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].message_id, inbound.id);
    assert_eq!(owed[0].channel, "cli");
    assert_eq!(owed[0].target, "cli:operator");
    assert_eq!(owed[0].body, "pong:ping");
    assert_eq!(owed[0].attempts, 0, "nothing has been attempted yet");

    // Owed-with-a-known-position is not the same as unanswerable, so the
    // operator's ambiguous pile stays empty while the send is merely pending.
    assert!(list_ambiguous_sends(home.path(), 10).unwrap().is_empty());

    let base = now();
    let claim = claim_outbound(home.path(), Some("cli"), Uuid::new_v4(), base, 60)
        .unwrap()
        .expect("the owed send is claimable");
    assert_eq!(
        claim.obligation().attempts,
        1,
        "the attempt is recorded before the network is touched"
    );
    assert_eq!(claim.obligation().body, "pong:ping");

    let settled = settle_outbound(
        home.path(),
        &claim,
        OutboundSettlement::Delivered {
            provider_message_id: "srv-1".into(),
        },
        base + 1,
    )
    .unwrap();
    assert_eq!(settled.status, "delivered");
    assert_eq!(settled.provider_message_id.as_deref(), Some("srv-1"));

    // Surfaces that predate the ledger read `gateway_messages` and must still
    // see the truth without knowing the ledger exists.
    let (reason, delivered_unix) = delivery_state(home.path(), &inbound.id).unwrap().unwrap();
    assert_eq!(delivered_unix, Some(base + 1));
    assert_eq!(reason, None);

    let receipts = list_outbox_receipts(home.path(), 10).unwrap();
    assert_eq!(receipts.len(), 1);
    assert!(!receipts[0].ambiguous_send);
    assert_eq!(receipts[0].terminal_status, "succeeded");

    let ledger = outbound_ledger_status(home.path()).unwrap();
    assert_eq!(ledger.delivered, 1);
    assert_eq!(ledger.pending, 0);
    assert!(list_unsettled_obligations(home.path(), 10)
        .unwrap()
        .is_empty());

    let gateway = gateway_status(home.path()).unwrap();
    assert_eq!(gateway.inbox_pending, 0);
    assert_eq!(gateway.outbox_total, 1);
    assert_eq!(gateway.ambiguous_sends, 0);
}

/// Regression for the truncation defect ADR-0070 found: the adapter used to send
/// `DrainResult::reply_preview`, a 200-character display field, so every longer
/// reply arrived cut in half while the gateway recorded a delivery receipt for it.
#[test]
fn a_reply_longer_than_the_display_preview_is_owed_whole() {
    let home = tempdir().unwrap();
    let long_reply = "x".repeat(640);

    let inbound = enqueue(home.path(), "cli", "ping", "offline", Some("cli:1")).unwrap();
    let drained = drain_one(home.path(), |message| {
        Ok((long_reply.clone(), message.session_id.clone()))
    })
    .unwrap()
    .unwrap();

    assert_eq!(
        drained.reply_preview.chars().count(),
        200,
        "the preview is still a preview"
    );

    let owed = list_pending_obligations(home.path(), 10).unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].message_id, inbound.id);
    assert_eq!(
        owed[0].body.chars().count(),
        640,
        "the obligation carries the reply, not the display truncation"
    );
}

#[test]
fn a_failed_turn_owes_no_send() {
    let home = tempdir().unwrap();
    enqueue(home.path(), "cli", "ping", "offline", Some("cli:1")).unwrap();

    // A turn that fails is retried to the gateway's own bound and then dead
    // lettered. No point along that path may put a debt on the ledger: someone
    // is owed a reply only once a turn actually produced one, and a debt minted
    // on the way to a dead letter is a message with no body to send.
    let mut outcomes = Vec::new();
    for _ in 0..3 {
        let drained = drain_one(home.path(), |_message| Err("model_unavailable".into()))
            .unwrap()
            .expect("a retryable failure returns the message to the inbox");
        outcomes.push(drained.status);
        assert!(list_unsettled_obligations(home.path(), 10)
            .unwrap()
            .is_empty());
    }
    assert_eq!(outcomes[0], "retry_scheduled:model_unavailable");
    assert_eq!(outcomes[2], "dead_lettered:model_unavailable");

    assert!(
        drain_one(home.path(), |_message| Ok(("unreachable".into(), None)))
            .unwrap()
            .is_none(),
        "a dead lettered message is terminal, not handed out a fourth time"
    );
    assert_eq!(outbound_ledger_status(home.path()).unwrap().pending, 0);
    assert!(
        list_ambiguous_sends(home.path(), 10).unwrap().is_empty(),
        "a failed turn is not an unanswered question"
    );
}

#[test]
fn a_turn_with_no_routing_address_owes_no_send() {
    let home = tempdir().unwrap();
    enqueue(home.path(), "cli", "ping", "offline", None).unwrap();

    drain_one(home.path(), |_message| Ok(("pong".into(), None)))
        .unwrap()
        .unwrap();

    assert!(
        list_unsettled_obligations(home.path(), 10)
            .unwrap()
            .is_empty(),
        "no external party can be owed a reply with nowhere to send it"
    );
}

#[test]
fn an_unknown_outcome_is_never_retried_without_an_operator() {
    let home = tempdir().unwrap();
    let message_id = succeeded_turn(home.path(), "cli", Some("cli:1"), "pong");
    let base = now();

    let claim = claim_outbound(home.path(), None, Uuid::new_v4(), base, 60)
        .unwrap()
        .unwrap();
    settle_outbound(
        home.path(),
        &claim,
        OutboundSettlement::Ambiguous {
            detail: "connection reset".into(),
        },
        base + 1,
    )
    .unwrap();

    assert!(list_pending_obligations(home.path(), 10)
        .unwrap()
        .is_empty());
    let stuck = list_ambiguous_obligations(home.path(), 10).unwrap();
    assert_eq!(stuck.len(), 1);
    assert_eq!(stuck[0].message_id, message_id);
    assert_eq!(stuck[0].last_detail.as_deref(), Some("connection reset"));

    // The whole point: no automatic path moves it. A second cycle finds nothing
    // to do rather than producing a probable duplicate.
    assert!(
        claim_outbound(home.path(), None, Uuid::new_v4(), base + 2, 60)
            .unwrap()
            .is_none(),
        "an ambiguous obligation is not claimable"
    );
    assert_eq!(sweep_stale_sends(home.path(), base + 10_000).unwrap(), 0);
    assert_eq!(
        list_ambiguous_obligations(home.path(), 10).unwrap().len(),
        1
    );

    // The coarse per-turn view and the ledger agree that a human is owed a
    // question here; only the ledger can say which question.
    let coarse = list_ambiguous_sends(home.path(), 10).unwrap();
    assert_eq!(coarse.len(), 1);
    assert_eq!(coarse[0].message_id, message_id);
}

#[test]
fn a_lease_that_dies_mid_flight_becomes_ambiguous_not_owed() {
    let home = tempdir().unwrap();
    succeeded_turn(home.path(), "cli", Some("cli:1"), "pong");
    let base = now();

    // Claimed, attempt recorded, then the process disappears.
    let claim = claim_outbound(home.path(), None, Uuid::new_v4(), base, 1)
        .unwrap()
        .unwrap();
    drop(claim);

    assert_eq!(sweep_stale_sends(home.path(), base + 30).unwrap(), 1);
    assert!(
        list_pending_obligations(home.path(), 10)
            .unwrap()
            .is_empty(),
        "attempted-outcome-unknown must never look like never-attempted"
    );
    assert_eq!(
        list_ambiguous_obligations(home.path(), 10).unwrap().len(),
        1
    );
}

#[test]
fn an_operator_resolution_is_the_only_way_out_of_ambiguity() {
    let home = tempdir().unwrap();
    let message_id = succeeded_turn(home.path(), "cli", Some("cli:1"), "pong");
    let base = now();

    let claim = claim_outbound(home.path(), None, Uuid::new_v4(), base, 60)
        .unwrap()
        .unwrap();
    settle_outbound(
        home.path(),
        &claim,
        OutboundSettlement::Ambiguous {
            detail: "timeout".into(),
        },
        base + 1,
    )
    .unwrap();
    let obligation_id = list_ambiguous_obligations(home.path(), 10).unwrap()[0]
        .obligation_id
        .clone();

    // Checked the channel: it never arrived. Retrying is safe now, and only now.
    let resolved = resolve_ambiguous_obligation(
        home.path(),
        &obligation_id,
        AmbiguityResolution::NotDelivered,
        base + 2,
    )
    .unwrap()
    .expect("the obligation was ambiguous");
    assert_eq!(resolved.status, "pending");

    let owed = list_pending_obligations(home.path(), 10).unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].message_id, message_id);
    assert!(
        claim_outbound(home.path(), None, Uuid::new_v4(), base + 3, 60)
            .unwrap()
            .is_some(),
        "a human decision returned it to the owed pool"
    );

    // Resolving a second time finds nothing ambiguous to resolve.
    assert!(resolve_ambiguous_obligation(
        home.path(),
        &obligation_id,
        AmbiguityResolution::NotDelivered,
        base + 4,
    )
    .unwrap()
    .is_none());
}

#[test]
fn resolving_ambiguity_as_delivered_writes_the_turn_receipt() {
    let home = tempdir().unwrap();
    let message_id = succeeded_turn(home.path(), "cli", Some("cli:1"), "pong");
    let base = now();

    let claim = claim_outbound(home.path(), None, Uuid::new_v4(), base, 60)
        .unwrap()
        .unwrap();
    settle_outbound(
        home.path(),
        &claim,
        OutboundSettlement::Ambiguous {
            detail: "timeout".into(),
        },
        base + 1,
    )
    .unwrap();
    let obligation_id = list_ambiguous_obligations(home.path(), 10).unwrap()[0]
        .obligation_id
        .clone();

    resolve_ambiguous_obligation(
        home.path(),
        &obligation_id,
        AmbiguityResolution::Delivered {
            provider_message_id: "srv-late".into(),
        },
        base + 2,
    )
    .unwrap()
    .unwrap();

    let (_, delivered_unix) = delivery_state(home.path(), &message_id).unwrap().unwrap();
    assert_eq!(delivered_unix, Some(base + 2));
    assert!(list_ambiguous_sends(home.path(), 10).unwrap().is_empty());
    assert_eq!(outbound_ledger_status(home.path()).unwrap().delivered, 1);
}

#[test]
fn a_definite_refusal_retries_to_a_bound_and_then_stops() {
    let home = tempdir().unwrap();
    let message_id = succeeded_turn(home.path(), "cli", Some("cli:1"), "pong");
    let base = now();

    // Retrying is safe here only because the platform refused each time. The
    // bound is read off the ledger rather than asserted from a private constant,
    // so this test states the contract an adapter author can rely on.
    let mut attempts = 0;
    while let Some(claim) =
        claim_outbound(home.path(), None, Uuid::new_v4(), base + attempts, 60).unwrap()
    {
        attempts += 1;
        assert!(attempts <= 16, "the retry bound never took effect");
        settle_outbound(
            home.path(),
            &claim,
            OutboundSettlement::Failed {
                detail: "chat_not_found".into(),
            },
            base + attempts,
        )
        .unwrap();
    }

    assert_eq!(
        attempts, 5,
        "five attempts, then the obligation is abandoned"
    );
    let ledger = outbound_ledger_status(home.path()).unwrap();
    assert_eq!(ledger.abandoned, 1);
    assert_eq!(ledger.pending, 0);
    assert_eq!(ledger.ambiguous, 0);

    let (reason, delivered_unix) = delivery_state(home.path(), &message_id).unwrap().unwrap();
    assert_eq!(delivered_unix, None);
    assert!(
        reason
            .as_deref()
            .is_some_and(|value| value.starts_with("external_send_failed")),
        "a refused send is a definite failure, not an open question: {reason:?}"
    );
    assert!(
        list_ambiguous_sends(home.path(), 10).unwrap().is_empty(),
        "an abandoned send has an answer, so it leaves the operator's pile"
    );
}

#[test]
fn each_turn_owes_exactly_one_send_and_channels_do_not_cross() {
    let home = tempdir().unwrap();
    succeeded_turn(home.path(), "telegram", Some("telegram:42"), "a");
    succeeded_turn(home.path(), "cli", Some("cli:1"), "b");
    succeeded_turn(home.path(), "telegram", Some("telegram:99"), "c");

    assert_eq!(list_pending_obligations(home.path(), 10).unwrap().len(), 3);
    let base = now();

    // An adapter claims only its own channel's debts.
    let claim = claim_outbound(home.path(), Some("cli"), Uuid::new_v4(), base, 60)
        .unwrap()
        .expect("the cli obligation is claimable");
    assert_eq!(claim.obligation().channel, "cli");
    assert_eq!(claim.obligation().target, "cli:1");
    assert!(
        claim_outbound(home.path(), Some("cli"), Uuid::new_v4(), base + 1, 60)
            .unwrap()
            .is_none(),
        "the cli channel owed exactly one send"
    );

    // Routing addresses are opaque to the gateway: stored exactly as the owning
    // channel wrote them, parsed only by that channel's adapter. Which of the two
    // telegram debts comes first is a tie the ledger breaks by obligation id, so
    // this asserts the shape rather than the order.
    let telegram = claim_outbound(home.path(), Some("telegram"), Uuid::new_v4(), base + 2, 60)
        .unwrap()
        .unwrap();
    assert_eq!(telegram.obligation().channel, "telegram");
    assert!(
        ["telegram:42", "telegram:99"].contains(&telegram.obligation().target.as_str()),
        "unexpected target {}",
        telegram.obligation().target
    );
}

#[test]
fn a_second_claimant_cannot_take_a_leased_send() {
    let home = tempdir().unwrap();
    succeeded_turn(home.path(), "cli", Some("cli:1"), "pong");
    let base = now();

    let first = claim_outbound(home.path(), None, Uuid::new_v4(), base, 600)
        .unwrap()
        .expect("first claimant wins");
    let second = claim_outbound(home.path(), None, Uuid::new_v4(), base + 1, 600).unwrap();
    assert!(second.is_none(), "a leased send has exactly one owner");

    settle_outbound(
        home.path(),
        &first,
        OutboundSettlement::Delivered {
            provider_message_id: "srv-1".into(),
        },
        base + 2,
    )
    .unwrap();
    assert_eq!(outbound_ledger_status(home.path()).unwrap().delivered, 1);
}

#[test]
fn reconcile_rebuilds_file_materializations_from_the_database() {
    let home = tempdir().unwrap();
    succeeded_turn(home.path(), "cli", Some("cli:1"), "pong");
    succeeded_turn(home.path(), "cli", Some("cli:2"), "pong");

    let gateway_root = home.path().join("gateway");
    let outbox = gateway_root.join("outbox");
    let processed = gateway_root.join("processed");
    assert_eq!(std::fs::read_dir(&outbox).unwrap().count(), 2);
    assert_eq!(std::fs::read_dir(&processed).unwrap().count(), 2);

    // The files are a materialization, not the record. Losing them loses nothing.
    for directory in [&outbox, &processed] {
        for entry in std::fs::read_dir(directory).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
    }
    assert_eq!(std::fs::read_dir(&outbox).unwrap().count(), 0);

    assert_eq!(reconcile(home.path()).unwrap(), 2);
    assert_eq!(std::fs::read_dir(&outbox).unwrap().count(), 2);
    assert_eq!(std::fs::read_dir(&processed).unwrap().count(), 2);
    assert_eq!(list_outbox_receipts(home.path(), 10).unwrap().len(), 2);

    // Reconciling again is not a second delivery.
    assert_eq!(reconcile(home.path()).unwrap(), 2);
    assert_eq!(std::fs::read_dir(&outbox).unwrap().count(), 2);
    assert_eq!(list_pending_obligations(home.path(), 10).unwrap().len(), 2);
}

#[test]
fn the_inbox_is_ingested_in_a_stable_order_and_drained_one_at_a_time() {
    let home = tempdir().unwrap();
    enqueue(home.path(), "cli", "one", "offline", Some("cli:1")).unwrap();
    enqueue(home.path(), "cli", "two", "offline", Some("cli:1")).unwrap();

    // Two messages received in the same second tie on receive time, so the
    // ordering contract is (received_unix, id) — deterministic for a given
    // database, not insertion order.
    let first_read = list_inbox(home.path()).unwrap();
    assert_eq!(first_read.len(), 2);
    let second_read = list_inbox(home.path()).unwrap();
    assert_eq!(first_read, second_read, "listing the inbox is stable");

    // Draining takes the head of that order and only that one.
    let drained = drain_one(home.path(), |message| {
        Ok((format!("re:{}", message.text), message.session_id.clone()))
    })
    .unwrap()
    .unwrap();
    assert_eq!(drained.id, first_read[0].id);

    let remaining = list_inbox(home.path()).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, first_read[1].id);
    assert_eq!(list_pending_obligations(home.path(), 10).unwrap().len(), 1);
}
