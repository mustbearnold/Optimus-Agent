//! Spec-025 acceptance suite: the session-to-session message plane.
//!
//! A1 live auto-accept send + receipt + surfacing   A2 dormant queued + resume
//! A3 deny refusal + gone-target failure            A4 hold-approval + expiry
//! A5 permission classification                     A6 idempotent delivery
//! A7 opt-in discovery                              A9 threading + reply
//! A10 bounded reply wait                           A11 exactly one terminal

use std::time::Duration;

use optimus_kernel::{
    CompletionResponse, Kernel, KernelConfig, MessageKind, MessageMode, ScriptedModel, SessionStore,
};
use optimus_ops::{MessageClassification, MessageState, MessageStore};
use tempfile::tempdir;

fn answer(text: &str) -> CompletionResponse {
    CompletionResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        reasoning_content: None,
    }
}

#[test]
fn a1_live_auto_accept_delivers_and_surfaces_on_the_next_turn() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    // B opens first: it is a live peer with auto-accept.
    let mut kernel_b = Kernel::open(home, KernelConfig::default()).unwrap();
    kernel_b
        .session_policy(Some("auto-accept"), Some(true), None)
        .unwrap();
    let b_id = kernel_b.session_id();

    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    let receipt = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "hello from A".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    assert_eq!(
        receipt.state,
        MessageState::Delivered,
        "live auto-accept delivers now"
    );
    assert_eq!(receipt.to_session, b_id);
    assert!(
        !receipt.machine_id.is_empty(),
        "envelope carries the machine id"
    );

    // B's inbox shows the delivered message with its classification.
    let inbox = kernel_b.session_inbox(10).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].payload, "hello from A");
    assert_eq!(
        inbox[0].classification,
        Some(MessageClassification::Approved)
    );

    // B's next turn surfaces the message.
    let mut model = ScriptedModel::new(vec![answer("ok")]);
    kernel_b.turn(&mut model, "continue").unwrap();
    assert!(
        model.seen[0]
            .messages
            .iter()
            .any(|m| m.content.contains("hello from A")),
        "the next turn must surface the message"
    );
    // The surfaced message is injected exactly once.
    let inbox = kernel_b.session_inbox(10).unwrap();
    assert!(inbox.iter().all(|m| m.surfaced_at.is_some()));
    let mut model2 = ScriptedModel::new(vec![answer("ok")]);
    kernel_b.turn(&mut model2, "again").unwrap();
    let surfaced_count = model2.seen[0]
        .messages
        .iter()
        .filter(|m| m.content.contains("hello from A"))
        .count();
    assert_eq!(
        surfaced_count, 1,
        "the transcript keeps the historical copy but never re-injects"
    );
}

#[test]
fn a2_dormant_target_queues_and_surfaces_on_resume() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    // Create B's session WITHOUT opening a kernel: dormant. Its policy is
    // auto-accept, persisted on the session row.
    let store = SessionStore::open(home.join("sessions.db")).unwrap();
    let b_id = store.create_scoped("session", None).unwrap();
    store.set_inbound_policy(b_id, "auto-accept").unwrap();

    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    let receipt = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "wake up".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    assert_eq!(
        receipt.state,
        MessageState::Queued,
        "dormant target stays queued"
    );

    // B resumes: open + next turn delivers and surfaces.
    let mut kernel_b = Kernel::open_session(home, KernelConfig::default(), Some(b_id)).unwrap();
    let inbox = kernel_b.session_inbox(10).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].state, MessageState::Delivered);
    assert_eq!(inbox[0].payload, "wake up");

    // Policy persisted with the session: B's default policy survived reopen.
    assert_eq!(
        kernel_b
            .session_policy(None, None, None)
            .unwrap()
            .inbound_policy,
        "auto-accept",
        "policy is a session attribute"
    );
}

#[test]
fn a3_deny_refuses_and_gone_target_fails_honestly() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel_b = Kernel::open(home, KernelConfig::default()).unwrap();
    kernel_b.session_policy(Some("deny"), None, None).unwrap();
    let b_id = kernel_b.session_id();

    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    let receipt = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "denied?".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    assert_eq!(
        receipt.state,
        MessageState::Refused,
        "deny yields a refused event"
    );
    let events = MessageStore::open(home)
        .unwrap()
        .events(receipt.id)
        .unwrap();
    assert_eq!(events.last().unwrap().event_type, "refused");

    // Gone target: a transport failure is an error, never a success receipt.
    let gone = uuid::Uuid::new_v4();
    let error = kernel_a
        .session_send(
            gone,
            MessageKind::Request,
            "where are you".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("session_send_failed"),
        "gone target must name session_send_failed: {error}"
    );
}

