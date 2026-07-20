//! Filesystem sandbox — path allowlist for desktop Files IPC.
//!
//! All FS tool access must resolve through [`FsRoots`]: canonicalize + prefix
//! check, reject `..` / symlink escape, and deny secret basenames unless
//! explicitly granted.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use optimus_runtime::is_secret_basename as is_denied_name;

/// Dedicated sandbox errors (map to IPC / [`crate::KernelError`] at the boundary).
#[derive(Debug, Error)]
pub enum FsSandboxError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("path not allowed: {0}")]
    Denied(String),
    #[error("secret path denied: {0}")]
    SecretDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("not a directory: {0}")]
    NotDirectory(String),
    #[error("invalid root: {0}")]
    InvalidRoot(String),
    #[error("empty roots")]
    EmptyRoots,
}

pub type Result<T> = std::result::Result<T, FsSandboxError>;

/// Entry kind returned by [`FsRoots::list_dir`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FsEntryKind {
    File,
    Dir,
    Symlink,
}

/// Directory listing entry (paths are relative display strings under a root).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FsEntry {
    pub name: String,
    /// Relative display path from the matching root (or absolute if rooted abs).
    pub path: String,
    pub kind: FsEntryKind,
    pub size: u64,
    pub mtime_unix: u64,
}

/// Result of a capped text read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadTextResult {
    pub content: String,
    pub truncated: bool,
}

/// Canonical allowlisted filesystem roots.
#[derive(Debug, Clone)]
pub struct FsRoots {
    roots: Vec<PathBuf>,
}

impl FsRoots {
    /// Canonicalize each existing directory root. Rejects empty list and non-dirs.
    pub fn new(roots: Vec<PathBuf>) -> Result<Self> {
        if roots.is_empty() {
            return Err(FsSandboxError::EmptyRoots);
        }
        let mut out = Vec::with_capacity(roots.len());
        for r in roots {
            if !r.exists() {
                return Err(FsSandboxError::InvalidRoot(format!(
                    "does not exist: {}",
                    r.display()
                )));
            }
            if !r.is_dir() {
                return Err(FsSandboxError::InvalidRoot(format!(
                    "not a directory: {}",
                    r.display()
                )));
            }
            let canon = fs::canonicalize(&r)
                .map_err(|e| FsSandboxError::InvalidRoot(format!("{}: {e}", r.display())))?;
            out.push(canon);
        }
        Ok(Self { roots: out })
    }

    /// Default roots for an Optimus home directory (home itself).
    pub fn from_home(home: &Path) -> Result<Self> {
        Self::new(vec![home.to_path_buf()])
    }

    /// Borrow the canonical roots.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Resolve an existing path under a root (file, dir, or symlink target).
    ///
    /// Relative paths are joined to each root until one exists under allowlist.
    /// Absolute paths must already lie under some root after canonicalization.
    pub fn resolve_existing(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        let candidates = self.candidate_paths(path)?;
        let mut last_err = FsSandboxError::NotFound(path.display().to_string());

        for candidate in candidates {
            match fs::canonicalize(&candidate) {
                Ok(canon) => {
                    if self.is_under_roots(&canon) {
                        return Ok(canon);
                    }
                    last_err = FsSandboxError::Denied(path.display().to_string());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    last_err = FsSandboxError::NotFound(path.display().to_string());
                }
                Err(e) => return Err(e.into()),
            }
        }

        Err(last_err)
    }

