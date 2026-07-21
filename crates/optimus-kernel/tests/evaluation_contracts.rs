use optimus_kernel::{
    build_evaluation_report, compare_evaluation_reports, priority2_dataset,
    priority2_offline_candidate_binding, project_evaluation_observations,
    run_offline_integrity_suite, run_offline_trajectory_suite, run_priority2_offline_evaluation,
    BaselineStore, CandidateBinding, EvaluationDataset, EvaluationMetric, EvaluationObservation,
    EvaluationReportV1, EvaluationResourceMeasurement, ExecutionStatus, MetricDirection,
    MetricThreshold, ReplayClassification,
};
use optimus_packs::ToolId;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn hash(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn binding() -> CandidateBinding {
    priority2_offline_candidate_binding(hash('a')).unwrap()
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
            trace_present: true,
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
fn report_rejects_observation_missing_required_trace() {
    let dataset = priority2_dataset();
    let mut observations = passing_observations(&dataset);
    observations[0].trace_present = false;

    let error = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap_err();

    assert!(error
        .to_string()
        .contains("required trace evidence is missing"));
}

#[test]
fn trace_presence_is_required_in_json_but_optional_cases_accept_false() {
    let mut dataset = priority2_dataset();
    dataset.cases[0].trace_required = false;
    let mut observations = passing_observations(&dataset);
    let with_optional_trace =
        build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();

    let mut encoded = serde_json::to_value(&observations[0]).unwrap();
    encoded.as_object_mut().unwrap().remove("trace_present");
    assert!(serde_json::from_value::<EvaluationObservation>(encoded).is_err());

    observations[0].trace_present = false;
    let without_optional_trace =
        build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    assert_eq!(without_optional_trace, with_optional_trace);
}

#[test]
fn offline_trajectory_runner_returns_exact_typed_persisted_evidence() {
    let directory = tempdir().unwrap();
    let report = run_offline_trajectory_suite(directory.path());
    assert!(report.all_ok(), "{:#?}", report.cases);
    assert_eq!(report.passed, 4);

    let dataset = priority2_dataset();
    for (result, contract) in report.cases.iter().zip(dataset.cases.iter().take(4)) {
        assert_eq!(result.id, contract.id);
        assert_eq!(
            result.assistant_text,
            contract.exact_assistant_text.as_deref().unwrap()
        );
        assert_eq!(result.invoked_tools, contract.expected_tools);
        assert_eq!(result.terminal_status, Some(ExecutionStatus::Succeeded));
        assert_eq!(result.replay, Some(ReplayClassification::FixtureReplayable));
        let trace = result.trace_context.expect("passing case retains trace");
        assert!(trace.parent_span_id.is_none());
    }
    assert_eq!(
        report.cases[1].invoked_tools,
        vec![ToolId::new("memory_recall")]
    );
}

#[test]
fn offline_trajectory_runner_never_fabricates_evidence_for_unusable_home() {
    let directory = tempdir().unwrap();
    let blocked_home = directory.path().join("not-a-directory");
    std::fs::write(&blocked_home, b"blocked").unwrap();

    let report = run_offline_trajectory_suite(&blocked_home);

    assert_eq!(report.passed, 0);
    assert_eq!(report.failed, 4);
    assert!(report.cases.iter().all(|result| {
        !result.ok
            && result.invoked_tools.is_empty()
            && result.terminal_status.is_none()
            && result.replay.is_none()
            && result.trace_context.is_none()
    }));
    assert_eq!(std::fs::read(&blocked_home).unwrap(), b"blocked");
}

#[test]
fn exact_suite_results_project_to_canonical_observations_with_explicit_resources() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let trajectory = run_offline_trajectory_suite(directory.path().join("trajectory"));
    let integrity = run_offline_integrity_suite(directory.path().join("integrity")).unwrap();
    let mut results = trajectory
        .cases
        .into_iter()
        .chain(integrity.cases)
        .collect::<Vec<_>>();
    results.reverse();
    let mut measurements = dataset
        .cases
        .iter()
        .enumerate()
        .map(|(index, case)| EvaluationResourceMeasurement {
            case_id: case.id.clone(),
            latency_millis: index as u64 + 1,
            cost_microunits: (index as u64 + 1) * 10,
        })
        .collect::<Vec<_>>();
    measurements.reverse();

    let observations = project_evaluation_observations(&dataset, &results, &measurements).unwrap();

    assert_eq!(
        observations
            .iter()
            .map(|observation| observation.case_id.as_str())
            .collect::<Vec<_>>(),
        dataset
            .cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(observations.iter().all(|observation| {
        observation.exact_text
            && observation.terminal_correct
            && observation.replay_correct
            && observation.trace_present
    }));
    assert_eq!(observations[1].expected_tools, 1);
    assert_eq!(observations[1].observed_tools, 1);
    assert_eq!(observations[1].matched_tools, 1);
    assert_eq!(observations[0].latency_millis, 1);
    assert_eq!(observations[9].cost_microunits, 100);

    results
        .iter_mut()
        .find(|result| result.id == "memory-then-answer")
        .unwrap()
        .invoked_tools
        .push(ToolId::new("memory_recall"));
    let duplicated = project_evaluation_observations(&dataset, &results, &measurements).unwrap();
    assert_eq!(duplicated[1].observed_tools, 2);
    assert_eq!(duplicated[1].matched_tools, 1);

    let mut duplicate_results = results.clone();
    duplicate_results.push(results[0].clone());
    assert!(project_evaluation_observations(&dataset, &duplicate_results, &measurements).is_err());
    let mut missing_measurement = measurements.clone();
    missing_measurement.pop();
    assert!(project_evaluation_observations(&dataset, &results, &missing_measurement).is_err());
    let mut unknown_measurement = measurements.clone();
    unknown_measurement[0].case_id = "unknown-case".into();
    assert!(project_evaluation_observations(&dataset, &results, &unknown_measurement).is_err());
}

#[test]
fn exact_offline_runner_produces_one_deterministic_candidate_report() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let measurements = dataset
        .cases
        .iter()
        .enumerate()
        .map(|(index, case)| EvaluationResourceMeasurement {
            case_id: case.id.clone(),
            latency_millis: index as u64 + 1,
            cost_microunits: (index as u64 + 1) * 10,
        })
        .collect::<Vec<_>>();
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
            5,
            10,
        )
        .unwrap(),
    ];

    let first =
        run_priority2_offline_evaluation(directory.path(), binding(), &measurements, &thresholds)
            .unwrap();
    let second =
        run_priority2_offline_evaluation(directory.path(), binding(), &measurements, &thresholds)
            .unwrap();

    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert!(first.passed);
    assert_eq!(first.binding, binding());
    assert_eq!(first.metrics[&EvaluationMetric::ExactText].samples, 10);
    assert_eq!(first.metrics[&EvaluationMetric::ExactText].value, 10_000);
    assert_eq!(
        first.metrics[&EvaluationMetric::ToolPrecision].value,
        10_000
    );
    assert_eq!(first.metrics[&EvaluationMetric::ToolRecall].value, 10_000);
    assert_eq!(
        first.metrics[&EvaluationMetric::TerminalAccuracy].value,
        10_000
    );
    assert_eq!(
        first.metrics[&EvaluationMetric::ReplayAccuracy].value,
        10_000
    );
    assert_eq!(first.metrics[&EvaluationMetric::LatencyMillis].value, 5);
    assert_eq!(first.metrics[&EvaluationMetric::CostMicrounits].value, 55);
    assert_eq!(
        std::fs::read_dir(directory.path().join("evaluation-runs"))
            .unwrap()
            .count(),
        2
    );

    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, b"preserve").unwrap();
    assert!(
        run_priority2_offline_evaluation(&blocked, binding(), &measurements, &thresholds,).is_err()
    );
    assert_eq!(std::fs::read(&blocked).unwrap(), b"preserve");
}

