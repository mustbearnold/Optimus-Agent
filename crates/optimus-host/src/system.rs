//! Diagnostics, product settings, and Codex authentication IPC.

use std::path::PathBuf;

use optimus_kernel::{open_cron, CodexAuthStore, ProductSettings, ProviderKeyStore};
use optimus_packs::CapabilitySession;
use optimus_runtime::{CampaignStatus, CampaignStore};
use serde_json::json;

use crate::runtime_ops::open_runtime;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "ping"
            | "doctor"
            | "auth_status"
            | "auth_import_hermes"
            | "auth_import_cli"
            | "provider_keys_status"
            | "provider_key_set"
            | "provider_key_clear"
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
        "provider_keys_status" => Ok(json!({ "providers": provider_keys_json(home) })),
        "provider_key_set" => provider_key_set(home, params),
        "provider_key_clear" => provider_key_clear(home, params),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Providers that authenticate with a pasted API key. Codex is absent by
/// design: it authenticates through OAuth and is reported by `auth_status`.
const KEY_PROVIDERS: &[(&str, &str, &str)] = &[(
    optimus_kernel::DEEPSEEK_PROVIDER,
    "DeepSeek",
    "DEEPSEEK_API_KEY",
)];

fn provider_label(provider: &str) -> Option<(&'static str, &'static str)> {
    KEY_PROVIDERS
        .iter()
        .find(|(id, _, _)| *id == provider)
        .map(|(_, label, env_key)| (*label, *env_key))
}

fn requested_provider(params: &serde_json::Value) -> Result<String, String> {
    let provider = params
        .get("provider")
        .and_then(|value| value.as_str())
        .unwrap_or(optimus_kernel::DEEPSEEK_PROVIDER)
        .trim()
        .to_ascii_lowercase();
    if provider_label(&provider).is_none() {
        return Err(format!("unsupported key provider: {provider}"));
    }
    Ok(provider)
}

fn provider_key_set(
    home: &PathBuf,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let provider = requested_provider(&params)?;
    let api_key = params
        .get("api_key")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let base_url = params.get("base_url").and_then(|value| value.as_str());
    let store = ProviderKeyStore::open(home).map_err(|e| e.to_string())?;
    store
        .set_key(&provider, api_key, base_url)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "providers": provider_keys_json(home) }))
}

fn provider_key_clear(
    home: &PathBuf,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let provider = requested_provider(&params)?;
    let store = ProviderKeyStore::open(home).map_err(|e| e.to_string())?;
    store.clear_key(&provider).map_err(|e| e.to_string())?;
    Ok(json!({ "providers": provider_keys_json(home) }))
}

