//! Durable cancellation and process-lifecycle regressions.

use optimus_graph::{Effect, JobSpec, JobStatus, NodeSpec, NodeStatus};
use optimus_runtime::{ApprovalGrant, Runtime, RuntimeError};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn cancelling_pending_job_is_atomic_terminal_and_idempotent() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let rt = Runtime::open(&db, &workspace).expect("runtime");
    let job_id = rt
        .create_job(JobSpec {
            label: "cancel pending".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "never write".into(),
                effect: Effect::WriteFile {
                    relative_path: "never.txt".into(),
                    contents: "no".into(),
                },
            }],
        })
        .expect("create");

    assert_eq!(
        rt.cancel_job(job_id).expect("first cancel"),
        JobStatus::Cancelled
    );
    assert_eq!(
        rt.cancel_job(job_id).expect("repeat cancel"),
        JobStatus::Cancelled
    );
    assert_eq!(
        rt.node_statuses(job_id).unwrap(),
        vec![NodeStatus::Cancelled]
    );
    assert!(!workspace.join("never.txt").exists());

    let connection = Connection::open(&db).expect("inspect");
    let terminal_events: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM events WHERE job_id=?1 AND terminal_slot=1",
            [job_id.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let requests: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM cancellation_requests WHERE job_id=?1",
            [job_id.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(terminal_events, 1);
    assert_eq!(requests, 1);
}

#[test]
fn cancelling_running_command_terminates_and_reaps_child_without_late_effect() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let worker_runtime = Runtime::open(&db, &workspace).expect("worker runtime");
    let job_id = worker_runtime
        .create_job(JobSpec {
            label: "cancel command".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "long command".into(),
                effect: Effect::RunCommand {
                    program: "cmd".into(),
                    args: vec![
                        "/C".into(),
                        "ping -n 5 127.0.0.1 >nul & echo survived>late.txt".into(),
                    ],
                },
            }],
        })
        .expect("create");
    worker_runtime
        .grant_approval(ApprovalGrant::for_job(job_id))
        .expect("approve");
    let worker = std::thread::spawn(move || worker_runtime.run_all(job_id));
    let controller = Runtime::open(&db, &workspace).expect("controller runtime");
    for _ in 0..100 {
        if controller.node_statuses(job_id).unwrap() == vec![NodeStatus::Running] {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        controller.node_statuses(job_id).unwrap(),
        vec![NodeStatus::Running]
    );

    let requested = controller.cancel_job(job_id).expect("request cancellation");
    let worker_result = worker.join().expect("worker thread");

    assert_eq!(requested, JobStatus::Running);
    assert!(matches!(worker_result, Err(RuntimeError::Cancelled { .. })));
    assert_eq!(controller.job_status(job_id).unwrap(), JobStatus::Cancelled);
    assert_eq!(
        controller.node_statuses(job_id).unwrap(),
        vec![NodeStatus::Cancelled]
    );
    assert!(!workspace.join("late.txt").exists());
}

#[test]
fn repeated_cancel_resume_run_and_recover_preserve_one_terminal_outcome() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let rt = Runtime::open(&db, &workspace).expect("runtime");
    let job_id = rt
        .create_job(JobSpec {
            label: "terminal uniqueness".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "cancel me".into(),
                effect: Effect::WriteFile {
                    relative_path: "never-again.txt".into(),
                    contents: "never".into(),
                },
            }],
        })
        .unwrap();

    assert_eq!(rt.cancel_job(job_id).unwrap(), JobStatus::Cancelled);
    assert_eq!(rt.resume(job_id).unwrap(), JobStatus::Cancelled);
    assert_eq!(rt.run_all(job_id).unwrap(), JobStatus::Cancelled);
    assert!(!rt.recover_crashed_job(job_id).unwrap());
    assert_eq!(rt.cancel_job(job_id).unwrap(), JobStatus::Cancelled);

    let connection = Connection::open(&db).unwrap();
    let terminal: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM events WHERE job_id=?1 AND terminal_slot=1",
            [job_id.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    let node_cancelled: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM events WHERE job_id=?1 AND kind='node_cancelled'",
            [job_id.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(terminal, 1);
    assert_eq!(node_cancelled, 1);
    assert!(!workspace.join("never-again.txt").exists());
}