#[test]
fn exact_offline_runner_preflights_caller_contracts_before_mutation() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let measurements = dataset
        .cases
        .iter()
        .map(|case| EvaluationResourceMeasurement {
            case_id: case.id.clone(),
            latency_millis: 1,
            cost_microunits: 0,
        })
        .collect::<Vec<_>>();

    let invalid_binding_home = directory.path().join("invalid-binding");
    let mut invalid_binding = binding();
    invalid_binding.provider.clear();
    assert!(run_priority2_offline_evaluation(
        &invalid_binding_home,
        invalid_binding,
        &measurements,
        &[],
    )
    .is_err());
    assert!(!invalid_binding_home.join("evaluation-runs").exists());

    let invalid_measurement_home = directory.path().join("invalid-measurement");
    let mut invalid_measurements = measurements.clone();
    invalid_measurements[0].case_id = "unknown-case".into();
    assert!(run_priority2_offline_evaluation(
        &invalid_measurement_home,
        binding(),
        &invalid_measurements,
        &[],
    )
    .is_err());
    assert!(!invalid_measurement_home.join("evaluation-runs").exists());

    let invalid_threshold_home = directory.path().join("invalid-threshold");
    let invalid_threshold = MetricThreshold {
        metric: EvaluationMetric::ExactText,
        direction: MetricDirection::Minimum,
        value: 10_001,
        min_samples: 10,
    };
    assert!(run_priority2_offline_evaluation(
        &invalid_threshold_home,
        binding(),
        &measurements,
        &[invalid_threshold],
    )
    .is_err());
    assert!(!invalid_threshold_home.join("evaluation-runs").exists());

    let duplicate_threshold_home = directory.path().join("duplicate-threshold");
    let threshold = MetricThreshold::new(
        EvaluationMetric::ExactText,
        MetricDirection::Minimum,
        10_000,
        10,
    )
    .unwrap();
    assert!(run_priority2_offline_evaluation(
        &duplicate_threshold_home,
        binding(),
        &measurements,
        &[threshold.clone(), threshold],
    )
    .is_err());
    assert!(!duplicate_threshold_home.join("evaluation-runs").exists());
}

