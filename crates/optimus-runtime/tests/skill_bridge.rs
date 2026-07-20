//! Skill permission bridge into SmartDeny job approvals.

use std::fs;

use optimus_graph::{Effect, JobSpec, NodeSpec, NodeStatus, PolicyMode, RuntimeConfig};
use optimus_runtime::{Runtime, RuntimeError};
use optimus_skills::{Permission, SkillDraft, SkillRegistry};
use tempfile::tempdir;

#[test]
fn skill_with_terminal_unlocks_run_command() {
    let root = tempdir().unwrap();
    let db = root.path().join("o.db");
    let ws = root.path().join("ws");
    fs::create_dir_all(&ws).unwrap();
    let skills = SkillRegistry::open(root.path().join("s.db")).unwrap();
    let skill = skills
        .create(SkillDraft {
            name: "echo".into(),
            body: "run echo".into(),
            permissions: vec![Permission::Terminal, Permission::FsWorkspace],
            pin: false,
        })
        .unwrap();

    let rt = Runtime::open_with_config(
        &db,
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "cmd".into(),
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

    let err = rt.run_next(job).unwrap_err();
    assert!(matches!(err, RuntimeError::NeedsApproval { .. }));

    rt.grant_from_skill(job, &skills, skill).unwrap();
    let step = rt.run_next(job).unwrap();
    assert_eq!(step.node_status, NodeStatus::Succeeded);
    let body = fs::read_to_string(ws.join("out.txt")).unwrap();
    assert!(body.to_lowercase().contains("hi"));
}

#[test]
fn skill_without_terminal_cannot_grant_run_command() {
    let root = tempdir().unwrap();
    let db = root.path().join("o.db");
    let ws = root.path().join("ws");
    fs::create_dir_all(&ws).unwrap();
    let skills = SkillRegistry::open(root.path().join("s.db")).unwrap();
    let skill = skills
        .create(SkillDraft {
            name: "files".into(),
            body: "files only".into(),
            permissions: vec![Permission::FsWorkspace],
            pin: false,
        })
        .unwrap();

    let rt = Runtime::open_with_config(
        &db,
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "cmd".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "echo".into(),
                effect: Effect::RunCommand {
                    program: "cmd".into(),
                    args: vec!["/C".into(), "echo x>x.txt".into()],
                },
            }],
        })
        .unwrap();

    let err = rt.grant_from_skill(job, &skills, skill).unwrap_err();
    assert!(matches!(err, RuntimeError::Skill(_)));
    // still blocked
    assert!(matches!(
        rt.run_next(job).unwrap_err(),
        RuntimeError::NeedsApproval { .. }
    ));
}
