//! Canonical workspace identity shared by project-bound runtime effects.

use std::fs;
use std::path::{Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use optimus_graph::{GraphError, RuntimeConfig};
use optimus_store::Store;
use sha2::{Digest, Sha256};

use crate::{Result, Runtime, RuntimeError};

pub(crate) struct DeveloperAccessState {
    pub grant: Option<optimus_policy::DeveloperAccessGrant>,
    pub roots: Vec<PathBuf>,
    pub dirs: Vec<(PathBuf, Dir)>,
}

impl Runtime {
    pub fn open(db_path: &Path, workspace: &Path) -> Result<Self> {
        Self::open_with_config(db_path, workspace, RuntimeConfig::default())
    }

    pub fn open_with_config(
        db_path: &Path,
        workspace: &Path,
        config: RuntimeConfig,
    ) -> Result<Self> {
        Self::open_with_developer_access(db_path, workspace, config, None, Vec::new())
    }

    /// Open a runtime with an explicit, already-validated local developer
    /// grant. The normal opener remains authority-free by default.
    pub fn open_with_developer_access(
        db_path: &Path,
        workspace: &Path,
        config: RuntimeConfig,
        developer_access: Option<optimus_policy::DeveloperAccessGrant>,
        developer_roots: Vec<PathBuf>,
    ) -> Result<Self> {
        fs::create_dir_all(workspace)?;
        let workspace = fs::canonicalize(workspace)?;
        let workspace_dir = Dir::open_ambient_dir(&workspace, ambient_authority())?;
        if let Some(grant) = developer_access.as_ref().filter(|grant| grant.enabled) {
            grant.validate().map_err(|error| {
                RuntimeError::PathEscape(format!("invalid developer grant: {error}"))
            })?;
        }
        let developer_dirs = if developer_access
            .as_ref()
            .is_some_and(|grant| grant.enabled && grant.capabilities.workspace_files)
        {
            developer_roots
                .iter()
                .map(|root| {
                    let root = fs::canonicalize(root)?;
                    if !root.is_dir() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::NotADirectory,
                            format!("developer root is not a directory: {}", root.display()),
                        ));
                    }
                    let dir = Dir::open_ambient_dir(&root, ambient_authority())?;
                    Ok((root, dir))
                })
                .collect::<std::result::Result<Vec<_>, std::io::Error>>()?
        } else {
            Vec::new()
        };
        let store = Store::open(db_path).map_err(GraphError::from)?;
        let workspace_sha256 = Self::path_sha256(&workspace);
        let store_sha256 = Self::path_sha256(&fs::canonicalize(db_path)?);
        let owned_localhost_scope = format!("{workspace_sha256}:{store_sha256}");
        Ok(Self {
            store,
            workspace,
            workspace_dir,
            config,
            developer: DeveloperAccessState {
                grant: developer_access,
                roots: developer_roots,
                dirs: developer_dirs,
            },
            owned_localhost: crate::owned_localhost::registry_for(
                owned_localhost_scope.clone(),
                workspace_sha256,
            ),
            owned_localhost_scope,
            session_id: uuid::Uuid::new_v4(),
        })
    }

    pub fn workspace_path(&self) -> &Path {
        &self.workspace
    }

    /// Stable identity used by project-bound effects to prevent cross-root replay.
    pub fn workspace_sha256(&self) -> String {
        Self::path_sha256(&self.workspace)
    }

    pub fn canonical_workspace_sha256(path: &Path) -> Result<String> {
        Ok(Self::path_sha256(&fs::canonicalize(path)?))
    }

    pub(crate) fn verify_workspace_sha256(&self, expected: &str) -> Result<()> {
        if expected.len() != 64 || expected != self.workspace_sha256() {
            return Err(RuntimeError::PathEscape(
                "project effect workspace identity does not match the active runtime root".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn path_sha256(path: &Path) -> String {
        format!("{:x}", Sha256::digest(path.to_string_lossy().as_bytes()))
    }

    /// Resolve a developer-only absolute path to one of the capability-bound
    /// directory handles. Existing symlinks and the nearest existing parent
    /// are canonicalized before the root check, so a link cannot redirect a
    /// write outside the selected scope.
    pub(crate) fn resolve_developer_absolute_path(
        &self,
        requested: &str,
    ) -> Result<(&Dir, PathBuf)> {
        let path = Path::new(requested);
        if !path.is_absolute() {
            return Err(RuntimeError::PathEscape(format!(
                "developer path must be absolute: {requested}"
            )));
        }
        if self.developer.dirs.is_empty() {
            return Err(RuntimeError::PathEscape(
                "Developer Full Access has no writable roots".into(),
            ));
        }

        let canonical_target = if path.exists() {
            fs::canonicalize(path)?
        } else {
            let file_name = path.file_name().ok_or_else(|| {
                RuntimeError::PathEscape(format!("invalid developer path: {requested}"))
            })?;
            let parent = path.parent().ok_or_else(|| {
                RuntimeError::PathEscape(format!("invalid developer path: {requested}"))
            })?;
            fs::canonicalize(parent)?.join(file_name)
        };
        let (index, root) = self
            .developer
            .dirs
            .iter()
            .enumerate()
            .filter(|(_, (root, _))| paths_equal_or_under(&canonical_target, root))
            .max_by_key(|(_, (root, _))| root.components().count())
            .ok_or_else(|| {
                RuntimeError::PathEscape(format!(
                    "developer path is outside the active scope: {requested}"
                ))
            })?;
        let relative = canonical_target
            .strip_prefix(&root.0)
            .map_err(|_| RuntimeError::PathEscape(format!("path is outside root: {requested}")))?
            .to_path_buf();
        if relative.as_os_str().is_empty() {
            return Err(RuntimeError::PathEscape(
                "the developer root itself is not a file target".into(),
            ));
        }
        if !self.developer_secrets_allowed()
            && relative
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(crate::is_secret_basename)
        {
            return Err(RuntimeError::PathEscape(format!(
                "secret path denied: {requested}"
            )));
        }
        Ok((&self.developer.dirs[index].1, relative))
    }

    pub(crate) fn resolve_effect_path(&self, requested: &str) -> Result<(&Dir, PathBuf)> {
        if Path::new(requested).is_absolute() {
            return self.resolve_developer_absolute_path(requested);
        }
        Ok((&self.workspace_dir, self.safe_relative_path(requested)?))
    }

    pub(crate) fn effect_absolute_path(&self, requested: &str) -> Result<PathBuf> {
        if Path::new(requested).is_absolute() {
            let (dir, relative) = self.resolve_developer_absolute_path(requested)?;
            let root = self
                .developer
                .dirs
                .iter()
                .find(|(_, candidate)| std::ptr::eq(candidate, dir))
                .map(|(root, _)| root)
                .ok_or_else(|| RuntimeError::PathEscape("developer root disappeared".into()))?;
            return Ok(root.join(relative));
        }
        Ok(self.workspace.join(self.safe_relative_path(requested)?))
    }

    pub fn developer_secrets_allowed(&self) -> bool {
        self.developer.grant.as_ref().is_some_and(|grant| {
            grant.enabled && grant.capabilities.workspace_files && grant.capabilities.secrets
        })
    }
}

fn paths_equal_or_under(path: &Path, root: &Path) -> bool {
    path == root || path.strip_prefix(root).is_ok()
}
