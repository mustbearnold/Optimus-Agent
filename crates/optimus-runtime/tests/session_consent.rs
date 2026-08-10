//! spec-014 R7 / ADR-0081: session-scoped capability consent.
//!
//! A5: "Given session consent for (SystemModify, OpaqueShell) under a live
//! Developer Full Access grant, when a `sh -c` effect runs in the same durable
//! session (including immediately after an approval resolution), then it
//! auto-grants with an exact-effect audit row; and after scope widening, DFA
//! disable, expiry, or revocation, then it asks again."

use optimus_graph::{Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_policy::{
    DeveloperAccessGrant, DeveloperCapabilities, DeveloperScope,
    DEVELOPER_ACCESS_CONFIRMATION_VERSION,
};
use optimus_runtime::{ApprovalGrant, Runtime};
use tempfile::tempdir;

#[cfg(windows)]
fn command_effect(script: &str) -> Effect {
    Effect::RunCommand {
        program: "cmd".into(),
        args: vec!["/C".into(), script.into()],
    }
}

#[cfg(unix)]
fn command_effect(script: &str) -> Effect {
    Effect::RunCommand {
        program: "sh".into(),
        args: vec!["-c".into(), script.into()],
    }
}

fn valid_grant(root: &str) -> DeveloperAccessGrant {
    // Terminal execution is on; OpaqueShell (SystemModify) is deliberately
    // NOT user-enablable, so `sh -c` still Asks under DFA — that is the pause
    // the session consent is supposed to remove.
    let capabilities = DeveloperCapabilities {
        terminal_execution: true,
        ..Default::default()
    };
    DeveloperAccessGrant {
        enabled: true,
        confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
        issued_unix: 1,
        pause_before_destructive: false,
        scope: DeveloperScope::SelectedRepository {
            root: root.into(),
            root_hash: None,
        },
        capabilities,
        ..Default::default()
    }
}

/// Open a runtime with a live DFA grant and a durable consent session id.
fn open_dfa_runtime(
    db: &std::path::Path,
    workspace: &std::path::Path,
    consent_session_id: &str,
) -> Runtime {
    Runtime::open_with_developer_access(
        db,
        workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: optimus_graph::AutonomyProfile::DeveloperFullAccess,
            consent_session_id: Some(consent_session_id.into()),
            ..Default::default()
        },
        Some(valid_grant(workspace.to_str().unwrap())),
        vec![workspace.to_path_buf()],
    )
    .unwrap()
}

/// Create a job whose single node is `sh -c "<script>"` (OpaqueShell).
fn shell_job(label: &str, script: &str) -> JobSpec {
    JobSpec {
        label: label.into(),
        budget: Default::default(),
        nodes: vec![NodeSpec {
            label: "echo".into(),
            effect: command_effect(script),
        }],
    }
}

#[test]
fn session_consent_auto_grants_under_live_dfa() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = open_dfa_runtime(&db, &ws, "sess-1");

    rt.grant_session_consent("sess-1", "opaque_shell", 8 * 3600)
        .unwrap();

    // First run: no consent yet consumed; the exact effect must auto-grant
    // without a pause.
    let job = rt.create_job(shell_job("consented", "echo ok")).unwrap();
    let status = rt.run_all(job).unwrap();
    assert_eq!(status, optimus_graph::JobStatus::Succeeded);

    // The auto-grant wrote an exact-effect audit row naming the consent.
    let approvals = rt.list_action_approvals(50).unwrap();
    assert_eq!(approvals.len(), 1);
    assert!(approvals[0]
        .actor
        .starts_with("session_consent:opaque_shell"));
}

#[test]
fn session_consent_expiry_asks_again() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = open_dfa_runtime(&db, &ws, "sess-2");

    // The store clamps TTL to the 8 h floor, so expiry cannot be reached by
    // sleeping. Instead, backdate the grant through a direct store handle:
    // created 9 h ago with the default 8 h TTL → already expired at use time.
    let store = optimus_store::Store::open(&db).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    store
        .grant_capability(
            "sess-2",
            "system.modify",
            "opaque_shell",
            &rt.workspace_sha256(),
            8 * 3600,
            now.saturating_sub(9 * 3600),
        )
        .unwrap();

    let job = rt.create_job(shell_job("expired", "echo late")).unwrap();
    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
}

#[test]
fn session_consent_revocation_asks_again() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = open_dfa_runtime(&db, &ws, "sess-3");

    rt.grant_session_consent("sess-3", "opaque_shell", 8 * 3600)
        .unwrap();
    assert!(rt.revoke_session_consent("sess-3", "opaque_shell").unwrap());

    let job = rt.create_job(shell_job("revoked", "echo nope")).unwrap();
    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
}

#[test]
fn session_consent_revoke_all_flips_the_session_back_to_asking() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = open_dfa_runtime(&db, &ws, "sess-4");

    rt.grant_session_consent("sess-4", "opaque_shell", 8 * 3600)
        .unwrap();
    rt.grant_session_consent("sess-4", "project_execute", 8 * 3600)
        .unwrap();
    assert_eq!(rt.revoke_session_consents("sess-4").unwrap(), 2);

    let job = rt
        .create_job(shell_job("revoked-all", "echo nope"))
        .unwrap();
    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
}

