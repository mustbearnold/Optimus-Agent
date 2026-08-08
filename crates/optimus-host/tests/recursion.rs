//! Recursive children at the daemon level (spec-034 acceptance).
//!
//! A1: a parent spawns three children in parallel; the admission
//! handles return before any child completes; every child records
//! exactly one terminal outcome (`succeeded`).
//! A2: after a host restart the adoption sweep re-runs a never-started
//! child, settles an interrupted child to `failed`/`crash_interrupted`,
//! settles a cancel-requested child to `cancelled`, and never produces
//! a second terminal.
//! A8: the parent client detaches while children run; the daemon keeps
//! running them to their terminals.
//! A9: a surface chat turn targeting a child session refuses with a
//! diagnostic that names the session.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use optimus_host::{start_with_io, RunningServer};
use optimus_kernel::get_session;
use rusqlite::Connection;
use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Serialize tests that bind loopback + mutate env-affecting state.
fn lock() -> std::sync::MutexGuard<'static, ()> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        match ENV_LOCK.try_lock() {
            Ok(guard) => return guard,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => return poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                assert!(Instant::now() < deadline, "ENV_LOCK held >90s");
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}

struct TestServer {
    server: RunningServer,
}

fn start_server(home: &std::path::Path) -> TestServer {
    let server = start_with_io(
        home,
        0,
        false,
        Box::new(std::io::empty()),
        Box::new(std::io::sink()),
    )
    .expect("serve starts");
    TestServer { server }
}

/// The dial ticket from the runtime record (the hello credential).
fn ticket(home: &std::path::Path) -> String {
    let record: Value = serde_json::from_str(
        &std::fs::read_to_string(home.join("host-runtime.json")).expect("runtime record"),
    )
    .expect("record json");
    record["token"].as_str().expect("record token").to_string()
}

fn ws_connect(server: &TestServer) -> WebSocket<MaybeTlsStream<std::net::TcpStream>> {
    let url = format!("ws://127.0.0.1:{}/ws", server.server.port());
    let request = tungstenite::http::Request::builder()
        .uri(url)
        .header("Host", "127.0.0.1")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("Sec-WebSocket-Version", "13")
        .body(())
        .unwrap();
    let (mut ws, _response) = connect(request).expect("ws upgrade");
    if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(40)))
            .expect("read timeout");
    }
    ws
}

fn hello(ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>, home: &std::path::Path) {
    ws.send(Message::Text(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "hello",
            "params": {"protocol_version": 1, "client_kind": "renderer", "ticket": ticket(home)}
        })
        .to_string()
        .into(),
    ))
    .expect("hello send");
    // Deterministic handshake: the server replies with the hello result,
    // then pushes host.ready (mirrors serve_protocol's two-read pattern).
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen_result = false;
    while Instant::now() < deadline {
        if let Message::Text(text) = ws.read().expect("hello read") {
            let value: Value = serde_json::from_str(&text).unwrap();
            if value.get("result").is_some() {
                seen_result = true;
            } else if seen_result {
                // host.ready consumed; handshake complete.
                return;
            }
        }
    }
    panic!("hello handshake did not complete");
}

/// Run one chat turn (offline, demo_children fixture) to its terminal.
fn chat_demo_children(
    ws: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    parent: uuid::Uuid,
    stream_id: u64,
) {
    ws.send(Message::Text(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "chat_start",
            "params": {
                "stream_id": stream_id,
                "request": {
                    "session": parent.to_string(),
                    "message": "spawn three children",
                    "provider": "offline",
                    "demo_children": true,
                },
            }
        })
        .to_string()
        .into(),
    ))
    .expect("chat send");
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        assert!(Instant::now() < deadline, "chat did not terminate");
        if let Message::Text(text) = ws.read().expect("chat read") {
            let value: Value = serde_json::from_str(&text).unwrap();
            if value.get("result").is_some() {
                // The chat_start ack; the stream events follow.
                continue;
            }
            let event = value.get("params").and_then(|e| e.get("event"));
            if let Some(event) = event {
                if event.get("type").and_then(|t| t.as_str()) == Some("done") {
                    return;
                }
            }
        }
    }
}

