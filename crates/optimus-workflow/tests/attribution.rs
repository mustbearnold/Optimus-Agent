//! Usage attribution (spec-034 R7): the child's usage is attributed to
//! the parent turn in the execution store, and the totals reconcile
//! with the child manifest's own model calls.

use optimus_workflow::children::{AdoptionAction, ChildStatus, ChildSupervisor};
use rusqlite::Connection;
use tempfile::tempdir;
use uuid::Uuid;

fn home() -> std::path::PathBuf {
    tempdir().unwrap().keep()
}

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

/// A running parent-turn manifest (the attribution FK target).
fn seed_parent_manifest(home: &std::path::Path, id: Uuid) {
    let conn = Connection::open(home.join("execution.db")).unwrap();
    conn.execute(
        "INSERT INTO execution_manifests
           (id, version, session_id, turn_id, provider, model, autonomy_profile,
            command_fs_envelope, status, created_unix, completed_unix)
         VALUES (?1, 1, ?2, ?3, 'offline', 'offline-model', 'review_changes',
                 'confined_no_network', 'running', 1, NULL)",
        rusqlite::params![
            id.to_string(),
            Uuid::new_v4().to_string(),
            format!("turn-parent-{}", id)
        ],
    )
    .unwrap();
    drop(conn);
}

fn seed_session(home: &std::path::Path, id: Uuid) {
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    conn.execute(
        "INSERT INTO sessions (id, title, created_at, updated_at, packs_json, messages_json)
         VALUES (?1, 't', 'ts:1', 'ts:1', '[]', '[]')",
        [id.to_string()],
    )
    .unwrap();
    drop(conn);
}

/// Seed a child with one manifest and three model calls with known
/// token sums: input 100, output 30, total 130, reasoning 10.
fn seed_child_with_usage(
    home: &std::path::Path,
    parent: Uuid,
    child: Uuid,
    parent_manifest: Uuid,
    child_manifest: Uuid,
    status: &str,
) {
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    conn.execute(
        "INSERT INTO session_children
           (parent_session_id, child_session_id, depth, task_sha256, provider, model,
            effect_policy, autonomy_profile, command_fs_envelope, children_max_depth,
            status, parent_manifest_id, created_at)
         VALUES (?1, ?2, 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'offline', NULL, 'smart_deny', 'review_changes', NULL, 1,
                 ?3, ?4, 'ts:2')",
        rusqlite::params![
            parent.to_string(),
            child.to_string(),
            status,
            parent_manifest.to_string(),
        ],
    )
    .unwrap();
    drop(conn);

    let conn = Connection::open(home.join("execution.db")).unwrap();
    conn.execute(
        "INSERT INTO execution_manifests
           (id, version, session_id, turn_id, provider, model, autonomy_profile,
            command_fs_envelope, status, created_unix)
         VALUES (?1, 1, ?2, ?3, 'offline', 'offline-model', 'review_changes',
                 'confined_no_network', 'running', 1)",
        rusqlite::params![
            child_manifest.to_string(),
            child.to_string(),
            format!("turn-{}", child)
        ],
    )
    .unwrap();
    for (step, input, output, total, reasoning) in
        [(1, 40, 10, 50, 5), (2, 30, 20, 50, 3), (3, 30, 0, 30, 2)]
    {
        conn.execute(
            "INSERT INTO execution_model_calls
               (manifest_id, step, provider, model, request_sha256, response_sha256,
                replay_class, duration_ms, input_tokens, output_tokens, total_tokens,
                reasoning_tokens, cached_input_tokens, cache_write_tokens)
             VALUES (?1, ?2, 'offline', 'offline-model', 'a', 'b', 'deterministic',
                     100, ?3, ?4, ?5, ?6, 0, 0)",
            rusqlite::params![
                child_manifest.to_string(),
                step,
                input,
                output,
                total,
                reasoning
            ],
        )
        .unwrap();
    }
    drop(conn);
}

