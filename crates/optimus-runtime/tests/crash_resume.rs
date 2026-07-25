//! Crash-resume golden trajectory for Phase 0.
//!
//! Process A commits node 0, crashes while node 1 is `running`.
//! Process B recovers interrupted work and finishes the job.

use std::fs;
use std::path::PathBuf;

use optimus_graph::{Effect, JobSpec, JobStatus, NodeSpec, NodeStatus, PolicyMode, RuntimeConfig};
use optimus_runtime::Runtime;
use rusqlite::Connection;
use tempfile::tempdir;

#[cfg(windows)]
fn replay_marker_command() -> Effect {
    Effect::RunCommand {
        program: "cmd".into(),
        args: vec!["/C".into(), "echo replayed>command-replayed.txt".into()],
    }
}

#[cfg(unix)]
fn replay_marker_command() -> Effect {
    Effect::RunCommand {
        program: "sh".into(),
        args: vec!["-c".into(), "printf replayed >command-replayed.txt".into()],
    }
}

fn workspace_hello(ws: &std::path::Path) -> PathBuf {
    ws.join("hello.txt")
}

#[test]
fn crash_mid_job_then_resume_finishes() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");

    // Unrestricted: this golden tests crash/resume durability, not SmartDeny.
    let unrestricted = RuntimeConfig {
        policy: PolicyMode::Unrestricted,
            ..Default::default()
        };
    let job_id = {
        let rt = Runtime::open_with_config(&db, &workspace, unrestricted.clone()).expect("open A");
        let job_id = rt
            .create_job(JobSpec {
                label: "crash-resume-golden".into(),
                budget: Default::default(),
                nodes: vec![
                    NodeSpec {
                        label: "write-hello".into(),
                        effect: Effect::WriteFile {
                            relative_path: "hello.txt".into(),
                            contents: "hello from optimus\n".into(),
                        },
                    },
                    NodeSpec {
                        label: "verify-hello".into(),
                        effect: Effect::AssertFileEquals {
                            relative_path: "hello.txt".into(),
                            expected: "hello from optimus\n".into(),
                        },
                    },
                    NodeSpec {
                        label: "write-done".into(),
                        effect: Effect::WriteFile {
                            relative_path: "done.marker".into(),
                            contents: "ok\n".into(),
                        },
                    },
                ],
            })
            .expect("create job");

        let step = rt.run_next(job_id).expect("run node 0");
        assert_eq!(step.node_index, 0);
        assert_eq!(step.node_status, NodeStatus::Succeeded);

        rt.begin_node_and_crash(job_id).expect("crash seam");
        job_id
    };

    let rt = Runtime::open_with_config(&db, &workspace, unrestricted).expect("open B");
    let recovered = rt.recover_crashed_running().expect("recover");
    assert!(
        recovered.contains(&job_id),
        "job should be recovered from running nodes"
    );

    let status = rt.resume(job_id).expect("resume");
    assert_eq!(status, JobStatus::Succeeded);

    let hello = fs::read_to_string(workspace_hello(&workspace)).expect("hello");
    assert_eq!(hello, "hello from optimus\n");
    let done = fs::read_to_string(workspace.join("done.marker")).expect("done");
    assert_eq!(done, "ok\n");

    let nodes = rt.node_statuses(job_id).expect("statuses");
    assert_eq!(
        nodes,
        vec![
            NodeStatus::Succeeded,
            NodeStatus::Succeeded,
            NodeStatus::Succeeded,
        ]
    );
}

#[test]
fn running_node_is_not_silently_marked_succeeded_on_recover() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");

    let job_id = {
        let rt = Runtime::open(&db, &workspace).expect("open");
        let job_id = rt
            .create_job(JobSpec {
                label: "interrupt-policy".into(),
                budget: Default::default(),
                nodes: vec![NodeSpec {
                    label: "only".into(),
                    effect: Effect::WriteFile {
                        relative_path: "x.txt".into(),
                        contents: "x\n".into(),
                    },
                }],
            })
            .expect("create");
        rt.begin_node_and_crash(job_id).expect("crash");
        job_id
    };

    let rt = Runtime::open(&db, &workspace).expect("reopen");
    rt.recover_crashed_running().expect("recover");
    let nodes = rt.node_statuses(job_id).expect("statuses");
    assert_eq!(nodes, vec![NodeStatus::Interrupted]);
}

