//! Recursive children: the durable supervision loop (spec-034 A1-A5).
//!
//! The daemon-level execution lives in `optimus-host/tests/recursion.rs`;
//! this suite drives the supervisor directly — admission, the
//! exactly-one-terminal guard, adoption decisions, cancellation, and
//! deletion — against real `sessions.db` + `execution.db` files.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use optimus_workflow::children::{AdoptionAction, ChildStatus, ChildSupervisor};
use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;
use uuid::Uuid;

fn home() -> std::path::PathBuf {
    let dir = tempdir().unwrap();
    dir.keep()
}

/// Bootstrap the kernel tables the supervisor reads (mirror of the
/// kernel schema: sessions, session_children, session_child_events,
/// execution_manifests, execution_model_calls, execution_child_attribution).
fn bootstrap(home: &std::path::Path) {
    let sessions = Connection::open(home.join("sessions.db")).unwrap();
    sessions
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                packs_json TEXT NOT NULL,
                messages_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS session_children (
                parent_session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                child_session_id TEXT PRIMARY KEY NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                depth INTEGER NOT NULL CHECK(depth >= 1),
                task_sha256 TEXT NOT NULL CHECK(length(task_sha256)=64),
                provider TEXT,
                model TEXT,
                effect_policy TEXT NOT NULL DEFAULT 'smart_deny',
                autonomy_profile TEXT NOT NULL DEFAULT 'review_changes',
                command_fs_envelope TEXT,
                children_max_depth INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL CHECK(status IN (
                    'spawned','running','succeeded','failed','cancelled'
                )),
                cancel_requested TEXT,
                deleted_at TEXT,
                parent_manifest_id TEXT,
                created_at TEXT NOT NULL,
                adopted_at TEXT,
                terminal_at TEXT,
                terminal_reason TEXT
            );
            CREATE TABLE IF NOT EXISTS session_child_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                child_session_id TEXT NOT NULL REFERENCES session_children(child_session_id) ON DELETE CASCADE,
                event_type TEXT NOT NULL,
                payload TEXT,
                recorded_at TEXT NOT NULL,
                UNIQUE(child_session_id, event_type)
            );
            ",
        )
        .unwrap();
    drop(sessions);
    let executions = Connection::open(home.join("execution.db")).unwrap();
    executions
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS execution_manifests (
                id TEXT PRIMARY KEY NOT NULL,
                version INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                turn_id TEXT NOT NULL UNIQUE,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                autonomy_profile TEXT NOT NULL,
                command_fs_envelope TEXT NOT NULL,
                status TEXT NOT NULL,
                created_unix INTEGER NOT NULL,
                completed_unix INTEGER
            );
            CREATE TABLE IF NOT EXISTS execution_model_calls (
                manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
                step INTEGER NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                request_sha256 TEXT NOT NULL,
                response_sha256 TEXT NOT NULL,
                replay_class TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER,
                output_tokens INTEGER,
                total_tokens INTEGER,
                reasoning_tokens INTEGER,
                cached_input_tokens INTEGER,
                cache_write_tokens INTEGER,
                PRIMARY KEY(manifest_id, step)
            );
            CREATE TABLE IF NOT EXISTS execution_child_attribution (
                parent_manifest_id TEXT NOT NULL REFERENCES execution_manifests(id) ON DELETE CASCADE,
                child_session_id TEXT NOT NULL,
                child_manifest_id TEXT NOT NULL UNIQUE REFERENCES execution_manifests(id) ON DELETE CASCADE,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_tokens INTEGER NOT NULL DEFAULT 0,
                cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_tokens INTEGER NOT NULL DEFAULT 0,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                attributed_at_unix INTEGER NOT NULL,
                PRIMARY KEY(parent_manifest_id, child_manifest_id)
            );
            ",
        )
        .unwrap();
    drop(executions);
}

