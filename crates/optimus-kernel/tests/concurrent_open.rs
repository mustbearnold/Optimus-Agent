//! Concurrent kernel-open regression (ADR-0086-era flake, fixed with
//! spec-025/ADR-0087).
//!
//! The host's worker pool opens kernels on the same home concurrently (one
//! kernel per chat stream). Every kernel open migrates the shared SQLite
//! stores (optimus.db, sessions.db, execution.db, memory.db, skills.db,
//! messages.db). Before the fix, racing openers failed with "duplicate
//! column name" (check-then-alter TOCTOU) or "database is locked" (no
//! busy timeout), which made whole turns fail fast — the serve-protocol
//! `stream_limit_rejects_the_17th` flake.
//!
//! This test mirrors the production shape: one fresh home, many kernels
//! opened simultaneously. Every opener must succeed, and a reopen after
//! the burst must succeed too (migrations stay idempotent).

use std::sync::{Arc, Barrier};

use optimus_kernel::{Kernel, KernelConfig};
use tempfile::tempdir;

#[test]
fn concurrent_kernel_opens_migrate_a_fresh_home_without_racing() {
    let dir = tempdir().unwrap();
    let home = dir.path().to_path_buf();

    let workers = 8;
    let barrier = Arc::new(Barrier::new(workers));
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|index| {
                let barrier = Arc::clone(&barrier);
                let home = home.clone();
                scope.spawn(move || {
                    // Maximize simultaneity: every opener waits at the
                    // barrier, then all hit the schema migrations at once.
                    barrier.wait();
                    let opened = Kernel::open(&home, KernelConfig::default());
                    (index, opened)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("opener thread panicked"))
            .collect::<Vec<_>>()
    });

    let mut failures = Vec::new();
    for (index, opened) in results {
        if let Err(error) = opened {
            failures.push(format!("opener {index}: {error}"));
        }
    }
    assert!(
        failures.is_empty(),
        "concurrent opens must all succeed (no migration race):\n{}",
        failures.join("\n")
    );

    // Idempotency: after the burst, a fresh open on the migrated home must
    // still succeed (no half-applied migration, no lock held over).
    Kernel::open(&home, KernelConfig::default()).expect("reopen of a migrated home must succeed");
}

#[test]
fn sequential_reopens_after_concurrent_migration_stay_healthy() {
    let dir = tempdir().unwrap();
    let home = dir.path().to_path_buf();

    // Prime the home once, then open in quick succession — the path a
    // second worker wave takes after the first finished migrating.
    let kernel = Kernel::open(&home, KernelConfig::default()).expect("first open");
    drop(kernel);
    for _ in 0..4 {
        let kernel = Kernel::open(&home, KernelConfig::default()).expect("reopen");
        drop(kernel);
    }
}
