//! The enqueue seams every channel shares, as an outside consumer sees them.
//!
//! Each adapter is a thin translation from a platform's shape into the one
//! durable gateway. The contract that matters is that the translation is
//! lossless in both directions: the address a message arrived from must still
//! be readable on the debt the turn commits, and the body the turn produced
//! must reach the transport whole rather than in the shortened form a list view
//! renders. Both are silent failures — a truncated reply still looks delivered,
//! and a mangled address still looks sent.
//!
//! These also pin the isolation between channels. One gateway serves all of
//! them, so a defect here does not drop a message, it delivers it to a stranger.

use std::path::Path;

use optimus_ops::{
    discord_enqueue, drain_one, gateway_status, list_ambiguous_obligations, list_inbox,
    list_outbox_receipts, list_pending_obligations, outbound_ledger_status, slack_enqueue,
    telegram_poll_once, MockTelegramTransport,
};
use tempfile::tempdir;

/// Take whatever is at the head of the inbox through a turn that succeeds.
fn drain_with_reply(home: &Path, reply: &str) -> String {
    let drained = drain_one(home, |message| {
        Ok((reply.to_string(), message.session_id.clone()))
    })
    .expect("drain")
    .expect("a pending message was available");
    drained.id
}

#[test]
fn a_discord_address_survives_the_turn_into_the_obligation() {
    let home = tempdir().unwrap();
    let inbound = discord_enqueue(home.path(), "hello discord", "ch-7").unwrap();
    assert_eq!(inbound.channel, "discord");
    assert_eq!(inbound.session_id.as_deref(), Some("discord:ch-7"));

    let drained_id = drain_with_reply(home.path(), "hi back");

    // The gateway never parses a routing address; it carries the one the adapter
    // minted through to whoever sends. A debt addressed to anything else is a
    // reply that would arrive in someone else's conversation.
    let owed = list_pending_obligations(home.path(), 10).unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].message_id, drained_id);
    assert_eq!(owed[0].channel, "discord");
    assert_eq!(owed[0].target, "discord:ch-7");
    assert_eq!(owed[0].body, "hi back");
}

#[test]
fn a_slack_address_survives_the_turn_into_the_obligation() {
    let home = tempdir().unwrap();
    let inbound = slack_enqueue(home.path(), "hello slack", "C024BE7LR").unwrap();
    assert_eq!(inbound.session_id.as_deref(), Some("slack:C024BE7LR"));

    drain_with_reply(home.path(), "hi back");

    let owed = list_pending_obligations(home.path(), 10).unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].channel, "slack");
    assert_eq!(owed[0].target, "slack:C024BE7LR");
}

#[test]
fn a_polled_message_is_routed_rather_than_pinned_to_the_scripted_model() {
    let home = tempdir().unwrap();
    let mut transport = MockTelegramTransport::default();
    transport.push_text(1, "42", "do something real");
    telegram_poll_once(home.path(), &mut transport, 0, |message| {
        Ok((format!("saw {}", message.text), message.session_id.clone()))
    })
    .unwrap();

    // The adapter used to stamp every polled update with `offline`, which pins the
    // turn to the scripted echo model no matter how this machine is configured.
    // A model that emits no tool calls is a bot that can never do anything — and
    // a bot that can never do anything is one whose approval spine can never
    // fire, so the safety path would have been untested in the only place it
    // matters. Routing is the caller's to decide, not the adapter's to hard-code.
    // Read it off the outbound row rather than the inbox: a poll answers what it
    // enqueues in the same cycle, so by now there is nothing pending to inspect.
    // The turn copies the inbound provider forward, so this is the same string
    // the route was resolved from.
    let answered = list_outbox_receipts(home.path(), 10).unwrap();
    assert_eq!(answered.len(), 1);
    assert_ne!(
        answered[0].outbound.provider, "offline",
        "a remote message must reach the configured route, not a fixed provider"
    );
    assert_eq!(answered[0].outbound.provider, "auto");
}

