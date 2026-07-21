//! Versioned evaluation datasets, checked integer metrics, and immutable baselines.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use optimus_packs::ToolId;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ExecutionStatus, KernelError, ReplayClassification, Result};

pub const EVALUATION_DATASET_VERSION: u16 = 1;
pub const EVALUATION_REPORT_VERSION: u16 = 1;
pub const MAX_EVALUATION_DATASET_BYTES: usize = 1_048_576;
pub const MAX_EVALUATION_CASES: usize = 1_000;

fn invalid(reason: impl Into<String>) -> KernelError {
    KernelError::Model(format!("invalid evaluation evidence: {}", reason.into()))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_hash(value: &str, field: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{field} must be a SHA-256 hex digest")));
    }
    Ok(())
}

fn validate_id(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(invalid(format!("{field} is not canonical")));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationCaseContract {
    pub id: String,
    pub exact_assistant_text: Option<String>,
    pub expected_tools: Vec<ToolId>,
    pub terminal_status: ExecutionStatus,
    pub replay: ReplayClassification,
    pub trace_required: bool,
    pub provenance_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationDataset {
    pub id: String,
    pub version: u16,
    pub provenance_sha256: String,
    pub cases: Vec<EvaluationCaseContract>,
}

impl EvaluationDataset {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id, "dataset id")?;
        if self.version != EVALUATION_DATASET_VERSION {
            return Err(invalid("unsupported dataset version"));
        }
        validate_hash(&self.provenance_sha256, "dataset provenance")?;
        if self.cases.is_empty() || self.cases.len() > MAX_EVALUATION_CASES {
            return Err(invalid("dataset case count is outside policy"));
        }
        let mut ids = BTreeSet::new();
        for case in &self.cases {
            validate_id(&case.id, "case id")?;
            validate_hash(&case.provenance_sha256, "case provenance")?;
            if !ids.insert(case.id.clone()) {
                return Err(invalid("duplicate evaluation case id"));
            }
            if case
                .expected_tools
                .iter()
                .map(ToolId::as_str)
                .collect::<BTreeSet<_>>()
                .len()
                != case.expected_tools.len()
            {
                return Err(invalid("duplicate expected tool identity"));
            }
            if case
                .exact_assistant_text
                .as_ref()
                .is_some_and(|text| text.len() > 16_384)
            {
                return Err(invalid("expected assistant text exceeds policy"));
            }
        }
        Ok(())
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_EVALUATION_DATASET_BYTES {
            return Err(invalid("dataset JSON size is outside policy"));
        }
        let dataset: Self = serde_json::from_slice(bytes)?;
        dataset.validate()?;
        Ok(dataset)
    }
}

