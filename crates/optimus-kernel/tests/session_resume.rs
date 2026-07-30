//! Durable session resume tests.

use optimus_graph::AutonomyProfile;
use optimus_kernel::{
    list_sessions, CompletionResponse, ExecutionStatus, ExecutionStore, Kernel, KernelConfig,
    KernelError, Message, PolicyMode, ReplayClassification, ScriptedModel, SessionStore, ToolCall,
    TurnStatus,
};
use optimus_packs::{PackError, PackId};
use rusqlite::{params, Connection};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn legacy_manifest_migrates_to_review_changes_authority() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("execution.db");
    let manifest_id = uuid::Uuid::new_v4();
    let session_id = uuid::Uuid::new_v4();
    let turn_id = uuid::Uuid::new_v4();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE execution_manifests(
               id TEXT PRIMARY KEY,version INTEGER NOT NULL,session_id TEXT NOT NULL,
               turn_id TEXT NOT NULL UNIQUE,provider TEXT NOT NULL,model TEXT NOT NULL,
               prompt_sha256 TEXT NOT NULL,tool_catalog_sha256 TEXT NOT NULL,
               policy_sha256 TEXT NOT NULL,status TEXT NOT NULL,
               created_unix INTEGER NOT NULL,completed_unix INTEGER
             );",
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO execution_manifests(
               id,version,session_id,turn_id,provider,model,prompt_sha256,
               tool_catalog_sha256,policy_sha256,status,created_unix,completed_unix
             ) VALUES (?1,1,?2,?3,'offline','offline-scripted',?4,?4,?4,'running',1,NULL)",
            params![
                manifest_id.to_string(),
                session_id.to_string(),
                turn_id.to_string(),
                "0".repeat(64),
            ],
        )
        .unwrap();
    drop(connection);

    let store = ExecutionStore::open(&path).unwrap();
    let manifest = store.manifest(manifest_id).unwrap();
    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.autonomy_profile, "review_changes");
}

#[test]
fn session_survives_process_reopen() {
    let dir = tempdir().unwrap();
    let home = dir.path();

    let session_id = {
        let mut k = Kernel::open(home, KernelConfig::default()).unwrap();
        let id = k.session_id();
        let mut model = ScriptedModel::new(vec![
            CompletionResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "a1".into(),
                    name: "activate_pack".into(),
                    arguments: json!({"name": "browser"}),
                }],
            },
            CompletionResponse {
                text: Some("browser on".into()),
                tool_calls: vec![],
            },
        ]);
        k.turn(&mut model, "enable browser please").unwrap();
        assert!(k.packs.loaded_packs().contains(&PackId::Browser));
        id
    };

    // "New process"
    let k2 = Kernel::open_session(home, KernelConfig::default(), Some(session_id)).unwrap();
    assert_eq!(k2.session_id(), session_id);
    assert!(k2.packs.loaded_packs().contains(&PackId::Browser));
    assert!(k2
        .messages
        .iter()
        .any(|m| m.content.contains("enable browser please")));
    assert!(k2.messages.iter().any(|m| m.content.contains("browser on")));

    let listed = list_sessions(home).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, session_id);
    assert!(listed[0].message_count >= 3);
}

#[test]
fn new_session_ids_differ() {
    let dir = tempdir().unwrap();
    let a = Kernel::open(dir.path(), KernelConfig::default())
        .unwrap()
        .session_id();
    let b = Kernel::open(dir.path(), KernelConfig::default())
        .unwrap()
        .session_id();
    assert_ne!(a, b);
    assert_eq!(list_sessions(dir.path()).unwrap().len(), 2);
}

#[test]
fn session_resume_rejects_unknown_persisted_pack() {
    let dir = tempdir().unwrap();
    let session_id = Kernel::open(dir.path(), KernelConfig::default())
        .unwrap()
        .session_id();
    let conn = rusqlite::Connection::open(dir.path().join("sessions.db")).unwrap();
    conn.execute(
        "UPDATE sessions SET packs_json = ?1 WHERE id = ?2",
        rusqlite::params!["[\"retired-pack\"]", session_id.to_string()],
    )
    .unwrap();

    let error = match Kernel::open_session(dir.path(), KernelConfig::default(), Some(session_id)) {
        Ok(_) => panic!("stale pack unexpectedly resumed"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        KernelError::Packs(PackError::UnknownPack(name)) if name == "retired-pack"
    ));
}

