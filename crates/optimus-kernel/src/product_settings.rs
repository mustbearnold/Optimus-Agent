//! Durable product settings under `{home}/settings.json`.
//!
//! Work-isolation modes map to [`optimus_graph::CommandFsEnvelope`] strength
//! (P12). Project-bound FS/memory product isolation beyond the command
//! envelope remains progressive (ADR-0027 / ADR-0031).

use std::fs;
use std::path::{Path, PathBuf};

use optimus_graph::CommandFsEnvelope;
use serde::{Deserialize, Serialize};

use crate::{atomic_write_user_only, KernelError, Result};

const SETTINGS_FILE: &str = "settings.json";
const SETTINGS_VERSION: u32 = 1;

/// How Optimus scopes project work (user-selected; default shared).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkIsolationMode {
    /// One home/workspace; projects organize sessions only.
    #[default]
    Shared,
    /// Active project binds FS/tools/memory/browser (Phase 1+).
    ProjectBound,
    /// Each project is a sealed profile home (Phase 2+).
    IsolatedProfiles,
}

impl WorkIsolationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::ProjectBound => "project_bound",
            Self::IsolatedProfiles => "isolated_profiles",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Shared => "Shared workbench",
            Self::ProjectBound => "Project-bound",
            Self::IsolatedProfiles => "Isolated profiles",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "shared" | "share" | "workbench" => Some(Self::Shared),
            "project_bound" | "project-bound" | "bound" => Some(Self::ProjectBound),
            "isolated_profiles" | "isolated" | "profiles" => Some(Self::IsolatedProfiles),
            _ => None,
        }
    }

    /// Whether **product FS isolation** beyond the command envelope is enforced.
    ///
    /// P12 always enforces a command capability envelope. P22 honesty:
    /// - `shared` → no product-bound project FS enforcement (envelope only)
    /// - `project_bound` → project roots + workspace hash for mutating effects
    /// - `isolated_profiles` → sealed profile homes via `ProfileStore` (S7);
    ///   product_fs_enforced still only true for `project_bound`
    pub fn product_fs_enforced(self) -> bool {
        matches!(self, Self::ProjectBound)
    }

    /// Whether the command FS envelope selected by this mode is enforced at spawn.
    pub fn command_envelope_enforced(self) -> bool {
        true
    }

    /// Legacy alias: true only when product FS isolation is enforced.
    /// Prefer [`product_fs_enforced`] + [`command_envelope_enforced`] in new code.
    pub fn enforcement_active(self) -> bool {
        self.product_fs_enforced()
    }

    /// Map product isolation intent to the command capability envelope.
    pub fn command_fs_envelope(self) -> CommandFsEnvelope {
        match self {
            Self::Shared | Self::ProjectBound => CommandFsEnvelope::Confined,
            Self::IsolatedProfiles => CommandFsEnvelope::ConfinedNoNetwork,
        }
    }
}

/// Product settings file (versioned JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductSettings {
    pub version: u32,
    pub work_isolation: WorkIsolationMode,
    /// When true, concurrent mutating work across projects is allowed once B/C enforce.
    pub allow_concurrent_projects: bool,
    /// Optional note when an unknown value was coerced on load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_note: Option<String>,
}

impl Default for ProductSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            work_isolation: WorkIsolationMode::Shared,
            allow_concurrent_projects: false,
            load_note: None,
        }
    }
}

impl ProductSettings {
    pub fn path(home: impl AsRef<Path>) -> PathBuf {
        home.as_ref().join(SETTINGS_FILE)
    }

