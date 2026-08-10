//! Spec-014 R4/R5 kernel approval vertical: the multi-node re-park path.
//!
//! A two-node runtime job parks on node 0; the human approves; node 0
//! settles and node 1 re-parks. The kernel must settle the first call's
//! outcome, finish its approval, synthesize the `{base}:node{n}` binding
//! (cloned ToolCall, never nested, one transaction), and return
//! `still_pending: true` so the host skips resuming and the second card
//! renders via the record (get_session projection -> reload).

use optimus_kernel::{
    ChatApprovalDecision, ChatApprovalStatus, ExecutionStatus, ExecutionStore, Kernel,
    KernelConfig, ProjectAuthorityStore, Role, SessionStore, ToolCall, ToolLifecycleEvent,
    ToolLifecyclePhase, TOOL_LIFECYCLE_SCHEMA_VERSION,
};
use optimus_packs::ToolId;
use optimus_runtime::PendingApproval;
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

/// The summary the kernel derives from a write effect's JSON.
/// The test constructs the FIRST binding manually (the second is synthesized
/// by the kernel itself), so the summary text only has to be self-consistent.
fn effect_sha256(effect_json: &str) -> String {
    format!("{:x}", Sha256::digest(effect_json.as_bytes()))
}

fn write_effect(workspace_sha256: &str, relative_path: &str, contents: &str) -> String {
    serde_json::to_string(&optimus_graph::Effect::ProjectWriteFile {
        workspace_sha256: workspace_sha256.to_string(),
        relative_path: relative_path.to_string(),
        contents: contents.to_string(),
    })
    .unwrap()
}

fn approve_required_event(
    manifest_id: uuid::Uuid,
    call: &ToolCall,
    binding: &optimus_kernel::ToolApprovalBinding,
) -> ToolLifecycleEvent {
    ToolLifecycleEvent {
        schema_version: TOOL_LIFECYCLE_SCHEMA_VERSION,
        event_id: format!("{manifest_id}:{}:approval_required", call.id),
        run_id: manifest_id.to_string(),
        call_id: call.id.clone(),
        tool_id: binding.tool_id.clone(),
        phase: ToolLifecyclePhase::ApprovalRequired,
        summary: binding.summary.clone(),
        duration_ms: None,
        outcome: None,
        approval: Some(binding.clone()),
    }
}