#[test]
fn an_empty_adapter_message_leaves_no_durable_trace() {
    let home = tempdir().unwrap();

    // Rejection has to happen before the gateway, not after: a row admitted and
    // then regretted is still a row some later drain will try to answer.
    assert!(discord_enqueue(home.path(), "", "ch-7").is_err());
    assert!(discord_enqueue(home.path(), "   \n\t ", "ch-7").is_err());
    assert!(slack_enqueue(home.path(), "", "C1").is_err());
    assert!(slack_enqueue(home.path(), "\u{a0}   ", "C1").is_err());

    assert!(list_inbox(home.path()).unwrap().is_empty());
    let status = gateway_status(home.path()).unwrap();
    assert_eq!(status.inbox_pending, 0);
    assert_eq!(status.inbox_claimed, 0);
    assert_eq!(status.outbox_total, 0);
    assert_eq!(list_pending_obligations(home.path(), 10).unwrap().len(), 0);
}

#[test]
fn the_whole_reply_reaches_the_transport_not_the_preview() {
    let home = tempdir().unwrap();
    let mut transport = MockTelegramTransport::default();
    transport.push_text(1, "42", "say something long");

    let reply = "x".repeat(900);
    let expected = reply.clone();
    let result = telegram_poll_once(home.path(), &mut transport, 0, |message| {
        Ok((reply.clone(), message.session_id.clone()))
    })
    .unwrap();

    assert_eq!(result.enqueued.len(), 1);
    assert_eq!(result.drained.len(), 1);
    assert_eq!(result.receipts.len(), 1);
    assert!(result.ambiguous.is_empty());
    assert!(result.failed_sends.is_empty());

    // `DrainResult::reply_preview` is a 200-character display field. What the
    // ledger owes — and therefore what the transport is handed — is the reply
    // itself. A preview reaching the platform is a truncated answer that every
    // local surface still reports as delivered.
    assert_eq!(transport.sent.len(), 1);
    assert_eq!(transport.sent[0].0, "42");
    assert_eq!(transport.sent[0].1, expected);

    assert_eq!(outbound_ledger_status(home.path()).unwrap().delivered, 1);
    assert_eq!(gateway_status(home.path()).unwrap().ambiguous_sends, 0);
}

#[test]
fn the_next_cycle_sends_what_is_owed_and_never_resends_what_is_unknown() {
    let home = tempdir().unwrap();
    let mut transport = MockTelegramTransport::default();
    transport.next_send_ambiguous = true;
    transport.push_text(1, "42", "first");

    let first = telegram_poll_once(home.path(), &mut transport, 0, |message| {
        Ok(("reply one".into(), message.session_id.clone()))
    })
    .unwrap();
    assert_eq!(first.ambiguous.len(), 1);
    assert!(first.receipts.is_empty());
    assert!(
        transport.sent.is_empty(),
        "an ambiguous send delivered nothing this process can confirm"
    );

    transport.push_text(2, "42", "second");
    let second = telegram_poll_once(home.path(), &mut transport, first.next_offset, |message| {
        Ok(("reply two".into(), message.session_id.clone()))
    })
    .unwrap();
    assert_eq!(second.receipts.len(), 1);
    assert!(second.ambiguous.is_empty());

    // The second cycle is a retry for the debt that is owed and not one for the
    // debt whose outcome is unknown. Resending the first would be the duplicate
    // this ledger exists to refuse; leaving the second unsent would be the
    // silence it exists to prevent. Both come out of the same claim loop, so
    // only a test that runs two cycles can tell them apart.
    assert_eq!(transport.sent.len(), 1);
    assert_eq!(transport.sent[0].1, "reply two");

    let unknown = list_ambiguous_obligations(home.path(), 10).unwrap();
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].message_id, first.drained[0]);

    let ledger = outbound_ledger_status(home.path()).unwrap();
    assert_eq!(ledger.delivered, 1);
    assert_eq!(ledger.ambiguous, 1);
    assert_eq!(ledger.pending, 0);
}