#[test]
fn session_consent_is_scoped_to_the_workspace() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws_a = dir.path().join("ws-a");
    let ws_b = dir.path().join("ws-b");
    std::fs::create_dir_all(&ws_a).unwrap();
    std::fs::create_dir_all(&ws_b).unwrap();

    // Grant minted in workspace A.
    let rt_a = open_dfa_runtime(&db, &ws_a, "sess-5");
    rt_a.grant_session_consent("sess-5", "opaque_shell", 8 * 3600)
        .unwrap();

    // The same durable session, same store — but running in workspace B. The
    // consent's scope_sha256 is pinned to A, so B must ask again.
    let rt_b = open_dfa_runtime(&db, &ws_b, "sess-5");
    let job = rt_b.create_job(shell_job("other-ws", "echo nope")).unwrap();
    let err = rt_b.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
}

#[test]
fn session_consent_requires_live_dfa() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();

    // Grant consent while DFA is live.
    let rt = open_dfa_runtime(&db, &ws, "sess-6");
    rt.grant_session_consent("sess-6", "opaque_shell", 8 * 3600)
        .unwrap();

    // Reopen the same store with NO developer grant: the consent row is still
    // there, but DFA liveness is revalidated at use time (A5) → Ask again.
    let rt_no_dfa = Runtime::open_with_config(
        &db,
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: optimus_graph::AutonomyProfile::DeveloperFullAccess,
            consent_session_id: Some("sess-6".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let job = rt_no_dfa
        .create_job(shell_job("no-dfa", "echo nope"))
        .unwrap();
    let err = rt_no_dfa.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
}

/// Open a runtime with an explicit developer grant. This mirrors
/// `open_dfa_runtime`, but the caller controls the grant itself, so a
/// test can flip `terminal_execution` without changing the shared helper.
fn open_dfa_runtime_with_grant(
    db: &std::path::Path,
    workspace: &std::path::Path,
    consent_session_id: &str,
    grant: DeveloperAccessGrant,
) -> Runtime {
    Runtime::open_with_developer_access(
        db,
        workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: optimus_graph::AutonomyProfile::DeveloperFullAccess,
            consent_session_id: Some(consent_session_id.into()),
            ..Default::default()
        },
        Some(grant),
        vec![workspace.to_path_buf()],
    )
    .unwrap()
}

#[test]
fn session_consent_requires_terminal_execution_capability() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();

    // The grant stays enabled, but terminal execution is off. A live
    // session consent must NOT cover OpaqueShell in this state: the
    // runtime revalidates `enabled && terminal_execution` at use time,
    // so the shell effect must Ask again.
    let mut grant = valid_grant(ws.to_str().unwrap());
    grant.capabilities.terminal_execution = false;
    let rt = open_dfa_runtime_with_grant(&db, &ws, "sess-7", grant);

    rt.grant_session_consent("sess-7", "opaque_shell", 8 * 3600)
        .unwrap();

    let job = rt
        .create_job(shell_job("no-terminal", "echo nope"))
        .unwrap();
    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
}

#[test]
fn session_consent_is_live_immediately_after_an_approval_resolution() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = open_dfa_runtime(&db, &ws, "sess-8");

    // No consent yet: the shell job pauses for a human approval.
    let asking = rt.create_job(shell_job("asks-first", "echo one")).unwrap();
    let err = rt.run_next(asking).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));

    // Resolve the pause manually: grant the job-scoped approval, then
    // continue. This is the approval-resolution step of A5.
    rt.grant_approval(ApprovalGrant::for_job(asking)).unwrap();
    assert_eq!(
        rt.run_all(asking).unwrap(),
        optimus_graph::JobStatus::Succeeded
    );

    // Consent is granted AFTER the resolution. The very next shell job
    // in the same durable session must auto-grant without a pause.
    rt.grant_session_consent("sess-8", "opaque_shell", 8 * 3600)
        .unwrap();
    let next = rt
        .create_job(shell_job("consented-after", "echo two"))
        .unwrap();
    assert_eq!(
        rt.run_all(next).unwrap(),
        optimus_graph::JobStatus::Succeeded
    );
}

#[test]
fn session_consent_auto_continues_across_multiple_nodes() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = open_dfa_runtime(&db, &ws, "sess-9");

    rt.grant_session_consent("sess-9", "opaque_shell", 8 * 3600)
        .unwrap();

    // Two OpaqueShell nodes in one job. Consent covers the class, so
    // run_all must auto-continue through BOTH nodes and finish.
    let job = rt
        .create_job(JobSpec {
            label: "two-shell-nodes".into(),
            budget: Default::default(),
            nodes: vec![
                NodeSpec {
                    label: "first".into(),
                    effect: command_effect("echo one > first.txt"),
                },
                NodeSpec {
                    label: "second".into(),
                    effect: command_effect("echo two > second.txt"),
                },
            ],
        })
        .unwrap();

    assert_eq!(
        rt.run_all(job).unwrap(),
        optimus_graph::JobStatus::Succeeded
    );
    assert_eq!(
        rt.node_statuses(job).unwrap(),
        vec![
            optimus_graph::NodeStatus::Succeeded,
            optimus_graph::NodeStatus::Succeeded,
        ]
    );
    // Both effects really ran, and each one wrote its own exact-effect
    // audit row naming the session consent.
    assert_eq!(
        std::fs::read_to_string(ws.join("first.txt"))
            .unwrap()
            .trim(),
        "one"
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("second.txt"))
            .unwrap()
            .trim(),
        "two"
    );
    let approvals = rt.list_action_approvals(50).unwrap();
    assert_eq!(approvals.len(), 2);
    assert!(approvals
        .iter()
        .all(|approval| approval.actor.starts_with("session_consent:opaque_shell")));
}