#[test]
fn crash_before_effect_leaves_durable_prepared_attempt_without_file_effect() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");
    let rt = Runtime::open(&db, &workspace).expect("open");
    let job_id = rt
        .create_job(JobSpec {
            label: "prepared-before-effect".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "write".into(),
                effect: Effect::WriteFile {
                    relative_path: "must-not-exist.txt".into(),
                    contents: "not yet".into(),
                },
            }],
        })
        .expect("create");

    rt.begin_node_and_crash(job_id).expect("prepare attempt");

    assert!(!workspace.join("must-not-exist.txt").exists());
    let connection = Connection::open(&db).expect("inspect");
    let attempts: Vec<(String, String)> = connection
        .prepare(
            "SELECT status,intent_json FROM effect_attempts
             WHERE job_id=?1 ORDER BY attempt_no",
        )
        .expect("effect_attempts schema")
        .query_map([job_id.0.to_string()], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query attempts")
        .collect::<std::result::Result<_, _>>()
        .expect("attempt rows");
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].0, "prepared");
    assert!(attempts[0].1.contains("must-not-exist.txt"));
}

#[test]
fn write_file_atomically_replaces_target_and_closes_attempt_with_receipt() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::write(workspace.join("replace.txt"), "old").expect("old target");
    let rt = Runtime::open_with_config(
        &db,
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::Unrestricted,
            ..Default::default()
        },
    )
    .expect("open");
    let job_id = rt
        .create_job(JobSpec {
            label: "atomic replacement".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "replace".into(),
                effect: Effect::WriteFile {
                    relative_path: "replace.txt".into(),
                    contents: "new contents".into(),
                },
            }],
        })
        .expect("create");

    rt.run_next(job_id).expect("run write");

    assert_eq!(
        fs::read_to_string(workspace.join("replace.txt")).unwrap(),
        "new contents"
    );
    assert!(fs::read_dir(&workspace).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("optimus-tmp")));
    let connection = Connection::open(&db).expect("inspect");
    let (status, receipt): (String, Option<String>) = connection
        .query_row(
            "SELECT status,receipt_json FROM effect_attempts WHERE job_id=?1",
            [job_id.0.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "succeeded");
    let receipt = receipt.expect("success receipt");
    assert!(receipt.contains("replace.txt"));
    assert!(receipt.contains("new contents".len().to_string().as_str()));
}

#[test]
fn prepared_command_becomes_ambiguous_and_is_not_blindly_replayed() {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).expect("workspace");
    let job_id = {
        let rt = Runtime::open_with_config(
            &db,
            &workspace,
            RuntimeConfig {
                policy: PolicyMode::Unrestricted,
            ..Default::default()
        },
        )
        .expect("open");
        let job_id = rt
            .create_job(JobSpec {
                label: "ambiguous command".into(),
                budget: Default::default(),
                nodes: vec![NodeSpec {
                    label: "command".into(),
                    effect: replay_marker_command(),
                }],
            })
            .expect("create");
        rt.begin_node_and_crash(job_id).expect("prepared command");
        job_id
    };

    let rt = Runtime::open_with_config(
        &db,
        &workspace,
        RuntimeConfig {
            policy: PolicyMode::Unrestricted,
            ..Default::default()
        },
    )
    .expect("reopen");
    rt.recover_crashed_job(job_id).expect("recover");
    let resume = rt.resume(job_id);

    assert!(resume.is_err());
    assert!(!workspace.join("command-replayed.txt").exists());
    let connection = Connection::open(&db).expect("inspect");
    let status: String = connection
        .query_row(
            "SELECT status FROM effect_attempts WHERE job_id=?1",
            [job_id.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "ambiguous");
}