#[test]
fn exact_offline_runner_accepts_only_the_derived_compiled_context() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let measurements = dataset
        .cases
        .iter()
        .map(|case| EvaluationResourceMeasurement {
            case_id: case.id.clone(),
            latency_millis: 1,
            cost_microunits: 0,
        })
        .collect::<Vec<_>>();
    let derived = priority2_offline_candidate_binding(hash('a')).unwrap();
    assert_eq!(derived.provider, "offline");
    assert_eq!(derived.model, "offline-scripted");
    assert_eq!(derived.source_tree_sha256, hash('a'));
    assert_eq!(
        derived,
        priority2_offline_candidate_binding(hash('a')).unwrap()
    );
    assert!(priority2_offline_candidate_binding("not-a-hash").is_err());

    let mutations = [
        ("contract", {
            let mut value = derived.clone();
            value.contract_sha256 = hash('b');
            value
        }),
        ("tools", {
            let mut value = derived.clone();
            value.tool_catalog_sha256 = hash('c');
            value
        }),
        ("route", {
            let mut value = derived.clone();
            value.route_policy_sha256 = hash('d');
            value
        }),
        ("provider", {
            let mut value = derived.clone();
            value.provider = "codex".into();
            value
        }),
        ("model", {
            let mut value = derived;
            value.model = "gpt-5.6-terra".into();
            value
        }),
    ];
    for (name, binding) in mutations {
        let home = directory.path().join(name);
        assert!(run_priority2_offline_evaluation(&home, binding, &measurements, &[],).is_err());
        assert!(!home.join("evaluation-runs").exists());
    }
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

#[test]
fn baseline_store_rejects_rehashed_report_with_missing_metric_before_insert() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let mut report = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    report.metrics.remove(&EvaluationMetric::CostMicrounits);
    rehash(&mut report);
    let store = BaselineStore::open(directory.path().join("evaluations.db")).unwrap();

    assert!(store.accept(&report).is_err());
    assert!(store.report(&report.report_sha256).is_err());
}

#[test]
fn baseline_store_rejects_rehashed_inconsistent_metric_arithmetic() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let mut report = build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    report
        .metrics
        .get_mut(&EvaluationMetric::ExactText)
        .unwrap()
        .value = 9_999;
    rehash(&mut report);
    let store = BaselineStore::open(directory.path().join("evaluations.db")).unwrap();

    assert!(store.accept(&report).is_err());
}

#[test]
fn baseline_store_rejects_rehashed_inconsistent_threshold_outcome() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let thresholds = vec![MetricThreshold::new(
        EvaluationMetric::ExactText,
        MetricDirection::Minimum,
        10_000,
        dataset.cases.len(),
    )
    .unwrap()];
    let mut report =
        build_evaluation_report(&dataset, binding(), &observations, &thresholds).unwrap();
    report.threshold_failures = vec![EvaluationMetric::ExactText];
    report.passed = false;
    rehash(&mut report);
    let store = BaselineStore::open(directory.path().join("evaluations.db")).unwrap();

    assert!(store.accept(&report).is_err());
}

#[test]
fn report_validation_rejects_invalid_binding_and_duplicate_threshold_dimensions() {
    let directory = tempdir().unwrap();
    let dataset = priority2_dataset();
    let observations = passing_observations(&dataset);
    let mut invalid_binding =
        build_evaluation_report(&dataset, binding(), &observations, &[]).unwrap();
    invalid_binding.binding.provider.clear();
    rehash(&mut invalid_binding);
    let store = BaselineStore::open(directory.path().join("evaluations.db")).unwrap();
    assert!(store.accept(&invalid_binding).is_err());

    let threshold = MetricThreshold::new(
        EvaluationMetric::ExactText,
        MetricDirection::Minimum,
        10_000,
        dataset.cases.len(),
    )
    .unwrap();
    assert!(build_evaluation_report(
        &dataset,
        binding(),
        &observations,
        &[threshold.clone(), threshold],
    )
    .is_err());
}
