//! Cron scheduling IPC.

use std::path::PathBuf;

use optimus_kernel::{open_cron, tick_cron};
use serde_json::json;
use uuid::Uuid;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "cron_list"
            | "cron_add"
            | "cron_tick"
            | "cron_set_enabled"
            | "cron_remove"
            | "cron_history"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "cron_list" => {
            let store = open_cron(home).map_err(|e| e.to_string())?;
            let rows: Vec<_> = store
                .list()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(cron_job_json)
                .collect();
            Ok(json!({ "jobs": rows }))
        }
        "cron_add" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("job")
                .trim();
            if name.is_empty() {
                return Err("name required".into());
            }
            if name.chars().count() > 120 {
                return Err("name too long (max 120)".into());
            }
            let every = params
                .get("every_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(3600)
                .max(5);
            let prompt = params
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("tick")
                .trim();
            if prompt.is_empty() {
                return Err("prompt required".into());
            }
            if prompt.chars().count() > 8_000 {
                return Err("prompt too long".into());
            }
            let provider = params
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("offline");
            // UI cannot mint lease tokens or bypass claim path — add only.
            let store = open_cron(home).map_err(|e| e.to_string())?;
            let j = store
                .add(name, every, prompt, provider)
                .map_err(|e| e.to_string())?;
            Ok(cron_job_json(j))
        }
        "cron_set_enabled" => {
            let id = parse_id(&params)?;
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| "enabled bool required".to_string())?;
            let store = open_cron(home).map_err(|e| e.to_string())?;
            // Pause clears lease fields in set_enabled — no UI lease mint.
            store
                .set_enabled(id, enabled)
                .map_err(|e| e.to_string())?;
            Ok(json!({ "id": id.to_string(), "enabled": enabled }))
        }
        "cron_remove" => {
            let id = parse_id(&params)?;
            let store = open_cron(home).map_err(|e| e.to_string())?;
            let removed = store.remove(id).map_err(|e| e.to_string())?;
            if !removed {
                return Err("schedule not found".into());
            }
            Ok(json!({ "id": id.to_string(), "removed": true }))
        }
        "cron_history" => {
            let id = parse_id(&params)?;
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20)
                .clamp(1, 100) as usize;
            let store = open_cron(home).map_err(|e| e.to_string())?;
            let rows: Vec<_> = store
                .history(id, limit)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|a| {
                    json!({
                        "attempt_id": a.attempt_id.to_string(),
                        "job_id": a.job_id.to_string(),
                        "status": a.status,
                        "started_unix": a.started_unix,
                        "completed_unix": a.completed_unix,
                        "detail": a.detail,
                    })
                })
                .collect();
            Ok(json!({ "attempts": rows, "job_id": id.to_string() }))
        }
        "cron_tick" => {
            // Tick remains host/operator; does not expose claim APIs to UI.
            let rows = tick_cron(home).map_err(|e| e.to_string())?;
            Ok(json!({ "ran": rows }))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

fn parse_id(params: &serde_json::Value) -> Result<Uuid, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "id required".to_string())?;
    Uuid::parse_str(id).map_err(|e| e.to_string())
}

fn cron_job_json(j: optimus_kernel::CronJob) -> serde_json::Value {
    json!({
        "id": j.id.to_string(),
        "name": j.name,
        "every_secs": j.every_secs,
        "enabled": j.enabled,
        "next_run_unix": j.next_run_unix,
        "last_status": j.last_status,
        "last_run_unix": j.last_run_unix,
        "provider": j.provider,
        "prompt": j.prompt,
        "created_at": j.created_at,
    })
}
