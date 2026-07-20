//! Approval, job, campaign, and bounded terminal IPC.

use std::path::{Path, PathBuf};

use optimus_graph::{Effect, JobSpec, NodeSpec};
use optimus_runtime::{CampaignStepSpec, CampaignStore, StepKind};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "approvals_list"
            | "approvals_grant"
            | "jobs_list"
            | "campaign_list"
            | "campaign_create"
            | "campaign_run"
            | "campaign_status"
            | "term_run"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "approvals_list" => {
            let rt = open_runtime(home)?;
            let rows: Vec<_> = rt
                .list_pending_approvals()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|p| {
                    json!({
                        "job_id": p.job_id.to_string(),
                        "job_label": p.job_label,
                        "job_status": format!("{:?}", p.job_status),
                        "node_label": p.node_label,
                        "node_index": p.node_index,
                        "has_grant": p.has_grant,
                        "effect_json": p.effect_json,
                    })
                })
                .collect();
            Ok(json!({ "pending": rows }))
        }
        "approvals_grant" => {
            let id = params
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "job_id required".to_string())?;
            let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
            let rt = open_runtime(home)?;
            let status = rt
                .grant_and_resume(optimus_runtime::job_id(id))
                .map_err(|e| e.to_string())?;
            let capture = rt
                .latest_command_capture(optimus_runtime::job_id(id))
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "job_id": id.to_string(),
                "status": format!("{status:?}"),
                "stdout": capture.as_ref().map(|value| value.stdout.as_str()).unwrap_or(""),
                "stderr": capture.as_ref().map(|value| value.stderr.as_str()).unwrap_or(""),
                "exit_code": capture.as_ref().and_then(|value| value.exit_code),
                "timed_out": capture.as_ref().map(|value| value.timed_out).unwrap_or(false),
                "truncated_stdout": capture.as_ref().map(|value| value.truncated_stdout).unwrap_or(false),
                "truncated_stderr": capture.as_ref().map(|value| value.truncated_stderr).unwrap_or(false),
            }))
        }
        "jobs_list" => {
            let rt = open_runtime(home)?;
            let rows: Vec<_> = rt
                .list_jobs_summary()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|j| {
                    json!({
                        "job_id": j.job_id.to_string(),
                        "label": j.label,
                        "status": format!("{:?}", j.status),
                        "steps_executed": j.steps_executed,
                        "max_steps": j.max_steps,
                    })
                })
                .collect();
            Ok(json!({ "jobs": rows }))
        }
        "campaign_list" => {
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let rows: Vec<_> = store
                .list()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|c| {
                    json!({
                        "id": c.id.to_string(),
                        "name": c.name,
                        "status": format!("{:?}", c.status),
                        "created_unix": c.created_unix,
                    })
                })
                .collect();
            Ok(json!({ "campaigns": rows }))
        }
        "campaign_create" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("campaign");
            let mut steps = Vec::new();
            if let Some(arr) = params.get("writes").and_then(|v| v.as_array()) {
                for w in arr {
                    let path = w.get("path").and_then(|v| v.as_str()).unwrap_or("out.txt");
                    let contents = w.get("contents").and_then(|v| v.as_str()).unwrap_or("");
                    steps.push(CampaignStepSpec {
                        label: path.to_string(),
                        kind: StepKind::WriteFile {
                            relative_path: path.into(),
                            contents: contents.into(),
                        },
                    });
                }
            }
            if steps.is_empty() {
                return Err("writes required".into());
            }
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let view = store.create(name, steps).map_err(|e| e.to_string())?;
            Ok(json!({
                "id": view.campaign.id.to_string(),
                "steps": view.steps.len(),
                "status": format!("{:?}", view.campaign.status),
            }))
        }
        "campaign_run" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id required".to_string())?;
            let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let view = store.run(id).map_err(|e| e.to_string())?;
            Ok(json!({
                "id": view.campaign.id.to_string(),
                "status": format!("{:?}", view.campaign.status),
                "steps": view.steps.iter().map(|s| json!({
                    "idx": s.idx,
                    "label": s.label,
                    "status": format!("{:?}", s.status),
                    "detail": s.detail,
                })).collect::<Vec<_>>(),
            }))
        }
        "campaign_status" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id required".to_string())?;
            let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let view = store
                .get(id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("campaign {id} not found"))?;
            Ok(json!({
                "id": view.campaign.id.to_string(),
                "name": view.campaign.name,
                "status": format!("{:?}", view.campaign.status),
                "steps": view.steps.iter().map(|s| json!({
                    "idx": s.idx,
                    "label": s.label,
                    "status": format!("{:?}", s.status),
                    "job_id": s.job_id.map(|j| j.to_string()),
                    "detail": s.detail,
                })).collect::<Vec<_>>(),
            }))
        }
        "term_run" => term_run(home, params),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Phase A terminal: one-shot `cmd /C` via Work Graph command capture (not interactive PTY).