    /// Load settings, creating defaults if missing. Unknown modes → Shared + note.
    pub fn load(home: impl AsRef<Path>) -> Result<Self> {
        let home = home.as_ref();
        let path = Self::path(home);
        if !path.exists() {
            let settings = Self::default();
            settings.save(home)?;
            return Ok(settings);
        }
        let raw = fs::read_to_string(&path)?;
        let mut value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| KernelError::Tool(format!("settings.json parse error: {e}")))?;
        let mut note = None;
        if let Some(mode_val) = value.get("work_isolation").cloned() {
            let ok = match mode_val.as_str() {
                Some(s) => WorkIsolationMode::parse(s).is_some(),
                _ => false,
            };
            if !ok {
                note = Some(format!(
                    "unknown work_isolation coerced to shared: {mode_val}"
                ));
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "work_isolation".into(),
                        serde_json::Value::String("shared".into()),
                    );
                }
            }
        }
        let mut settings: ProductSettings = serde_json::from_value(value)
            .map_err(|e| KernelError::Tool(format!("settings.json schema error: {e}")))?;
        settings.version = SETTINGS_VERSION;
        if note.is_some() {
            settings.load_note = note;
            settings.save(home)?;
        }
        Ok(settings)
    }

    pub fn save(&self, home: impl AsRef<Path>) -> Result<()> {
        let home = home.as_ref();
        fs::create_dir_all(home)?;
        let path = Self::path(home);
        let mut out = self.clone();
        out.version = SETTINGS_VERSION;
        out.load_note = None; // do not persist ephemeral load notes
        let body = serde_json::to_vec_pretty(&out)?;
        atomic_write_user_only(&path, &body)?;
        Ok(())
    }

    /// Apply a partial update from UI/IPC JSON.
    pub fn apply_patch(&mut self, patch: &serde_json::Value) -> Result<()> {
        if let Some(mode) = patch.get("work_isolation") {
            let parsed = match mode.as_str() {
                Some(s) => WorkIsolationMode::parse(s)
                    .ok_or_else(|| KernelError::Tool(format!("invalid work_isolation: {s}")))?,
                _ => return Err(KernelError::Tool("work_isolation must be a string".into())),
            };
            self.work_isolation = parsed;
        }
        if let Some(v) = patch.get("allow_concurrent_projects") {
            let b = v.as_bool().ok_or_else(|| {
                KernelError::Tool("allow_concurrent_projects must be a boolean".into())
            })?;
            self.allow_concurrent_projects = b;
        }
        Ok(())
    }

    pub fn to_public_json(&self) -> serde_json::Value {
        let configured = self.work_isolation.as_str();
        let product_fs = self.work_isolation.product_fs_enforced();
        let cmd_env = self.work_isolation.command_envelope_enforced();
        serde_json::json!({
            "version": self.version,
            "work_isolation": configured,
            "configured_mode": configured,
            "work_isolation_label": self.work_isolation.label(),
            "allow_concurrent_projects": self.allow_concurrent_projects,
            // Product FS isolation only (not command envelope).
            "enforcement_active": product_fs,
            "product_fs_enforced": product_fs,
            "command_envelope_enforced": cmd_env,
            // Honest effective product mode for status bars (never claim isolated_profiles).
            "enforced_mode": if product_fs {
                "project_bound"
            } else {
                "shared"
            },
            "enforcement_note": match self.work_isolation {
                WorkIsolationMode::Shared => {
                    "Shared mode: command FS envelope is confined; product FS isolation is not enforced."
                }
                WorkIsolationMode::ProjectBound => {
                    "Project-bound: mutating effects bind workspace_sha256 and Rust project roots. \
                     allow_concurrent_projects is a settings flag only until a multi-project mutate lease store ships."
                }
                WorkIsolationMode::IsolatedProfiles => {
                    "Isolated profiles (configured): command envelope ConfinedNoNetwork on Linux; ProfileStore sealed homes available (S7). Cross-profile deny-by-default. Status must not claim distributed multi-DB isolation."
                }
            },
            "command_fs_envelope": self.work_isolation.command_fs_envelope().as_str(),
            "load_note": self.load_note,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_load_creates_shared_settings() {
        let dir = tempdir().unwrap();
        let s = ProductSettings::load(dir.path()).unwrap();
        assert_eq!(s.work_isolation, WorkIsolationMode::Shared);
        assert!(!s.allow_concurrent_projects);
        assert!(ProductSettings::path(dir.path()).is_file());
    }

    #[test]
    fn isolation_honesty_shared_never_claims_product_fs_enforced() {
        let s = ProductSettings::default();
        let j = s.to_public_json();
        assert_eq!(j["configured_mode"], "shared");
        assert_eq!(j["enforced_mode"], "shared");
        assert_eq!(j["product_fs_enforced"], false);
        assert_eq!(j["enforcement_active"], false);
        assert_eq!(j["command_envelope_enforced"], true);

        let bound = ProductSettings {
            work_isolation: WorkIsolationMode::ProjectBound,
            ..Default::default()
        };
        let j2 = bound.to_public_json();
        assert_eq!(j2["configured_mode"], "project_bound");
        assert_eq!(j2["enforced_mode"], "project_bound");
        assert_eq!(j2["product_fs_enforced"], true);

        let isolated = ProductSettings {
            work_isolation: WorkIsolationMode::IsolatedProfiles,
            ..Default::default()
        };
        let j3 = isolated.to_public_json();
        assert_eq!(j3["configured_mode"], "isolated_profiles");
        // Sealed homes not enforced yet — must not claim isolated product FS.
        assert_eq!(j3["enforced_mode"], "shared");
        assert_eq!(j3["product_fs_enforced"], false);
        assert_eq!(j3["command_envelope_enforced"], true);
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempdir().unwrap();
        let mut s = ProductSettings::load(dir.path()).unwrap();
        s.work_isolation = WorkIsolationMode::IsolatedProfiles;
        s.allow_concurrent_projects = true;
        s.save(dir.path()).unwrap();
        let again = ProductSettings::load(dir.path()).unwrap();
        assert_eq!(again.work_isolation, WorkIsolationMode::IsolatedProfiles);
        assert!(again.allow_concurrent_projects);
    }

    #[test]
    fn unknown_mode_coerces_to_shared() {
        let dir = tempdir().unwrap();
        let path = ProductSettings::path(dir.path());
        fs::write(
            &path,
            r#"{"version":1,"work_isolation":"galaxy_brain","allow_concurrent_projects":false}"#,
        )
        .unwrap();
        let s = ProductSettings::load(dir.path()).unwrap();
        assert_eq!(s.work_isolation, WorkIsolationMode::Shared);
        assert!(s.load_note.as_ref().unwrap().contains("galaxy_brain"));
    }

    #[test]
    fn apply_patch_validates() {
        let mut s = ProductSettings::default();
        s.apply_patch(&serde_json::json!({
            "work_isolation": "project_bound",
            "allow_concurrent_projects": true
        }))
        .unwrap();
        assert_eq!(s.work_isolation, WorkIsolationMode::ProjectBound);
        assert!(s.allow_concurrent_projects);
        assert!(s
            .apply_patch(&serde_json::json!({"work_isolation": "nope"}))
            .is_err());
    }
}