/// Poll the registry until every child of `parent` is terminal or the
/// deadline elapses. Returns the registry rows as (child_id, status).
fn wait_children_terminal(
    home: &std::path::Path,
    parent: uuid::Uuid,
    count: usize,
) -> Vec<(String, String)> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let conn = Connection::open(home.join("sessions.db")).unwrap();
        let rows = {
            let mut stmt = conn
                .prepare(
                    "SELECT child_session_id, status FROM session_children
                     WHERE parent_session_id = ?1 ORDER BY created_at ASC",
                )
                .unwrap();
            stmt.query_map([parent.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };
        drop(conn);
        if rows.len() >= count
            && rows
                .iter()
                .all(|(_, status)| matches!(status.as_str(), "succeeded" | "failed" | "cancelled"))
        {
            return rows;
        }
        assert!(
            Instant::now() < deadline,
            "children did not settle: {rows:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn attribution_rows(home: &std::path::Path) -> Vec<(String, i64, i64)> {
    let conn = Connection::open(home.join("execution.db")).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT child_session_id, total_tokens, duration_ms
             FROM execution_child_attribution ORDER BY child_session_id",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn child_events(home: &std::path::Path, child: &str) -> Vec<String> {
    let conn = Connection::open(home.join("sessions.db")).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT event_type FROM session_child_events
             WHERE child_session_id = ?1 ORDER BY sequence ASC",
        )
        .unwrap();
    stmt.query_map([child], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

/// A1 + A8 + R7: three parallel spawns, admission-before-completion,
/// daemon survival after client detach, and attributed usage.
#[test]
fn a1_spawn_three_children_in_parallel_and_they_settle_after_detach() {
    let _guard = lock();
    let home = tempfile::tempdir().unwrap();
    let server = start_server(home.path());
    let mut ws = ws_connect(&server);
    hello(&mut ws, home.path());
    let parent = {
        let store = optimus_kernel::SessionStore::open(home.path().join("sessions.db")).unwrap();
        store.create("parent").unwrap()
    };

    chat_demo_children(&mut ws, parent, 1001);

    // The parent client detaches before the children finish.
    ws.close(None).ok();
    let children = wait_children_terminal(home.path(), parent, 3);

    assert_eq!(children.len(), 3);
    for (child, status) in &children {
        assert_eq!(status, "succeeded", "child {child} must succeed offline");
        let events = child_events(home.path(), child);
        // Exactly one terminal event type, and the lifecycle order.
        assert_eq!(
            events.iter().filter(|e| e.as_str() == "succeeded").count(),
            1,
            "exactly one terminal event per child, got {events:?}"
        );
        assert!(events.contains(&"spawned".to_string()));
        assert!(events.contains(&"adopted".to_string()));
        assert!(events.contains(&"running".to_string()));
        assert!(events.contains(&"succeeded".to_string()));
    }

    // R7: attribution rows exist for all three children and reconcile
    // with the children's own model-call totals.
    let attributions = attribution_rows(home.path());
    assert_eq!(attributions.len(), 3, "one attribution row per child");
    let conn = Connection::open(home.path().join("execution.db")).unwrap();
    for (child, total, _duration) in &attributions {
        let child_total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(total_tokens), 0) FROM execution_model_calls
                 WHERE manifest_id = (
                     SELECT id FROM execution_manifests
                     WHERE session_id = ?1 ORDER BY created_unix DESC LIMIT 1)",
                [child],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            *total, child_total,
            "attribution totals must reconcile with the child manifest"
        );
    }

    // The context tree (R8): session detail carries the three children
    // with their attributed usage. The offline fixture records no
    // tokens, so the detail must reconcile with the manifest totals
    // (which the reconciliation above proved match the attribution rows).
    let detail = get_session(home.path(), parent).expect("session detail");
    assert_eq!(detail.children.len(), 3);
    for child in &detail.children {
        assert_eq!(child.status, "succeeded");
        let manifest_total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(total_tokens), 0) FROM execution_model_calls
                 WHERE manifest_id = (
                     SELECT id FROM execution_manifests
                     WHERE session_id = ?1 ORDER BY created_unix DESC LIMIT 1)",
                [child.child_session_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            child.total_tokens as i64, manifest_total,
            "child {} usage must reconcile with its manifest",
            child.child_session_id
        );
    }
}

