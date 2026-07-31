//! Canonical workspace identity shared by project-bound runtime effects.

use std::fs;
use std::path::Path;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use optimus_graph::{GraphError, RuntimeConfig};
use optimus_store::Store;
use sha2::{Digest, Sha256};

use crate::{Result, Runtime, RuntimeError};

impl Runtime {
    pub fn open(db_path: &Path, workspace: &Path) -> Result<Self> {
        Self::open_with_config(db_path, workspace, RuntimeConfig::default())
    }

    pub fn open_with_config(
        db_path: &Path,
        workspace: &Path,
        config: RuntimeConfig,
    ) -> Result<Self> {
        fs::create_dir_all(workspace)?;
        let workspace = fs::canonicalize(workspace)?;
        let workspace_dir = Dir::open_ambient_dir(&workspace, ambient_authority())?;
        let store = Store::open(db_path).map_err(GraphError::from)?;
        let workspace_sha256 = Self::path_sha256(&workspace);
        let store_sha256 = Self::path_sha256(&fs::canonicalize(db_path)?);
        let owned_localhost_scope = format!("{workspace_sha256}:{store_sha256}");
        Ok(Self {
            store,
            workspace,
            workspace_dir,
            config,
            owned_localhost: crate::owned_localhost::registry_for(
                owned_localhost_scope.clone(),
                workspace_sha256,
            ),
            owned_localhost_scope,
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
}