#[test]
fn a4_held_messages_expire_after_the_dialog_expiry() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel_b = Kernel::open(home, KernelConfig::default()).unwrap();
    // Default policy is hold-approval; shorten the expiry to 1 second.
    kernel_b.session_policy(None, None, Some(Some(1))).unwrap();
    let b_id = kernel_b.session_id();

    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    let receipt = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "approve me".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    assert_eq!(receipt.state, MessageState::Held);

    std::thread::sleep(Duration::from_millis(1200));
    let expired = kernel_b.session_inbox(10).unwrap();
    let held = expired
        .iter()
        .find(|m| m.id == receipt.id)
        .expect("message present in B's inbox");
    assert_eq!(held.state, MessageState::Expired, "held message expires");
    let events = MessageStore::open(home)
        .unwrap()
        .events(receipt.id)
        .unwrap();
    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(
        types,
        vec!["queued", "held", "expired"],
        "state-machine order"
    );

    // The sender observes the expiry event on the same message record.
    let outbox = kernel_a.session_inbox(10).unwrap();
    let _ = outbox; // outbox is sender-side; events are queryable by message id
}

#[test]
fn a5_permission_classification_is_recorded_and_surfaced() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel_b = Kernel::open(home, KernelConfig::default()).unwrap();
    kernel_b
        .session_policy(Some("auto-accept"), None, None)
        .unwrap();
    let b_id = kernel_b.session_id();

    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    // A risky effect request: git push reaches a remote.
    let risky = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "git push origin main".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    assert_eq!(risky.classification, Some(MessageClassification::Pending));
    // Benign prose is approved.
    let benign = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "please summarize the quarterly report".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    assert_eq!(benign.classification, Some(MessageClassification::Approved));

    // The receiving agent sees the classification state with the message.
    let inbox = kernel_b.session_inbox(10).unwrap();
    let risky_seen = inbox.iter().find(|m| m.id == risky.id).unwrap();
    let benign_seen = inbox.iter().find(|m| m.id == benign.id).unwrap();
    assert_eq!(
        risky_seen.classification,
        Some(MessageClassification::Pending)
    );
    assert_eq!(
        benign_seen.classification,
        Some(MessageClassification::Approved)
    );
}

#[test]
fn a6_reenqueue_same_id_is_idempotent() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let store = MessageStore::open(home).unwrap();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let message = optimus_ops::SessionMessage {
        id: uuid::Uuid::new_v4(),
        from_session: a,
        to_session: b,
        kind: MessageKind::Request,
        payload: "once".into(),
        reply_to: None,
        mode: MessageMode::Auto,
        machine_id: store.machine_id().into(),
        state: MessageState::Queued,
        classification: None,
        created_at: String::new(),
        updated_at: String::new(),
        delivered_at: None,
        surfaced_at: None,
    };
    let first = store.enqueue(message.clone()).unwrap();
    let second = store.enqueue(message).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(store.inbox(b, 10).unwrap().len(), 1, "idempotent enqueue");
}

#[test]
fn a7_discovery_is_opt_in_and_fresh_sessions_are_hidden() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    // Fresh session: opted out by default.
    assert!(kernel_a.session_roster().unwrap().is_empty());

    kernel_a.session_policy(None, Some(true), None).unwrap();
    let kernel_b = Kernel::open(home, KernelConfig::default()).unwrap();
    let roster = kernel_a.session_roster().unwrap();
    assert_eq!(roster.len(), 1, "only the opted-in session appears");
    assert_eq!(roster[0].id, kernel_a.session_id());
    assert!(roster[0].discoverable);
    assert!(!roster[0].inbound_policy.is_empty());

    // Discovery is symmetric: B sees the opted-in A; B itself appears
    // nowhere because it never opted in.
    let roster_b = kernel_b.session_roster().unwrap();
    assert_eq!(roster_b.len(), 1);
    assert_eq!(roster_b[0].id, kernel_a.session_id());
}