/// A9: a surface chat turn targeting a child session refuses.
#[test]
fn a9_surface_turn_on_a_child_session_refuses() {
    let _guard = lock();
    let home = tempfile::tempdir().unwrap();
    let server = start_server(home.path());
    let mut ws = ws_connect(&server);
    hello(&mut ws, home.path());
    let parent = {
        let store = optimus_kernel::SessionStore::open(home.path().join("sessions.db")).unwrap();
        store.create("parent").unwrap()
    };

    chat_demo_children(&mut ws, parent, 1002);
    let children = wait_children_terminal(home.path(), parent, 3);
    let child = children[0].0.clone();

    // A fresh connection targets the child session directly.
    let mut ws2 = ws_connect(&server);
    hello(&mut ws2, home.path());
    ws2.send(Message::Text(
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "chat_start",
            "params": {
                "stream_id": 2001,
                "request": {
                    "session": child,
                    "message": "hello from a surface",
                    "provider": "offline",
                },
            }
        })
        .to_string()
        .into(),
    ))
    .expect("chat send");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline, "refusal did not arrive");
        if let Message::Text(text) = ws2.read().expect("chat read") {
            let value: Value = serde_json::from_str(&text).unwrap();
            if let Some(event) = value.get("params").and_then(|e| e.get("event")) {
                let kind = event.get("type").and_then(|t| t.as_str());
                if kind == Some("done") || kind == Some("error") {
                    let payload = event.get("result").cloned().unwrap_or_default();
                    let message = if kind == Some("done") {
                        payload.to_string()
                    } else {
                        event.get("error").cloned().unwrap_or_default().to_string()
                    };
                    assert!(
                        message.contains("is a child session; only the daemon may run its turns"),
                        "the refusal must name the child-session rule, got {message}"
                    );
                    return;
                }
            }
        }
    }
}