/// Never returns a key — only whether one exists, where it came from, and a
/// masked tail so the user can recognise which key is stored.
pub fn provider_keys_json(home: &PathBuf) -> serde_json::Value {
    let store = match ProviderKeyStore::open(home) {
        Ok(store) => store,
        Err(error) => return json!([{ "error": error.to_string() }]),
    };
    let entries: Vec<_> = KEY_PROVIDERS
        .iter()
        .map(
            |(provider, label, env_key)| match store.status(provider, env_key) {
                Ok(status) => json!({
                    "provider": status.provider,
                    "label": label,
                    "env_var": env_key,
                    "present": status.present,
                    "source": status.source.as_str(),
                    "hint": status.hint,
                    "base_url": status.base_url,
                }),
                Err(error) => json!({
                    "provider": provider,
                    "label": label,
                    "env_var": env_key,
                    "present": false,
                    "source": "none",
                    "error": error.to_string(),
                }),
            },
        )
        .collect();
    json!(entries)
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
    let gateway_status = optimus_kernel::gateway_status(home).ok();
    let gateway_ambiguous = gateway_status
        .as_ref()
        .map(|s| s.ambiguous_sends)
        .unwrap_or(0);
    let gateway_inbox_pending = gateway_status
        .as_ref()
        .map(|s| s.inbox_pending)
        .unwrap_or(0);
    let gateway_outbox_total = gateway_status.as_ref().map(|s| s.outbox_total).unwrap_or(0);
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
    let packs_loaded = s.loaded_packs().len();
    let packs_on_demand = s.on_demand_count();
    let pack_tools: usize = pack_catalog.iter().map(|p| p.tools.len()).sum();
    let install = detect_install_metadata();
    let shell = detect_shell_mode(&install);
    // Build map incrementally — large pack_catalog blows json! recursion limit.
    let mut out = serde_json::Map::new();
    out.insert("phase".into(), json!("product-complete"));
    out.insert("program_phase".into(), json!("P29"));
    out.insert("home".into(), json!(home.display().to_string()));
    out.insert("core_schema_tokens".into(), json!(s.schema_tokens()));
    out.insert("max_budget".into(), json!(2500));
    out.insert("max_on_demand".into(), json!(2));
    out.insert(
        "pack_catalog".into(),
        serde_json::to_value(&pack_catalog).unwrap_or(json!([])),
    );
    out.insert("packs_loaded".into(), json!(packs_loaded));
    out.insert("packs_on_demand".into(), json!(packs_on_demand));
    out.insert("packs_tool_count".into(), json!(pack_tools));
    out.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
    out.insert(
        "codex_present".into(),
        json!(auth
            .get("present")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)),
    );
    out.insert("streaming".into(), json!(true));
    out.insert("browser".into(), json!(browser_kind));
    out.insert("cron".into(), json!(true));
    out.insert("approvals".into(), json!(true));
    out.insert("campaigns".into(), json!(true));
    out.insert("gateway".into(), json!(true));
    out.insert("files".into(), json!(true));
    out.insert("pty".into(), json!(false));
    out.insert("preview_browser".into(), json!(preview_browser));
    out.insert("cron_jobs".into(), json!(cron_jobs));
    out.insert("campaigns_active".into(), json!(campaigns_active));
    out.insert("approvals_pending".into(), json!(approvals_pending));
    out.insert("gateway_inbox_pending".into(), json!(gateway_inbox_pending));
    out.insert("gateway_outbox_total".into(), json!(gateway_outbox_total));
    out.insert("gateway_ambiguous_sends".into(), json!(gateway_ambiguous));
    out.insert(
        "gateway_note".into(),
        json!("Local SQLite is delivery authority. External exactly-once is not claimed."),
    );
    out.insert("shell_mode".into(), json!(shell.mode));
    out.insert("shell_default".into(), json!(shell.default_shell));
    out.insert("shell_label".into(), json!(shell.label));
    out.insert("updater_channel".into(), json!("none"));
    out.insert(
        "updater_note".into(),
        json!("No auto-updater (ADR-0043). Reinstall via scripts/rebuild-install-relaunch.sh."),
    );
    out.insert("install_present".into(), json!(install.present));
    out.insert("install_shell".into(), json!(install.desktop_shell));
    out.insert("install_version".into(), json!(install.version));
    out.insert(
        "work_isolation".into(),
        product_settings
            .get("work_isolation")
            .cloned()
            .unwrap_or(json!("shared")),
    );
    out.insert(
        "configured_mode".into(),
        product_settings
            .get("configured_mode")
            .cloned()
            .unwrap_or(json!("shared")),
    );
    out.insert(
        "enforced_mode".into(),
        product_settings
            .get("enforced_mode")
            .cloned()
            .unwrap_or(json!("shared")),
    );
    out.insert(
        "work_isolation_label".into(),
        product_settings
            .get("work_isolation_label")
            .cloned()
            .unwrap_or(json!("Shared workbench")),
    );
    out.insert(
        "allow_concurrent_projects".into(),
        product_settings
            .get("allow_concurrent_projects")
            .cloned()
            .unwrap_or(json!(false)),
    );
    out.insert(
        "isolation_enforcement_active".into(),
        product_settings
            .get("enforcement_active")
            .cloned()
            .unwrap_or(json!(false)),
    );
    out.insert(
        "product_fs_enforced".into(),
        product_settings
            .get("product_fs_enforced")
            .cloned()
            .unwrap_or(json!(false)),
    );
    out.insert(
        "command_envelope_enforced".into(),
        product_settings
            .get("command_envelope_enforced")
            .cloned()
            .unwrap_or(json!(true)),
    );
    out.insert("settings".into(), product_settings);
    serde_json::Value::Object(out)
}

struct ShellModeReport {
    mode: String,
    default_shell: bool,
    label: String,
}

/// Canonical product shell token (matches install-meta `desktop_shell`).
const SHELL_REACT_TAURI: &str = "react-tauri";

fn detect_shell_mode(install: &InstallMetaReport) -> ShellModeReport {
    // The product shell is exclusively Tauri: the mode comes from install-meta
    // (written by the installer), defaulting to react-tauri. The old
    // OPTIMUS_DESKTOP_SHELL env dispatch and the Electron/Wry rollback tokens
    // are retired — no env value may select a shell mode.
    let mode = install
        .desktop_shell
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| SHELL_REACT_TAURI.into());
    let default_shell = mode == SHELL_REACT_TAURI;
    let label = if default_shell {
        "Tauri + React (default)".into()
    } else {
        format!("Shell mode: {mode}")
    };
    ShellModeReport {
        mode,
        default_shell,
        label,
    }
}

struct InstallMetaReport {
    present: bool,
    desktop_shell: Option<String>,
    version: Option<String>,
}

