use std::process::Command;

use optimus_eval::{
    build_evaluation_report, priority2_dataset, priority2_offline_candidate_binding,
    CandidateBinding, EvaluationComparison, EvaluationMetric, EvaluationObservation,
    EvaluationReportV1, MAX_EVALUATION_DATASET_BYTES,
};
use tempfile::tempdir;

fn hash(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn report(source_hash: String, latency_millis: u64) -> EvaluationReportV1 {
    report_with_binding(
        priority2_offline_candidate_binding(source_hash).unwrap(),
        latency_millis,
    )
}

fn report_with_binding(binding: CandidateBinding, latency_millis: u64) -> EvaluationReportV1 {
    let dataset = priority2_dataset();
    let observations = dataset
        .cases
        .iter()
        .map(|case| EvaluationObservation {
            case_id: case.id.clone(),
            exact_text: true,
            expected_tools: case.expected_tools.len(),
            observed_tools: case.expected_tools.len(),
            matched_tools: case.expected_tools.len(),
            terminal_correct: true,
            replay_correct: true,
            trace_present: true,
            latency_millis,
            cost_microunits: 1,
        })
        .collect::<Vec<_>>();
    build_evaluation_report(&dataset, binding, &observations, &[]).unwrap()
}

fn command(
    home: &std::path::Path,
    baseline: &std::path::Path,
    candidate: &std::path::Path,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_optimus"))
        .arg("--home")
        .arg(home)
        .args(["eval", "compare", "--baseline"])
        .arg(baseline)
        .arg("--candidate")
        .arg(candidate)
        .output()
        .unwrap()
}

fn assert_failure_without_mutation(
    directory: &std::path::Path,
    name: &str,
    baseline_bytes: &[u8],
    candidate_bytes: &[u8],
) {
    let home = directory.join(format!("{name}-home"));
    let baseline = directory.join(format!("{name}-baseline.json"));
    let candidate = directory.join(format!("{name}-candidate.json"));
    std::fs::write(&baseline, baseline_bytes).unwrap();
    std::fs::write(&candidate, candidate_bytes).unwrap();
    let output = command(&home, &baseline, &candidate);
    assert!(!output.status.success(), "{name}");
    assert!(output.stdout.is_empty(), "{name}");
    assert!(!home.exists(), "{name}");
    assert_eq!(std::fs::read(&baseline).unwrap(), baseline_bytes, "{name}");
    assert_eq!(
        std::fs::read(&candidate).unwrap(),
        candidate_bytes,
        "{name}"
    );
}

#[test]
fn eval_compare_prints_exact_read_only_comparison_for_distinct_source_trees() {
    let directory = tempdir().unwrap();
    let home = directory.path().join("absent-home");
    let baseline_path = directory.path().join("baseline.json");
    let candidate_path = directory.path().join("candidate.json");
    let baseline = report(hash('a'), 1);
    let candidate = report(hash('b'), 2);
    let baseline_bytes = serde_json::to_vec(&baseline).unwrap();
    let candidate_bytes = serde_json::to_vec(&candidate).unwrap();
    std::fs::write(&baseline_path, &baseline_bytes).unwrap();
    std::fs::write(&candidate_path, &candidate_bytes).unwrap();

    let output = command(&home, &baseline_path, &candidate_path);
    let second_home = directory.path().join("second-absent-home");
    let second_output = command(&second_home, &baseline_path, &candidate_path);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(second_output.status.success());
    assert_eq!(second_output.stdout, output.stdout);
    let comparison: EvaluationComparison = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(comparison.baseline_sha256, baseline.report_sha256);
    assert_eq!(comparison.candidate_sha256, candidate.report_sha256);
    assert!(comparison.improved.is_empty());
    assert_eq!(comparison.regressed, vec![EvaluationMetric::LatencyMillis]);
    assert_eq!(comparison.equal.len(), 6);
    assert!(!home.exists());
    assert!(!second_home.exists());
    assert_eq!(std::fs::read(&baseline_path).unwrap(), baseline_bytes);
    assert_eq!(std::fs::read(&candidate_path).unwrap(), candidate_bytes);
}

#[test]
fn eval_compare_rejects_bounded_invalid_or_incompatible_evidence_without_mutation() {
    let directory = tempdir().unwrap();
    let baseline = report(hash('a'), 1);
    let candidate = report(hash('b'), 2);
    let baseline_bytes = serde_json::to_vec(&baseline).unwrap();
    let candidate_bytes = serde_json::to_vec(&candidate).unwrap();

    assert_failure_without_mutation(directory.path(), "malformed", b"not-json", &candidate_bytes);
    assert_failure_without_mutation(
        directory.path(),
        "oversized-baseline",
        &vec![b' '; MAX_EVALUATION_DATASET_BYTES + 1],
        &candidate_bytes,
    );
    assert_failure_without_mutation(
        directory.path(),
        "oversized-candidate",
        &baseline_bytes,
        &vec![b' '; MAX_EVALUATION_DATASET_BYTES + 1],
    );

    let mut tampered = candidate.clone();
    tampered.report_sha256 = hash('f');
    assert_failure_without_mutation(
        directory.path(),
        "tampered",
        &baseline_bytes,
        &serde_json::to_vec(&tampered).unwrap(),
    );

    let mut drifted_binding = priority2_offline_candidate_binding(hash('b')).unwrap();
    drifted_binding.route_policy_sha256 = hash('c');
    let drifted = report_with_binding(drifted_binding, 2);
    assert_failure_without_mutation(
        directory.path(),
        "context-drift",
        &baseline_bytes,
        &serde_json::to_vec(&drifted).unwrap(),
    );
}