/// A2: adoption after a host restart — re-run never-started children,
/// settle interrupted ones, settle cancel-requested ones, and never
/// double-execute.
#[test]
fn a2_adoption_after_restart_runs_never_started_and_settles_interrupted() {
    let _guard = lock();
    let home = tempfile::tempdir().unwrap();
    let parent = uuid::Uuid::new_v4();
    let never_started = uuid::Uuid::new_v4();
    let interrupted = uuid::Uuid::new_v4();
    let cancel_requested = uuid::Uuid::new_v4();

    // Seed the pre-crash state: three non-terminal children. The real
    // additive schemas come from the kernel stores (the sessions and
    // execution tables, plus the registry the sweep would create).
    {
        drop(optimus_kernel::SessionStore::open(home.path().join("sessions.db")).unwrap());
        drop(optimus_kernel::ExecutionStore::open(home.path().join("execution.db")).unwrap());
        let conn = Connection::open(home.path().join("sessions.db")).unwrap();
        conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, packs_json, messages_json)
             VALUES (?1, 'parent', 'ts:1', 'ts:1', '[]', '[]'),
                    (?2, 'never started', 'ts:2', 'ts:2', '[]', '[]'),
                    (?3, 'interrupted', 'ts:3', 'ts:3', '[]', '[]'),
                    (?4, 'cancel requested', 'ts:4', 'ts:4', '[]', '[]')",
            [
                parent.to_string(),
                never_started.to_string(),
                interrupted.to_string(),
                cancel_requested.to_string(),
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_children
               (parent_session_id, child_session_id, depth, task_sha256, provider, model,
                effect_policy, autonomy_profile, command_fs_envelope, children_max_depth,
                status, parent_manifest_id, created_at)
             VALUES
               (?1, ?2, 1, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'offline', NULL, 'smart_deny', 'review_changes', NULL, 1,
                'spawned', NULL, 'ts:2'),
               (?1, ?3, 1, 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'offline', NULL, 'smart_deny', 'review_changes', NULL, 1,
                'running', NULL, 'ts:3'),
               (?1, ?4, 1, 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'offline', NULL, 'smart_deny', 'review_changes', NULL, 1,
                'spawned', NULL, 'ts:4')",
            [
                parent.to_string(),
                never_started.to_string(),
                interrupted.to_string(),
                cancel_requested.to_string(),
            ],
        )
        .unwrap();
        // The never-started child's task prompt lives in its transcript.
        conn.execute(
            "UPDATE sessions SET messages_json = ?1 WHERE id = ?2",
            [
                json!([
                    {"role": "system", "content": "you are a helper"},
                    {"role": "user", "content": "resume this task after the crash"},
                ])
                .to_string(),
                never_started.to_string(),
            ],
        )
        .unwrap();
        // The interrupted child has a manifest: the turn started, so
        // adoption must settle, never re-run.
        conn.execute(
            "UPDATE session_children SET cancel_requested = 'parent requested' WHERE child_session_id = ?1",
            [cancel_requested.to_string()],
        )
        .unwrap();
        drop(conn);

        let conn = Connection::open(home.path().join("execution.db")).unwrap();
        conn.execute(
            "INSERT INTO execution_manifests
               (id, version, session_id, turn_id, provider, model, autonomy_profile,
                command_fs_envelope, prompt_sha256, tool_catalog_sha256, policy_sha256,
                status, created_unix, completed_unix)
             VALUES ('00000000-0000-0000-0000-00000000000a', 1, ?1, 'turn-interrupted',
                     'offline', 'offline-model', 'review_changes', 'confined_no_network',
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                     'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                     'running', 1, NULL)",
            [interrupted.to_string()],
        )
        .unwrap();
        drop(conn);
    }

    // Restart: the adoption sweep runs before the server accepts clients.
    let server = start_server(home.path());
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let conn = Connection::open(home.path().join("sessions.db")).unwrap();
        let statuses = {
            let mut stmt = conn
                .prepare("SELECT child_session_id, status, terminal_reason FROM session_children")
                .unwrap();
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };
        drop(conn);
        let all_terminal = statuses
            .iter()
            .all(|(_, status, _)| matches!(status.as_str(), "succeeded" | "failed" | "cancelled"));
        if all_terminal {
            let by_id = statuses
                .into_iter()
                .map(|(id, status, reason)| (id, (status, reason)))
                .collect::<std::collections::HashMap<_, _>>();
            // The never-started child re-ran offline and succeeded.
            let (status, reason) = by_id.get(&never_started.to_string()).unwrap();
            assert_eq!(status, "succeeded", "never-started child must re-run");
            assert_eq!(reason, &None);
            // The interrupted child settled without re-running.
            let (status, reason) = by_id.get(&interrupted.to_string()).unwrap();
            assert_eq!(
                status, "failed",
                "interrupted child must settle, not re-run"
            );
            assert_eq!(
                reason.as_deref(),
                Some("crash_interrupted"),
                "the settle must carry the crash-window reason"
            );
            // The cancel-requested child settled without re-running.
            let (status, reason) = by_id.get(&cancel_requested.to_string()).unwrap();
            assert_eq!(
                status, "cancelled",
                "cancel-requested child must settle cancelled"
            );
            assert_eq!(
                reason.as_deref(),
                Some("cancel_requested"),
                "the settle must carry the durable-cancel reason"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "adoption did not settle: {statuses:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(server);

    // Exactly one terminal per child: the crash never produces a second
    // terminal event.
    for child in [never_started, interrupted, cancel_requested] {
        let events = child_events(home.path(), &child.to_string());
        let terminal = events
            .iter()
            .filter(|e| matches!(e.as_str(), "succeeded" | "failed" | "cancelled"))
            .count();
        assert_eq!(
            terminal, 1,
            "exactly one terminal event for {child}: {events:?}"
        );
    }
}
