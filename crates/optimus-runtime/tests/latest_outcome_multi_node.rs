//! Regression: `latest_effect_attempt_outcome` must return the most recently
//! executed node's outcome on a multi-node job (spec-014 R5).
//!
//! attempt_no is scoped per node (UNIQUE(node_id, attempt_no)), so node 0 and
//! node 1 both start at 1. Ordering the "latest" query by attempt_no alone
//! surfaced node 0's stale outcome after node 1 ran, which broke the kernel's
//! approval-resolve provenance check on the second card.

use optimus_runtime::Runtime;
use tempfile::tempdir;

fn write_effect(workspace_sha256: &str, relative_path: &str, contents: &str) -> String {
    serde_json::to_string(&optimus_graph::Effect::ProjectWriteFile {
        workspace_sha256: workspace_sha256.to_string(),
        relative_path: relative_path.to_string(),
        contents: contents.to_string(),
    })
    .unwrap()
}

#[test]
fn latest_outcome_is_the_last_node_not_the_lowest_attempt_no() {
    let root = tempdir().unwrap();
    let ws = root.path().join("ws");
    let rt = Runtime::open(&root.path().join("optimus.db"), &ws).unwrap();
    let sha = rt.workspace_sha256();
    let job = rt
        .create_job(optimus_graph::JobSpec {
            label: "two".into(),
            budget: Default::default(),
            nodes: vec![
                optimus_graph::NodeSpec {
                    label: "a".into(),
                    effect: serde_json::from_str(&write_effect(&sha, "a.txt", "a")).unwrap(),
                },
                optimus_graph::NodeSpec {
                    label: "b".into(),
                    effect: serde_json::from_str(&write_effect(&sha, "b.txt", "b")).unwrap(),
                },
            ],
        })
        .unwrap();

    // Node 0 parks; approve it; node 0 settles and node 1 re-parks.
    assert!(matches!(
        rt.run_next(job).unwrap_err(),
        optimus_runtime::RuntimeError::NeedsApproval { .. }
    ));
    rt.grant_approval(optimus_runtime::ApprovalGrant::for_job(job))
        .unwrap();
    assert_eq!(
        rt.run_all(job).unwrap(),
        optimus_runtime::JobStatus::AwaitingApproval
    );

    // Node 1 parks; approve it; the whole job succeeds.
    let pending_after_first = rt.list_pending_approvals().unwrap();
    assert_eq!(pending_after_first.len(), 1);
    assert_eq!(pending_after_first[0].node_index, Some(1));
    rt.grant_approval(optimus_runtime::ApprovalGrant::for_job(job))
        .unwrap();
    assert_eq!(
        rt.run_all(job).unwrap(),
        optimus_runtime::JobStatus::Succeeded
    );

    // The latest outcome must be node 1's ("b"), not node 0's stale "a".
    let latest = rt.latest_effect_outcome(job).unwrap().unwrap();
    assert_eq!(latest.status, "succeeded");
    assert_eq!(latest.node_id, pending_after_first[0].node_id.unwrap());
    let receipt: serde_json::Value =
        serde_json::from_str(latest.receipt_json.as_deref().unwrap()).unwrap();
    assert_eq!(receipt["relative_path"], "b.txt");
}