pub fn priority2_dataset() -> EvaluationDataset {
    let trajectory = |id: &str, text: Option<&str>, tools: &[&str]| EvaluationCaseContract {
        id: id.into(),
        exact_assistant_text: text.map(str::to_string),
        expected_tools: tools.iter().map(|value| ToolId::new(*value)).collect(),
        terminal_status: ExecutionStatus::Succeeded,
        replay: ReplayClassification::FixtureReplayable,
        trace_required: true,
        provenance_sha256: digest(format!("priority2:{id}").as_bytes()),
    };
    let integrity = |id: &str| EvaluationCaseContract {
        id: id.into(),
        exact_assistant_text: None,
        expected_tools: Vec::new(),
        terminal_status: ExecutionStatus::Succeeded,
        replay: ReplayClassification::Deterministic,
        trace_required: true,
        provenance_sha256: digest(format!("priority2:{id}").as_bytes()),
    };
    let dataset = EvaluationDataset {
        id: "priority2-integrity".into(),
        version: EVALUATION_DATASET_VERSION,
        provenance_sha256: digest(
            b"docs/specifications/priority-2-replay-observability-evaluation-100-nanotasks.md",
        ),
        cases: vec![
            trajectory("offline-echo", Some("pong"), &[]),
            trajectory(
                "memory-then-answer",
                Some("You prefer helix."),
                &["memory_recall"],
            ),
            trajectory(
                "pack-activate-browser",
                Some("browser pack ready"),
                &["activate_pack"],
            ),
            trajectory(
                "write-file-job",
                Some("wrote notes/eval.txt"),
                &["write_file"],
            ),
            integrity("sensitivity_denial"),
            integrity("smartdeny_approval"),
            integrity("route_policy_denial"),
            integrity("cooperative_cancellation"),
            integrity("stale_completion_fence"),
            integrity("gateway_dead_letter"),
        ],
    };
    dataset
        .validate()
        .expect("built-in Priority-2 dataset is valid");
    dataset
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationMetric {
    ExactText,
    ToolPrecision,
    ToolRecall,
    TerminalAccuracy,
    ReplayAccuracy,
    LatencyMillis,
    CostMicrounits,
}

impl EvaluationMetric {
    const ALL: [Self; 7] = [
        Self::ExactText,
        Self::ToolPrecision,
        Self::ToolRecall,
        Self::TerminalAccuracy,
        Self::ReplayAccuracy,
        Self::LatencyMillis,
        Self::CostMicrounits,
    ];
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    Minimum,
    Maximum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricThreshold {
    pub metric: EvaluationMetric,
    pub direction: MetricDirection,
    pub value: u64,
    pub min_samples: usize,
}

impl MetricThreshold {
    pub fn new(
        metric: EvaluationMetric,
        direction: MetricDirection,
        value: u64,
        min_samples: usize,
    ) -> Result<Self> {
        if min_samples == 0 || min_samples > MAX_EVALUATION_CASES {
            return Err(invalid("threshold sample count is outside policy"));
        }
        if matches!(
            metric,
            EvaluationMetric::ExactText
                | EvaluationMetric::ToolPrecision
                | EvaluationMetric::ToolRecall
                | EvaluationMetric::TerminalAccuracy
                | EvaluationMetric::ReplayAccuracy
        ) && value > 10_000
        {
            return Err(invalid("accuracy threshold exceeds 10000 basis points"));
        }
        Ok(Self {
            metric,
            direction,
            value,
            min_samples,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationObservation {
    pub case_id: String,
    pub exact_text: bool,
    pub expected_tools: usize,
    pub observed_tools: usize,
    pub matched_tools: usize,
    pub terminal_correct: bool,
    pub replay_correct: bool,
    pub trace_present: bool,
    pub latency_millis: u64,
    pub cost_microunits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateBinding {
    pub source_tree_sha256: String,
    pub contract_sha256: String,
    pub tool_catalog_sha256: String,
    pub route_policy_sha256: String,
    pub provider: String,
    pub model: String,
}

impl CandidateBinding {
    fn validate(&self) -> Result<()> {
        for (value, field) in [
            (&self.source_tree_sha256, "source tree"),
            (&self.contract_sha256, "contract"),
            (&self.tool_catalog_sha256, "tool catalog"),
            (&self.route_policy_sha256, "route policy"),
        ] {
            validate_hash(value, field)?;
        }
        if self.provider.is_empty() || self.model.is_empty() {
            return Err(invalid("candidate provider/model identity is empty"));
        }
        Ok(())
    }

    fn same_evaluation_context(&self, other: &Self) -> bool {
        self.contract_sha256 == other.contract_sha256
            && self.tool_catalog_sha256 == other.tool_catalog_sha256
            && self.route_policy_sha256 == other.route_policy_sha256
            && self.provider == other.provider
            && self.model == other.model
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricScore {
    pub metric: EvaluationMetric,
    pub numerator: u64,
    pub denominator: u64,
    pub samples: usize,
    pub value: u64,
}

fn evaluate_thresholds(
    metrics: &BTreeMap<EvaluationMetric, MetricScore>,
    thresholds: &[MetricThreshold],
) -> Result<Vec<EvaluationMetric>> {
    let mut dimensions = BTreeSet::new();
    let mut failures = Vec::new();
    for threshold in thresholds {
        let checked = MetricThreshold::new(
            threshold.metric,
            threshold.direction,
            threshold.value,
            threshold.min_samples,
        )?;
        if !dimensions.insert(checked.metric) {
            return Err(invalid("duplicate evaluation threshold metric"));
        }
        let score = metrics
            .get(&checked.metric)
            .ok_or_else(|| invalid("evaluation threshold metric is missing"))?;
        let failed = score.samples < checked.min_samples
            || match checked.direction {
                MetricDirection::Minimum => score.value < checked.value,
                MetricDirection::Maximum => score.value > checked.value,
            };
        if failed {
            failures.push(checked.metric);
        }
    }
    failures.sort();
    Ok(failures)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationReportV1 {
    pub version: u16,
    pub dataset_id: String,
    pub dataset_version: u16,
    pub dataset_sha256: String,
    pub binding: CandidateBinding,
    pub metrics: BTreeMap<EvaluationMetric, MetricScore>,
    pub thresholds: Vec<MetricThreshold>,
    pub threshold_failures: Vec<EvaluationMetric>,
    pub passed: bool,
    pub report_sha256: String,
}

pub fn build_evaluation_report(
    dataset: &EvaluationDataset,
    binding: CandidateBinding,
    observations: &[EvaluationObservation],
    thresholds: &[MetricThreshold],
) -> Result<EvaluationReportV1> {
    dataset.validate()?;
    binding.validate()?;
    if observations.len() != dataset.cases.len() {
        return Err(invalid("observation count does not match dataset"));
    }
    let expected = dataset
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for observation in observations {
        let case = expected
            .get(observation.case_id.as_str())
            .ok_or_else(|| invalid("observation case is not in dataset"))?;
        if !seen.insert(observation.case_id.as_str())
            || observation.expected_tools != case.expected_tools.len()
            || observation.matched_tools > observation.expected_tools
            || observation.matched_tools > observation.observed_tools
        {
            return Err(invalid("observation identity/tool counts are inconsistent"));
        }
        if case.trace_required && !observation.trace_present {
            return Err(invalid("required trace evidence is missing"));
        }
    }
    let samples = observations.len();
    let ratio = |metric, numerator: usize, denominator: usize| MetricScore {
        metric,
        numerator: numerator as u64,
        denominator: denominator as u64,
        samples,
        value: if denominator == 0 {
            10_000
        } else {
            (numerator as u64 * 10_000) / denominator as u64
        },
    };
    let exact = observations.iter().filter(|value| value.exact_text).count();
    let terminal = observations
        .iter()
        .filter(|value| value.terminal_correct)
        .count();
    let replay = observations
        .iter()
        .filter(|value| value.replay_correct)
        .count();
    let matched = observations.iter().try_fold(0usize, |total, value| {
        total
            .checked_add(value.matched_tools)
            .ok_or_else(|| invalid("matched tool count overflow"))
    })?;
    let expected_tools = observations.iter().try_fold(0usize, |total, value| {
        total
            .checked_add(value.expected_tools)
            .ok_or_else(|| invalid("expected tool count overflow"))
    })?;
    let observed_tools = observations.iter().try_fold(0usize, |total, value| {
        total
            .checked_add(value.observed_tools)
            .ok_or_else(|| invalid("observed tool count overflow"))
    })?;
    let latency = observations.iter().try_fold(0u64, |total, value| {
        total
            .checked_add(value.latency_millis)
            .ok_or_else(|| invalid("latency overflow"))
    })?;
    let cost = observations.iter().try_fold(0u64, |total, value| {
        total
            .checked_add(value.cost_microunits)
            .ok_or_else(|| invalid("cost overflow"))
    })?;
    let mut metrics = BTreeMap::new();
    for score in [
        ratio(EvaluationMetric::ExactText, exact, samples),
        ratio(EvaluationMetric::ToolPrecision, matched, observed_tools),
        ratio(EvaluationMetric::ToolRecall, matched, expected_tools),
        ratio(EvaluationMetric::TerminalAccuracy, terminal, samples),
        ratio(EvaluationMetric::ReplayAccuracy, replay, samples),
        MetricScore {
            metric: EvaluationMetric::LatencyMillis,
            numerator: latency,
            denominator: samples as u64,
            samples,
            value: latency / samples as u64,
        },
        MetricScore {
            metric: EvaluationMetric::CostMicrounits,
            numerator: cost,
            denominator: samples as u64,
            samples,
            value: cost / samples as u64,
        },
    ] {
        metrics.insert(score.metric, score);
    }
    let threshold_failures = evaluate_thresholds(&metrics, thresholds)?;
    let mut report = EvaluationReportV1 {
        version: EVALUATION_REPORT_VERSION,
        dataset_id: dataset.id.clone(),
        dataset_version: dataset.version,
        dataset_sha256: digest(&serde_json::to_vec(dataset)?),
        binding,
        metrics,
        thresholds: thresholds.to_vec(),
        passed: threshold_failures.is_empty(),
        threshold_failures,
        report_sha256: String::new(),
    };
    report.report_sha256 = digest(&serde_json::to_vec(&report)?);
    verify_report(&report)?;
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationComparison {
    pub baseline_sha256: String,
    pub candidate_sha256: String,
    pub improved: Vec<EvaluationMetric>,
    pub equal: Vec<EvaluationMetric>,
    pub regressed: Vec<EvaluationMetric>,
}

pub fn compare_evaluation_reports(
    baseline: &EvaluationReportV1,
    candidate: &EvaluationReportV1,
) -> Result<EvaluationComparison> {
    verify_report(baseline)?;
    verify_report(candidate)?;
    if baseline.dataset_id != candidate.dataset_id
        || baseline.dataset_version != candidate.dataset_version
        || baseline.dataset_sha256 != candidate.dataset_sha256
        || !baseline.binding.same_evaluation_context(&candidate.binding)
    {
        return Err(invalid("baseline and candidate bindings differ"));
    }
    if baseline.thresholds != candidate.thresholds {
        return Err(invalid("baseline and candidate threshold policies differ"));
    }
    if baseline.metrics.keys().ne(candidate.metrics.keys()) {
        return Err(invalid("baseline and candidate metric sets differ"));
    }
    let mut comparison = EvaluationComparison {
        baseline_sha256: baseline.report_sha256.clone(),
        candidate_sha256: candidate.report_sha256.clone(),
        improved: Vec::new(),
        equal: Vec::new(),
        regressed: Vec::new(),
    };
    for (metric, before) in &baseline.metrics {
        let after = candidate
            .metrics
            .get(metric)
            .ok_or_else(|| invalid("candidate metric set differs"))?;
        let ordering = if matches!(
            metric,
            EvaluationMetric::LatencyMillis | EvaluationMetric::CostMicrounits
        ) {
            before.value.cmp(&after.value)
        } else {
            after.value.cmp(&before.value)
        };
        match ordering {
            std::cmp::Ordering::Greater => comparison.improved.push(*metric),
            std::cmp::Ordering::Equal => comparison.equal.push(*metric),
            std::cmp::Ordering::Less => comparison.regressed.push(*metric),
        }
    }
    Ok(comparison)
}

pub struct BaselineStore {
    conn: Connection,
}

impl BaselineStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS evaluation_baselines(
               report_sha256 TEXT PRIMARY KEY,dataset_id TEXT NOT NULL,dataset_version INTEGER NOT NULL,
               report_json TEXT NOT NULL
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn accept(&self, report: &EvaluationReportV1) -> Result<()> {
        verify_report(report)?;
        self.conn.execute(
            "INSERT INTO evaluation_baselines(report_sha256,dataset_id,dataset_version,report_json)
             VALUES(?1,?2,?3,?4)",
            params![
                report.report_sha256,
                report.dataset_id,
                report.dataset_version as i64,
                serde_json::to_string(report)?
            ],
        )?;
        Ok(())
    }

    pub fn report(&self, hash: &str) -> Result<EvaluationReportV1> {
        validate_hash(hash, "report")?;
        let text = self
            .conn
            .query_row(
                "SELECT report_json FROM evaluation_baselines WHERE report_sha256=?1",
                params![hash],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| invalid("baseline report does not exist"))?;
        let report: EvaluationReportV1 = serde_json::from_str(&text)?;
        verify_report(&report)?;
        Ok(report)
    }
}

fn verify_report(report: &EvaluationReportV1) -> Result<()> {
    if report.version != EVALUATION_REPORT_VERSION {
        return Err(invalid("unsupported evaluation report version"));
    }
    validate_id(&report.dataset_id, "report dataset id")?;
    if report.dataset_version != EVALUATION_DATASET_VERSION {
        return Err(invalid("unsupported evaluation dataset version"));
    }
    validate_hash(&report.dataset_sha256, "report dataset")?;
    report.binding.validate()?;
    let expected_metrics = EvaluationMetric::ALL.into_iter().collect::<BTreeSet<_>>();
    let actual_metrics = report.metrics.keys().copied().collect::<BTreeSet<_>>();
    if actual_metrics != expected_metrics {
        return Err(invalid("evaluation report metric set is incomplete"));
    }
    let samples = report
        .metrics
        .values()
        .next()
        .map(|score| score.samples)
        .ok_or_else(|| invalid("evaluation report has no metrics"))?;
    if samples == 0 || samples > MAX_EVALUATION_CASES {
        return Err(invalid("evaluation report sample count is outside policy"));
    }
    for (metric, score) in &report.metrics {
        if score.metric != *metric || score.samples != samples {
            return Err(invalid("evaluation metric identity or samples differ"));
        }
        let expected_value = if matches!(
            metric,
            EvaluationMetric::LatencyMillis | EvaluationMetric::CostMicrounits
        ) {
            if score.denominator != samples as u64 {
                return Err(invalid("evaluation mean denominator differs from samples"));
            }
            score.numerator / score.denominator
        } else {
            if score.numerator > score.denominator {
                return Err(invalid("evaluation ratio numerator exceeds denominator"));
            }
            if matches!(
                metric,
                EvaluationMetric::ExactText
                    | EvaluationMetric::TerminalAccuracy
                    | EvaluationMetric::ReplayAccuracy
            ) && score.denominator != samples as u64
            {
                return Err(invalid(
                    "evaluation accuracy denominator differs from samples",
                ));
            }
            match score.denominator {
                0 => 10_000,
                denominator => score
                    .numerator
                    .checked_mul(10_000)
                    .and_then(|numerator| numerator.checked_div(denominator))
                    .ok_or_else(|| invalid("evaluation ratio arithmetic failed"))?,
            }
        };
        if score.value != expected_value {
            return Err(invalid("evaluation metric value is inconsistent"));
        }
    }
    let expected_failures = evaluate_thresholds(&report.metrics, &report.thresholds)?;
    if report.threshold_failures != expected_failures {
        return Err(invalid("evaluation threshold failures are inconsistent"));
    }
    if report.passed != expected_failures.is_empty() {
        return Err(invalid("evaluation report passed state is inconsistent"));
    }
    let mut unhashed = report.clone();
    unhashed.report_sha256.clear();
    if digest(&serde_json::to_vec(&unhashed)?) != report.report_sha256 {
        return Err(invalid("evaluation report hash mismatch"));
    }
    Ok(())
}
