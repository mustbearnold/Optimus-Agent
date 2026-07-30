use std::collections::BTreeMap;
use std::fs;

use optimus_eval::{
    FixtureId, FixtureKind, ReplayBundle, ReplayBundleId, ReplayExecutionStatus, ReplayFixture,
    ReplayStage, ReplayStore, REPLAY_BUNDLE_VERSION,
};
use optimus_kernel::{ExecutionManifest, ExecutionStatus, EXECUTION_MANIFEST_VERSION};
use tempfile::tempdir;
use uuid::Uuid;

fn sha(value: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value))
}

fn source_manifest(status: ExecutionStatus) -> ExecutionManifest {
    ExecutionManifest {
        id: Uuid::new_v4(),
        version: EXECUTION_MANIFEST_VERSION,
        session_id: Uuid::new_v4(),
        turn_id: Uuid::new_v4(),
        provider: "offline".into(),
        model: "offline-scripted".into(),
        autonomy_profile: "review_changes".into(),
        prompt_sha256: sha(b"prompt"),
        tool_catalog_sha256: sha(b"tools"),
        policy_sha256: sha(b"policy"),
        status,
    }
}

fn fixture(stage: u32, kind: FixtureKind, bytes: &[u8]) -> ReplayFixture {
    ReplayFixture::new(stage, kind, "application/json", bytes.to_vec()).unwrap()
}

fn bundle(source: &ExecutionManifest) -> ReplayBundle {
    let response = fixture(1, FixtureKind::ModelResponse, br#"{"text":"pong"}"#);
    let terminal = fixture(
        2,
        FixtureKind::TerminalEvidence,
        br#"{"status":"succeeded"}"#,
    );
    ReplayBundle {
        id: ReplayBundleId::new(),
        version: REPLAY_BUNDLE_VERSION,
        source_manifest_id: source.id,
        trace_id: Uuid::new_v4().to_string(),
        contract_sha256: sha(b"contract-v1"),
        tool_catalog_sha256: source.tool_catalog_sha256.clone(),
        policy_sha256: source.policy_sha256.clone(),
        expected_terminal_sha256: terminal.id.as_str().into(),
        stages: vec![
            ReplayStage::fixture(
                1,
                FixtureKind::ModelResponse,
                sha(b"request"),
                response.id.clone(),
            ),
            ReplayStage::fixture(
                2,
                FixtureKind::TerminalEvidence,
                sha(b"terminal-input"),
                terminal.id.clone(),
            ),
        ],
        fixtures: vec![response, terminal],
    }
}

#[test]
fn fixture_identity_bundle_validation_and_roundtrip_are_fail_closed() {
    assert!(FixtureId::parse("not-a-sha").is_err());
    let source = source_manifest(ExecutionStatus::Succeeded);
    let valid = bundle(&source);
    valid.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<ReplayBundle>(&serde_json::to_string(&valid).unwrap()).unwrap(),
        valid
    );

    let mut duplicate = valid.clone();
    duplicate.fixtures.push(duplicate.fixtures[0].clone());
    assert!(duplicate.validate().is_err());

    let mut missing = valid.clone();
    missing.fixtures.remove(0);
    assert!(missing.validate().is_err());

    let mut extra = valid.clone();
    extra
        .fixtures
        .push(fixture(9, FixtureKind::ToolOutcome, b"{}"));
    assert!(extra.validate().is_err());

    let mut future = valid;
    future.version += 1;
    assert!(future.validate().is_err());
}

#[test]
fn replay_store_is_atomic_immutable_bounded_reopenable_and_corruption_safe() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("replay.db");
    let source = source_manifest(ExecutionStatus::Succeeded);
    let expected = bundle(&source);
    let store = ReplayStore::open(&path).unwrap();
    store.insert_bundle(&source, &expected).unwrap();
    assert_eq!(store.bundle(expected.id).unwrap(), expected);
    assert!(store.insert_bundle(&source, &expected).is_err());
    drop(store);

    let reopened = ReplayStore::open(&path).unwrap();
    assert_eq!(reopened.bundle(expected.id).unwrap(), expected);
    drop(reopened);

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE replay_fixtures SET bytes=X'00' WHERE rowid=(
               SELECT rowid FROM replay_fixtures WHERE bundle_id=?1 ORDER BY stage LIMIT 1
             )",
            [expected.id.to_string()],
        )
        .unwrap();
    drop(connection);
    assert!(ReplayStore::open(&path)
        .unwrap()
        .bundle(expected.id)
        .is_err());

    let running = source_manifest(ExecutionStatus::Running);
    let running_bundle = bundle(&running);
    assert!(ReplayStore::open(directory.path().join("running.db"))
        .unwrap()
        .insert_bundle(&running, &running_bundle)
        .is_err());
}

#[test]
fn replay_executor_consumes_verified_fixtures_without_effect_handles() {
    let directory = tempdir().unwrap();
    let source = source_manifest(ExecutionStatus::Succeeded);
    let expected = bundle(&source);
    let store = ReplayStore::open(directory.path().join("replay.db")).unwrap();
    store.insert_bundle(&source, &expected).unwrap();

    let plan = store
        .plan(&source, expected.id, &expected.trace_id)
        .unwrap();
    assert_eq!(plan.stages.len(), 2);
    let report = store.execute(&plan).unwrap();
    assert_eq!(report.status, ReplayExecutionStatus::Succeeded);
    assert_eq!(report.completed_stages, 2);
    assert!(report.blockers.is_empty());
    assert_eq!(store.report(report.id).unwrap(), report);
    assert!(
        store.execute(&plan).is_err(),
        "one immutable report per bundle"
    );

    assert!(!directory.path().join("unexpected-effect.txt").exists());
    let files = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        files.iter().all(|name| name.starts_with("replay.db")),
        "replay owns only its SQLite database files: {files:?}"
    );
}

#[test]
fn replay_plan_rejects_trace_and_source_drift_before_execution() {
    let directory = tempdir().unwrap();
    let source = source_manifest(ExecutionStatus::Succeeded);
    let expected = bundle(&source);
    let store = ReplayStore::open(directory.path().join("replay.db")).unwrap();
    store.insert_bundle(&source, &expected).unwrap();

    assert!(store.plan(&source, expected.id, "other-trace").is_err());
    let mut drifted = source.clone();
    drifted.policy_sha256 = sha(b"changed");
    assert!(store
        .plan(&drifted, expected.id, &expected.trace_id)
        .is_err());
}

#[test]
fn replay_input_mismatch_persists_failed_report_and_stops_later_stages() {
    let directory = tempdir().unwrap();
    let source = source_manifest(ExecutionStatus::Succeeded);
    let expected = bundle(&source);
    let store = ReplayStore::open(directory.path().join("replay.db")).unwrap();
    store.insert_bundle(&source, &expected).unwrap();
    let plan = store
        .plan(&source, expected.id, &expected.trace_id)
        .unwrap();
    let inputs = BTreeMap::from([(1, sha(b"changed-request")), (2, sha(b"terminal-input"))]);

    let report = store.execute_with_input_hashes(&plan, &inputs).unwrap();
    assert_eq!(report.status, ReplayExecutionStatus::Failed);
    assert_eq!(report.completed_stages, 0);
    assert_eq!(report.blockers, vec!["stage_1_input_hash_mismatch"]);
    assert_eq!(store.report(report.id).unwrap(), report);
}