#[test]
fn durable_tool_message_is_bound_to_terminal_effect_attempt() {
    let directory = tempdir().unwrap();
    let mut kernel = Kernel::open(
        directory.path(),
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
        },
    )
    .unwrap();
    let session_id = kernel.session_id();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "write-call-1".into(),
                name: "write_file".into(),
                arguments: json!({"path":"causal.txt","contents":"linked"}),
            }],
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
        },
    ]);

    kernel.turn(&mut model, "write with provenance").unwrap();

    let store = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let links = store.effect_links(session_id).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].tool_call_id, "write-call-1");
    assert_eq!(links[0].outcome, "succeeded");
    assert_eq!(links[0].effect_hash.len(), 64);
    assert_eq!(links[0].receipt_hash.as_ref().unwrap().len(), 64);
    let (_, messages, _) = store.load(session_id).unwrap();
    assert!(messages.iter().any(|message| {
        message.tool_call_id.as_deref() == Some("write-call-1")
            && message
                .content
                .contains(links[0].job_id.to_string().as_str())
    }));
}

#[test]
fn missing_tool_message_is_repaired_from_effect_link_on_reopen() {
    let directory = tempdir().unwrap();
    let mut kernel = Kernel::open(
        directory.path(),
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
        },
    )
    .unwrap();
    let session_id = kernel.session_id();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "write-call-repair".into(),
                name: "write_file".into(),
                arguments: json!({"path":"repair-me.txt","contents":"durable"}),
            }],
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
        },
    ]);
    kernel
        .turn(&mut model, "write then lose tool card")
        .unwrap();

    let store = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let links = store.effect_links(session_id).unwrap();
    assert_eq!(links.len(), 1);
    let (packs, mut messages, title) = store.load(session_id).unwrap();
    messages.retain(|message| {
        !(message.role == optimus_kernel::Role::Tool
            && message.tool_call_id.as_deref() == Some("write-call-repair"))
    });
    store.save(session_id, &title, &packs, &messages).unwrap();
    assert!(!store
        .load(session_id)
        .unwrap()
        .1
        .iter()
        .any(|message| { message.tool_call_id.as_deref() == Some("write-call-repair") }));

    let reopened =
        Kernel::open_session(directory.path(), KernelConfig::default(), Some(session_id)).unwrap();
    assert!(reopened.messages.iter().any(|message| {
        message.role == optimus_kernel::Role::Tool
            && message.tool_call_id.as_deref() == Some("write-call-repair")
            && message.content.contains("repaired")
            && message
                .content
                .contains(links[0].job_id.to_string().as_str())
    }));
    // Repair is durable for the next open.
    let (_, messages_after, _) = store.load(session_id).unwrap();
    assert!(messages_after.iter().any(|message| {
        message.tool_call_id.as_deref() == Some("write-call-repair")
            && message.content.contains("repaired")
    }));
}

#[test]
fn conflicting_effect_link_rolls_back_session_snapshot() {
    let directory = tempdir().unwrap();
    let mut kernel = Kernel::open(
        directory.path(),
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
        },
    )
    .unwrap();
    let session_id = kernel.session_id();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![ToolCall {
                id: "write-call-1".into(),
                name: "write_file".into(),
                arguments: json!({"path":"causal.txt","contents":"linked"}),
            }],
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
        },
    ]);
    kernel.turn(&mut model, "write with provenance").unwrap();
    let store = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let mut conflicting = store.effect_links(session_id).unwrap().pop().unwrap();
    conflicting.effect_hash = "0".repeat(64);
    let before = store.load(session_id).unwrap().1;
    let replacement = vec![Message {
        role: optimus_kernel::Role::User,
        content: "must roll back".into(),
        tool_call_id: None,
        name: None,
    }];

    let error = store
        .save_with_effect_links(session_id, "changed", &[], &replacement, &[conflicting])
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("conflicting durable effect provenance"));
    assert_eq!(store.load(session_id).unwrap().1, before);
}

