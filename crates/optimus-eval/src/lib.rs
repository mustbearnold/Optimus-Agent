//! Offline evaluation and zero-effect replay for Optimus.
//!
//! This crate depends on `optimus-kernel` for the turn loop and execution
//! evidence types. The kernel does **not** depend on this crate (no cycle).
//! Operator CLI and evaluation tests should import from `optimus_eval`.

mod eval;
mod evaluation;
mod replay;

pub use eval::{
    builtin_suite, evaluate_integrity_observations, run_case, run_offline_integrity_suite,
    run_offline_trajectory_suite, run_suite, EvalCase, EvalCaseResult, EvalReport,
    IntegrityObservation, REQUIRED_INTEGRITY_EVALS,
};
pub use evaluation::{
    build_evaluation_report, compare_evaluation_reports, priority2_dataset,
    priority2_offline_candidate_binding, project_evaluation_observations,
    run_priority2_offline_evaluation, BaselineStore, CandidateBinding, EvaluationCaseContract,
    EvaluationComparison, EvaluationDataset, EvaluationMetric, EvaluationObservation,
    EvaluationReportV1, EvaluationResourceMeasurement, MetricDirection, MetricScore,
    MetricThreshold, EVALUATION_DATASET_VERSION, EVALUATION_REPORT_VERSION, MAX_EVALUATION_CASES,
    MAX_EVALUATION_DATASET_BYTES,
};
pub use replay::{
    FixtureId, FixtureKind, ReplayBundle, ReplayBundleId, ReplayExecutionReport,
    ReplayExecutionStatus, ReplayFixture, ReplayPlan, ReplayStage, ReplayStore,
    MAX_REPLAY_BUNDLE_BYTES, MAX_REPLAY_FIXTURES, MAX_REPLAY_FIXTURE_BYTES, REPLAY_BUNDLE_VERSION,
    REPLAY_REPORT_VERSION,
};
