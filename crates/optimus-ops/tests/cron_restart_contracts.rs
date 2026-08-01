//! What the cron schedule store owes a worker that did not come back.
//!
//! The inline suite already covers contention between two live stores. This
//! covers the case that has no live counterpart to argue with: a process that
//! claimed a job and then died. Every assertion here is separated by a store
//! that is closed and reopened, because that is the only way to state the
//! difference between state a connection is holding and state the file actually
//! contains.
//!
//! The laws under test are 9 and 10 — a long-running operation must support
//! cancellation, and every execution must produce exactly one terminal outcome.
//! A crashed worker is precisely where both are easy to violate: the attempt it
//! abandoned is running forever unless something else ends it.

use optimus_ops::{CronError, CronStore};
use tempfile::tempdir;
use uuid::Uuid;

const LEASE_SECS: u64 = 30;

#[test]
fn a_lease_left_by_a_dead_worker_still_fences_the_next_one() {
    let home = tempdir().unwrap();
    let path = home.path().join("cron.db");
    let mut store = CronStore::open(&path).unwrap();
    let job = store.add("nightly", 5, "summarize", "offline").unwrap();
    let base = job.next_run_unix;

    let claim = store
        .claim_due(base, Uuid::new_v4(), LEASE_SECS)
        .unwrap()
        .pop()
        .expect("a job at its run time is due");
    assert_eq!(claim.job().id, job.id);
    drop(store);

    // The worker is gone; nothing released the lease. Until it expires the job
    // must stay invisible, because the alternative is a second worker running a
    // scheduled prompt the first one may still be running.
    let mut store = CronStore::open(&path).unwrap();
    assert!(store.due(base + 1).unwrap().is_empty());
    assert!(store
        .claim_due(base + 1, Uuid::new_v4(), LEASE_SECS)
        .unwrap()
        .is_empty());
    assert_eq!(
        store.attempt_status(claim.attempt_id()).unwrap().as_deref(),
        Some("running"),
        "the abandoned attempt is still open, which is why the fence has to hold"
    );
}

#[test]
fn an_expired_lease_returns_the_job_and_terminalizes_the_attempt_it_abandoned() {
    let home = tempdir().unwrap();
    let path = home.path().join("cron.db");
    let mut store = CronStore::open(&path).unwrap();
    let job = store.add("nightly", 5, "summarize", "offline").unwrap();
    let base = job.next_run_unix;

    let orphan = store
        .claim_due(base, Uuid::new_v4(), LEASE_SECS)
        .unwrap()
        .pop()
        .unwrap();
    let orphan_attempt = orphan.attempt_id();
    drop(store);

    let mut store = CronStore::open(&path).unwrap();
    let taken_over = store
        .claim_due(base + LEASE_SECS + 1, Uuid::new_v4(), LEASE_SECS)
        .unwrap()
        .pop()
        .expect("an expired lease returns the job to the pool");
    assert_ne!(taken_over.attempt_id(), orphan_attempt);

    // Law 10 is what the takeover has to preserve: the attempt nobody will ever
    // report on gets an outcome anyway. Leaving it `running` would mean one
    // execution with no terminal outcome and a job whose history lies forever.
    assert_eq!(
        store.attempt_status(orphan_attempt).unwrap().as_deref(),
        Some("expired")
    );

    // And the claim value that outlived its process cannot write a result over
    // the worker that legitimately holds the job now.
    assert!(matches!(
        store.complete_claim(&orphan, "ok", base + LEASE_SECS + 2),
        Err(CronError::LeaseLost { job_id }) if job_id == job.id
    ));
    store
        .complete_claim(&taken_over, "ok", base + LEASE_SECS + 2)
        .unwrap();
    assert_eq!(
        store
            .attempt_status(taken_over.attempt_id())
            .unwrap()
            .as_deref(),
        Some("succeeded")
    );
}

#[test]
fn an_attempt_stranded_by_a_crash_is_still_cancellable() {
    let home = tempdir().unwrap();
    let path = home.path().join("cron.db");
    let mut store = CronStore::open(&path).unwrap();
    let job = store.add("long", 60, "work", "offline").unwrap();
    let base = job.next_run_unix;
    let stranded = store
        .claim_due(base, Uuid::new_v4(), 3_600)
        .unwrap()
        .pop()
        .unwrap();
    drop(store);

    // An hour-long lease means waiting for expiry is not a recovery story. Law 9
    // says the operator must be able to end it, and after a restart the operator
    // has no claim handle — only the job id the store can still resolve.
    let store = CronStore::open(&path).unwrap();
    assert!(store.cancel_running(job.id, base + 5).unwrap());
    assert_eq!(
        store
            .attempt_status(stranded.attempt_id())
            .unwrap()
            .as_deref(),
        Some("cancelled")
    );
    assert!(
        !store.cancel_running(job.id, base + 6).unwrap(),
        "a second cancellation has nothing left to cancel"
    );

    let projected = store.list().unwrap().pop().unwrap();
    assert_eq!(projected.last_status.as_deref(), Some("cancelled"));
    assert!(projected.lease_owner_id.is_none());
    assert_eq!(projected.next_run_unix, base + 5 + 60);
}

#[test]
fn every_attempt_a_job_ever_ran_survives_the_process_that_ran_it() {
    let home = tempdir().unwrap();
    let path = home.path().join("cron.db");
    let store = CronStore::open(&path).unwrap();
    let job = store.add("recurring", 5, "work", "offline").unwrap();
    let base = job.next_run_unix;
    drop(store);

    // Three runs, three process lifetimes, three different endings.
    let mut store = CronStore::open(&path).unwrap();
    let first = store.claim_due(base, Uuid::new_v4(), LEASE_SECS).unwrap();
    store.complete_claim(&first[0], "ok", base + 1).unwrap();
    drop(store);

    let mut store = CronStore::open(&path).unwrap();
    let second = store
        .claim_due(base + 6, Uuid::new_v4(), LEASE_SECS)
        .unwrap();
    store
        .complete_claim(&second[0], "error: provider refused", base + 7)
        .unwrap();
    drop(store);

    let mut store = CronStore::open(&path).unwrap();
    let third = store
        .claim_due(base + 12, Uuid::new_v4(), LEASE_SECS)
        .unwrap();
    assert_eq!(third.len(), 1);
    store.cancel_running(job.id, base + 13).unwrap();
    drop(store);

    // History is the only record an operator has of runs no surface was open
    // for, so it has to be complete and newest-first regardless of which process
    // wrote each row.
    let store = CronStore::open(&path).unwrap();
    let history = store.history(job.id, 10).unwrap();
    let outcomes: Vec<_> = history.iter().map(|a| a.status.as_str()).collect();
    assert_eq!(outcomes, ["cancelled", "failed", "succeeded"]);
    assert!(
        history.iter().all(|a| a.completed_unix.is_some()),
        "a recorded attempt with no completion time is an execution with no terminal outcome"
    );
    assert_eq!(
        history[1].detail.as_deref(),
        Some("error: provider refused")
    );
}
