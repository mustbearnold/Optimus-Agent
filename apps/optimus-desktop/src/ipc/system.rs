//! Diagnostics, product settings, and Codex authentication IPC.

use std::path::PathBuf;

use optimus_kernel::{open_cron, CodexAuthStore, ProductSettings};
use optimus_packs::CapabilitySession;
use optimus_runtime::{CampaignStatus, CampaignStore};
use serde_json::json;

use super::runtime_ops::open_runtime;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "ping"
            | "doctor"
            | "auth_status"
            | "auth_import_hermes"
            | "auth_import_cli"
            | "settings_get"
            | "settings_set"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "ping" => Ok(json!({ "pong": true, "home": home.display().to_string() })),
        "doctor" => Ok(doctor_json(home)),
        "auth_status" => Ok(auth_status_json(home)),
        "settings_get" => settings_get(home),
        "settings_set" => settings_set(home, params),
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

fn settings_get(home: &PathBuf) -> Result<serde_json::Value, String> {
    let settings = ProductSettings::load(home).map_err(|e| e.to_string())?;
    Ok(json!({ "settings": settings.to_public_json() }))
}

fn settings_set(home: &PathBuf, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut settings = ProductSettings::load(home).map_err(|e| e.to_string())?;
    settings.apply_patch(&params).map_err(|e| e.to_string())?;
    settings.save(home).map_err(|e| e.to_string())?;
    Ok(json!({ "settings": settings.to_public_json() }))
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
    let browser_kind = if optimus_kernel::chrome_binary_path().is_some() {
        "cdp"
    } else {
        "http-ssrf-safe"
    };
    let preview_browser = browser_kind == "cdp";
    let product_settings = ProductSettings::load(home)
        .map(|s| s.to_public_json())
        .unwrap_or_else(|e| {
            // Fail closed on product-FS isolation claims when settings cannot load.
            json!({
                "work_isolation": "shared",
                "configured_mode": "shared",
                "work_isolation_label": "Shared workbench",
                "allow_concurrent_projects": false,
                "enforcement_active": false,
                "product_fs_enforced": false,
                "command_envelope_enforced": true,
                "enforced_mode": "shared",
                "error": e.to_string(),
            })
        });
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
        "browser": browser_kind,
        "cron": true,
        "approvals": true,
        "campaigns": true,
        "gateway": true,
        "files": true,
        "pty": false,
        "preview_browser": preview_browser,
        "cron_jobs": cron_jobs,
        "campaigns_active": campaigns_active,
        "approvals_pending": approvals_pending,
        "work_isolation": product_settings.get("work_isolation").cloned().unwrap_or(json!("shared")),
        "configured_mode": product_settings.get("configured_mode").cloned().unwrap_or(json!("shared")),
        "enforced_mode": product_settings.get("enforced_mode").cloned().unwrap_or(json!("shared")),
        "work_isolation_label": product_settings.get("work_isolation_label").cloned().unwrap_or(json!("Shared workbench")),
        "allow_concurrent_projects": product_settings.get("allow_concurrent_projects").cloned().unwrap_or(json!(false)),
        // Product FS isolation only (false unless project_bound). Never default true.
        "isolation_enforcement_active": product_settings.get("enforcement_active").cloned().unwrap_or(json!(false)),
        "product_fs_enforced": product_settings.get("product_fs_enforced").cloned().unwrap_or(json!(false)),
        "command_envelope_enforced": product_settings.get("command_envelope_enforced").cloned().unwrap_or(json!(true)),
        "settings": product_settings,
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

#[cfg(test)]
mod tests {
    use super::{doctor_json, settings_get, settings_set};
    use serde_json::json;

    #[test]
    fn doctor_preview_browser_matches_chrome_detection() {
        let dir = tempfile::tempdir().expect("temp home");
        let home = dir.path().to_path_buf();
        let doc = doctor_json(&home);
        let browser = doc
            .get("browser")
            .and_then(|v| v.as_str())
            .expect("browser field");
        let preview = doc
            .get("preview_browser")
            .and_then(|v| v.as_bool())
            .expect("preview_browser field");
        assert!(
            browser == "cdp" || browser == "http-ssrf-safe",
            "unexpected browser kind: {browser}"
        );
        assert_eq!(preview, browser == "cdp");
        assert_eq!(
            preview,
            optimus_kernel::chrome_binary_path().is_some(),
            "doctor.preview_browser must track chrome_binary_path()"
        );
    }

    #[test]
    fn settings_get_set_round_trip_and_doctor_fields() {
        let dir = tempfile::tempdir().expect("temp home");
        let home = dir.path().to_path_buf();
        let got = settings_get(&home).unwrap();
        assert_eq!(got["settings"]["work_isolation"], "shared");
        let set = settings_set(
            &home,
            json!({
                "work_isolation": "isolated_profiles",
                "allow_concurrent_projects": true
            }),
        )
        .unwrap();
        assert_eq!(set["settings"]["work_isolation"], "isolated_profiles");
        assert_eq!(set["settings"]["configured_mode"], "isolated_profiles");
        assert_eq!(set["settings"]["allow_concurrent_projects"], true);
        // Sealed profile homes are not product-FS enforced yet (After P29).
        assert_eq!(set["settings"]["enforcement_active"], false);
        assert_eq!(set["settings"]["product_fs_enforced"], false);
        assert_eq!(set["settings"]["enforced_mode"], "shared");
        assert_eq!(set["settings"]["command_fs_envelope"], "confined_no_network");
        assert_eq!(set["settings"]["command_envelope_enforced"], true);
        let again = settings_get(&home).unwrap();
        assert_eq!(again["settings"]["work_isolation"], "isolated_profiles");
        let doc = doctor_json(&home);
        assert_eq!(doc["work_isolation"], "isolated_profiles");
        assert_eq!(doc["configured_mode"], "isolated_profiles");
        assert_eq!(doc["enforced_mode"], "shared");
        assert_eq!(doc["allow_concurrent_projects"], true);
        assert_eq!(doc["isolation_enforcement_active"], false);
        assert_eq!(doc["product_fs_enforced"], false);

        let bound = settings_set(
            &home,
            json!({
                "work_isolation": "project_bound",
                "allow_concurrent_projects": false
            }),
        )
        .unwrap();
        assert_eq!(bound["settings"]["configured_mode"], "project_bound");
        assert_eq!(bound["settings"]["enforced_mode"], "project_bound");
        assert_eq!(bound["settings"]["enforcement_active"], true);
        assert_eq!(bound["settings"]["product_fs_enforced"], true);
    }
}
