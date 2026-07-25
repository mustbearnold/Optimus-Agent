//! Hermes-vs-Optimus comparative runner scaffold (Track Z.1).
//!
//! Runs a single offline Optimus scenario and records a comparison shell.
//! Does **not** claim Hermes gate PASS or fill performance evidence.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{run_offline_trajectory_suite, EvalReport};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparativeScenarioResult {
    pub scenario_id: String,
    pub optimus_ok: bool,
    pub optimus_cases: usize,
    pub optimus_passed: usize,
    pub hermes_status: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparativeReport {
    pub schema_version: u32,
    pub scenarios: Vec<ComparativeScenarioResult>,
    pub hermes_gate: String,
}

/// Run one comparative scenario: Optimus offline trajectory suite vs Hermes "not run".
pub fn run_comparative_offline_scenario(home: impl AsRef<Path>) -> ComparativeReport {
    let report: EvalReport = run_offline_trajectory_suite(home);
    let total = report.cases.len();
    let passed = report.passed;
    ComparativeReport {
        schema_version: 1,
        scenarios: vec![ComparativeScenarioResult {
            scenario_id: "offline-trajectory-suite".into(),
            optimus_ok: report.failed == 0 && total > 0,
            optimus_cases: total,
            optimus_passed: passed,
            hermes_status: "not_run".into(),
            note: "Comparative scaffold only — Hermes binary not invoked; gate remains unverified."
                .into(),
        }],
        hermes_gate: "unverified".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn comparative_offline_does_not_claim_hermes_pass() {
        let dir = tempdir().unwrap();
        let report = run_comparative_offline_scenario(dir.path());
        assert_eq!(report.hermes_gate, "unverified");
        assert_eq!(report.scenarios.len(), 1);
        assert_eq!(report.scenarios[0].hermes_status, "not_run");
        assert!(report.scenarios[0].optimus_cases > 0);
    }
}