#[test]
fn failed_turn_persists_accepted_boundary_and_exactly_one_terminal_event() {
    let directory = tempdir().unwrap();
    let config = KernelConfig {
        max_steps: 0,
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open(directory.path(), config).unwrap();
    let session_id = kernel.session_id();
    let mut model = ScriptedModel::new(vec![]);

    assert!(matches!(
        kernel
            .turn(&mut model, "accepted before failure")
            .unwrap_err(),
        KernelError::MaxSteps(0)
    ));

    let store = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let turns = store.turns(session_id).unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].status, TurnStatus::Failed);
    assert_eq!(turns[0].error_code.as_deref(), Some("max_steps"));
    assert!(store.active_turn(session_id).unwrap().is_none());
    assert_eq!(store.turn_event_count(turns[0].id).unwrap(), 2);
    let (_, messages, title) = store.load(session_id).unwrap();
    assert!(messages
        .iter()
        .any(|message| message.content == "accepted before failure"));

    assert!(store
        .finish_turn(
            turns[0].id,
            session_id,
            &title,
            &[],
            &messages,
            TurnStatus::Failed,
            Some("max_steps")
        )
        .is_err());
    assert_eq!(store.turn_event_count(turns[0].id).unwrap(), 2);
}

#[test]
fn interrupted_turn_resumes_without_duplicating_user_segment() {
    let directory = tempdir().unwrap();
    let kernel = Kernel::open(directory.path(), KernelConfig::default()).unwrap();
    let session_id = kernel.session_id();
    let start_message_count = kernel.messages.len();
    let mut accepted_messages = kernel.messages.clone();
    accepted_messages.push(Message {
        role: optimus_kernel::Role::User,
        content: "resume this once".into(),
        tool_call_id: None,
        name: None,
    });
    let store = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let turn_id = store
        .begin_turn(
            session_id,
            "resume this once",
            &["core".into()],
            &accepted_messages,
            start_message_count,
        )
        .unwrap();
    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    let (manifest_id, original_trace) = executions
        .begin_traced(
            session_id,
            turn_id,
            "offline",
            "offline-scripted",
            "review_changes",
            b"resume this once",
            b"tools",
            b"policy",
        )
        .unwrap();
    drop(executions);
    drop(store);
    drop(kernel);

    let mut resumed =
        Kernel::open_session(directory.path(), KernelConfig::default(), Some(session_id)).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: Some("continued".into()),
        tool_calls: vec![],
    }]);
    let result = resumed.resume_pending_turn(&mut model).unwrap();

    assert_eq!(result.assistant_text, "continued");
    assert_eq!(result.trace_context, original_trace);
    assert_eq!(
        resumed
            .messages
            .iter()
            .filter(|message| message.content == "resume this once")
            .count(),
        1
    );
    let store = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let turn = store
        .turns(session_id)
        .unwrap()
        .into_iter()
        .find(|turn| turn.id == turn_id)
        .unwrap();
    assert_eq!(turn.status, TurnStatus::Succeeded);
    assert_eq!(store.turn_event_count(turn_id).unwrap(), 2);
    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    assert_eq!(executions.find_by_turn(turn_id).unwrap(), Some(manifest_id));
    assert_eq!(
        executions.trace_context(manifest_id).unwrap(),
        Some(original_trace)
    );
}

#[test]
fn resume_rejects_terminal_traced_manifest_before_model_execution() {
    let directory = tempdir().unwrap();
    let kernel = Kernel::open(directory.path(), KernelConfig::default()).unwrap();
    let session_id = kernel.session_id();
    let start_message_count = kernel.messages.len();
    let mut accepted_messages = kernel.messages.clone();
    accepted_messages.push(Message {
        role: optimus_kernel::Role::User,
        content: "must not rerun".into(),
        tool_call_id: None,
        name: None,
    });
    let sessions = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let turn_id = sessions
        .begin_turn(
            session_id,
            "must not rerun",
            &["core".into()],
            &accepted_messages,
            start_message_count,
        )
        .unwrap();
    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    let (manifest_id, _) = executions
        .begin_traced(
            session_id,
            turn_id,
            "offline",
            "offline-scripted",
            "review_changes",
            b"must not rerun",
            b"tools",
            b"policy",
        )
        .unwrap();
    executions
        .finish(manifest_id, ExecutionStatus::Succeeded)
        .unwrap();
    drop(executions);
    drop(sessions);
    drop(kernel);

    let mut resumed =
        Kernel::open_session(directory.path(), KernelConfig::default(), Some(session_id)).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: Some("should not run".into()),
        tool_calls: vec![],
    }]);

    assert!(resumed.resume_pending_turn(&mut model).is_err());
    let sessions = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    assert!(sessions.active_turn(session_id).unwrap().is_some());
    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    assert_eq!(
        executions
            .replay_report(manifest_id)
            .unwrap()
            .model_call_count,
        0
    );
}

