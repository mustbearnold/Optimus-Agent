//! ADR-0044: Standard project trust auto-authorizes ordinary project writes
//! with a durable exact-effect grant; Review changes still pauses.

use optimus_graph::{
    AutonomyProfile, Effect, JobSpec, JobStatus, NodeSpec, PolicyMode, RuntimeConfig,
};
use optimus_runtime::Runtime;
use tempfile::tempdir;

#[test]
fn standard_auto_allows_project_write_with_trust_receipt() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open_with_config(
        &dir.path().join("o.db"),
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: AutonomyProfile::Standard,
            ..Default::default()
        },
    )
    .unwrap();
    let root_hash = rt.workspace_sha256();

    let job = rt
        .create_job(JobSpec {
            label: "write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "app".into(),
                effect: Effect::ProjectWriteFile {
                    workspace_sha256: root_hash,
                    relative_path: "src/App.tsx".into(),
                    contents: "export const ok = 1;\n".into(),
                },
            }],
        })
        .unwrap();

    // Must not pause for approval under Standard.
    let status = rt.run_all(job).unwrap();
    assert_eq!(status, JobStatus::Succeeded);
    let written = std::fs::read_to_string(ws.join("src/App.tsx")).unwrap();
    assert!(written.contains("export const ok"));
}

#[test]
fn review_changes_still_pauses_project_write() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open_with_config(
        &dir.path().join("o.db"),
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: AutonomyProfile::ReviewChanges,
            ..Default::default()
        },
    )
    .unwrap();

    let root_hash = rt.workspace_sha256();
    let job = rt
        .create_job(JobSpec {
            label: "write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "app".into(),
                effect: Effect::ProjectWriteFile {
                    workspace_sha256: root_hash,
                    relative_path: "a.txt".into(),
                    contents: "x".into(),
                },
            }],
        })
        .unwrap();

    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
}

#[test]
fn read_only_denies_project_write() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open_with_config(
        &dir.path().join("o.db"),
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: AutonomyProfile::ReadOnly,
            ..Default::default()
        },
    )
    .unwrap();

    let root_hash = rt.workspace_sha256();
    let job = rt
        .create_job(JobSpec {
            label: "write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "app".into(),
                effect: Effect::ProjectWriteFile {
                    workspace_sha256: root_hash,
                    relative_path: "a.txt".into(),
                    contents: "x".into(),
                },
            }],
        })
        .unwrap();

    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::PolicyDenied { .. }
    ));
}

#[test]
fn standard_auto_allows_project_command() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open_with_config(
        &dir.path().join("o.db"),
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: AutonomyProfile::Standard,
            ..Default::default()
        },
    )
    .unwrap();

    let root_hash = rt.workspace_sha256();
    #[cfg(unix)]
    let effect = Effect::ProjectRunCommand {
        workspace_sha256: root_hash,
        program: "sh".into(),
        args: vec!["-c".into(), "echo trust-ok".into()],
    };
    #[cfg(windows)]
    let effect = Effect::ProjectRunCommand {
        workspace_sha256: root_hash,
        program: "cmd".into(),
        args: vec!["/C".into(), "echo trust-ok".into()],
    };

    let job = rt
        .create_job(JobSpec {
            label: "cmd".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "echo".into(),
                effect,
            }],
        })
        .unwrap();

    let status = rt.run_all(job).unwrap();
    assert_eq!(status, JobStatus::Succeeded);
}

/// Typing yolo while an approval is on screen releases that approval, because
/// unblocking it is the whole reason the operator typed it. The exact action is
/// still recorded — the receipt names the yolo actor instead of a human.
#[test]
fn yolo_releases_the_open_approval_and_records_who_did_it() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let rt = Runtime::open_with_config(
        &dir.path().join("o.db"),
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: AutonomyProfile::ReviewChanges,
            ..Default::default()
        },
    )
    .unwrap();

    let root_hash = rt.workspace_sha256();
    let job = rt
        .create_job(JobSpec {
            label: "write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "app".into(),
                effect: Effect::ProjectWriteFile {
                    workspace_sha256: root_hash,
                    relative_path: "a.txt".into(),
                    contents: "x".into(),
                },
            }],
        })
        .unwrap();

    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(
        err,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
    assert_eq!(rt.list_pending_approvals().unwrap().len(), 1);

    let released = rt.release_open_approvals_under_yolo().unwrap();
    assert_eq!(released, 1);

    // The paused node now runs without a human ever approving it.
    rt.run_next(job).unwrap();
    assert_eq!(std::fs::read_to_string(ws.join("a.txt")).unwrap(), "x");

    // Releasing twice is a no-op: the grant already exists.
    assert_eq!(rt.release_open_approvals_under_yolo().unwrap(), 0);
}