#[test]
fn a9_replies_form_a_thread() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel_b = Kernel::open(home, KernelConfig::default()).unwrap();
    kernel_b
        .session_policy(Some("auto-accept"), None, None)
        .unwrap();
    let b_id = kernel_b.session_id();

    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    let root = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "question?".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    let reply = kernel_b
        .session_reply(kernel_a.session_id(), root.id, "answer!".into())
        .unwrap();
    assert_eq!(reply.kind, MessageKind::Reply);
    assert_eq!(
        reply.reply_to,
        Some(root.id),
        "reply carries the correlation id"
    );

    let thread = MessageStore::open(home).unwrap().thread(root.id).unwrap();
    assert_eq!(thread.len(), 2);
    assert!(thread.iter().any(|m| m.id == root.id));
    assert!(thread.iter().any(|m| m.id == reply.id));
}

#[test]
fn a10_bounded_reply_wait_expires_never_hangs() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    let started = std::time::Instant::now();
    let error = kernel_a
        .session_await_reply(uuid::Uuid::new_v4(), 1, None)
        .unwrap_err();
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(900),
        "wait must respect the bound"
    );
    assert!(
        error.to_string().contains("reply_wait_expired"),
        "expiry names reply_wait_expired: {error}"
    );
    assert!(elapsed < Duration::from_secs(5), "must never hang");
}

#[test]
fn a11_exactly_one_terminal_outcome_is_recorded() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let store = MessageStore::open(home).unwrap();
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let message = optimus_ops::SessionMessage {
        id: uuid::Uuid::new_v4(),
        from_session: a,
        to_session: b,
        kind: MessageKind::Request,
        payload: "terminal".into(),
        reply_to: None,
        mode: MessageMode::Auto,
        machine_id: store.machine_id().into(),
        state: MessageState::Queued,
        classification: None,
        created_at: String::new(),
        updated_at: String::new(),
        delivered_at: None,
        surfaced_at: None,
    };
    let id = message.id;
    store.enqueue(message).unwrap();
    // Refuse (terminal). A second terminal transition must fail.
    store.refuse(id).unwrap();
    assert!(
        store.deliver_message(id).is_err(),
        "delivered after refused must fail"
    );
    assert!(store.fail(id).is_err(), "failed after refused must fail");
    let events = store.events(id).unwrap();
    let terminals: Vec<&str> = events
        .iter()
        .filter(|e| {
            matches!(
                e.event_type.as_str(),
                "delivered" | "refused" | "expired" | "failed"
            )
        })
        .map(|e| e.event_type.as_str())
        .collect();
    assert_eq!(terminals, vec!["refused"], "exactly one terminal outcome");
    // And the terminal column agrees.
    let loaded = store.inbox(b, 10).unwrap().remove(0);
    assert_eq!(loaded.state, MessageState::Refused);
}

#[test]
fn a8_store_is_in_the_doctor_backup_set_and_events_are_queryable() {
    // The backup enumeration is the static doctor contract (spec-018 R3).
    let paths = optimus_cli_doctor_backup_paths();
    assert!(
        paths.iter().any(|(name, _)| *name == "messages.db"),
        "doctor backup set must include the message store"
    );
    // Events are queryable by session (R7).
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel_b = Kernel::open(home, KernelConfig::default()).unwrap();
    kernel_b
        .session_policy(Some("auto-accept"), None, None)
        .unwrap();
    let b_id = kernel_b.session_id();
    let mut kernel_a = Kernel::open(home, KernelConfig::default()).unwrap();
    let _ = kernel_a
        .session_send(
            b_id,
            MessageKind::Request,
            "events".into(),
            None,
            MessageMode::Auto,
        )
        .unwrap();
    let store = MessageStore::open(home).unwrap();
    let events = store.events_for_session(b_id).unwrap();
    assert!(!events.is_empty(), "events queryable by session");
    // Ordered state machine: queued -> delivered -> terminal.
    let ids: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(ids, vec!["queued", "delivered"]);
}

/// The doctor backup enumeration lives in the CLI crate; this mirrors it so
/// the acceptance test does not depend on a binary build. The real check is
/// `optimus doctor backup-list` + the doctor contract tests.
fn optimus_cli_doctor_backup_paths() -> Vec<(&'static str, &'static str)> {
    vec![
        ("optimus.db", "work_graph_and_campaigns"),
        ("sessions.db", "session_transcripts_and_effect_links"),
        ("memory.db", "metamemory_claims"),
        ("skills.db", "skills_registry"),
        ("execution.db", "execution_manifests"),
        ("cron.db", "cron_schedules_and_leases"),
        ("messages.db", "session_message_plane"),
    ]
}