#[test]
fn resume_rejects_untraced_manifest_before_model_execution() {
    let directory = tempdir().unwrap();
    let kernel = Kernel::open(directory.path(), KernelConfig::default()).unwrap();
    let session_id = kernel.session_id();
    let mut accepted_messages = kernel.messages.clone();
    accepted_messages.push(Message {
        role: optimus_kernel::Role::User,
        content: "missing trace".into(),
        tool_call_id: None,
        name: None,
    });
    let sessions = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let turn_id = sessions
        .begin_turn(
            session_id,
            "missing trace",
            &["core".into()],
            &accepted_messages,
            kernel.messages.len(),
        )
        .unwrap();
    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    let manifest_id = executions
        .begin(
            session_id,
            turn_id,
            "offline",
            "offline-scripted",
            b"missing trace",
            b"tools",
            b"policy",
        )
        .unwrap();
    drop(executions);
    drop(sessions);
    drop(kernel);

    let mut resumed =
        Kernel::open_session(directory.path(), KernelConfig::default(), Some(session_id)).unwrap();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: Some("should not run".into()),
        tool_calls: vec![],
    }]);

    assert!(resumed.resume_pending_turn(&mut model).is_err());
    let sessions = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    assert!(sessions.active_turn(session_id).unwrap().is_some());
    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    assert_eq!(
        executions
            .replay_report(manifest_id)
            .unwrap()
            .model_call_count,
        0
    );
    assert_eq!(executions.trace_context(manifest_id).unwrap(), None);
}

#[test]
fn kernel_turn_persists_versioned_manifest_and_replay_report() {
    let directory = tempdir().unwrap();
    let mut kernel = Kernel::open(
        directory.path(),
        KernelConfig {
            autonomy_profile: AutonomyProfile::Standard,
            ..KernelConfig::default()
        },
    )
    .unwrap();
    let session_id = kernel.session_id();
    let mut model = ScriptedModel::new(vec![CompletionResponse {
        text: Some("manifest complete".into()),
        tool_calls: vec![],
    }]);

    kernel.turn(&mut model, "record this execution").unwrap();

    let sessions = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let turn = sessions.turns(session_id).unwrap().pop().unwrap();
    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    let manifest_id = executions.find_by_turn(turn.id).unwrap().unwrap();
    let manifest = executions.manifest(manifest_id).unwrap();
    assert_eq!(manifest.version, 2);
    assert_eq!(manifest.provider, "offline");
    assert_eq!(manifest.model, "offline-scripted");
    assert_eq!(manifest.autonomy_profile, "standard");
    assert_eq!(manifest.status, ExecutionStatus::Succeeded);
    assert_eq!(manifest.prompt_sha256.len(), 64);
    assert_eq!(manifest.tool_catalog_sha256.len(), 64);
    assert_eq!(manifest.policy_sha256.len(), 64);
    let report = executions.replay_report(manifest_id).unwrap();
    assert_eq!(
        report.classification,
        ReplayClassification::FixtureReplayable
    );
    assert_eq!(report.model_call_count, 1);
    assert_eq!(report.tool_call_count, 0);
}

#[test]
fn missing_tool_messages_for_two_effect_links_are_both_repaired() {
    let directory = tempdir().unwrap();
    let mut kernel = Kernel::open(
        directory.path(),
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
        },
    )
    .unwrap();
    let session_id = kernel.session_id();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![
                ToolCall {
                    id: "write-a".into(),
                    name: "write_file".into(),
                    arguments: json!({"path":"a.txt","contents":"A"}),
                },
                ToolCall {
                    id: "write-b".into(),
                    name: "write_file".into(),
                    arguments: json!({"path":"b.txt","contents":"B"}),
                },
            ],
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
        },
    ]);
    kernel.turn(&mut model, "two durable writes").unwrap();
    let store = SessionStore::open(directory.path().join("sessions.db")).unwrap();
    let links = store.effect_links(session_id).unwrap();
    assert!(links.len() >= 2, "links={}", links.len());
    let (packs, mut messages, title) = store.load(session_id).unwrap();
    messages.retain(|message| message.role != optimus_kernel::Role::Tool);
    store.save(session_id, &title, &packs, &messages).unwrap();
    let reopened =
        Kernel::open_session(directory.path(), KernelConfig::default(), Some(session_id)).unwrap();
    for call_id in ["write-a", "write-b"] {
        assert!(
            reopened.messages.iter().any(|message| {
                message.role == optimus_kernel::Role::Tool
                    && message.tool_call_id.as_deref() == Some(call_id)
                    && message.content.contains("repaired")
            }),
            "missing repaired tool for {call_id}"
        );
    }
}
