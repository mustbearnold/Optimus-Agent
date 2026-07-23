//! Command stdout/stderr capture tests.

use optimus_graph::{Effect, JobSpec, NodeSpec};
use optimus_runtime::{ApprovalGrant, Runtime};
use rusqlite::Connection;
use tempfile::tempdir;

#[cfg(windows)]
fn successful_command() -> Effect {
    Effect::RunCommand {
        program: "cmd".into(),
        args: vec!["/C".into(), "echo capture-me".into()],
    }
}

#[cfg(unix)]
fn successful_command() -> Effect {
    Effect::RunCommand {
        program: "sh".into(),
        args: vec!["-c".into(), "printf 'capture-me\\n'".into()],
    }
}

#[cfg(windows)]
fn failing_command() -> Effect {
    Effect::RunCommand {
        program: "cmd".into(),
        args: vec!["/C".into(), "echo err-out & exit /B 7".into()],
    }
}

#[cfg(unix)]
fn failing_command() -> Effect {
    Effect::RunCommand {
        program: "sh".into(),
        args: vec!["-c".into(), "printf 'err-out\\n'; exit 7".into()],
    }
}

#[test]
fn captures_stdout_on_success() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let db = dir.path().join("t.db");
    let rt = Runtime::open(&db, &ws).unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "echo".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "run".into(),
                effect: successful_command(),
            }],
        })
        .unwrap();
    rt.grant_approval(ApprovalGrant::for_job(job)).unwrap();
    let status = rt.run_all(job).unwrap();
    assert_eq!(format!("{status:?}"), "Succeeded");
    let cap = rt.latest_command_capture(job).unwrap().expect("capture");
    assert!(cap.stdout.contains("capture-me"), "stdout={}", cap.stdout);
    assert_eq!(cap.exit_code, Some(0));
    assert!(!cap.timed_out);
}

#[test]
fn captures_on_nonzero_exit() {
    let dir = tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let db = dir.path().join("fail.db");
    let rt = Runtime::open(&db, &ws).unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "fail".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "run".into(),
                effect: failing_command(),
            }],
        })
        .unwrap();
    rt.grant_approval(ApprovalGrant::for_job(job)).unwrap();
    let _ = rt.run_all(job);
    let cap = rt.latest_command_capture(job).unwrap().expect("capture");
    assert!(
        cap.stdout.contains("err-out") || cap.exit_code == Some(7),
        "cap={cap:?}"
    );
    let connection = Connection::open(&db).unwrap();
    let (status, receipt): (String, Option<String>) = connection
        .query_row(
            "SELECT status,receipt_json FROM effect_attempts WHERE job_id=?1",
            [job.0.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "failed");
    assert!(receipt.unwrap().contains("err-out"));
}
