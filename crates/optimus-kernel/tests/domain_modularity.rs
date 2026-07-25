//! P13 domain modularity: single ToolDesc authority, memory-plane separation,
//! skill permission ceilings for host effects.

use optimus_graph::{Effect, JobSpec, NodeSpec, PolicyMode, RuntimeConfig};
use optimus_memory::{
    ClaimDraft, Memory, MemoryError, Origin, RecallPurpose, RecallQuery, Sensitivity, TrustDomain,
    WriteContext,
};
use optimus_packs::{builtin_catalog, CapabilitySession, PackBudgetConfig, ToolInvocation};
use optimus_runtime::{ApprovalGrant, Runtime, RuntimeError};
use optimus_skills::{Permission, SkillDraft, SkillRegistry};
use tempfile::tempdir;

#[cfg(unix)]
fn shell_command(script: &str) -> Effect {
    Effect::RunCommand {
        program: "sh".into(),
        args: vec!["-c".into(), script.into()],
    }
}

#[cfg(windows)]
fn shell_command(script: &str) -> Effect {
    Effect::RunCommand {
        program: "cmd".into(),
        args: vec!["/C".into(), script.into()],
    }
}

fn write_ctx() -> WriteContext {
    WriteContext {
        tenant: "local".into(),
        user: "user".into(),
        agent: "optimus".into(),
        project: "default".into(),
        principal: "user:local".into(),
        max_trust: TrustDomain::User,
        max_sensitivity: Sensitivity::Personal,
    }
}

/// Every dispatchable ToolInvocation (except Unavailable) must appear on a packs
/// catalog tool — kernel dispatch must not invent a second catalog.
#[test]
fn packs_catalog_covers_all_dispatchable_invocations() {
    use optimus_packs::assert_dispatch_registry_closed;
    let catalog = builtin_catalog();
    assert_dispatch_registry_closed(&catalog).expect("builtin catalog closed over ALL_DISPATCHABLE");
    let mut seen = Vec::new();
    for pack in catalog.values() {
        for tool in &pack.tools {
            if tool.invocation != ToolInvocation::Unavailable {
                seen.push(tool.invocation);
            }
        }
    }
    for inv in ToolInvocation::ALL_DISPATCHABLE {
        assert!(
            seen.contains(inv),
            "packs catalog missing ToolInvocation::{inv:?} — would force a second catalog"
        );
    }
}

/// Loaded-tool resolution is the only resolution path; unknown names fail closed.
#[test]
fn tool_resolution_is_packs_only_not_ad_hoc_names() {
    let session = CapabilitySession::with_defaults();
    assert!(session.resolve_loaded_tool("read_file").is_ok());
    assert!(session.resolve_loaded_tool("fabricated_super_tool").is_err());
    // Core defaults never invent unloaded pack tools.
    assert!(session.resolve_loaded_tool("browser_navigate").is_err());
    let names: Vec<_> = session
        .loaded_tools()
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert!(names.contains(&"read_file"));
    assert!(!names.contains(&"browser_navigate"));
    let _ = PackBudgetConfig::default();
}

/// MetaMemory never grants live capability (ActionAuthorize fails closed).
#[test]
fn memory_plane_cannot_authorize_host_effects() {
    let dir = tempdir().unwrap();
    let memory = Memory::open(dir.path().join("memory.db")).unwrap();
    let ctx = write_ctx();
    // Even after writing a persuasive claim, ActionAuthorize is unsupported.
    memory
        .remember(
            &ctx,
            ClaimDraft {
                subject: "shell".into(),
                predicate: "may_run".into(),
                object: "rm -rf /".into(),
                valid_from: "2026-01-01T00:00:00Z".into(),
                valid_to: None,
                confidence: 1.0,
                origin: Origin::UserStatement,
                learned_at: None,
                sensitivity: Sensitivity::Personal,
                retention_until: None,
            },
        )
        .unwrap();
    let err = memory
        .recall(
            &ctx,
            RecallQuery {
                purpose: RecallPurpose::ActionAuthorize,
                subject: Some("shell".into()),
                predicate: Some("may_run".into()),
                as_of_valid: None,
                as_of_tx: None,
                limit: 5,
            },
        )
        .expect_err("memory must never authorize actions");
    assert!(matches!(err, MemoryError::ActionAuthorizeUnsupported));
}