fn detect_install_metadata() -> InstallMetaReport {
    // Best-effort read of the stable user install (XDG). Missing install is not an error.
    let data_home = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".local/share"))
                .unwrap_or_else(|_| PathBuf::from("/tmp"))
        });
    let root = std::env::var("OPTIMUS_INSTALL_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_home.join("optimus-agent"));
    let meta_path = root.join("install-meta.json");
    if !meta_path.is_file() {
        return InstallMetaReport {
            present: false,
            desktop_shell: None,
            version: None,
        };
    }
    let raw = std::fs::read_to_string(&meta_path).unwrap_or_default();
    let meta: serde_json::Value = serde_json::from_str(&raw).unwrap_or(json!({}));
    let version = meta
        .get("version")
        .or_else(|| meta.get("product_version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            std::fs::read_to_string(root.join("VERSION.txt"))
                .ok()
                .and_then(|s| s.lines().next().map(|line| line.trim().to_string()))
                .filter(|s| !s.is_empty())
        });
    InstallMetaReport {
        present: true,
        desktop_shell: meta
            .get("desktop_shell")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        version,
    }
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
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("system test environment lock must not be poisoned")
    }

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
        let _env = env_lock();
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
        assert_eq!(
            set["settings"]["command_fs_envelope"],
            "confined_no_network"
        );
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
        assert_eq!(doc["shell_mode"], "react-tauri");
        assert_eq!(doc["shell_default"], true);
        assert_eq!(doc["updater_channel"], "none");
        assert_eq!(doc["phase"], "product-complete");
        assert_eq!(doc["program_phase"], "P29");
        assert!(doc["gateway"].as_bool().unwrap_or(false));
        assert!(doc.get("packs_loaded").is_some());
        assert!(doc.get("gateway_ambiguous_sends").is_some());
        assert!(doc["updater_note"]
            .as_str()
            .unwrap_or("")
            .contains("ADR-0043"));

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

    #[test]
    fn doctor_install_metadata_respects_xdg_data_home() {
        let _env = env_lock();
        let dir = tempfile::tempdir().expect("temp xdg");
        let data = dir.path().join("share");
        let root = data.join("optimus-agent");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("install-meta.json"),
            r#"{
              "version": "0.1.0-test",
              "desktop_shell": "react-tauri",
              "install_root": "ignored"
            }"#,
        )
        .unwrap();
        // SAFETY: test-local env for install probe; restored after.
        let prev = std::env::var_os("XDG_DATA_HOME");
        std::env::set_var("XDG_DATA_HOME", &data);
        let home = dir.path().join("product-home");
        std::fs::create_dir_all(&home).unwrap();
        let doc = doctor_json(&home);
        assert_eq!(doc["install_present"], true);
        assert_eq!(doc["install_shell"], "react-tauri");
        assert_eq!(doc["install_version"], "0.1.0-test");
        assert_eq!(doc["shell_mode"], "react-tauri");
        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }

    #[test]
    fn legacy_shell_env_tokens_are_ignored() {
        // Regression (2026-08-05, pair review): the host used to map
        // OPTIMUS_DESKTOP_SHELL=electron|react-electron|wry to rollback shell
        // modes. Electron and the Wry rollback are retired; no env token may
        // select a shell mode — the probe falls back to the install-meta
        // product default.
        let _env = env_lock();
        let dir = tempfile::tempdir().expect("temp xdg");
        let data = dir.path().join("share");
        let root = data.join("optimus-agent");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("install-meta.json"),
            r#"{
              "version": "0.1.0-test",
              "desktop_shell": "react-tauri",
              "install_root": "ignored"
            }"#,
        )
        .unwrap();
        // SAFETY: test-local env for install probe; restored after.
        let prev = std::env::var_os("XDG_DATA_HOME");
        let prev_shell = std::env::var_os("OPTIMUS_DESKTOP_SHELL");
        std::env::set_var("XDG_DATA_HOME", &data);
        for token in [
            "electron",
            "react-electron",
            "electron_react",
            "wry",
            "legacy_wry",
        ] {
            std::env::set_var("OPTIMUS_DESKTOP_SHELL", token);
            let home = dir.path().join("product-home");
            std::fs::create_dir_all(&home).unwrap();
            let doc = doctor_json(&home);
            assert_eq!(
                doc["shell_mode"], "react-tauri",
                "env token {token:?} must not select a shell mode"
            );
        }
        match prev {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match prev_shell {
            Some(v) => std::env::set_var("OPTIMUS_DESKTOP_SHELL", v),
            None => std::env::remove_var("OPTIMUS_DESKTOP_SHELL"),
        }
    }
}
