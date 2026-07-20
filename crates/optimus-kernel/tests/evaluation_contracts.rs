use optimus_kernel::{
    build_evaluation_report, compare_evaluation_reports, priority2_dataset, BaselineStore,
    CandidateBinding, EvaluationDataset, EvaluationMetric, EvaluationObservation,
    EvaluationReportV1, MetricDirection, MetricThreshold,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn hash(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn binding() -> CandidateBinding {
    CandidateBinding {
        source_tree_sha256: hash('a'),
        contract_sha256: hash('b'),
        tool_catalog_sha256: hash('c'),
        route_policy_sha256: hash('d'),
        provider: "offline".into(),
        model: "offline-scripted".into(),
    }
}

fn rehash(report: &mut EvaluationReportV1) {
    report.report_sha256.clear();
    report.report_sha256 = format!("{:x}", Sha256::digest(serde_json::to_vec(report).unwrap()));
}

fn passing_observations(dataset: &EvaluationDataset) -> Vec<EvaluationObservation> {
    dataset
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
            latency_millis: 10,
            cost_microunits: 0,
        })
        .collect()
}

#[test]
fn priority2_dataset_is_exact_versioned_bounded_and_source_backed() {
    let dataset = priority2_dataset();
    dataset.validate().unwrap();
    assert_eq!(dataset.cases.len(), 10);
    assert_eq!(
        dataset
            .cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "offline-echo",
            "memory-then-answer",
            "pack-activate-browser",
            "write-file-job",
            "sensitivity_denial",
            "smartdeny_approval",
            "route_policy_denial",
            "cooperative_cancellation",
            "stale_completion_fence",
            "gateway_dead_letter",
        ]
    );
    let encoded = serde_json::to_vec(&dataset).unwrap();
    assert_eq!(EvaluationDataset::from_json(&encoded).unwrap(), dataset);

    let mut duplicate = dataset.clone();
    duplicate.cases.push(duplicate.cases[0].clone());
    assert!(duplicate.validate().is_err());
    let mut untrusted = dataset;
    untrusted.provenance_sha256 = "bad".into();
    assert!(untrusted.validate().is_err());
}

#[test]
fn reports_bind_candidate_compute_checked_metrics_and_are_byte_deterministic() {
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let thresholds = vec![
        MetricThreshold::new(
            EvaluationMetric::ExactText,
            MetricDirection::Minimum,
            10_000,
            10,
        )
        .unwrap(),
        MetricThreshold::new(
            EvaluationMetric::LatencyMillis,
            MetricDirection::Maximum,
            20,
            10,
        )
        .unwrap(),
    ];
    let first = build_evaluation_report(&dataset, binding(), &observations, &thresholds).unwrap();
    let second = build_evaluation_report(&dataset, binding(), &observations, &thresholds).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert!(first.passed);
    assert_eq!(first.metrics[&EvaluationMetric::ExactText].value, 10_000);
    assert_eq!(
        first.metrics[&EvaluationMetric::ToolPrecision].value,
        10_000
    );
    assert_eq!(first.metrics[&EvaluationMetric::ToolRecall].value, 10_000);
    assert_eq!(first.metrics[&EvaluationMetric::LatencyMillis].value, 10);
}

#[test]
fn immutable_baseline_comparison_reports_regression_without_rewriting_history() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let baseline = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    let store = BaselineStore::open(directory.path().join("evaluations.db")).unwrap();
    store.accept(&baseline).unwrap();
    assert_eq!(store.report(&baseline.report_sha256).unwrap(), baseline);
    assert!(store.accept(&baseline).is_err());

    let mut degraded = observations;
    degraded[0].exact_text = false;
    degraded[0].latency_millis = 100;
    let mut candidate_binding = binding();
    candidate_binding.source_tree_sha256 = hash('e');
    let candidate = build_evaluation_report(&dataset, candidate_binding, &degraded, &[]).unwrap();
    assert_ne!(
        baseline.binding.source_tree_sha256,
        candidate.binding.source_tree_sha256
    );
    let comparison = compare_evaluation_reports(&baseline, &candidate).unwrap();
    assert_eq!(
        comparison.regressed,
        vec![EvaluationMetric::ExactText, EvaluationMetric::LatencyMillis]
    );
    assert!(comparison.improved.is_empty());
}

#[test]
fn comparison_rejects_tampered_report_hashes() {
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let baseline = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    let mut candidate = baseline.clone();
    candidate.report_sha256 = hash('f');

    assert!(compare_evaluation_reports(&baseline, &candidate).is_err());
}

#[test]
fn comparison_rejects_threshold_policy_drift() {
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let baseline = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    let changed_thresholds = vec![MetricThreshold::new(
        EvaluationMetric::ExactText,
        MetricDirection::Minimum,
        9_000,
        dataset.cases.len(),
    )
    .unwrap()];
    let candidate =
        build_evaluation_report(&dataset, binding(), &observations, &changed_thresholds).unwrap();

    assert!(compare_evaluation_reports(&baseline, &candidate).is_err());
}

#[test]
fn comparison_rejects_metric_set_drift_even_with_a_valid_hash() {
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let mut baseline = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    baseline.metrics.remove(&EvaluationMetric::CostMicrounits);
    rehash(&mut baseline);
    let candidate = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();

    assert!(compare_evaluation_reports(&baseline, &candidate).is_err());
}

#[test]
fn comparison_rejects_non_source_binding_drift() {
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let baseline = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    let mut incompatible_binding = binding();
    incompatible_binding.contract_sha256 = hash('f');
    let candidate =
        build_evaluation_report(&dataset, incompatible_binding, &observations, &[]).unwrap();

    assert!(compare_evaluation_reports(&baseline, &candidate).is_err());
}