#[test]
fn two_node_job_reparks_after_first_approval_and_synthesizes_node_binding() {
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let authority = ProjectAuthorityStore::open(home.path()).unwrap();
    let selection = authority.stage_native_selection(project.path()).unwrap();
    authority
        .authorize_project(
            "project-a",
            std::slice::from_ref(&selection.path),
            Some(&selection.path),
            std::slice::from_ref(&selection.grant_token),
        )
        .unwrap();
    let mut kernel =
        Kernel::open_project_session(home.path(), KernelConfig::default(), None, "project-a")
            .unwrap();
    let session_id = kernel.session_id();
    let workspace_sha = kernel.runtime.workspace_sha256();

    // Build the multi-node job directly in the runtime: no chat creator makes
    // two-node jobs, so the vertical constructs the state the kernel must
    // resolve (both nodes high-risk -> node 0 parks, node 1 re-parks).
    let node0_effect = write_effect(&workspace_sha, "src/proof.txt", "safe");
    let node1_effect = write_effect(&workspace_sha, "src/proof2.txt", "safe2");
    let job = kernel
        .runtime
        .create_job(optimus_graph::JobSpec {
            label: "two-writes".into(),
            budget: Default::default(),
            nodes: vec![
                optimus_graph::NodeSpec {
                    label: "write-1".into(),
                    effect: serde_json::from_str(&node0_effect).unwrap(),
                },
                optimus_graph::NodeSpec {
                    label: "write-1:node1".into(),
                    effect: serde_json::from_str(&node1_effect).unwrap(),
                },
            ],
        })
        .unwrap();
    let park = kernel.runtime.run_next(job).unwrap_err();
    assert!(matches!(
        park,
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));

    // Seed the kernel-side execution state so the resolve path sees a real
    // manifest + turn + pending approval for the first node.
    let sessions = SessionStore::open(home.path().join("sessions.db")).unwrap();
    let system = optimus_kernel::Message {
        role: Role::System,
        content: "system".into(),
        tool_call_id: None,
        name: None,
        reasoning_content: None,
    };
    let user = optimus_kernel::Message {
        role: Role::User,
        content: "write both files".into(),
        tool_call_id: None,
        name: None,
        reasoning_content: None,
    };
    let turn_id = sessions
        .begin_turn(session_id, "session", &["core".into()], &[system, user], 1)
        .unwrap();
    let executions = ExecutionStore::open(home.path().join("execution.db")).unwrap();
    let manifest_id = executions
        .begin(
            session_id, turn_id, "offline", "offline", b"prompt", b"{}", b"{}",
        )
        .unwrap();

    let pending0: PendingApproval = kernel
        .runtime
        .list_pending_approvals()
        .unwrap()
        .into_iter()
        .find(|p| p.job_id == job)
        .expect("node 0 must be pending");
    let node0_id = pending0.node_id.expect("node 0 identity");
    let sha0 = effect_sha256(&pending0.effect_json);
    assert_eq!(pending0.node_index, Some(0));

    let call = ToolCall {
        id: "write-1".into(),
        name: "write_file".into(),
        arguments: json!({"path": "src/proof.txt", "contents": "safe"}),
    };
    let binding = optimus_kernel::ToolApprovalBinding {
        run_id: manifest_id,
        call_id: "write-1".into(),
        tool_id: ToolId::new("write_file"),
        job_id: job,
        node_id: node0_id,
        node_index: 0,
        effect_sha256: sha0.clone(),
        summary: "Write src/proof.txt (4 bytes)".into(),
        command_class: None,
    };
    let event = approve_required_event(manifest_id, &call, &binding);
    executions
        .record_chat_approval_required(manifest_id, &call, &event, &binding)
        .unwrap();

    // Resolve node 0: grant + resume runs node 0, node 1 parks -> re-park.
    let resolution = kernel
        .resolve_chat_approval_exact(
            manifest_id,
            "write-1",
            job,
            node0_id,
            0,
            &sha0,
            ChatApprovalDecision::Approve,
        )
        .unwrap();

    assert!(resolution.still_pending, "node 1 must still be parked");
    assert_eq!(resolution.status, ChatApprovalStatus::Approved);
    assert_eq!(resolution.event.phase, ToolLifecyclePhase::Succeeded);
    // The synthesized next binding is the second card's identity.
    let pending_binding = resolution.pending_binding.expect("synthesized binding");
    assert_eq!(pending_binding.call_id, "write-1:node1");
    assert_eq!(pending_binding.node_index, 1);
    assert_eq!(pending_binding.job_id, job);
    assert_eq!(pending_binding.run_id, manifest_id);
    assert_eq!(pending_binding.tool_id, ToolId::new("write_file"));
    let pending_event = resolution.pending_event.expect("second card event");
    assert_eq!(pending_event.phase, ToolLifecyclePhase::ApprovalRequired);
    assert_eq!(pending_event.call_id, "write-1:node1");
    assert_eq!(pending_event.approval, Some(pending_binding.clone()));

    // Node 0 actually ran; the first call's approval is finished.
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/proof.txt")).unwrap(),
        "safe"
    );
    assert!(!project.path().join("src/proof2.txt").exists());
    assert!(
        executions
            .pending_chat_approval(manifest_id, "write-1")
            .unwrap()
            .is_none(),
        "the first call's approval must be finished"
    );
    let (next_binding, next_call) = executions
        .pending_chat_approval(manifest_id, "write-1:node1")
        .unwrap()
        .expect("the synthesized approval must be pending in the record");
    assert_eq!(next_call.id, "write-1:node1");
    assert_eq!(next_call.name, "write_file");
    assert_eq!(next_binding.node_index, 1);

    // The runtime still has exactly one pending approval: the re-parked node.
    let pending_after: Vec<PendingApproval> = kernel
        .runtime
        .list_pending_approvals()
        .unwrap()
        .into_iter()
        .filter(|p| p.job_id == job)
        .collect();
    assert_eq!(pending_after.len(), 1);
    let node1_id = pending_after[0].node_id.expect("node 1 identity");
    let sha1 = effect_sha256(&pending_after[0].effect_json);
    assert_eq!(pending_after[0].node_index, Some(1));

    // The turn and manifest stay Running (the host skips resume).
    let active = sessions.active_turn(session_id).unwrap().unwrap();
    assert_eq!(active.status, optimus_kernel::TurnStatus::Running);
    assert_eq!(
        executions.manifest(manifest_id).unwrap().status,
        ExecutionStatus::Running
    );

    // Resolve the second card: node 1 runs, the job is terminal.
    let second = kernel
        .resolve_chat_approval_exact(
            manifest_id,
            "write-1:node1",
            job,
            node1_id,
            1,
            &sha1,
            ChatApprovalDecision::Approve,
        )
        .unwrap();
    assert!(!second.still_pending, "both nodes settled");
    assert_eq!(second.status, ChatApprovalStatus::Approved);
    assert_eq!(second.event.phase, ToolLifecyclePhase::Succeeded);
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/proof2.txt")).unwrap(),
        "safe2"
    );
    assert_eq!(
        kernel.runtime.job_status(job).unwrap(),
        optimus_runtime::JobStatus::Succeeded
    );
    assert!(
        executions
            .pending_chat_approval(manifest_id, "write-1:node1")
            .unwrap()
            .is_none(),
        "the second approval must be finished"
    );
}
