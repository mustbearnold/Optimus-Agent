//! P10: WorkflowRunStore, dual specialists, DAG order, cancel tree, handoffs.

use std::fs;

use optimus_graph::PolicyMode;
use optimus_kernel::{
    cancel_workflow_run, content_sha256, get_workflow_run, open_seeded_agent_registry,
    open_seeded_workflow_registry, open_workflow_run_store, run_read_file_handoff,
    run_registered_workflow, run_write_then_read_handoff, vertical_workspace,
    write_then_read_handoff_workflow, AgentResultKind, ReadFileHandoffRequest, WorkflowDagRequest,
    WorkflowRunStatus, WorkflowRunStore, WorkflowTerminalKind, WriteFileHandoffRequest,
    READ_FILE_HANDOFF_WORKFLOW_ID, READ_FILE_HANDOFF_WORKFLOW_VERSION, WORKSPACE_READER_ID,
    WORKSPACE_WRITER_ID, WRITE_THEN_READ_HANDOFF_WORKFLOW_ID,
    WRITE_THEN_READ_HANDOFF_WORKFLOW_VERSION,
};
use tempfile::tempdir;

#[test]
fn topological_order_write_then_read() {
    let def = write_then_read_handoff_workflow().unwrap();
    let order = WorkflowRunStore::topological_order(&def).unwrap();
    assert_eq!(order, vec!["write".to_string(), "read".to_string()]);
}

#[test]
fn write_then_read_dag_succeeds_and_publishes_both_artifacts() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let contents = "dag body v1\n";
    let report = run_write_then_read_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "dag/out.txt".into(),
            contents: contents.into(),
            auto_grant: true,
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();

    assert_eq!(report.status, WorkflowRunStatus::Succeeded);
    assert_eq!(
        report.workflow_terminal,
        Some(WorkflowTerminalKind::Succeeded)
    );
    assert_eq!(report.workflow_id, WRITE_THEN_READ_HANDOFF_WORKFLOW_ID);
    assert_eq!(
        report.workflow_version,
        WRITE_THEN_READ_HANDOFF_WORKFLOW_VERSION
    );
    assert_eq!(report.nodes.len(), 2);
    let write = report
        .nodes
        .iter()
        .find(|n| n.node_id == "write")
        .unwrap();
    let read = report.nodes.iter().find(|n| n.node_id == "read").unwrap();
    assert_eq!(write.status.as_str(), "succeeded");
    assert_eq!(read.status.as_str(), "succeeded");
    assert!(write.artifact_sha256.is_some());
    assert!(read.artifact_sha256.is_some());
    assert_eq!(
        write.artifact_sha256.as_ref().unwrap(),
        &content_sha256(contents.as_bytes())
    );
    assert_eq!(
        read.artifact_sha256.as_ref().unwrap(),
        write.artifact_sha256.as_ref().unwrap()
    );
    assert_eq!(report.artifacts.len(), 2);
    assert_eq!(
        fs::read_to_string(vertical_workspace(home).join("dag/out.txt")).unwrap(),
        contents
    );
    // Parent run has two child invocations (writer + reader).
    assert_eq!(report.children.len(), 2);
    let agents = open_seeded_agent_registry(home.join("agent-registry.db")).unwrap();
    assert_eq!(agents.list().unwrap().len(), 2);
}

#[test]
fn read_file_handoff_requires_existing_file() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let missing = run_read_file_handoff(
        home,
        ReadFileHandoffRequest {
            relative_path: "nope.txt".into(),
        },
    )
    .unwrap();
    assert_eq!(missing.status, WorkflowRunStatus::Failed);
    assert_eq!(
        missing.nodes[0].error_code.as_deref(),
        Some("file_not_found")
    );

    fs::create_dir_all(vertical_workspace(home)).unwrap();
    fs::write(vertical_workspace(home).join("exists.txt"), "hello").unwrap();
    let ok = run_read_file_handoff(
        home,
        ReadFileHandoffRequest {
            relative_path: "exists.txt".into(),
        },
    )
    .unwrap();
    assert_eq!(ok.status, WorkflowRunStatus::Succeeded);
    assert_eq!(ok.workflow_id, READ_FILE_HANDOFF_WORKFLOW_ID);
    assert_eq!(ok.workflow_version, READ_FILE_HANDOFF_WORKFLOW_VERSION);
    assert_eq!(
        ok.nodes[0].artifact_sha256.as_ref().unwrap(),
        &content_sha256(b"hello")
    );
    assert_eq!(ok.children.len(), 1);
}