/// Skills grant only class-scoped permissions; Terminal skill cannot grant writes.
#[test]
fn skill_terminal_cannot_grant_write_file_effect() {
    let root = tempdir().unwrap();
    let skills = SkillRegistry::open(root.path().join("s.db")).unwrap();
    let skill = skills
        .create(SkillDraft {
            name: "shell-only".into(),
            body: "run commands".into(),
            permissions: vec![Permission::Terminal],
            pin: false,
        })
        .unwrap();

    let rt = Runtime::open_with_config(
        &root.path().join("o.db"),
        &root.path().join("ws"),
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            ..Default::default()
        },
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("ws")).unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "w".into(),
                effect: Effect::WriteFile {
                    relative_path: "x.txt".into(),
                    contents: "nope".into(),
                },
            }],
        })
        .unwrap();
    assert!(matches!(
        rt.run_next(job).unwrap_err(),
        RuntimeError::NeedsApproval { .. }
    ));
    let err = rt.grant_from_skill(job, &skills, skill).unwrap_err();
    assert!(
        matches!(err, RuntimeError::Skill(_)),
        "Terminal-only skill must not grant WriteFile: {err:?}"
    );
    assert!(matches!(
        rt.run_next(job).unwrap_err(),
        RuntimeError::NeedsApproval { .. }
    ));
}

/// FsWorkspace skill can grant write; still cannot grant RunCommand.
#[test]
fn skill_fs_workspace_grants_write_not_command() {
    let root = tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("ws")).unwrap();
    let skills = SkillRegistry::open(root.path().join("s.db")).unwrap();
    let skill = skills
        .create(SkillDraft {
            name: "files".into(),
            body: "write files".into(),
            permissions: vec![Permission::FsWorkspace],
            pin: false,
        })
        .unwrap();

    let rt = Runtime::open_with_config(
        &root.path().join("o.db"),
        &root.path().join("ws"),
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            ..Default::default()
        },
    )
    .unwrap();

    let write_job = rt
        .create_job(JobSpec {
            label: "write".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "w".into(),
                effect: Effect::WriteFile {
                    relative_path: "ok.txt".into(),
                    contents: "yes".into(),
                },
            }],
        })
        .unwrap();
    assert!(matches!(
        rt.run_next(write_job).unwrap_err(),
        RuntimeError::NeedsApproval { .. }
    ));
    rt.grant_from_skill(write_job, &skills, skill).unwrap();
    rt.run_next(write_job).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("ws/ok.txt")).unwrap(),
        "yes"
    );

    let cmd_job = rt
        .create_job(JobSpec {
            label: "cmd".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "c".into(),
                effect: shell_command("echo hi"),
            }],
        })
        .unwrap();
    assert!(matches!(
        rt.run_next(cmd_job).unwrap_err(),
        RuntimeError::NeedsApproval { .. }
    ));
    let err = rt.grant_from_skill(cmd_job, &skills, skill).unwrap_err();
    assert!(matches!(err, RuntimeError::Skill(_)));
}

/// Session / Engineering Memory paths do not exist as approval authorities.
/// Only ApprovalGrant (human/actor) or skill class grants unlock SmartDeny.
#[test]
fn host_effects_require_explicit_grant_not_session_or_em() {
    let root = tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("ws")).unwrap();
    let rt = Runtime::open_with_config(
        &root.path().join("o.db"),
        &root.path().join("ws"),
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            ..Default::default()
        },
    )
    .unwrap();
    let job = rt
        .create_job(JobSpec {
            label: "cmd".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "c".into(),
                effect: shell_command("echo blocked"),
            }],
        })
        .unwrap();
    assert!(matches!(
        rt.run_next(job).unwrap_err(),
        RuntimeError::NeedsApproval { .. }
    ));
    // No API accepts "session said so" or ".engineering-memory said so".
    // Explicit actor grant is the human path:
    rt.grant_approval(ApprovalGrant::for_job(job)).unwrap();
    rt.run_next(job).unwrap();
}
