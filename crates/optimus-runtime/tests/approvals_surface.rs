//! SmartDeny pending approval list + grant/resume.

use optimus_graph::{Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_runtime::{ApprovalGrant, Runtime};
use rusqlite::Connection;
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

#[test]
fn pending_approval_list_and_grant_resume() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("o.db");
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open_with_config(
        &db,
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            ..Default::default()
        },
    )
    .unwrap();

    let job = rt
        .create_job(JobSpec {
            label: "needs-ok".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "echo".into(),
                effect: command_effect("echo approved"),
            }],
        })
        .unwrap();

    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));

    let pending = rt.list_pending_approvals().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].job_id, job);
    assert!(!pending[0].has_grant);

    let status = rt.grant_and_resume(job).unwrap();
    // May already be Succeeded after single-node resume, or still running.
    let _ = status;
    let _ = rt.run_all(job);
    assert_eq!(
        rt.job_status(job).unwrap(),
        optimus_graph::JobStatus::Succeeded
    );
    assert!(rt.list_pending_approvals().unwrap().is_empty());
}

#[test]
fn grant_approval_marks_has_grant() {
    let dir = tempdir().unwrap();
    let rt = Runtime::open(&dir.path().join("o.db"), &dir.path().join("ws")).unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "g".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "c".into(),
                effect: command_effect("exit 0"),
            }],
        })
        .unwrap();
    let _ = rt.run_next(job);
    rt.grant_approval(ApprovalGrant::for_job(job)).unwrap();
    let p = rt.list_pending_approvals().unwrap();
    assert_eq!(p.len(), 1);
    assert!(p[0].has_grant);
}

#[test]
fn grant_is_bound_to_exact_node_effect_hash_actor_and_expiry() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("bound.db");
    let rt = Runtime::open(&db, &dir.path().join("ws")).unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "bound".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "command".into(),
                effect: command_effect("exit 0"),
            }],
        })
        .unwrap();
    let _ = rt.run_next(job);

    rt.grant_approval(ApprovalGrant::for_job_by(job, "alice", 120))
        .unwrap();

    let connection = Connection::open(&db).unwrap();
    let (node_id, effect_hash, actor, created, expires): (String, String, String, i64, i64) =
        connection
            .query_row(
                "SELECT node_id,effect_hash,actor,created_unix,expires_unix
                 FROM action_approvals WHERE job_id=?1",
                [job.0.to_string()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
    assert!(!node_id.is_empty());
    assert_eq!(effect_hash.len(), 64);
    assert!(effect_hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(actor, "alice");
    assert!(expires > created);
}

#[test]
fn corrupt_effect_is_rejected_before_projection_or_event_mutation() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("corrupt.db");
    let rt = Runtime::open(&db, &dir.path().join("ws")).unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "corrupt effect".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write".into(),
                effect: Effect::WriteFile {
                    relative_path: "never.txt".into(),
                    contents: "never".into(),
                },
            }],
        })
        .unwrap();
    let connection = Connection::open(&db).unwrap();
    connection
        .execute(
            "UPDATE nodes SET effect_json='{broken' WHERE job_id=?1",
            [job.0.to_string()],
        )
        .unwrap();
    let before: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM events WHERE job_id=?1",
            [job.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();

    assert!(rt.run_next(job).is_err());
    assert_eq!(
        rt.job_status(job).unwrap(),
        optimus_graph::JobStatus::Pending
    );
    let after: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM events WHERE job_id=?1",
            [job.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
    assert!(!dir.path().join("ws/never.txt").exists());
}

#[test]
fn plain_write_file_is_high_risk_under_smart_deny() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open_with_config(
        &dir.path().join("write.db"),
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            ..Default::default()
        },
    )
    .unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "write high-risk".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write".into(),
                effect: Effect::WriteFile {
                    relative_path: "proof.txt".into(),
                    contents: "needs approval".into(),
                },
            }],
        })
        .unwrap();

    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
    assert!(!ws.join("proof.txt").exists());
    assert_eq!(
        rt.grant_and_resume(job).unwrap(),
        optimus_graph::JobStatus::Succeeded
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("proof.txt")).unwrap(),
        "needs approval"
    );
}

#[test]
fn illegal_write_path_is_rejected_before_approval_wait() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open(&dir.path().join("preflight.db"), &ws).unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "preflight".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write".into(),
                effect: Effect::WriteFile {
                    relative_path: "../escape.txt".into(),
                    contents: "no".into(),
                },
            }],
        })
        .unwrap();

    let err = rt.run_next(job).unwrap_err();
    assert!(
        matches!(err, optimus_runtime::RuntimeError::PathEscape(_)),
        "expected path preflight, got {err:?}"
    );
    assert_eq!(
        rt.job_status(job).unwrap(),
        optimus_graph::JobStatus::Pending
    );
    assert!(rt.list_pending_approvals().unwrap().is_empty());
}

