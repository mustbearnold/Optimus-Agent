//! Durable product settings under `{home}/settings.json`.
//!
//! Phase 0 stores work-isolation *intent*. Runtime enforcement of
//! `project_bound` / `isolated_profiles` lands in later phases (ADR-0027).

use std::fs;
use std::path::{Path, PathBuf};

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

    /// Whether this mode is fully enforced by runtime policy yet.
    pub fn enforcement_active(self) -> bool {
        matches!(self, Self::Shared)
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
        let mut value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            KernelError::Tool(format!("settings.json parse error: {e}"))
        })?;
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
        let mut settings: ProductSettings = serde_json::from_value(value).map_err(|e| {
            KernelError::Tool(format!("settings.json schema error: {e}"))
        })?;
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
                Some(s) => WorkIsolationMode::parse(s).ok_or_else(|| {
                    KernelError::Tool(format!("invalid work_isolation: {s}"))
                })?,
                _ => {
                    return Err(KernelError::Tool(
                        "work_isolation must be a string".into(),
                    ))
                }
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
        serde_json::json!({
            "version": self.version,
            "work_isolation": self.work_isolation.as_str(),
            "work_isolation_label": self.work_isolation.label(),
            "allow_concurrent_projects": self.allow_concurrent_projects,
            "enforcement_active": self.work_isolation.enforcement_active(),
            "enforcement_note": if self.work_isolation.enforcement_active() {
                "Shared mode is the active runtime path."
            } else {
                "Mode stored as intent; project-bound/profile enforcement ships in a later phase."
            },
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