fn seed_session(home: &std::path::Path, id: Uuid, title: &str) {
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, title, created_at, updated_at, packs_json, messages_json)
         VALUES (?1, ?2, 'ts:1', 'ts:1', '[]', '[]')",
        [id.to_string(), title.to_string()],
    )
    .unwrap();
    drop(conn);
}

fn seed_child(
    home: &std::path::Path,
    parent: Uuid,
    child: Uuid,
    depth: u32,
    status: &str,
    cancel_requested: Option<&str>,
) {
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    conn.execute(
        "INSERT INTO session_children
           (parent_session_id, child_session_id, depth, task_sha256, provider, model,
            effect_policy, autonomy_profile, command_fs_envelope, children_max_depth,
            status, cancel_requested, created_at)
         VALUES (?1, ?2, ?3, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'offline', NULL, 'smart_deny', 'review_changes', NULL, 1,
                 ?4, ?5, 'ts:2')",
        rusqlite::params![
            parent.to_string(),
            child.to_string(),
            depth,
            status,
            cancel_requested
        ],
    )
    .unwrap();
    drop(conn);
}

fn seed_manifest(home: &std::path::Path, id: Uuid, session: Uuid, status: &str) {
    let conn = Connection::open(home.join("execution.db")).unwrap();
    conn.execute(
        "INSERT INTO execution_manifests
           (id, version, session_id, turn_id, provider, model, autonomy_profile,
            command_fs_envelope, status, created_unix)
         VALUES (?1, 1, ?2, ?3, 'offline', 'offline-model', 'review_changes',
                 'confined_no_network', ?4, 1)",
        rusqlite::params![
            id.to_string(),
            session.to_string(),
            format!("turn-{}", session),
            status
        ],
    )
    .unwrap();
    // One model call with a known token total (reconciliation input).
    conn.execute(
        "INSERT INTO execution_model_calls
           (manifest_id, step, provider, model, request_sha256, response_sha256,
            replay_class, duration_ms, input_tokens, output_tokens, total_tokens,
            reasoning_tokens, cached_input_tokens, cache_write_tokens)
         VALUES (?1, 1, 'offline', 'offline-model', 'a', 'b', 'deterministic',
                 100, 40, 10, 50, 5, 0, 0)",
        [id.to_string()],
    )
    .unwrap();
    drop(conn);
}