    /// Resolve a path for create/write: parent must exist and stay under roots.
    ///
    /// Final path need not exist. Rejects `..` escape and parents outside roots.
    pub fn resolve_for_create(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        let candidates = self.candidate_paths(path)?;
        let mut last_err = FsSandboxError::Denied(path.display().to_string());

        for candidate in candidates {
            // If the path already exists, same rules as resolve_existing.
            if candidate.exists() {
                match fs::canonicalize(&candidate) {
                    Ok(canon) if self.is_under_roots(&canon) => return Ok(canon),
                    Ok(_) => {
                        last_err = FsSandboxError::Denied(path.display().to_string());
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            let parent = candidate.parent().unwrap_or_else(|| Path::new("."));
            let parent_canon = match fs::canonicalize(parent) {
                Ok(p) => p,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    last_err = FsSandboxError::NotFound(parent.display().to_string());
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            if !self.is_under_roots(&parent_canon) {
                last_err = FsSandboxError::Denied(path.display().to_string());
                continue;
            }

            let name = match candidate.file_name() {
                Some(n) => n,
                None => {
                    last_err = FsSandboxError::Denied(path.display().to_string());
                    continue;
                }
            };
            return Ok(parent_canon.join(name));
        }

        Err(last_err)
    }

    /// List a directory under the allowlist.
    pub fn list_dir(&self, rel_or_abs: impl AsRef<Path>) -> Result<Vec<FsEntry>> {
        let abs = self.resolve_existing(rel_or_abs.as_ref())?;
        let meta = fs::symlink_metadata(&abs)?;
        if !meta.is_dir() {
            return Err(FsSandboxError::NotDirectory(abs.display().to_string()));
        }

        let display_base = self.display_relative(&abs);
        let mut entries = Vec::new();

        for ent in fs::read_dir(&abs)? {
            let ent = ent?;
            let name = ent.file_name().to_string_lossy().into_owned();
            let path = if display_base.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", display_base.trim_end_matches(['/', '\\']), name)
            };

            // Prefer symlink_metadata so symlinks are not followed for kind.
            let md = match fs::symlink_metadata(ent.path()) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let ft = md.file_type();
            let kind = if ft.is_symlink() {
                FsEntryKind::Symlink
            } else if ft.is_dir() {
                FsEntryKind::Dir
            } else {
                FsEntryKind::File
            };
            let size = if md.is_file() { md.len() } else { 0 };
            let mtime_unix = mtime_to_unix(md.modified().ok());
            entries.push(FsEntry {
                name,
                path,
                kind,
                size,
                mtime_unix,
            });
        }

        entries.sort_by_key(|entry| entry.name.to_lowercase());
        Ok(entries)
    }

    /// Read a UTF-8-lossy text file under the allowlist, capped at `max_bytes`.
    ///
    /// Denies secret basenames unless `allow_secrets` is true.
    pub fn read_text(
        &self,
        path: impl AsRef<Path>,
        max_bytes: usize,
        allow_secrets: bool,
    ) -> Result<ReadTextResult> {
        let path = path.as_ref();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if !allow_secrets && is_denied_name(name) {
                return Err(FsSandboxError::SecretDenied(name.to_string()));
            }
        }

        let abs = self.resolve_existing(path)?;
        if let Some(name) = abs.file_name().and_then(|n| n.to_str()) {
            if !allow_secrets && is_denied_name(name) {
                return Err(FsSandboxError::SecretDenied(name.to_string()));
            }
        }

        let meta = fs::metadata(&abs)?;
        if meta.is_dir() {
            return Err(FsSandboxError::NotDirectory(abs.display().to_string()));
        }

        let data = fs::read(&abs)?;
        let truncated = data.len() > max_bytes;
        let slice = if truncated {
            &data[..max_bytes]
        } else {
            &data[..]
        };
        let content = String::from_utf8_lossy(slice).into_owned();
        Ok(ReadTextResult { content, truncated })
    }

    fn candidate_paths(&self, path: &Path) -> Result<Vec<PathBuf>> {
        if path.as_os_str().is_empty() {
            // Empty path → each root itself
            return Ok(self.roots.clone());
        }

        if path.is_absolute() {
            return Ok(vec![path.to_path_buf()]);
        }

        // Relative: join against every root
        Ok(self.roots.iter().map(|r| r.join(path)).collect())
    }

    fn is_under_roots(&self, canon: &Path) -> bool {
        let n = normalize_lexically(canon);
        self.roots.iter().any(|root| {
            let r = normalize_lexically(root);
            paths_equal_or_under(&n, &r)
        })
    }

    fn display_relative(&self, abs: &Path) -> String {
        let n = normalize_lexically(abs);
        for root in &self.roots {
            let r = normalize_lexically(root);
            if let Ok(rel) = n.strip_prefix(&r) {
                return rel.to_string_lossy().replace('\\', "/");
            }
        }
        n.to_string_lossy().replace('\\', "/")
    }
}

fn mtime_to_unix(t: Option<SystemTime>) -> u64 {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Strip Windows `\\?\` extended prefix for prefix comparisons.
fn normalize_lexically(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        let stripped = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else if let Some(rest) = s.strip_prefix(r"\\?\") {
            rest.to_string()
        } else {
            s.into_owned()
        };
        PathBuf::from(stripped)
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn paths_equal_or_under(path: &Path, root: &Path) -> bool {
    if path == root {
        return true;
    }
    // Component-wise prefix so we don't match `C:\foo` under `C:\foobar`.
    let path_comps: Vec<_> = path.components().collect();
    let root_comps: Vec<_> = root.components().collect();
    if path_comps.len() < root_comps.len() {
        return false;
    }
    for (a, b) in path_comps.iter().zip(root_comps.iter()) {
        if !components_eq(a, b) {
            return false;
        }
    }
    true
}

fn components_eq(a: &Component<'_>, b: &Component<'_>) -> bool {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        match (a, b) {
            (Component::Prefix(ap), Component::Prefix(bp)) => {
                // Case-insensitive drive compare
                ap.as_os_str().eq_ignore_ascii_case(bp.as_os_str())
            }
            (Component::RootDir, Component::RootDir) => true,
            (Component::Normal(a), Component::Normal(b)) => {
                AsRef::<OsStr>::as_ref(a).eq_ignore_ascii_case(b)
            }
            (Component::CurDir, Component::CurDir) => true,
            (Component::ParentDir, Component::ParentDir) => true,
            _ => false,
        }
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_file(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn allow_list_under_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write_file(&root.join("notes.txt"), "hello sandbox");

        let fs = FsRoots::new(vec![root.to_path_buf()]).unwrap();
        let resolved = fs.resolve_existing("notes.txt").unwrap();
        assert!(resolved.ends_with("notes.txt"));

        let text = fs.read_text("notes.txt", 1024, false).unwrap();
        assert_eq!(text.content, "hello sandbox");
        assert!(!text.truncated);

        let entries = fs.list_dir("").unwrap();
        assert!(entries.iter().any(|e| e.name == "notes.txt"));
        let ent = entries.iter().find(|e| e.name == "notes.txt").unwrap();
        assert_eq!(ent.kind, FsEntryKind::File);
        assert_eq!(ent.path, "notes.txt");
    }

    #[test]
    fn deny_path_with_dotdot_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("jail");
        fs::create_dir_all(&root).unwrap();
        write_file(&root.join("inside.txt"), "in");
        // Sibling outside jail
        write_file(&dir.path().join("secret-outside.txt"), "OUT");

        let fs = FsRoots::new(vec![root.clone()]).unwrap();
        let err = fs
            .resolve_existing(Path::new("..").join("secret-outside.txt"))
            .unwrap_err();
        match err {
            FsSandboxError::Denied(_) | FsSandboxError::NotFound(_) => {}
            other => panic!("expected Denied/NotFound, got {other:?}"),
        }

        let err = fs
            .read_text(Path::new("..").join("secret-outside.txt"), 1024, false)
            .unwrap_err();
        assert!(
            matches!(err, FsSandboxError::Denied(_) | FsSandboxError::NotFound(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn deny_absolute_outside_root() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("jail");
        fs::create_dir_all(&root).unwrap();
        write_file(&root.join("ok.txt"), "ok");

        let outside = tempdir().unwrap();
        write_file(&outside.path().join("nope.txt"), "nope");

        let fs = FsRoots::new(vec![root]).unwrap();
        let abs_outside = outside.path().join("nope.txt");
        let err = fs.resolve_existing(&abs_outside).unwrap_err();
        assert!(
            matches!(err, FsSandboxError::Denied(_)),
            "expected Denied, got {err:?}"
        );
    }

    #[test]
    fn deny_reading_env_by_default() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join(".env"), "SECRET=1");
        write_file(&dir.path().join("readme.md"), "public");

        let fs = FsRoots::from_home(dir.path()).unwrap();

        assert!(is_denied_name(".env"));
        assert!(is_denied_name(".ENV"));
        assert!(is_denied_name("auth.json"));
        assert!(is_denied_name("Auth.JSON"));
        assert!(is_denied_name("key.pem"));
        assert!(is_denied_name("id_rsa"));
        assert!(is_denied_name(".netrc"));
        assert!(!is_denied_name("readme.md"));
        assert!(!is_denied_name("env.txt"));

        let err = fs.read_text(".env", 1024, false).unwrap_err();
        assert!(
            matches!(err, FsSandboxError::SecretDenied(_)),
            "got {err:?}"
        );

        // Grant flag allows secrets under root
        let ok = fs.read_text(".env", 1024, true).unwrap();
        assert_eq!(ok.content, "SECRET=1");

        let normal = fs.read_text("readme.md", 1024, false).unwrap();
        assert_eq!(normal.content, "public");
    }

    #[test]
    fn allow_reading_normal_file() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), "abcdefghij");
        let fs = FsRoots::new(vec![dir.path().to_path_buf()]).unwrap();

        let full = fs.read_text("a.txt", 100, false).unwrap();
        assert_eq!(full.content, "abcdefghij");
        assert!(!full.truncated);

        let trunc = fs.read_text("a.txt", 4, false).unwrap();
        assert_eq!(trunc.content, "abcd");
        assert!(trunc.truncated);
    }

    #[test]
    fn resolve_for_create_under_root() {
        let dir = tempdir().unwrap();
        let fs = FsRoots::new(vec![dir.path().to_path_buf()]).unwrap();
        let target = fs.resolve_for_create("new/file.txt").unwrap_err();
        // parent new/ does not exist yet
        assert!(matches!(
            target,
            FsSandboxError::NotFound(_) | FsSandboxError::Denied(_)
        ));

        fs::create_dir_all(dir.path().join("new")).unwrap();
        let created = fs.resolve_for_create("new/file.txt").unwrap();
        assert!(created.ends_with("file.txt"));

        let outside = tempdir().unwrap();
        let err = fs
            .resolve_for_create(outside.path().join("x.txt"))
            .unwrap_err();
        assert!(matches!(err, FsSandboxError::Denied(_)), "got {err:?}");
    }

    #[test]
    fn deny_symlink_escape_when_supported() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("jail");
        fs::create_dir_all(&root).unwrap();
        write_file(&root.join("inside.txt"), "in");

        let outside = dir.path().join("outside.txt");
        write_file(&outside, "OUTSIDE");

        let link = root.join("escape_link");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &link).unwrap();
        }
        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_file(&outside, &link).is_err() {
                // Symlink creation often needs elevation on Windows — skip.
                eprintln!("skip symlink escape test: cannot create symlink");
                return;
            }
        }

        let fs = FsRoots::new(vec![root]).unwrap();
        let err = fs.resolve_existing("escape_link").unwrap_err();
        assert!(
            matches!(err, FsSandboxError::Denied(_)),
            "symlink escape should be Denied, got {err:?}"
        );
    }

    #[test]
    fn list_dir_nested_relative_paths() {
        let dir = tempdir().unwrap();
        write_file(&dir.path().join("sub").join("b.txt"), "b");
        let fs = FsRoots::new(vec![dir.path().to_path_buf()]).unwrap();
        let entries = fs.list_dir("sub").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "b.txt");
        assert_eq!(entries[0].path, "sub/b.txt");
        assert_eq!(entries[0].kind, FsEntryKind::File);
    }
}
