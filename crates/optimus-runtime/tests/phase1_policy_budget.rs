//! Phase 1: policy, budgets, bounded commands, multi-job resume.

use std::fs;
use std::time::Duration;

use optimus_graph::{Effect, JobSpec, JobStatus, NodeSpec, NodeStatus, PolicyMode, RuntimeConfig};
use optimus_runtime::{ApprovalGrant, Runtime, RuntimeError};
use tempfile::tempdir;

fn open_rt(mode: PolicyMode) -> (tempfile::TempDir, Runtime) {
    let root = tempdir().expect("tempdir");
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    let rt =
        Runtime::open_with_config(&db, &workspace, RuntimeConfig { policy: mode }).expect("open");
    (root, rt)
}

#[test]
fn run_command_denied_without_approval_in_smart_deny() {
    let (_root, rt) = open_rt(PolicyMode::SmartDeny);
    let job = rt
        .create_job(JobSpec {
            label: "needs-approval".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "echo".into(),
                effect: Effect::RunCommand {
                    program: "cmd".into(),
                    args: vec!["/C".into(), "echo hi>out.txt".into()],
                },
            }],
        })
        .unwrap();

    let err = rt.run_next(job).expect_err("must need approval");
    assert!(matches!(err, RuntimeError::NeedsApproval { .. }), "{err:?}");
    assert_eq!(
        rt.node_statuses(job).unwrap(),
        vec![NodeStatus::AwaitingApproval]
    );
    assert_eq!(rt.job_status(job).unwrap(), JobStatus::AwaitingApproval);
}

#[test]
fn run_command_succeeds_after_grant() {
    let (_root, rt) = open_rt(PolicyMode::SmartDeny);
    let job = rt
        .create_job(JobSpec {
            label: "granted".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "echo".into(),
                effect: Effect::RunCommand {
                    program: "cmd".into(),
                    args: vec!["/C".into(), "echo hi>out.txt".into()],
                },
            }],
        })
        .unwrap();

    let _ = rt.run_next(job).expect_err("await");
    rt.grant_approval(ApprovalGrant::for_job(job)).unwrap();
    let step = rt.run_next(job).expect("run after grant");
    assert_eq!(step.node_status, NodeStatus::Succeeded);
    assert_eq!(rt.job_status(job).unwrap(), JobStatus::Succeeded);
    let ws = rt.workspace_path();
    let body = fs::read_to_string(ws.join("out.txt")).expect("out");
    assert!(body.to_lowercase().contains("hi"), "{body}");
}

#[test]
fn max_steps_budget_trips_circuit() {
    let (_root, rt) = open_rt(PolicyMode::Unrestricted);
    let job = rt
        .create_job(JobSpec {
            label: "budget".into(),
            budget: optimus_graph::JobBudget {
                max_steps: 1,
                max_consecutive_failures: 3,
                command_timeout_ms: 30_000,
            },
            nodes: vec![
                NodeSpec {
                    label: "a".into(),
                    effect: Effect::WriteFile {
                        relative_path: "a.txt".into(),
                        contents: "a\n".into(),
                    },
                },
                NodeSpec {
                    label: "b".into(),
                    effect: Effect::WriteFile {
                        relative_path: "b.txt".into(),
                        contents: "b\n".into(),
                    },
                },
            ],
        })
        .unwrap();

    rt.run_next(job).unwrap();
    let err = rt.run_next(job).expect_err("budget");
    assert!(
        matches!(err, RuntimeError::BudgetExceeded { .. }),
        "{err:?}"
    );
    assert_eq!(rt.job_status(job).unwrap(), JobStatus::Failed);
    assert!(!rt.workspace_path().join("b.txt").exists());
}

#[test]
fn command_timeout_kills_long_sleep() {
    let (_root, rt) = open_rt(PolicyMode::Unrestricted);
    let job = rt
        .create_job(JobSpec {
            label: "timeout".into(),
            budget: optimus_graph::JobBudget {
                max_steps: 10,
                max_consecutive_failures: 3,
                command_timeout_ms: 500,
            },
            nodes: vec![NodeSpec {
                label: "sleep".into(),
                effect: Effect::RunCommand {
                    program: "powershell".into(),
                    args: vec![
                        "-NoProfile".into(),
                        "-Command".into(),
                        "Start-Sleep -Seconds 30".into(),
                    ],
                },
            }],
        })
        .unwrap();

    let started = std::time::Instant::now();
    let err = rt.run_next(job).expect_err("timeout");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "timeout took too long: {:?}",
        started.elapsed()
    );
    assert!(
        matches!(
            err,
            RuntimeError::Effector(ref m) if m.contains("timed out")
        ) || matches!(
            err,
            RuntimeError::CommandFailed { capture: ref c, .. } if c.timed_out
        ),
        "{err:?}"
    );
    assert_eq!(rt.node_statuses(job).unwrap(), vec![NodeStatus::Failed]);
}

#[test]
fn resume_all_recovers_multiple_interrupted_jobs() {
    let root = tempdir().unwrap();
    let db = root.path().join("optimus.db");
    let workspace = root.path().join("workspace");
    fs::create_dir_all(&workspace).unwrap();

    let (j1, j2) = {
        let rt = Runtime::open(&db, &workspace).unwrap();
        let j1 = rt
            .create_job(JobSpec {
                label: "one".into(),
                budget: Default::default(),
                nodes: vec![
                    NodeSpec {
                        label: "w1".into(),
                        effect: Effect::WriteFile {
                            relative_path: "one.txt".into(),
                            contents: "1\n".into(),
                        },
                    },
                    NodeSpec {
                        label: "d1".into(),
                        effect: Effect::WriteFile {
                            relative_path: "one.done".into(),
                            contents: "ok\n".into(),
                        },
                    },
                ],
            })
            .unwrap();
        let j2 = rt
            .create_job(JobSpec {
                label: "two".into(),
                budget: Default::default(),
                nodes: vec![
                    NodeSpec {
                        label: "w2".into(),
                        effect: Effect::WriteFile {
                            relative_path: "two.txt".into(),
                            contents: "2\n".into(),
                        },
                    },
                    NodeSpec {
                        label: "d2".into(),
                        effect: Effect::WriteFile {
                            relative_path: "two.done".into(),
                            contents: "ok\n".into(),
                        },
                    },
                ],
            })
            .unwrap();
        rt.run_next(j1).unwrap();
        rt.begin_node_and_crash(j1).unwrap();
        rt.run_next(j2).unwrap();
        rt.begin_node_and_crash(j2).unwrap();
        (j1, j2)
    };

    let rt = Runtime::open(&db, &workspace).unwrap();
    let results = rt.resume_all().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|(_, s)| *s == JobStatus::Succeeded));
    assert_eq!(
        fs::read_to_string(workspace.join("one.done")).unwrap(),
        "ok\n"
    );
    assert_eq!(
        fs::read_to_string(workspace.join("two.done")).unwrap(),
        "ok\n"
    );
    let _ = (j1, j2);
}