fn status_of(home: &std::path::Path, child: Uuid) -> (String, Option<String>, Option<String>) {
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    let row = conn
        .query_row(
            "SELECT status, terminal_reason, cancel_requested FROM session_children
             WHERE child_session_id = ?1",
            [child.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .unwrap();
    drop(conn);
    row
}

/// A1's registry half: three parallel admissions, all durable before
/// any settle; each settles exactly once; the events carry the
/// lifecycle in order.
#[test]
fn three_parallel_children_each_settle_exactly_once() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent, "parent");
    let supervisor = ChildSupervisor::open(&home).unwrap();
    let mut children = Vec::new();
    for _ in 0..3 {
        let child = Uuid::new_v4();
        seed_session(&home, child, "child");
        children.push(child);
    }
    let parent_manifest = Uuid::new_v4();
    seed_manifest(&home, parent_manifest, parent, "running");

    // Parallel admission: all three rows are durable (spawned) before
    // any child runs.
    for child in &children {
        seed_child(&home, parent, *child, 1, "spawned", None);
    }
    for child in &children {
        // Attribution source (R7): each child has one running manifest;
        // seed_manifest also seeds one call of (input 40, output 10,
        // total 50).
        let manifest = Uuid::new_v4();
        seed_manifest(&home, manifest, *child, "running");
    }
    for child in &children {
        supervisor.mark_running(*child, false, None, None).unwrap();
        supervisor
            .settle(
                *child,
                ChildStatus::Succeeded,
                None,
                42,
                Some(parent_manifest),
            )
            .unwrap();
    }
    for child in &children {
        let (status, _, cancel) = status_of(&home, *child);
        assert_eq!(status, "succeeded");
        assert!(cancel.is_none(), "settle must clear the cancel marker");
    }
    // Exactly one terminal event per child.
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    for child in &children {
        let terminal_events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_child_events
                 WHERE child_session_id = ?1 AND event_type IN
                       ('succeeded','failed','cancelled')",
                [child.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(terminal_events, 1);
    }
    drop(conn);

    // A second settle refuses: exactly-one-terminal is enforced.
    let err = supervisor
        .settle(
            children[0],
            ChildStatus::Failed,
            Some("again"),
            1,
            Some(parent_manifest),
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("terminal"),
        "double settle must refuse: {err}"
    );

    // Attribution: one row per child, reconciled against the model
    // calls (50 total tokens from the seeded call).
    let conn = Connection::open(home.join("execution.db")).unwrap();
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM execution_child_attribution
             WHERE parent_manifest_id = ?1",
            [parent_manifest.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 3);
    let (total, input, output): (i64, i64, i64) = conn
        .query_row(
            "SELECT total_tokens, input_tokens, output_tokens
             FROM execution_child_attribution LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!((total, input, output), (50, 40, 10));
}

/// A2's decision half: adoption re-runs only never-started children;
/// interrupted and cancel-requested children settle without re-running.
#[test]
fn adoption_plan_distinguishes_rerun_from_settle() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent, "parent");
    let never_started = Uuid::new_v4();
    let interrupted = Uuid::new_v4();
    let cancel_requested = Uuid::new_v4();
    let tombstoned = Uuid::new_v4();
    seed_session(&home, never_started, "c");
    seed_session(&home, interrupted, "c");
    seed_session(&home, cancel_requested, "c");
    seed_session(&home, tombstoned, "c");
    seed_child(&home, parent, never_started, 1, "spawned", None);
    seed_child(&home, parent, interrupted, 1, "running", None);
    seed_child(
        &home,
        parent,
        cancel_requested,
        1,
        "spawned",
        Some("parent requested"),
    );
    seed_child(&home, parent, tombstoned, 1, "succeeded", None);
    seed_manifest(&home, Uuid::new_v4(), interrupted, "running");

    let supervisor = ChildSupervisor::open(&home).unwrap();
    // Tombstone the succeeded child (adoption must skip it).
    supervisor.tombstone(tombstoned).unwrap();

    let plan = supervisor.adoption_plan().unwrap();
    let runs: Vec<_> = plan
        .iter()
        .filter_map(|action| match action {
            AdoptionAction::Run {
                child_session_id, ..
            } => Some(*child_session_id),
            _ => None,
        })
        .collect();
    let settles: Vec<_> = plan
        .iter()
        .filter_map(|action| match action {
            AdoptionAction::Settle {
                child_session_id,
                status,
                reason,
            } => Some((*child_session_id, *status, *reason)),
            _ => None,
        })
        .collect();

    assert_eq!(
        runs,
        vec![never_started],
        "only the never-started child re-runs"
    );
    assert_eq!(settles.len(), 2);
    let by_id: HashMap<_, _> = settles.iter().map(|(id, s, r)| (*id, (*s, *r))).collect();
    assert_eq!(
        by_id[&interrupted],
        (ChildStatus::Failed, "crash_interrupted")
    );
    assert_eq!(
        by_id[&cancel_requested],
        (ChildStatus::Cancelled, "cancel_requested")
    );
    assert!(
        !plan.iter().any(|a| matches!(a, AdoptionAction::Run { child_session_id, .. } if *child_session_id == tombstoned)),
        "tombstoned children are not adopted"
    );
}

