//! Subcommands answered without opening Optimus state.
//!
//! Split out of `main.rs`, the largest entry in
//! `docs/architecture/module-size-baseline.json`, which may only shrink.

use optimus_eval::{compare_evaluation_reports, EvaluationReportV1};
use serde_json::json;

use crate::{read_bounded_json, Cli, Commands, EvalCmd, OPTIMUS_VERSION_MANIFEST};

pub fn run_read_only_eval(cli: &Cli) -> Option<Result<(), Box<dyn std::error::Error>>> {
    let Some(Commands::Eval {
        cmd: EvalCmd::Compare {
            baseline,
            candidate,
        },
    }) = &cli.command
    else {
        return None;
    };
    Some((|| {
        let baseline: EvaluationReportV1 = read_bounded_json(baseline, "baseline report")?;
        let candidate: EvaluationReportV1 = read_bounded_json(candidate, "candidate report")?;
        let comparison = compare_evaluation_reports(&baseline, &candidate)?;
        println!("{}", serde_json::to_string_pretty(&comparison)?);
        Ok(())
    })())
}

pub fn embedded_version_status() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let manifest: serde_json::Value = serde_json::from_str(OPTIMUS_VERSION_MANIFEST)?;
    let target = manifest
        .pointer("/hermes_target/version")
        .and_then(serde_json::Value::as_str)
        .ok_or("embedded version manifest is missing hermes_target.version")?;
    let claim_status = manifest
        .pointer("/parity_claim/status")
        .and_then(serde_json::Value::as_str)
        .ok_or("embedded version manifest is missing parity_claim.status")?;
    let parity_version = if claim_status == "verified" {
        manifest
            .pointer("/parity_claim/hermes_version")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    let feature_contracts = manifest
        .pointer("/baseline/feature_count")
        .and_then(serde_json::Value::as_u64)
        .ok_or("embedded version manifest is missing baseline.feature_count")?;
    Ok(json!({
        "product": "Optimus Agent",
        "product_version": env!("CARGO_PKG_VERSION"),
        "hermes_target_version": target,
        "hermes_parity_version": parity_version,
        "parity_claim_status": claim_status,
        "frozen_hermes_feature_contracts": feature_contracts,
    }))
}

pub fn run_read_only_version(cli: &Cli) -> Option<Result<(), Box<dyn std::error::Error>>> {
    let Some(Commands::Version { json: as_json }) = &cli.command else {
        return None;
    };
    Some((|| {
        let status = embedded_version_status()?;
        if *as_json {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            println!(
                "Optimus Agent {}",
                status["product_version"].as_str().unwrap_or("unknown")
            );
            println!(
                "Hermes target: {}",
                status["hermes_target_version"]
                    .as_str()
                    .unwrap_or("unknown")
            );
            println!(
                "Hermes parity: {}",
                status["hermes_parity_version"]
                    .as_str()
                    .unwrap_or("unverified")
            );
            println!(
                "Frozen Hermes feature contracts: {}",
                status["frozen_hermes_feature_contracts"]
                    .as_u64()
                    .unwrap_or(0)
            );
        }
        Ok(())
    })())
}
