//! Multi-agent verticals: workspace_writer/reader + handoff workflows (P10).

use std::fs;

use optimus_graph::PolicyMode;
use optimus_kernel::{
    cancel_write_file_handoff, content_sha256, open_seeded_agent_registry,
    open_seeded_workflow_registry, read_file_handoff_workflow, run_write_file_handoff,
    vertical_workspace, workspace_reader_descriptor, workspace_writer_descriptor,
    write_file_handoff_workflow, write_then_read_handoff_workflow, AgentResultKind, ArtifactStore,
    WorkflowTerminalKind, WriteFileHandoffRequest, READ_FILE_HANDOFF_WORKFLOW_ID,
    WORKSPACE_READER_ID, WORKSPACE_WRITER_ID, WORKSPACE_WRITER_VERSION,
    WRITE_FILE_HANDOFF_WORKFLOW_ID, WRITE_FILE_HANDOFF_WORKFLOW_VERSION,
    WRITE_THEN_READ_HANDOFF_WORKFLOW_ID,
};
use tempfile::tempdir;

#[test]
fn seeds_builtin_specialist_and_workflow_registries() {
    let dir = tempdir().unwrap();
    let agents = open_seeded_agent_registry(dir.path().join("agents.db")).unwrap();
    let workflows = open_seeded_workflow_registry(dir.path().join("workflows.db")).unwrap();

    let listed_agents = agents.list().unwrap();
    assert_eq!(listed_agents.len(), 2);
    let ids: Vec<_> = listed_agents.iter().map(|a| a.id.as_str()).collect();
    assert!(ids.contains(&WORKSPACE_WRITER_ID));
    assert!(ids.contains(&WORKSPACE_READER_ID));
    assert_eq!(
        listed_agents
            .iter()
            .find(|a| a.id.as_str() == WORKSPACE_WRITER_ID)
            .unwrap(),
        &workspace_writer_descriptor().unwrap()
    );
    assert_eq!(
        listed_agents
            .iter()
            .find(|a| a.id.as_str() == WORKSPACE_READER_ID)
            .unwrap(),
        &workspace_reader_descriptor().unwrap()
    );

    let listed_workflows = workflows.list().unwrap();
    assert_eq!(listed_workflows.len(), 3);
    let wf_ids: Vec<_> = listed_workflows.iter().map(|w| w.id.as_str()).collect();
    assert!(wf_ids.contains(&WRITE_FILE_HANDOFF_WORKFLOW_ID));
    assert!(wf_ids.contains(&READ_FILE_HANDOFF_WORKFLOW_ID));
    assert!(wf_ids.contains(&WRITE_THEN_READ_HANDOFF_WORKFLOW_ID));
    write_file_handoff_workflow().unwrap().validate().unwrap();
    read_file_handoff_workflow().unwrap().validate().unwrap();
    write_then_read_handoff_workflow()
        .unwrap()
        .validate()
        .unwrap();

    // Idempotent reseed does not fail or duplicate.
    open_seeded_agent_registry(dir.path().join("agents.db")).unwrap();
    open_seeded_workflow_registry(dir.path().join("workflows.db")).unwrap();
    assert_eq!(agents.list().unwrap().len(), 2);
    assert_eq!(workflows.list().unwrap().len(), 3);
}

#[test]
fn write_file_handoff_succeeds_under_unrestricted_policy() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let contents = "handoff body\n";
    let report = run_write_file_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "notes/handoff.txt".into(),
            contents: contents.into(),
            auto_grant: false,
            policy: PolicyMode::Unrestricted,
        },
    )
    .unwrap();

    assert_eq!(report.workflow_terminal, WorkflowTerminalKind::Succeeded);
    assert_eq!(report.agent_result.kind, AgentResultKind::Succeeded);
    assert_eq!(report.agent_id, WORKSPACE_WRITER_ID);
    assert_eq!(report.agent_version, WORKSPACE_WRITER_VERSION);
    assert_eq!(report.workflow_id, WRITE_FILE_HANDOFF_WORKFLOW_ID);
    assert_eq!(
        report.workflow_version,
        WRITE_FILE_HANDOFF_WORKFLOW_VERSION
    );
    assert!(report.job_id.is_some());
    assert!(report.run_id.is_some());
    assert_eq!(
        fs::read_to_string(vertical_workspace(home).join("notes/handoff.txt")).unwrap(),
        contents
    );
    let artifact = report.artifact.expect("handoff artifact");
    assert_eq!(artifact.sha256, content_sha256(contents.as_bytes()));
    let store = ArtifactStore::open(home).unwrap();
    assert!(store
        .list()
        .unwrap()
        .iter()
        .any(|row| row.sha256 == artifact.sha256 && !row.deleted));
}

#[test]
fn write_file_handoff_blocks_on_smart_deny_without_grant() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let report = run_write_file_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "blocked.txt".into(),
            contents: "secret".into(),
            auto_grant: false,
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();

    assert_eq!(report.workflow_terminal, WorkflowTerminalKind::Failed);
    assert_eq!(report.agent_result.kind, AgentResultKind::Failed);
    assert_eq!(
        report.agent_result.error.as_ref().unwrap().code,
        "approval_required"
    );
    assert!(!vertical_workspace(home).join("blocked.txt").exists());
    assert!(report.artifact.is_none());
}

#[test]
fn write_file_handoff_runs_after_auto_grant_under_smart_deny() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let report = run_write_file_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "approved.txt".into(),
            contents: "granted".into(),
            auto_grant: true,
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();

    assert_eq!(report.workflow_terminal, WorkflowTerminalKind::Succeeded);
    assert_eq!(report.agent_result.kind, AgentResultKind::Succeeded);
    assert_eq!(
        fs::read_to_string(vertical_workspace(home).join("approved.txt")).unwrap(),
        "granted"
    );
    assert!(report.agent_result.artifacts.iter().any(|artifact| {
        artifact.uri.starts_with("artifact:") && artifact.sha256.len() == 64
    }));
}

#[test]
fn cancel_request_fences_running_invocation_from_late_success() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let report = run_write_file_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "done.txt".into(),
            contents: "done".into(),
            auto_grant: true,
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    let cancelled = cancel_write_file_handoff(
        home,
        report.invocation_id,
        report.job_id,
        "too late",
    )
    .unwrap();
    assert!(
        !cancelled,
        "terminal invocations must not accept late cancellation requests"
    );
}

#[test]
fn rejects_path_escape_inputs_before_invocation() {
    let dir = tempdir().unwrap();
    let err = run_write_file_handoff(
        dir.path(),
        WriteFileHandoffRequest {
            relative_path: "../escape.txt".into(),
            contents: "no".into(),
            auto_grant: true,
            policy: PolicyMode::Unrestricted,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("relative_path"));
}