#[test]
fn project_write_approval_cannot_replay_in_a_different_workspace() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("project-bound.db");
    let root_a = dir.path().join("project-a");
    let root_b = dir.path().join("project-b");
    std::fs::create_dir_all(&root_a).unwrap();
    std::fs::create_dir_all(&root_b).unwrap();
    let runtime_a = Runtime::open(&db, &root_a).unwrap();
    let job = runtime_a
        .create_job(JobSpec {
            label: "project-bound write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write".into(),
                effect: Effect::ProjectWriteFile {
                    workspace_sha256: runtime_a.workspace_sha256(),
                    relative_path: "proof.txt".into(),
                    contents: "bound to project a".into(),
                },
            }],
        })
        .unwrap();

    assert_eq!(
        runtime_a.run_all(job).unwrap(),
        optimus_graph::JobStatus::AwaitingApproval
    );

    let runtime_b = Runtime::open(&db, &root_b).unwrap();
    runtime_b
        .grant_approval(ApprovalGrant::for_job(job))
        .unwrap();
    assert_eq!(
        runtime_b.run_all(job).unwrap(),
        optimus_graph::JobStatus::Failed
    );
    assert!(!root_a.join("proof.txt").exists());
    assert!(!root_b.join("proof.txt").exists());
}

#[test]
fn exact_project_write_runs_after_approval_in_the_bound_workspace() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("project-approved.db");
    let root = dir.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    let runtime = Runtime::open(&db, &root).unwrap();
    let job = runtime
        .create_job(JobSpec {
            label: "approved project write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write".into(),
                effect: Effect::ProjectWriteFile {
                    workspace_sha256: runtime.workspace_sha256(),
                    relative_path: "nested/proof.txt".into(),
                    contents: "approved".into(),
                },
            }],
        })
        .unwrap();

    assert_eq!(
        runtime.run_all(job).unwrap(),
        optimus_graph::JobStatus::AwaitingApproval
    );
    assert_eq!(
        runtime.grant_and_resume(job).unwrap(),
        optimus_graph::JobStatus::Succeeded
    );
    assert_eq!(
        std::fs::read_to_string(root.join("nested/proof.txt")).unwrap(),
        "approved"
    );
}

#[test]
fn denial_revocation_and_expiry_never_authorize_execution() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("lifecycle.db");
    let rt = Runtime::open(&db, &dir.path().join("ws")).unwrap();

    let denied = command_job(&rt, "denied", 1);
    let _ = rt.run_next(denied);
    rt.deny_approval(
        ApprovalGrant::for_job_by(denied, "reviewer", 120),
        "unsafe request",
    )
    .unwrap();
    assert_eq!(
        rt.run_all(denied).unwrap(),
        optimus_graph::JobStatus::AwaitingApproval
    );

    let revoked = command_job(&rt, "revoked", 1);
    let _ = rt.run_next(revoked);
    rt.grant_approval(ApprovalGrant::for_job_by(revoked, "alice", 120))
        .unwrap();
    assert!(rt
        .list_pending_approvals()
        .unwrap()
        .iter()
        .any(|pending| { pending.job_id == revoked && pending.has_grant }));
    rt.revoke_approval(revoked, "bob", "approval withdrawn")
        .unwrap();
    assert_eq!(
        rt.run_all(revoked).unwrap(),
        optimus_graph::JobStatus::AwaitingApproval
    );

    let expired = command_job(&rt, "expired", 1);
    let _ = rt.run_next(expired);
    rt.grant_approval(ApprovalGrant::for_job_by(expired, "alice", 120))
        .unwrap();
    let connection = Connection::open(&db).unwrap();
    connection
        .execute(
            "UPDATE action_approvals SET created_unix=0,expires_unix=1 WHERE job_id=?1",
            [expired.0.to_string()],
        )
        .unwrap();
    assert_eq!(
        rt.run_all(expired).unwrap(),
        optimus_graph::JobStatus::AwaitingApproval
    );

    let decisions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM action_approvals
             WHERE (job_id=?1 AND decision='denied' AND actor='reviewer')
                OR (job_id=?2 AND decision='revoked' AND revoked_by='bob')",
            [denied.0.to_string(), revoked.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(decisions, 2);
}

#[test]
fn grants_do_not_transfer_to_changed_effects_or_other_nodes() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("transfer.db");
    let rt = Runtime::open(&db, &dir.path().join("ws")).unwrap();

    let changed = command_job(&rt, "changed", 1);
    let _ = rt.run_next(changed);
    rt.grant_approval(ApprovalGrant::for_job_by(changed, "alice", 120))
        .unwrap();
    let replacement = serde_json::to_string(&command_effect("echo changed")).unwrap();
    Connection::open(&db)
        .unwrap()
        .execute(
            "UPDATE nodes SET effect_json=?1 WHERE job_id=?2",
            rusqlite::params![replacement, changed.0.to_string()],
        )
        .unwrap();
    assert_eq!(
        rt.run_all(changed).unwrap(),
        optimus_graph::JobStatus::AwaitingApproval
    );

    let two_nodes = command_job(&rt, "two nodes", 2);
    let _ = rt.run_next(two_nodes);
    rt.grant_approval(ApprovalGrant::for_job_by(two_nodes, "alice", 120))
        .unwrap();
    assert_eq!(
        rt.run_all(two_nodes).unwrap(),
        optimus_graph::JobStatus::AwaitingApproval
    );
    assert_eq!(
        rt.node_statuses(two_nodes).unwrap(),
        vec![
            optimus_graph::NodeStatus::Succeeded,
            optimus_graph::NodeStatus::AwaitingApproval,
        ]
    );
}

fn command_job(rt: &Runtime, label: &str, nodes: usize) -> optimus_graph::JobId {
    rt.create_job(JobSpec {
        label: label.into(),
        budget: Default::default(),
        nodes: (0..nodes)
            .map(|index| NodeSpec {
                label: format!("command {index}"),
                effect: command_effect("exit 0"),
            })
            .collect(),
    })
    .unwrap()
}
