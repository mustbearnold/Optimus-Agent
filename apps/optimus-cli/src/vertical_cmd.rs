//! `optimus vertical` — built-in multi-agent verticals (P10 DAG + specialists).
//!
//! Extracted from `apps/optimus-cli/src/main.rs` (spec-015 A1) to keep the
//! baselined main dispatch within its module-size ratchet while `optimus
//! serve` gains its subcommand arm.

use std::path::Path;

use optimus_kernel::{
    open_seeded_agent_registry, open_seeded_workflow_registry, run_read_file_handoff,
    run_write_file_handoff, run_write_then_read_handoff, ReadFileHandoffRequest,
    WriteFileHandoffRequest,
};

use crate::parsers;
use crate::VerticalCmd;

pub fn run(home: &Path, cmd: VerticalCmd) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        VerticalCmd::List => {
            let agents = open_seeded_agent_registry(home.join("agent-registry.db"))?;
            let workflows = open_seeded_workflow_registry(home.join("workflow-registry.db"))?;
            println!("agents:");
            for agent in agents.list()? {
                println!(
                    "  {}@{} — {}",
                    agent.id.as_str(),
                    agent.version.as_str(),
                    agent.responsibility
                );
            }
            println!("workflows:");
            for workflow in workflows.list()? {
                println!(
                    "  {}@{} — {}",
                    workflow.id.as_str(),
                    workflow.version.as_str(),
                    workflow.description
                );
            }
        }
        VerticalCmd::WriteFile {
            path,
            contents,
            auto_grant,
            policy,
            json,
        } => {
            let policy = parsers::parse_policy_mode(&policy)?;
            let report = run_write_file_handoff(
                home,
                WriteFileHandoffRequest {
                    relative_path: path,
                    contents,
                    auto_grant,
                    policy,
                },
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "workflow={}@{} terminal={:?} agent={}@{} invocation={}",
                    report.workflow_id,
                    report.workflow_version,
                    report.workflow_terminal,
                    report.agent_id,
                    report.agent_version,
                    report.invocation_id
                );
                if let Some(run_id) = report.run_id {
                    println!("run={run_id}");
                }
                if let Some(job) = report.job_id {
                    println!("job={job}");
                }
                println!("summary={}", report.agent_result.summary);
                if let Some(artifact) = report.artifact {
                    println!("artifact={} bytes={}", artifact.sha256, artifact.size_bytes);
                }
            }
        }
        VerticalCmd::ReadFile { path, json } => {
            let report = run_read_file_handoff(
                home,
                ReadFileHandoffRequest {
                    relative_path: path,
                },
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "workflow={}@{} status={:?} run={}",
                    report.workflow_id, report.workflow_version, report.status, report.run_id
                );
                println!("summary={}", report.summary);
                for artifact in report.artifacts {
                    println!("artifact={} bytes={}", artifact.sha256, artifact.size_bytes);
                }
            }
        }
        VerticalCmd::WriteThenRead {
            path,
            contents,
            auto_grant,
            policy,
            json,
        } => {
            let policy = parsers::parse_policy_mode(&policy)?;
            let report = run_write_then_read_handoff(
                home,
                WriteFileHandoffRequest {
                    relative_path: path,
                    contents,
                    auto_grant,
                    policy,
                },
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "workflow={}@{} status={:?} run={} nodes={} children={}",
                    report.workflow_id,
                    report.workflow_version,
                    report.status,
                    report.run_id,
                    report.nodes.len(),
                    report.children.len()
                );
                for node in &report.nodes {
                    println!(
                        "  node={} status={} artifact={:?}",
                        node.node_id,
                        node.status.as_str(),
                        node.artifact_sha256
                    );
                }
                println!("summary={}", report.summary);
            }
        }
    }
    Ok(())
}
