//! The delivery ledger's state machine, checked against arbitrary sequences.
//!
//! Architectural law 10 — every execution produces exactly one terminal outcome
//! — is a statement about *all* interleavings, and the example tests next door
//! can only assert it about the handful someone thought to write down. What
//! actually threatens it is an ordering nobody imagined: a sweep landing between
//! a claim and its settlement, an operator resolving something a retry was about
//! to re-arm, a crash in the one step that had no crash test.
//!
//! So these generate the sequence instead. The invariants are the ones a
//! messaging surface depends on without knowing it: every owed send is in
//! exactly one state, delivery and abandonment are permanent, an unknown outcome
//! only stops being unknown when a human says so, and the coarse view an
//! operator actually reads never disagrees with the ledger about how many open
//! questions there are.
//!
//! Case count is deliberately below proptest's default. Each step is real SQLite
//! I/O against a temp directory rather than an in-memory move, and the reachable
//! state space here is small enough that a few dozen sequences saturate it.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use optimus_ops::{
    claim_outbound, drain_one, enqueue, list_ambiguous_obligations, list_ambiguous_sends,
    list_outbox_receipts, list_pending_obligations, list_unsettled_obligations,
    outbound_ledger_status, resolve_ambiguous_obligation, settle_outbound, sweep_stale_sends,
    AmbiguityResolution, OutboundLedgerStatus, OutboundSettlement,
};
use proptest::prelude::*;
use tempfile::tempdir;
use uuid::Uuid;

/// Long enough that nothing expires a crashed lease by accident.
///
/// `outbound_ledger_status` expires stale leases against the real clock every
/// time it is read, so a short lease would make the sweep step meaningless — the
/// invariant check itself would have already moved the row.
const CRASH_LEASE_SECS: u64 = 3_600;

#[derive(Debug, Clone, Copy)]
enum Step {
    /// Claim the oldest owed send and settle it as accepted by the platform.
    SendConfirmed,
    /// Claim and settle as a definite refusal — retryable up to the bound.
    SendRefused,
    /// Claim and settle as an unknown outcome — not retryable at all.
    SendUnknown,
    /// Claim and then vanish, leaving a lease nobody will ever settle.
    Crash,
    /// Run the stale-lease sweep far enough ahead to catch any crash above.
    Sweep,
    ResolveDelivered,
    ResolveNotDelivered,
    ResolveAbandon,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => Just(Step::SendConfirmed),
        2 => Just(Step::SendRefused),
        2 => Just(Step::SendUnknown),
        2 => Just(Step::Crash),
        2 => Just(Step::Sweep),
        1 => Just(Step::ResolveDelivered),
        1 => Just(Step::ResolveNotDelivered),
        1 => Just(Step::ResolveAbandon),
    ]
}

fn real_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_secs()
}

/// Put `count` owed sends into a fresh gateway by running that many turns.
fn owed_sends(home: &Path, count: usize) {
    for index in 0..count {
        let session = format!("cli:{index}");
        enqueue(
            home,
            "cli",
            &format!("ask {index}"),
            "offline",
            Some(&session),
        )
        .expect("enqueue");
        drain_one(home, |message| {
            Ok((
                format!("answer to {}", message.text),
                message.session_id.clone(),
            ))
        })
        .expect("drain")
        .expect("the message just enqueued is drainable");
    }
}

fn apply(home: &Path, step: Step, clock: u64) {
    let settle_next = |settlement: OutboundSettlement| {
        if let Some(claim) =
            claim_outbound(home, None, Uuid::new_v4(), clock, 300).expect("claim outbound")
        {
            settle_outbound(home, &claim, settlement, clock).expect("settle outbound");
        }
    };
    let resolve_next = |resolution: AmbiguityResolution| {
        if let Some(open) = list_ambiguous_obligations(home, 50)
            .expect("list ambiguous")
            .first()
        {
            resolve_ambiguous_obligation(home, &open.obligation_id, resolution, clock)
                .expect("resolve ambiguity");
        }
    };

    match step {
        Step::SendConfirmed => settle_next(OutboundSettlement::Delivered {
            provider_message_id: format!("srv-{clock}"),
        }),
        Step::SendRefused => settle_next(OutboundSettlement::Failed {
            detail: "chat_not_found".into(),
        }),
        Step::SendUnknown => settle_next(OutboundSettlement::Ambiguous {
            detail: "connection reset".into(),
        }),
        Step::Crash => {
            // Dropping the claim is the whole step: the lease stays on disk with
            // nobody left to answer for it.
            drop(
                claim_outbound(home, None, Uuid::new_v4(), clock, CRASH_LEASE_SECS).expect("claim"),
            );
        }
        Step::Sweep => {
            sweep_stale_sends(home, clock + CRASH_LEASE_SECS + 1).expect("sweep");
        }
        Step::ResolveDelivered => resolve_next(AmbiguityResolution::Delivered {
            provider_message_id: format!("late-{clock}"),
        }),
        Step::ResolveNotDelivered => resolve_next(AmbiguityResolution::NotDelivered),
        Step::ResolveAbandon => resolve_next(AmbiguityResolution::Abandon {
            detail: "operator gave up".into(),
        }),
    }
}