fn term_line_denied(line: &str) -> Option<&'static str> {
    let l = line.to_ascii_lowercase();
    const BAD: &[&str] = &[
        "format ",
        "format\t",
        "del /s",
        "rd /s",
        "rmdir /s",
        "rm -rf",
        "rm -r ",
        "shutdown",
        "reg delete",
        "powershell -enc",
        "msiexec",
        "start /b",
        "curl ",
        "wget ",
        "invoke-webrequest",
        "invoke-restmethod",
        "certutil",
        "bitsadmin",
        "net user",
        "net localgroup",
    ];
    for b in BAD {
        if l.contains(b) {
            return Some("command blocked by term allowlist policy");
        }
    }
    if line.len() > 2_000 {
        return Some("command too long");
    }
    None
}

fn term_run(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let line = params
        .get("line")
        .or_else(|| params.get("cmd"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "line required".to_string())?;
    if let Some(reason) = term_line_denied(line) {
        return Err(reason.into());
    }
    let rt = open_runtime(home)?;
    let job = rt
        .create_job(JobSpec {
            label: format!("term:{}", line.chars().take(48).collect::<String>()),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "term_run".into(),
                effect: Effect::RunCommand {
                    program: "cmd".into(),
                    args: vec!["/C".into(), line.into()],
                },
            }],
        })
        .map_err(|e| e.to_string())?;
    let status = match rt.run_all(job) {
        Ok(s) => s,
        Err(optimus_runtime::RuntimeError::CommandFailed { capture, .. }) => {
            return Ok(json!({
                "job_id": job.0.to_string(),
                "status": "Failed",
                "stdout": capture.stdout,
                "stderr": capture.stderr,
                "exit_code": capture.exit_code,
                "timed_out": capture.timed_out,
                "truncated_stdout": capture.truncated_stdout,
                "truncated_stderr": capture.truncated_stderr,
                "line": line,
                "pty": false,
                "mode": "job-stream",
            }));
        }
        Err(e) => return Err(e.to_string()),
    };
    let cap = rt
        .latest_command_capture(job)
        .map_err(|e| e.to_string())?
        .unwrap_or(optimus_runtime::CommandCapture {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            truncated_stdout: false,
            truncated_stderr: false,
            timed_out: false,
        });
    Ok(json!({
        "job_id": job.0.to_string(),
        "status": format!("{status:?}"),
        "stdout": cap.stdout,
        "stderr": cap.stderr,
        "exit_code": cap.exit_code,
        "timed_out": cap.timed_out,
        "truncated_stdout": cap.truncated_stdout,
        "truncated_stderr": cap.truncated_stderr,
        "line": line,
        "pty": false,
        "mode": "job-stream",
    }))
}

pub(super) fn open_runtime(home: &std::path::Path) -> Result<optimus_runtime::Runtime, String> {
    let db = home.join("optimus.db");
    let ws = home.join("workspace");
    std::fs::create_dir_all(&ws).map_err(|e| e.to_string())?;
    optimus_runtime::Runtime::open(&db, &ws).map_err(|e| e.to_string())
}
