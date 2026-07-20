//! Cron scheduling IPC.

use std::path::PathBuf;

use optimus_kernel::{open_cron, tick_cron};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(method, "cron_list" | "cron_add" | "cron_tick")
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
                .map(|j| {
                    json!({
                        "id": j.id.to_string(),
                        "name": j.name,
                        "every_secs": j.every_secs,
                        "enabled": j.enabled,
                        "next_run_unix": j.next_run_unix,
                        "last_status": j.last_status,
                        "provider": j.provider,
                        "prompt": j.prompt,
                    })
                })
                .collect();
            Ok(json!({ "jobs": rows }))
        }
        "cron_add" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("job");
            let every = params
                .get("every_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(3600);
            let prompt = params
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("tick");
            let provider = params
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("offline");
            let store = open_cron(home).map_err(|e| e.to_string())?;
            let j = store
                .add(name, every, prompt, provider)
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "id": j.id.to_string(),
                "next_run_unix": j.next_run_unix,
                "name": j.name,
            }))
        }
        "cron_tick" => {
            let rows = tick_cron(home).map_err(|e| e.to_string())?;
            Ok(json!({ "ran": rows }))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}