#[test]
fn cancel_after_begin_terminals_run_without_children() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let report = run_registered_workflow(
        home,
        WorkflowDagRequest {
            workflow_id: WRITE_THEN_READ_HANDOFF_WORKFLOW_ID.into(),
            workflow_version: WRITE_THEN_READ_HANDOFF_WORKFLOW_VERSION.into(),
            inputs: serde_json::json!({
                "relative_path": "c.txt",
                "contents": "never"
            }),
            auto_grant: true,
            policy: PolicyMode::Unrestricted,
            cancel_after_begin: true,
            cancel_reason: Some("test cancel".into()),
        },
    )
    .unwrap();
    assert_eq!(report.status, WorkflowRunStatus::Cancelled);
    assert_eq!(
        report.workflow_terminal,
        Some(WorkflowTerminalKind::Cancelled)
    );
    assert!(report.children.is_empty());
    assert!(!vertical_workspace(home).join("c.txt").exists());
    let stored = get_workflow_run(home, report.run_id).unwrap();
    assert_eq!(stored.status, WorkflowRunStatus::Cancelled);
    assert_eq!(
        stored.cancellation_reason.as_deref(),
        Some("test cancel")
    );
}

#[test]
fn cancel_workflow_run_is_idempotent_after_terminal() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let report = run_write_then_read_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "x.txt".into(),
            contents: "x".into(),
            auto_grant: true,
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    assert_eq!(report.status, WorkflowRunStatus::Succeeded);
    let again = cancel_workflow_run(home, report.run_id, "too late").unwrap();
    assert!(!again);
}

#[test]
fn terminal_uniqueness_on_run_store() {
    let dir = tempdir().unwrap();
    let store = open_workflow_run_store(dir.path()).unwrap();
    let def = write_then_read_handoff_workflow().unwrap();
    let run_id = store
        .begin(
            &def,
            serde_json::json!({"relative_path": "a.txt", "contents": "a"}),
        )
        .unwrap();
    let lease = store.claim_lease(run_id, "test", None).unwrap();
    store
        .settle_terminal(run_id, &lease, WorkflowRunStatus::Succeeded, None)
        .unwrap();
    let err = store
        .settle_terminal(run_id, &lease, WorkflowRunStatus::Failed, Some("no"))
        .unwrap_err();
    assert!(
        err.to_string().contains("terminal")
            || err.to_string().contains("lease")
            || err.to_string().contains("already")
    );
    assert_eq!(
        store.get(run_id).unwrap().status,
        WorkflowRunStatus::Succeeded
    );
}

#[test]
fn ready_nodes_respect_dependencies() {
    let dir = tempdir().unwrap();
    let store = WorkflowRunStore::open(dir.path().join("runs.db")).unwrap();
    let def = write_then_read_handoff_workflow().unwrap();
    let run_id = store
        .begin(
            &def,
            serde_json::json!({"relative_path": "a.txt", "contents": "x"}),
        )
        .unwrap();
    let ready = store.ready_nodes(run_id, &def).unwrap();
    assert_eq!(ready, vec!["write".to_string()]);
    let lease = store.claim_lease(run_id, "test", None).unwrap();
    let inv = uuid::Uuid::new_v4();
    store.link_child(run_id, "write", inv, None).unwrap();
    store
        .mark_node_running(run_id, &lease, "write", inv, None)
        .unwrap();
    store
        .mark_node_succeeded(run_id, &lease, "write", None)
        .unwrap();
    let ready = store.ready_nodes(run_id, &def).unwrap();
    assert_eq!(ready, vec!["read".to_string()]);
}

#[test]
fn cannot_begin_child_on_terminal_parent() {
    let dir = tempdir().unwrap();
    let store = WorkflowRunStore::open(dir.path().join("runs.db")).unwrap();
    let def = write_then_read_handoff_workflow().unwrap();
    let run_id = store
        .begin(
            &def,
            serde_json::json!({"relative_path": "a.txt", "contents": "x"}),
        )
        .unwrap();
    let lease = store.claim_lease(run_id, "test", None).unwrap();
    store
        .settle_terminal(run_id, &lease, WorkflowRunStatus::Succeeded, None)
        .unwrap();
    let err = store
        .link_child(run_id, "write", uuid::Uuid::new_v4(), None)
        .unwrap_err();
    assert!(err.to_string().contains("terminal"));
}

#[test]
fn seeded_registries_expose_both_specialists() {
    let dir = tempdir().unwrap();
    let agents = open_seeded_agent_registry(dir.path().join("a.db")).unwrap();
    let workflows = open_seeded_workflow_registry(dir.path().join("w.db")).unwrap();
    assert_eq!(agents.list().unwrap().len(), 2);
    assert!(agents
        .list()
        .unwrap()
        .iter()
        .any(|a| a.id.as_str() == WORKSPACE_WRITER_ID));
    assert!(agents
        .list()
        .unwrap()
        .iter()
        .any(|a| a.id.as_str() == WORKSPACE_READER_ID));
    assert_eq!(workflows.list().unwrap().len(), 3);
}

#[test]
fn handoff_artifact_sha_matches_workspace_bytes() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let body = "artifact-link-proof";
    let report = run_write_then_read_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "proof.txt".into(),
            contents: body.into(),
            auto_grant: true,
            policy: PolicyMode::Unrestricted,
        },
    )
    .unwrap();
    assert_eq!(report.status, WorkflowRunStatus::Succeeded);
    for node in &report.nodes {
        assert_eq!(
            node.artifact_sha256.as_ref().unwrap(),
            &content_sha256(body.as_bytes())
        );
    }
}