#[test]
fn a_definite_refusal_leaves_the_debt_owed_rather_than_unknown() {
    let home = tempdir().unwrap();
    let mut transport = MockTelegramTransport::default();
    transport.next_send_failed = true;
    transport.push_text(1, "42", "hello");

    let result = telegram_poll_once(home.path(), &mut transport, 0, |message| {
        Ok(("reply".into(), message.session_id.clone()))
    })
    .unwrap();

    assert_eq!(result.failed_sends.len(), 1);
    assert!(result.receipts.is_empty());
    assert!(transport.sent.is_empty());

    // A platform that said no is a knowable outcome, so the debt goes back into
    // the pending pool for a later cycle instead of waiting on an operator. The
    // adapter stops the loop after it rather than retrying immediately, which is
    // what gives the attempt bound a real interval to count.
    let owed = list_pending_obligations(home.path(), 10).unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].attempts, 1);
    assert!(list_ambiguous_obligations(home.path(), 10)
        .unwrap()
        .is_empty());
}

#[test]
fn the_telegram_adapter_never_delivers_another_channel() {
    let home = tempdir().unwrap();
    let discord = discord_enqueue(home.path(), "discord question", "ch-7").unwrap();

    let mut transport = MockTelegramTransport::default();
    transport.push_text(1, "42", "telegram question");
    let result = telegram_poll_once(home.path(), &mut transport, 0, |message| {
        Ok((
            format!("answered {}", message.channel),
            message.session_id.clone(),
        ))
    })
    .unwrap();
    assert_eq!(result.enqueued.len(), 1);

    // The inbox is one FIFO shared by every channel, and both of these arrived in
    // the same second, so which one sits at the head is a tie broken by id. The
    // adapter is therefore allowed to find a foreign message and decline it —
    // what it is never allowed to do is answer it, lose it, or leave it leased.
    for (chat, body) in &transport.sent {
        assert_eq!(chat, "42", "a telegram send went to {chat}");
        assert!(
            !body.contains("discord"),
            "a discord turn was delivered over telegram: {body}"
        );
    }
    assert!(
        list_inbox(home.path())
            .unwrap()
            .iter()
            .any(|message| message.id == discord.id),
        "the foreign message was consumed by an adapter that does not own it"
    );
    assert_eq!(
        gateway_status(home.path()).unwrap().inbox_claimed,
        0,
        "a declined claim must be released, not held"
    );

    // Whatever did get answered owes its reply to its own channel and no other.
    for receipt in list_outbox_receipts(home.path(), 10).unwrap() {
        assert_eq!(receipt.outbound.channel, "telegram");
    }
    for owed in list_pending_obligations(home.path(), 10).unwrap() {
        assert!(
            owed.target.starts_with(&format!("{}:", owed.channel)),
            "obligation on {} addressed to {}",
            owed.channel,
            owed.target
        );
    }
}

#[test]
fn three_channels_share_one_gateway_without_crossing() {
    let home = tempdir().unwrap();
    discord_enqueue(home.path(), "from discord", "d1").unwrap();
    slack_enqueue(home.path(), "from slack", "s1").unwrap();
    optimus_ops::enqueue(
        home.path(),
        "cli",
        "from cli",
        "offline",
        Some("cli:operator"),
    )
    .unwrap();

    // Answer each turn with the channel it came from so a crossed address shows
    // up as a body that does not match its own target.
    for _ in 0..3 {
        drain_one(home.path(), |message| {
            Ok((
                format!("re:{}", message.channel),
                message.session_id.clone(),
            ))
        })
        .unwrap()
        .unwrap();
    }

    let owed = list_pending_obligations(home.path(), 10).unwrap();
    assert_eq!(owed.len(), 3);
    for obligation in &owed {
        assert_eq!(obligation.body, format!("re:{}", obligation.channel));
        assert!(obligation
            .target
            .starts_with(&format!("{}:", obligation.channel)));
    }

    let mut channels: Vec<_> = owed.iter().map(|o| o.channel.as_str()).collect();
    channels.sort_unstable();
    assert_eq!(channels, ["cli", "discord", "slack"]);
}