/// R7: the attribution row carries the child manifest's aggregated
/// usage, and the totals reconcile with the child's own model calls.
#[test]
fn attribution_reconciles_with_the_child_manifest_totals() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent);
    let parent_manifest = Uuid::new_v4();
    seed_parent_manifest(&home, parent_manifest);
    let child = Uuid::new_v4();
    seed_session(&home, child);
    let child_manifest = Uuid::new_v4();
    seed_child_with_usage(
        &home,
        parent,
        child,
        parent_manifest,
        child_manifest,
        "running",
    );

    let supervisor = ChildSupervisor::open(&home).unwrap();
    supervisor
        .settle(
            child,
            ChildStatus::Succeeded,
            None,
            300,
            Some(parent_manifest),
        )
        .unwrap();

    let conn = Connection::open(home.join("execution.db")).unwrap();
    let (total, input, output, reasoning, duration, attributed): (i64, i64, i64, i64, i64, i64) =
        conn.query_row(
            "SELECT total_tokens, input_tokens, output_tokens, reasoning_tokens,
                    duration_ms, attributed_at_unix
             FROM execution_child_attribution WHERE child_manifest_id = ?1",
            [child_manifest.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        (total, input, output, reasoning),
        (130, 100, 30, 10),
        "the snapshot must aggregate the child model calls"
    );
    assert_eq!(duration, 300);
    assert!(attributed > 0);

    // Reconciliation: the snapshot equals the live sums, and the child
    // manifest is no longer running.
    let live: (i64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0)
             FROM execution_model_calls WHERE manifest_id = ?1",
            [child_manifest.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(live, (100, 30));
    let manifest_status: String = conn
        .query_row(
            "SELECT status FROM execution_manifests WHERE id = ?1",
            [child_manifest.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(manifest_status, "succeeded");
}

/// R7: a second settle cannot double-attribute — the child manifest id
/// is unique in the attribution table.
#[test]
fn attribution_is_unique_per_child_manifest() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent);
    let parent_manifest = Uuid::new_v4();
    seed_parent_manifest(&home, parent_manifest);
    let child = Uuid::new_v4();
    seed_session(&home, child);
    let child_manifest = Uuid::new_v4();
    seed_child_with_usage(
        &home,
        parent,
        child,
        parent_manifest,
        child_manifest,
        "running",
    );

    let supervisor = ChildSupervisor::open(&home).unwrap();
    supervisor
        .settle(
            child,
            ChildStatus::Succeeded,
            None,
            300,
            Some(parent_manifest),
        )
        .unwrap();
    // A re-settle is refused by the exactly-one-terminal guard before
    // it can reach the attribution write.
    let err = supervisor
        .settle(
            child,
            ChildStatus::Failed,
            Some("again"),
            1,
            Some(parent_manifest),
        )
        .unwrap_err();
    assert!(err.to_string().contains("terminal"));

    let conn = Connection::open(home.join("execution.db")).unwrap();
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM execution_child_attribution
             WHERE child_manifest_id = ?1",
            [child_manifest.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1, "one attribution row per child manifest, forever");
}

/// R7 (crash window): the settle without a live parent still attributes
/// through the durable parent manifest id stored in the registry.
#[test]
fn crash_settle_attributes_through_the_durable_parent_manifest() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent);
    let parent_manifest = Uuid::new_v4();
    seed_parent_manifest(&home, parent_manifest);
    let child = Uuid::new_v4();
    seed_session(&home, child);
    let child_manifest = Uuid::new_v4();
    seed_child_with_usage(
        &home,
        parent,
        child,
        parent_manifest,
        child_manifest,
        "running",
    );

    let supervisor = ChildSupervisor::open(&home).unwrap();
    // The daemon died; the supervisor settles from the adoption sweep
    // with no live parent manifest in hand (None).
    supervisor
        .settle(
            child,
            ChildStatus::Failed,
            Some("crash_interrupted"),
            0,
            None,
        )
        .unwrap();

    let conn = Connection::open(home.join("execution.db")).unwrap();
    let (parent_id, total): (String, i64) = conn
        .query_row(
            "SELECT parent_manifest_id, total_tokens FROM execution_child_attribution
             WHERE child_manifest_id = ?1",
            [child_manifest.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(parent_id, parent_manifest.to_string());
    assert_eq!(total, 130);
}

/// The registry row snapshot round-trips the full policy inheritance
/// (R5): the re-adoption run rebuilds the child kernel from it.
#[test]
fn policy_snapshot_round_trips_through_the_registry() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent);
    let child = Uuid::new_v4();
    seed_session(&home, child);
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    conn.execute(
        "INSERT INTO session_children
           (parent_session_id, child_session_id, depth, task_sha256, provider, model,
            effect_policy, autonomy_profile, command_fs_envelope, children_max_depth,
            status, created_at)
         VALUES (?1, ?2, 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'deepseek', 'deepseek-chat',
                 'unrestricted', 'autonomous', 'unrestricted_host', 2,
                 'spawned', 'ts:2')",
        rusqlite::params![parent.to_string(), child.to_string()],
    )
    .unwrap();
    drop(conn);

    let supervisor = ChildSupervisor::open(&home).unwrap();
    let plan = supervisor.adoption_plan().unwrap();
    let run = plan
        .iter()
        .find_map(|action| match action {
            AdoptionAction::Run {
                child_session_id,
                provider,
                model,
                effect_policy,
                autonomy_profile,
                command_fs_envelope,
                children_max_depth,
                ..
            } if *child_session_id == child => Some((
                provider.clone(),
                model.clone(),
                effect_policy.clone(),
                autonomy_profile.clone(),
                command_fs_envelope.clone(),
                *children_max_depth,
            )),
            _ => None,
        })
        .expect("the spawned child must re-run");
    assert_eq!(
        run,
        (
            Some("deepseek".into()),
            Some("deepseek-chat".into()),
            "unrestricted".into(),
            "autonomous".into(),
            Some("unrestricted_host".into()),
            2
        ),
        "the adoption run must carry the persisted inheritance snapshot"
    );
}

/// The attribution JSON the model sees (R2 list surface) includes the
/// durable markers but never the raw prompt.
#[test]
fn the_list_surface_shows_markers_not_prompts() {
    let home = home();
    bootstrap(&home);
    let parent = Uuid::new_v4();
    seed_session(&home, parent);
    let child = Uuid::new_v4();
    seed_session(&home, child);
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    conn.execute(
        "INSERT INTO session_children
           (parent_session_id, child_session_id, depth, task_sha256, provider, model,
            effect_policy, autonomy_profile, command_fs_envelope, children_max_depth,
            status, cancel_requested, deleted_at, created_at)
         VALUES (?1, ?2, 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'offline', NULL, 'smart_deny', 'review_changes', NULL, 1,
                 'cancelled', NULL, 'ts:9', 'ts:2')",
        rusqlite::params![parent.to_string(), child.to_string()],
    )
    .unwrap();
    drop(conn);

    let supervisor = ChildSupervisor::open(&home).unwrap();
    let row = supervisor.row(child).unwrap().unwrap();
    let payload = serde_json::to_value(&row).unwrap();
    assert_eq!(payload["status"], "cancelled");
    assert_eq!(payload["deleted_at"], "ts:9");
    assert!(
        payload.get("task_prompt").is_none(),
        "the registry row must not carry the raw prompt"
    );
}
