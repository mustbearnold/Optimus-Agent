//! Durable, user-mediated project filesystem authority.
//!
//! Renderer project catalogs are presentation state. A root becomes available
//! to tools only after the native folder picker stages an opaque, short-lived
//! grant and the project dialog consumes that grant for one project identity.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    atomic_write_user_only, harden_user_only, is_denied_name, verify_user_only, KernelError, Result,
};

pub const PROJECT_AUTHORITY_VERSION: u32 = 1;
const AUTHORITY_FILE: &str = "project-authority.json";
const AUTHORITY_LOCK_FILE: &str = "project-authority.lock";
const ROOT_GRANT_TTL_SECONDS: u64 = 10 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRootSelection {
    pub path: String,
    pub grant_token: String,
    pub expires_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectScope {
    pub project_id: String,
    pub roots: Vec<PathBuf>,
    pub primary_root: PathBuf,
    pub updated_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StagedRoot {
    token_sha256: String,
    path: PathBuf,
    expires_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthorityDocument {
    version: u32,
    #[serde(default)]
    projects: BTreeMap<String, ProjectScope>,
    #[serde(default)]
    staged_roots: Vec<StagedRoot>,
}

impl Default for AuthorityDocument {
    fn default() -> Self {
        Self {
            version: PROJECT_AUTHORITY_VERSION,
            projects: BTreeMap::new(),
            staged_roots: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectAuthorityStore {
    home: PathBuf,
}

impl ProjectAuthorityStore {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let home = home.as_ref().to_path_buf();
        fs::create_dir_all(&home)?;
        let store = Self { home };
        let _lock = store.lock_mutation()?;
        if !store.path().exists() {
            store.save(&AuthorityDocument::default())?;
        } else {
            let _ = store.load()?;
        }
        Ok(store)
    }

    pub fn stage_native_selection(&self, path: impl AsRef<Path>) -> Result<ProjectRootSelection> {
        self.stage_native_selection_at(path.as_ref(), now_unix()?)
    }

    pub fn authorize_project(
        &self,
        project_id: &str,
        desired_roots: &[String],
        primary_root: Option<&str>,
        grant_tokens: &[String],
    ) -> Result<Option<ProjectScope>> {
        self.authorize_project_at(
            project_id,
            desired_roots,
            primary_root,
            grant_tokens,
            now_unix()?,
        )
    }

    pub fn scope(&self, project_id: &str) -> Result<Option<ProjectScope>> {
        validate_project_id(project_id)?;
        Ok(self.load()?.projects.get(project_id).cloned())
    }

    pub fn list_scopes(&self) -> Result<Vec<ProjectScope>> {
        Ok(self.load()?.projects.into_values().collect())
    }

    fn stage_native_selection_at(&self, path: &Path, now: u64) -> Result<ProjectRootSelection> {
        let canonical = canonical_project_root(path, &self.home)?;
        let token = Uuid::new_v4().simple().to_string();
        let expires_unix = now
            .checked_add(ROOT_GRANT_TTL_SECONDS)
            .ok_or_else(|| KernelError::Tool("project root grant expiry overflow".into()))?;
        let _lock = self.lock_mutation()?;
        let mut document = self.load()?;
        document
            .staged_roots
            .retain(|grant| grant.expires_unix > now);
        document.staged_roots.push(StagedRoot {
            token_sha256: token_hash(&token),
            path: canonical.clone(),
            expires_unix,
        });
        self.save(&document)?;
        Ok(ProjectRootSelection {
            path: canonical.display().to_string(),
            grant_token: token,
            expires_unix,
        })
    }

    fn authorize_project_at(
        &self,
        project_id: &str,
        desired_roots: &[String],
        primary_root: Option<&str>,
        grant_tokens: &[String],
        now: u64,
    ) -> Result<Option<ProjectScope>> {
        validate_project_id(project_id)?;
        let _lock = self.lock_mutation()?;
        let mut document = self.load()?;
        document
            .staged_roots
            .retain(|grant| grant.expires_unix > now);

        let existing = document.projects.get(project_id).cloned();
        let existing_roots = existing
            .as_ref()
            .map(|scope| scope.roots.iter().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let mut roots = Vec::with_capacity(desired_roots.len());
        for raw in desired_roots {
            let canonical = canonical_project_root(Path::new(raw), &self.home)?;
            if !roots.contains(&canonical) {
                roots.push(canonical);
            }
        }

        if roots.is_empty() {
            document.projects.remove(project_id);
            self.save(&document)?;
            return Ok(None);
        }

        let token_hashes = grant_tokens
            .iter()
            .map(|token| token_hash(token))
            .collect::<BTreeSet<_>>();
        let mut consumed = BTreeSet::new();
        for root in &roots {
            if existing_roots.contains(root) {
                continue;
            }
            let Some(grant) = document.staged_roots.iter().find(|grant| {
                grant.path == *root
                    && grant.expires_unix > now
                    && token_hashes.contains(&grant.token_sha256)
            }) else {
                return Err(KernelError::Tool(format!(
                    "project root requires a current native folder selection: {}",
                    root.display()
                )));
            };
            consumed.insert(grant.token_sha256.clone());
        }

        let primary = canonical_project_root(
            Path::new(
                primary_root.unwrap_or(desired_roots.first().map(String::as_str).unwrap_or("")),
            ),
            &self.home,
        )?;
        if !roots.contains(&primary) {
            return Err(KernelError::Tool(
                "primary project root must be one of the authorized roots".into(),
            ));
        }

        document
            .staged_roots
            .retain(|grant| !consumed.contains(&grant.token_sha256));
        let scope = ProjectScope {
            project_id: project_id.to_string(),
            roots,
            primary_root: primary,
            updated_unix: now,
        };
        document
            .projects
            .insert(project_id.to_string(), scope.clone());
        self.save(&document)?;
        Ok(Some(scope))
    }

    fn path(&self) -> PathBuf {
        self.home.join(AUTHORITY_FILE)
    }

    fn lock_mutation(&self) -> Result<File> {
        let path = self.home.join(AUTHORITY_LOCK_FILE);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = options.open(&path)?;
        if !file.metadata()?.is_file() {
            return Err(KernelError::Tool(
                "project authority lock is not a regular file".into(),
            ));
        }
        harden_user_only(&path)?;
        verify_user_only(&path)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    fn load(&self) -> Result<AuthorityDocument> {
        verify_user_only(&self.path())?;
        let raw = fs::read(self.path())?;
        let document: AuthorityDocument = serde_json::from_slice(&raw).map_err(|error| {
            KernelError::Tool(format!("project authority parse error: {error}"))
        })?;
        if document.version != PROJECT_AUTHORITY_VERSION {
            return Err(KernelError::Tool(format!(
                "unsupported project authority version: {}",
                document.version
            )));
        }
        Ok(document)
    }

    fn save(&self, document: &AuthorityDocument) -> Result<()> {
        let body = serde_json::to_vec_pretty(document)?;
        atomic_write_user_only(&self.path(), &body)
    }
}

fn canonical_project_root(path: &Path, authority_home: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(KernelError::Tool("project root cannot be empty".into()));
    }
    let canonical = fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(KernelError::Tool(format!(
            "project root is not a directory: {}",
            canonical.display()
        )));
    }
    if canonical.parent().is_none() {
        return Err(KernelError::Tool(
            "filesystem root cannot be a project root".into(),
        ));
    }
    if canonical.to_str().is_none() {
        return Err(KernelError::Tool(
            "project root must use a Unicode-compatible path".into(),
        ));
    }
    let canonical_authority_home = fs::canonicalize(authority_home)?;
    if canonical == canonical_authority_home || canonical_authority_home.starts_with(&canonical) {
        return Err(KernelError::Tool(
            "runtime home or one of its ancestors cannot be a project root".into(),
        ));
    }
    if canonical
        .components()
        .any(|component| component.as_os_str().to_str().is_some_and(is_denied_name))
    {
        return Err(KernelError::Tool(
            "secret-bearing path cannot be a project root".into(),
        ));
    }
    Ok(canonical)
}

/// Narrow an authorized project scope down to one engineering run's checkout
/// (ADR-0052 §2).
///
/// The worktree becomes the run's whole filesystem world: not one root among
/// several, but the only one. Everything the project authorized stays outside
/// it, including the main checkout the human is using.
///
/// A run cannot widen its own authority this way — the worktree has to already
/// lie *strictly inside* something the user authorized, so a run record naming
/// `/etc` or naming the project root itself is refused rather than honoured.
pub(crate) fn dev_run_scope(
    scope: &ProjectScope,
    worktree: &Path,
    home: &Path,
) -> Result<ProjectScope> {
    let canonical = canonical_project_root(worktree, home)?;
    let contained = scope.roots.iter().any(|root| {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        canonical.starts_with(&root) && canonical != root
    });
    if !contained {
        return Err(KernelError::Tool(format!(
            "engineering worktree {} is not inside an authorized root of project {}",
            canonical.display(),
            scope.project_id
        )));
    }
    Ok(ProjectScope {
        project_id: scope.project_id.clone(),
        roots: vec![canonical.clone()],
        primary_root: canonical,
        updated_unix: scope.updated_unix,
    })
}

/// Workspace and filesystem roots for a session bound to a project, optionally
/// narrowed to one engineering run's checkout.
pub(crate) fn session_roots(
    home: &Path,
    project_id: &str,
    dev_worktree: Option<&Path>,
) -> Result<(PathBuf, Vec<PathBuf>)> {
    let scope = ProjectAuthorityStore::open(home)?
        .scope(project_id)?
        .ok_or_else(|| {
            KernelError::Tool(format!(
                "project {project_id} has no runtime-authorized root"
            ))
        })?;
    let scope = match dev_worktree {
        Some(worktree) => dev_run_scope(&scope, worktree, home)?,
        None => scope,
    };
    Ok((scope.primary_root, scope.roots))
}

fn validate_project_id(project_id: &str) -> Result<()> {
    if project_id.is_empty()
        || project_id.len() > 128
        || !project_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(KernelError::Tool("invalid project identity".into()));
    }
    Ok(())
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn now_unix() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| KernelError::Tool(format!("system clock before epoch: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use tempfile::tempdir;

    #[test]
    fn native_selection_is_required_once_and_scope_persists() {
        let home = tempdir().unwrap();
        let root = tempdir().unwrap();
        let store = ProjectAuthorityStore::open(home.path()).unwrap();
        let selection = store.stage_native_selection_at(root.path(), 100).unwrap();

        let scope = store
            .authorize_project_at(
                "project-a",
                std::slice::from_ref(&selection.path),
                Some(&selection.path),
                std::slice::from_ref(&selection.grant_token),
                101,
            )
            .unwrap()
            .unwrap();

        assert_eq!(scope.primary_root, fs::canonicalize(root.path()).unwrap());
        assert_eq!(
            ProjectAuthorityStore::open(home.path())
                .unwrap()
                .scope("project-a")
                .unwrap(),
            Some(scope)
        );

        let other = tempdir().unwrap();
        let error = store
            .authorize_project_at(
                "project-a",
                &[selection.path, other.path().display().to_string()],
                Some(&other.path().display().to_string()),
                &[selection.grant_token],
                102,
            )
            .unwrap_err();
        assert!(error.to_string().contains("native folder selection"));
    }

    #[test]
    fn fabricated_or_expired_grants_fail_without_mutating_scope() {
        let home = tempdir().unwrap();
        let root = tempdir().unwrap();
        let store = ProjectAuthorityStore::open(home.path()).unwrap();
        let root_text = root.path().display().to_string();

        assert!(store
            .authorize_project_at(
                "project-a",
                std::slice::from_ref(&root_text),
                Some(&root_text),
                &["fabricated".into()],
                100,
            )
            .is_err());
        assert!(store.scope("project-a").unwrap().is_none());

        let selection = store.stage_native_selection_at(root.path(), 100).unwrap();
        assert!(store
            .authorize_project_at(
                "project-a",
                std::slice::from_ref(&selection.path),
                Some(&selection.path),
                &[selection.grant_token],
                100 + ROOT_GRANT_TTL_SECONDS + 1,
            )
            .is_err());
        assert!(store.scope("project-a").unwrap().is_none());
    }

    #[test]
    fn authorized_scope_can_be_narrowed_without_another_grant() {
        let home = tempdir().unwrap();
        let first = tempdir().unwrap();
        let second = tempdir().unwrap();
        let store = ProjectAuthorityStore::open(home.path()).unwrap();
        let first = store.stage_native_selection_at(first.path(), 100).unwrap();
        let second = store.stage_native_selection_at(second.path(), 100).unwrap();
        store
            .authorize_project_at(
                "project-a",
                &[first.path.clone(), second.path.clone()],
                Some(&first.path),
                &[first.grant_token, second.grant_token],
                101,
            )
            .unwrap();

        let narrowed = store
            .authorize_project_at(
                "project-a",
                std::slice::from_ref(&first.path),
                Some(&first.path),
                &[],
                102,
            )
            .unwrap()
            .unwrap();
        assert_eq!(narrowed.roots.len(), 1);
        assert_eq!(narrowed.primary_root, PathBuf::from(first.path));
    }

    #[test]
    fn primary_root_must_be_authorized() {
        let home = tempdir().unwrap();
        let first = tempdir().unwrap();
        let second = tempdir().unwrap();
        let store = ProjectAuthorityStore::open(home.path()).unwrap();
        let selection = store.stage_native_selection_at(first.path(), 100).unwrap();

        let error = store
            .authorize_project_at(
                "project-a",
                std::slice::from_ref(&selection.path),
                Some(&second.path().display().to_string()),
                &[selection.grant_token],
                101,
            )
            .unwrap_err();
        assert!(error.to_string().contains("primary project root"));
    }

    #[test]
    fn concurrent_native_selections_do_not_overwrite_each_other() {
        let home = tempdir().unwrap();
        let first_root = tempdir().unwrap();
        let second_root = tempdir().unwrap();
        let store = ProjectAuthorityStore::open(home.path()).unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let first = {
            let store = store.clone();
            let barrier = Arc::clone(&barrier);
            let root = first_root.path().to_path_buf();
            std::thread::spawn(move || {
                barrier.wait();
                store.stage_native_selection_at(&root, 100).unwrap()
            })
        };
        let second = {
            let store = store.clone();
            let barrier = Arc::clone(&barrier);
            let root = second_root.path().to_path_buf();
            std::thread::spawn(move || {
                barrier.wait();
                store.stage_native_selection_at(&root, 100).unwrap()
            })
        };
        barrier.wait();
        let first = first.join().unwrap();
        let second = second.join().unwrap();

        let scope = store
            .authorize_project_at(
                "project-a",
                &[first.path.clone(), second.path.clone()],
                Some(&first.path),
                &[first.grant_token, second.grant_token],
                101,
            )
            .unwrap()
            .unwrap();
        assert_eq!(scope.roots.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn authority_document_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let home = tempdir().unwrap();
        let external = tempdir().unwrap();
        let target = external.path().join("authority.json");
        fs::write(
            &target,
            serde_json::to_vec(&AuthorityDocument::default()).unwrap(),
        )
        .unwrap();
        symlink(&target, home.path().join(AUTHORITY_FILE)).unwrap();

        let error = ProjectAuthorityStore::open(home.path()).unwrap_err();
        assert!(error.to_string().contains("not a regular file"));
    }
}
