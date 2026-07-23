use std::process::Command;
use std::sync::OnceLock;

use optimus_kernel::{
    priority2_dataset, CandidateBinding, EvalReport, EvaluationMetric, EvaluationReportV1,
    EvaluationResourceMeasurement, MetricDirection, MetricThreshold, MAX_EVALUATION_DATASET_BYTES,
};
use tempfile::tempdir;

fn binding() -> CandidateBinding {
    static BINDING: OnceLock<CandidateBinding> = OnceLock::new();
    BINDING
        .get_or_init(|| {
            let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(std::path::Path::parent)
                .unwrap();
            let python = std::env::var_os("PYTHON").unwrap_or_else(|| {
                std::ffi::OsString::from(if cfg!(windows) { "python" } else { "python3" })
            });
            let output = Command::new(python)
                .current_dir(workspace)
                .arg("scripts/engineering_memory.py")
                .arg("binding")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            serde_json::from_slice(&output.stdout).unwrap()
        })
        .clone()
}

fn measurements() -> Vec<EvaluationResourceMeasurement> {
    priority2_dataset()
        .cases
        .into_iter()
        .enumerate()
        .map(|(index, case)| EvaluationResourceMeasurement {
            case_id: case.id,
            latency_millis: index as u64 + 1,
            cost_microunits: (index as u64 + 1) * 10,
        })
        .collect()
}

#[test]
fn eval_report_command_prints_the_exact_candidate_report() {
    let directory = tempdir().unwrap();
    let home = directory.path().join("home");
    let binding_path = directory.path().join("binding.json");
    let measurements_path = directory.path().join("measurements.json");
    let thresholds_path = directory.path().join("thresholds.json");
    std::fs::write(&binding_path, serde_json::to_vec(&binding()).unwrap()).unwrap();
    std::fs::write(
        &measurements_path,
        serde_json::to_vec(&measurements()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &thresholds_path,
        serde_json::to_vec(&vec![MetricThreshold::new(
            EvaluationMetric::ExactText,
            MetricDirection::Minimum,
            10_000,
            10,
        )
        .unwrap()])
        .unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_optimus"))
        .arg("--home")
        .arg(&home)
        .arg("eval")
        .arg("report")
        .arg("--binding")
        .arg(&binding_path)
        .arg("--measurements")
        .arg(&measurements_path)
        .arg("--thresholds")
        .arg(&thresholds_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: EvaluationReportV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report.passed);
    assert_eq!(report.binding, binding());
    assert_eq!(report.thresholds.len(), 1);
    assert_eq!(report.metrics[&EvaluationMetric::ExactText].samples, 10);
    assert_eq!(report.metrics[&EvaluationMetric::LatencyMillis].value, 5);
    assert_eq!(report.metrics[&EvaluationMetric::CostMicrounits].value, 55);

    let no_threshold_home = directory.path().join("no-threshold-home");
    let no_threshold_output = Command::new(env!("CARGO_BIN_EXE_optimus"))
        .arg("--home")
        .arg(&no_threshold_home)
        .arg("eval")
        .arg("report")
        .arg("--binding")
        .arg(&binding_path)
        .arg("--measurements")
        .arg(&measurements_path)
        .output()
        .unwrap();
    assert!(no_threshold_output.status.success());
    let no_threshold_report: EvaluationReportV1 =
        serde_json::from_slice(&no_threshold_output.stdout).unwrap();
    assert!(no_threshold_report.thresholds.is_empty());
}

#[test]
fn eval_report_command_prints_failing_report_before_nonzero_exit() {
    let directory = tempdir().unwrap();
    let home = directory.path().join("home");
    let binding_path = directory.path().join("binding.json");
    let measurements_path = directory.path().join("measurements.json");
    let thresholds_path = directory.path().join("thresholds.json");
    std::fs::write(&binding_path, serde_json::to_vec(&binding()).unwrap()).unwrap();
    std::fs::write(
        &measurements_path,
        serde_json::to_vec(&measurements()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &thresholds_path,
        serde_json::to_vec(&vec![MetricThreshold::new(
            EvaluationMetric::CostMicrounits,
            MetricDirection::Maximum,
            0,
            10,
        )
        .unwrap()])
        .unwrap(),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_optimus"))
        .arg("--home")
        .arg(&home)
        .arg("eval")
        .arg("report")
        .arg("--binding")
        .arg(&binding_path)
        .arg("--measurements")
        .arg(&measurements_path)
        .arg("--thresholds")
        .arg(&thresholds_path)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: EvaluationReportV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!report.passed);
    assert_eq!(
        report.threshold_failures,
        vec![EvaluationMetric::CostMicrounits]
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("evaluation thresholds failed"));
}

#[test]
fn eval_report_command_bounds_json_before_execution_without_echoing_content() {
    let directory = tempdir().unwrap();
    let measurements_path = directory.path().join("measurements.json");
    std::fs::write(
        &measurements_path,
        serde_json::to_vec(&measurements()).unwrap(),
    )
    .unwrap();

    let malformed_home = directory.path().join("malformed-home");
    let malformed_path = directory.path().join("malformed.json");
    let marker = "private-input-marker";
    std::fs::write(&malformed_path, marker).unwrap();
    let malformed = Command::new(env!("CARGO_BIN_EXE_optimus"))
        .arg("--home")
        .arg(&malformed_home)
        .arg("eval")
        .arg("report")
        .arg("--binding")
        .arg(&malformed_path)
        .arg("--measurements")
        .arg(&measurements_path)
        .output()
        .unwrap();
    assert!(!malformed.status.success());
    assert!(malformed.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&malformed.stderr).contains(marker));
    assert!(!malformed_home.join("evaluation-runs").exists());

    let oversized_home = directory.path().join("oversized-home");
    let oversized_path = directory.path().join("oversized.json");
    std::fs::write(
        &oversized_path,
        vec![b' '; MAX_EVALUATION_DATASET_BYTES + 1],
    )
    .unwrap();
    let oversized = Command::new(env!("CARGO_BIN_EXE_optimus"))
        .arg("--home")
        .arg(&oversized_home)
        .arg("eval")
        .arg("report")
        .arg("--binding")
        .arg(&oversized_path)
        .arg("--measurements")
        .arg(&measurements_path)
        .output()
        .unwrap();
    assert!(!oversized.status.success());
    assert!(String::from_utf8_lossy(&oversized.stderr).contains("size is outside policy"));
    assert!(!oversized_home.join("evaluation-runs").exists());
}

#[test]
fn legacy_eval_run_json_remains_available() {
    let directory = tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_optimus"))
        .arg("--home")
        .arg(directory.path().join("home"))
        .arg("eval")
        .arg("run")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: EvalReport = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report.all_ok());
    assert_eq!(report.passed, 4);
}