/// A4/A5: cancellation settles a runner-less child at the cancel call;
/// deletion serializes with the run and never changes the terminal.
#[test]
fn cancel_and_delete_serialize_with_the_run() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent, "parent");
    let child = Uuid::new_v4();
    seed_session(&home, child, "child");
    seed_child(&home, parent, child, 1, "spawned", None);
    let supervisor = ChildSupervisor::open(&home).unwrap();

    // The durable cancel marker lands first (R6).
    supervisor
        .cancel_request(child, "parent requested")
        .unwrap();
    let (_, _, cancel) = status_of(&home, child);
    assert_eq!(cancel.as_deref(), Some("parent requested"));

    // Adoption settles the marked child without re-running.
    let plan = supervisor.adoption_plan().unwrap();
    assert!(matches!(
        plan.as_slice(),
        [AdoptionAction::Settle {
            child_session_id,
            status: ChildStatus::Cancelled,
            reason: "cancel_requested",
            ..
        }] if *child_session_id == child
    ));
    for action in plan {
        if let AdoptionAction::Settle {
            child_session_id,
            status,
            reason,
        } = action
        {
            supervisor
                .settle(child_session_id, status, Some(reason), 0, None)
                .unwrap();
        }
    }
    let (status, reason, cancel) = status_of(&home, child);
    assert_eq!(status, "cancelled");
    assert_eq!(reason.as_deref(), Some("cancel_requested"));
    assert!(cancel.is_none(), "the terminal settle clears the marker");

    // Deletion of the terminal child writes the tombstone and keeps
    // the terminal outcome (R6).
    supervisor.tombstone(child).unwrap();
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    let (status, deleted_at): (String, Option<String>) = conn
        .query_row(
            "SELECT status, deleted_at FROM session_children WHERE child_session_id = ?1",
            [child.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    drop(conn);
    assert_eq!(status, "cancelled", "deletion never changes the terminal");
    assert!(deleted_at.is_some(), "the tombstone is durable");
}

/// The adoption sweep's bounded wait: a running child whose runner is
/// gone settles promptly (no 10s stall on the crash window).
#[test]
fn runner_lost_settle_is_prompt() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent, "parent");
    let child = Uuid::new_v4();
    seed_session(&home, child, "child");
    seed_child(&home, parent, child, 1, "running", None);
    seed_manifest(&home, Uuid::new_v4(), child, "running");
    let supervisor = ChildSupervisor::open(&home).unwrap();

    // A crashed runner leaves no live token: the daemon settles at the
    // cancel call instead of waiting (R6 crash window).
    let started = Instant::now();
    let row = supervisor.row(child).unwrap().unwrap();
    supervisor
        .settle(
            child,
            ChildStatus::Cancelled,
            Some("runner_lost"),
            0,
            row.parent_manifest_id,
        )
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the runner-lost settle must not block on the wait bound"
    );
    let (status, reason, _) = status_of(&home, child);
    assert_eq!(status, "cancelled");
    assert_eq!(reason.as_deref(), Some("runner_lost"));
    // The orphaned manifest settles to match the registry terminal,
    // not dangling.
    let conn = Connection::open(home.join("execution.db")).unwrap();
    let manifest_status: String = conn
        .query_row(
            "SELECT status FROM execution_manifests WHERE session_id = ?1",
            [child.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        manifest_status, "cancelled",
        "the orphaned manifest settles too"
    );
}

/// The task prompt survives in the transcript (R4 adoption input).
#[test]
fn task_prompt_reads_the_transcript_last_user_message() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent, "parent");
    let child = Uuid::new_v4();
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, title, created_at, updated_at, packs_json, messages_json)
         VALUES (?1, 'child', 'ts:1', 'ts:1', '[]', ?2)",
        rusqlite::params![
            child.to_string(),
            json!([
                {"role": "system", "content": "you are a helper"},
                {"role": "user", "content": "summarize the roadmap"},
            ])
            .to_string()
        ],
    )
    .unwrap();
    drop(conn);
    seed_child(&home, parent, child, 1, "spawned", None);

    let supervisor = ChildSupervisor::open(&home).unwrap();
    assert_eq!(
        supervisor.task_prompt(child).unwrap().as_deref(),
        Some("summarize the roadmap")
    );
}