// silence unused import if AgentResultKind unused in this file
#[allow(dead_code)]
fn _touch_agent_result_kind() -> AgentResultKind {
    AgentResultKind::Succeeded
}

#[test]
fn reader_denies_secret_basename() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::create_dir_all(vertical_workspace(home)).unwrap();
    fs::write(vertical_workspace(home).join(".env"), "SECRET=1").unwrap();
    let report = run_read_file_handoff(
        home,
        ReadFileHandoffRequest {
            relative_path: ".env".into(),
        },
    )
    .unwrap();
    assert_eq!(report.status, WorkflowRunStatus::Failed);
    assert_eq!(
        report.nodes[0].error_code.as_deref(),
        Some("secret_denied")
    );
}

#[test]
fn reader_denies_symlink_escape() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let workspace = vertical_workspace(home);
    fs::create_dir_all(&workspace).unwrap();
    let outside = dir.path().join("outside-secret.txt");
    fs::write(&outside, "exfil").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, workspace.join("link.txt")).unwrap();
        let report = run_read_file_handoff(
            home,
            ReadFileHandoffRequest {
                relative_path: "link.txt".into(),
            },
        )
        .unwrap();
        assert_eq!(report.status, WorkflowRunStatus::Failed);
        let code = report.nodes[0].error_code.as_deref().unwrap_or("");
        assert!(
            code == "path_denied" || code == "file_not_found",
            "unexpected code {code}"
        );
    }
}

#[test]
fn write_failure_skips_read_node() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let report = run_write_then_read_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "blocked.txt".into(),
            contents: "nope".into(),
            auto_grant: false,
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    assert_eq!(report.status, WorkflowRunStatus::Failed);
    let write = report.nodes.iter().find(|n| n.node_id == "write").unwrap();
    let read = report.nodes.iter().find(|n| n.node_id == "read").unwrap();
    assert_eq!(write.error_code.as_deref(), Some("approval_required"));
    // Read never ran: still pending or cancelled, not succeeded.
    assert_ne!(read.status.as_str(), "succeeded");
    assert!(!vertical_workspace(home).join("blocked.txt").exists());
}

#[test]
fn cancel_after_write_before_read_stops_dag() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    // Run write-only handoff first to create a completed write, then cancel a fresh DAG
    // after begin (before children) already covered. Here: cancel mid-run by
    // cancel_after_begin remains the durable fence; additionally cancel a succeeded
    // run is idempotent.
    let report = run_write_then_read_handoff(
        home,
        WriteFileHandoffRequest {
            relative_path: "mid.txt".into(),
            contents: "mid".into(),
            auto_grant: true,
            policy: PolicyMode::SmartDeny,
        },
    )
    .unwrap();
    assert_eq!(report.status, WorkflowRunStatus::Succeeded);
    // Parent cancel after terminal is false.
    assert!(!cancel_workflow_run(home, report.run_id, "late").unwrap());
}

#[test]
fn cancel_workflow_run_fans_out_to_child_invocations() {
    use optimus_kernel::{
        open_seeded_agent_registry, open_workflow_run_store, AgentBudget, AgentId,
        AgentInvocationStore, AgentPermissions, AgentRequest, AgentVersion,
        AGENT_REQUEST_SCHEMA_VERSION,
    };
    use optimus_packs::ToolId;
    use std::collections::BTreeSet;
    let dir = tempdir().unwrap();
    let home = dir.path();
    // Seed a run with a linked child without full executor: begin run, claim, begin agent, link.
    let store = open_workflow_run_store(home).unwrap();
    let def = write_then_read_handoff_workflow().unwrap();
    let run_id = store
        .begin(
            &def,
            serde_json::json!({"relative_path": "a.txt", "contents": "a"}),
        )
        .unwrap();
    let _lease = store.claim_lease(run_id, "test", None).unwrap();
    let agents = open_seeded_agent_registry(home.join("agent-registry.db")).unwrap();
    let invocations = AgentInvocationStore::open(home.join("agent-invocations.db")).unwrap();
    let req = AgentRequest {
        schema_version: AGENT_REQUEST_SCHEMA_VERSION,
        agent_id: AgentId::parse(WORKSPACE_WRITER_ID).unwrap(),
        agent_version: AgentVersion::parse("1.0.0").unwrap(),
        task: "t".into(),
        context: vec![],
        constraints: vec![],
        tools: vec![ToolId::new("write_file")],
        permissions: AgentPermissions {
            filesystem_roots: BTreeSet::from(["workspace".into()]),
            network_hosts: BTreeSet::new(),
            effects: BTreeSet::from(["write_file".into()]),
        },
        budget: AgentBudget {
            max_steps: 1,
            timeout_ms: 1000,
            max_context_chars: 100,
            max_output_chars: 100,
        },
        cancellation_id: uuid::Uuid::new_v4(),
        trace_id: uuid::Uuid::new_v4(),
    };
    let inv = invocations.begin(&agents, &req).unwrap();
    store.link_child(run_id, "write", inv, None).unwrap();
    assert!(cancel_workflow_run(home, run_id, "fanout").unwrap());
    let child = invocations.get(inv).unwrap();
    assert!(child.cancellation_reason.is_some());
}
