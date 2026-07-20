//! Diagnostics and Codex authentication IPC.

use std::path::PathBuf;

use optimus_kernel::{open_cron, CodexAuthStore};
use optimus_packs::CapabilitySession;
use optimus_runtime::{CampaignStatus, CampaignStore};
use serde_json::json;

use super::runtime_ops::open_runtime;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "ping" | "doctor" | "auth_status" | "auth_import_hermes" | "auth_import_cli"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    _params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "ping" => Ok(json!({ "pong": true, "home": home.display().to_string() })),
        "doctor" => Ok(doctor_json(home)),
        "auth_status" => Ok(auth_status_json(home)),
        "auth_import_hermes" => {
            let store = CodexAuthStore::open(home).map_err(|e| e.to_string())?;
            let msg = store.import_from_hermes().map_err(|e| e.to_string())?;
            Ok(json!({ "message": msg, "auth": auth_status_json(home) }))
        }
        "auth_import_cli" => {
            let store = CodexAuthStore::open(home).map_err(|e| e.to_string())?;
            let msg = store.import_from_codex_cli().map_err(|e| e.to_string())?;
            Ok(json!({ "message": msg, "auth": auth_status_json(home) }))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

pub fn doctor_json(home: &PathBuf) -> serde_json::Value {
    let s = CapabilitySession::with_defaults();
    let pack_catalog: Vec<_> = s.catalog().values().cloned().collect();
    let auth = auth_status_json(home);
    let cron_jobs = open_cron(home)
        .ok()
        .and_then(|store| store.list().ok())
        .map(|jobs| jobs.len())
        .unwrap_or(0);
    let campaigns_active = CampaignStore::open(home)
        .ok()
        .and_then(|store| store.list().ok())
        .map(|list| {
            list.into_iter()
                .filter(|c| {
                    matches!(
                        c.status,
                        CampaignStatus::Running
                            | CampaignStatus::Pending
                            | CampaignStatus::AwaitingApproval
                    )
                })
                .count()
        })
        .unwrap_or(0);
    let approvals_pending = open_runtime(home)
        .ok()
        .and_then(|rt| rt.list_pending_approvals().ok())
        .map(|rows| rows.len())
        .unwrap_or(0);
    json!({
        "phase": "desktop-5-sidebars-preview",
        "home": home.display().to_string(),
        "core_schema_tokens": s.schema_tokens(),
        "max_budget": 2500,
        "max_on_demand": 2,
        "pack_catalog": pack_catalog,
        "version": env!("CARGO_PKG_VERSION"),
        "codex_present": auth.get("present").and_then(|v| v.as_bool()).unwrap_or(false),
        "streaming": true,
        "browser": "http-ssrf-safe",
        "cron": true,
        "approvals": true,
        "campaigns": true,
        "gateway": true,
        "files": true,
        "pty": false,
        "preview_browser": false,
        "cron_jobs": cron_jobs,
        "campaigns_active": campaigns_active,
        "approvals_pending": approvals_pending,
    })
}

pub fn auth_status_json(home: &PathBuf) -> serde_json::Value {
    match CodexAuthStore::open(home) {
        Ok(store) => match store.status() {
            Ok(s) => json!({
                "present": s.present,
                "access_expiring": s.access_expiring,
                "has_refresh": s.has_refresh,
                "mode": s.source_note,
                "base_url": s.base_url,
                "account_id": s.account_id,
            }),
            Err(e) => json!({ "present": false, "error": e.to_string() }),
        },
        Err(e) => json!({ "present": false, "error": e.to_string() }),
    }
}