/// Every invariant that must hold after any step, in any order, forever.
fn check(
    home: &Path,
    total: usize,
    previous: &OutboundLedgerStatus,
    step: Step,
) -> Result<OutboundLedgerStatus, TestCaseError> {
    let status = outbound_ledger_status(home).expect("ledger status");
    let counted =
        status.pending + status.sending + status.delivered + status.ambiguous + status.abandoned;

    // One state each. A send counted twice is a duplicate waiting to happen; a
    // send counted zero times is a reply that silently stopped being owed.
    prop_assert_eq!(counted, total, "after {:?}: {:?}", step, status);

    // Both terminal states are absorbing. Anything that could walk a delivered
    // send back to pending is a duplicate message to a real person.
    prop_assert!(
        status.delivered >= previous.delivered,
        "{:?} un-delivered a send: {:?} -> {:?}",
        step,
        previous,
        status
    );
    prop_assert!(
        status.abandoned >= previous.abandoned,
        "{:?} revived an abandoned send: {:?} -> {:?}",
        step,
        previous,
        status
    );

    // Ambiguity is the one state no automatic path may leave. Every step that
    // shrinks the pile has to be an operator decision.
    if status.ambiguous < previous.ambiguous {
        prop_assert!(
            matches!(
                step,
                Step::ResolveDelivered | Step::ResolveNotDelivered | Step::ResolveAbandon
            ),
            "{:?} resolved an unknown outcome without an operator",
            step
        );
    }

    // The list views and the counters are read by different surfaces and must
    // never tell an operator two different stories.
    let limit = total.saturating_mul(2).max(4);
    prop_assert_eq!(
        list_pending_obligations(home, limit)
            .expect("pending")
            .len(),
        status.pending
    );
    prop_assert_eq!(
        list_ambiguous_obligations(home, limit)
            .expect("ambiguous")
            .len(),
        status.ambiguous
    );
    prop_assert_eq!(
        list_unsettled_obligations(home, limit)
            .expect("unsettled")
            .len(),
        status.pending + status.sending + status.ambiguous
    );

    // The turn-level projection is what surfaces that never heard of the ledger
    // read. A receipt that shows delivery the ledger has not recorded — or hides
    // one it has — is the lie this projection exists to prevent.
    let receipts = list_outbox_receipts(home, limit).expect("receipts");
    prop_assert_eq!(
        receipts
            .iter()
            .filter(|receipt| receipt.delivered_unix.is_some())
            .count(),
        status.delivered
    );
    prop_assert_eq!(
        list_ambiguous_sends(home, limit)
            .expect("ambiguous sends")
            .len(),
        status.ambiguous,
        "the operator's coarse view disagrees with the ledger"
    );

    Ok(status)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// The safety half: no interleaving loses a send, resurrects a settled one,
    /// or quietly decides an unknown outcome on the operator's behalf.
    #[test]
    fn no_ordering_of_sends_sweeps_and_resolutions_breaks_the_ledger(
        total in 1usize..4,
        steps in prop::collection::vec(step(), 1..16),
    ) {
        let home = tempdir().unwrap();
        owed_sends(home.path(), total);

        let base = real_now();
        let mut previous = outbound_ledger_status(home.path()).unwrap();
        prop_assert_eq!(previous.pending, total);

        for (index, step) in steps.into_iter().enumerate() {
            apply(home.path(), step, base + index as u64);
            previous = check(home.path(), total, &previous, step)?;
        }
    }

    /// The liveness half: whatever state a sequence leaves behind, an operator
    /// can still finish it. A ledger that is safe but has a corner nothing can
    /// drain would strand a reply forever and satisfy every assertion above.
    #[test]
    fn any_state_the_ledger_reaches_can_still_be_drained_to_terminal(
        total in 1usize..4,
        steps in prop::collection::vec(step(), 1..16),
    ) {
        let home = tempdir().unwrap();
        owed_sends(home.path(), total);

        let base = real_now();
        for (index, step) in steps.into_iter().enumerate() {
            apply(home.path(), step, base + index as u64);
        }

        // Now do what an operator with no more patience does: expire whatever is
        // in flight, send whatever is owed, and give up on whatever is unknown.
        let bound = 4 * total + 4;
        let mut rounds = 0;
        let mut clock = base + 1_000;
        loop {
            let status = outbound_ledger_status(home.path()).unwrap();
            if status.pending == 0 && status.sending == 0 && status.ambiguous == 0 {
                break;
            }
            prop_assert!(
                rounds < bound,
                "no progress after {} rounds: {:?}",
                rounds,
                status
            );
            apply(home.path(), Step::Sweep, clock);
            apply(home.path(), Step::SendConfirmed, clock);
            apply(home.path(), Step::ResolveAbandon, clock);
            rounds += 1;
            clock += 1;
        }

        let status = outbound_ledger_status(home.path()).unwrap();
        prop_assert_eq!(status.delivered + status.abandoned, total);
        prop_assert!(list_unsettled_obligations(home.path(), 8).unwrap().is_empty());
    }
}
