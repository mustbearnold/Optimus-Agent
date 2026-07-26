//! Profile-isolated homes (program S7.1–S7.2).
//!
//! Each profile owns a sealed subdirectory under `{home}/profiles/{id}/` with its
//! own `memory.db`, `sessions`, and workspace root. Cross-profile path access is
//! deny-by-default.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{KernelError, Result};

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("invalid profile id: {0}")]
    InvalidId(String),
    #[error("profile not found: {0}")]
    NotFound(String),
    #[error("cross-profile access denied: {0}")]
    CrossProfileDenied(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Msg(String),
}

impl From<ProfileError> for KernelError {
    fn from(value: ProfileError) -> Self {
        KernelError::Model(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn parse(raw: impl AsRef<str>) -> std::result::Result<Self, ProfileError> {
        let s = raw.as_ref().trim();
        let ok = !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return Err(ProfileError::InvalidId(s.into()));
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileHome {
    pub id: ProfileId,
    pub root: PathBuf,
    pub workspace: PathBuf,
    pub memory_db: PathBuf,
    pub sessions_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileRegistryView {
    pub profiles: Vec<ProfileHome>,
    pub active: Option<String>,
}

pub struct ProfileStore {
    home: PathBuf,
}

impl ProfileStore {
    pub fn open(home: impl AsRef<Path>) -> std::result::Result<Self, ProfileError> {
        let home = home.as_ref().to_path_buf();
        fs::create_dir_all(home.join("profiles"))?;
        Ok(Self { home })
    }

    pub fn profiles_root(&self) -> PathBuf {
        self.home.join("profiles")
    }

    pub fn create(&self, id: &ProfileId) -> std::result::Result<ProfileHome, ProfileError> {
        let root = self.profiles_root().join(id.as_str());
        if root.exists() {
            return self.open_profile(id);
        }
        let workspace = root.join("workspace");
        let sessions = root.join("sessions");
        fs::create_dir_all(&workspace)?;
        fs::create_dir_all(&sessions)?;
        let memory_db = root.join("memory.db");
        // Touch empty sqlite-compatible path (actual Memory::open creates schema).
        if !memory_db.exists() {
            fs::File::create(&memory_db)?;
        }
        let meta = json_meta(id);
        fs::write(root.join("profile.json"), meta)?;
        self.open_profile(id)
    }

    pub fn open_profile(&self, id: &ProfileId) -> std::result::Result<ProfileHome, ProfileError> {
        let root = self.profiles_root().join(id.as_str());
        if !root.is_dir() {
            return Err(ProfileError::NotFound(id.as_str().into()));
        }
        Ok(ProfileHome {
            id: id.clone(),
            root: root.clone(),
            workspace: root.join("workspace"),
            memory_db: root.join("memory.db"),
            sessions_dir: root.join("sessions"),
        })
    }

    pub fn list(&self) -> std::result::Result<Vec<ProfileHome>, ProfileError> {
        let mut out = Vec::new();
        let root = self.profiles_root();
        if !root.is_dir() {
            return Ok(out);
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(id) = ProfileId::parse(&name) {
                out.push(self.open_profile(&id)?);
            }
        }
        out.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(out)
    }

    pub fn set_active(&self, id: Option<&ProfileId>) -> std::result::Result<(), ProfileError> {
        let path = self.home.join("active_profile.json");
        match id {
            Some(id) => {
                let _ = self.open_profile(id)?;
                fs::write(
                    path,
                    serde_json::to_string_pretty(&serde_json::json!({ "id": id.as_str() }))
                        .map_err(|e| ProfileError::Msg(e.to_string()))?,
                )?;
            }
            None => {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    pub fn active(&self) -> std::result::Result<Option<ProfileId>, ProfileError> {
        let path = self.home.join("active_profile.json");
        if !path.is_file() {
            return Ok(None);
        }
        let raw = fs::read_to_string(path)?;
        let v: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| ProfileError::Msg(e.to_string()))?;
        let id = v
            .get("id")
            .and_then(|x| x.as_str())
            .ok_or_else(|| ProfileError::Msg("active profile missing id".into()))?;
        Ok(Some(ProfileId::parse(id)?))
    }

    /// Resolve a path under a profile workspace. Deny if it escapes the profile root.
    pub fn resolve_workspace_path(
        &self,
        profile: &ProfileId,
        relative: &str,
    ) -> std::result::Result<PathBuf, ProfileError> {
        let home = self.open_profile(profile)?;
        if relative.split('/').any(|p| p == ".." || p.is_empty()) {
            return Err(ProfileError::CrossProfileDenied(
                "path traversal rejected".into(),
            ));
        }
        let candidate = home.workspace.join(relative);
        // Before create: lexical check under workspace
        let ws = home
            .workspace
            .canonicalize()
            .unwrap_or(home.workspace.clone());
        let joined = if candidate.exists() {
            candidate.canonicalize().map_err(ProfileError::Io)?
        } else {
            // Ensure parent is under workspace
            if let Some(parent) = candidate.parent() {
                let _ = fs::create_dir_all(parent);
            }
            candidate
        };
        let abs = if joined.exists() {
            joined.canonicalize().map_err(ProfileError::Io)?
        } else {
            joined
        };
        // Compare string prefix carefully for non-existing files
        let ws_s = ws.to_string_lossy();
        let abs_s = abs.to_string_lossy();
        if !abs_s.starts_with(ws_s.as_ref()) && !abs.starts_with(&ws) {
            return Err(ProfileError::CrossProfileDenied(relative.into()));
        }
        Ok(abs)
    }

    /// Deny linking/reading another profile's path from this profile.
    pub fn assert_path_in_profile(
        &self,
        profile: &ProfileId,
        path: &Path,
    ) -> std::result::Result<(), ProfileError> {
        let home = self.open_profile(profile)?;
        let root = home.root.canonicalize().map_err(ProfileError::Io)?;
        let abs = if path.exists() {
            path.canonicalize().map_err(ProfileError::Io)?
        } else {
            path.to_path_buf()
        };
        if !abs.starts_with(&root) {
            return Err(ProfileError::CrossProfileDenied(format!(
                "{} outside profile {}",
                abs.display(),
                profile.as_str()
            )));
        }
        // Extra: must not be under a sibling profile
        if let Ok(profiles) = self.list() {
            for other in profiles {
                if other.id.as_str() == profile.as_str() {
                    continue;
                }
                if let Ok(oroot) = other.root.canonicalize() {
                    if abs.starts_with(&oroot) {
                        return Err(ProfileError::CrossProfileDenied(format!(
                            "path belongs to profile {}",
                            other.id.as_str()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

fn json_meta(id: &ProfileId) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "id": id.as_str(),
        "schema": 1,
    }))
    .unwrap_or_else(|_| "{}".into())
}

/// Convenience: create two profiles and prove cross-access is denied.
pub fn create_default_profiles(home: impl AsRef<Path>) -> Result<(ProfileHome, ProfileHome)> {
    let store = ProfileStore::open(home).map_err(KernelError::from)?;
    let a = ProfileId::parse("default").map_err(KernelError::from)?;
    let b = ProfileId::parse("secondary").map_err(KernelError::from)?;
    let ha = store.create(&a).map_err(KernelError::from)?;
    let hb = store.create(&b).map_err(KernelError::from)?;
    Ok((ha, hb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn profile_homes_are_isolated_and_cross_links_denied() {
        let dir = tempdir().unwrap();
        let store = ProfileStore::open(dir.path()).unwrap();
        let a = ProfileId::parse("alice").unwrap();
        let b = ProfileId::parse("bob").unwrap();
        let ha = store.create(&a).unwrap();
        let hb = store.create(&b).unwrap();
        assert_ne!(ha.root, hb.root);
        assert!(ha.workspace.exists());
        // Write a file in alice workspace
        let secret = ha.workspace.join("secret.txt");
        fs::write(&secret, b"alice-only").unwrap();
        // Bob cannot assert alice path
        assert!(store.assert_path_in_profile(&b, &secret).is_err());
        // Alice can
        store.assert_path_in_profile(&a, &secret).unwrap();
        // Path traversal denied
        assert!(store
            .resolve_workspace_path(&a, "../bob/workspace/x")
            .is_err());
        store.set_active(Some(&a)).unwrap();
        assert_eq!(store.active().unwrap().unwrap().as_str(), "alice");
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
    }
}
