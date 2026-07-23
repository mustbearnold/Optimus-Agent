//! Content-addressed artifact store under `{home}/artifacts`.
//!
//! Blobs live at `blobs/<sha256[0..2]>/<sha256>`. Metadata is append-only JSONL
//! in `index.jsonl`. Same bytes always map to the same SHA-256 path.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{KernelError, Result};

const MAX_LIST: usize = 200;
const MAX_BULK_DELETE: usize = 50;
const MAX_LABEL: usize = 256;
const MAX_SOURCE: usize = 128;
const MAX_MEDIA_TYPE: usize = 128;
const MAX_BYTES: usize = 12 * 1024 * 1024; // 12 MiB
const MAX_BASE64_INPUT: usize = ((MAX_BYTES + 2) / 3) * 4 + 8_192;

/// Per-item bulk-delete outcomes under one exclusive store lock.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkDeleteResult {
    pub deleted: Vec<String>,
    pub failed: Vec<BulkDeleteFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkDeleteFailure {
    pub sha256: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub sha256: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub source: String,
    pub label: String,
    pub created_at_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

pub struct ArtifactStore {
    root: PathBuf,
    index_path: PathBuf,
    lock_path: PathBuf,
}

impl ArtifactStore {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let root = home.as_ref().join("artifacts");
        ensure_owned_directory(&root, "artifact root")?;
        ensure_owned_directory(&root.join("blobs"), "artifact blob root")?;
        let index_path = root.join("index.jsonl");
        let lock_path = root.join("store.lock");
        open_owned_file(&index_path, false)?;
        open_owned_file(&lock_path, false)?;
        Ok(Self {
            root,
            index_path,
            lock_path,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn put_bytes(
        &self,
        bytes: &[u8],
        media_type: &str,
        source: &str,
        label: &str,
        session_id: Option<&str>,
    ) -> Result<ArtifactRecord> {
        if bytes.is_empty() {
            return Err(KernelError::Tool("artifact bytes must be non-empty".into()));
        }
        if bytes.len() > MAX_BYTES {
            return Err(KernelError::Tool(format!(
                "artifact exceeds max size {MAX_BYTES} bytes"
            )));
        }
        let media_type = sanitize_field(media_type, MAX_MEDIA_TYPE, "application/octet-stream");
        let source = sanitize_field(source, MAX_SOURCE, "unknown");
        let label = sanitize_field(label, MAX_LABEL, "artifact");
        let sha256 = hex_sha256(bytes);
        let _lock = self.lock_exclusive()?;
        let blob_path = self.blob_path(&sha256);
        if let Some(parent) = blob_path.parent() {
            ensure_owned_directory(parent, "artifact blob shard")?;
        }
        self.publish_blob_unlocked(&blob_path, bytes, &sha256)?;

        let record = ArtifactRecord {
            sha256: sha256.clone(),
            media_type,
            size_bytes: bytes.len() as u64,
            source,
            label,
            created_at_unix: now_unix(),
            session_id: session_id
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(128).collect()),
            deleted: false,
        };
        self.append_index_unlocked(&record)?;
        Ok(record)
    }

    /// Decode standard base64 (with optional whitespace) and store the payload.
    pub fn put_base64(
        &self,
        b64: &str,
        media_type: &str,
        source: &str,
        label: &str,
        session_id: Option<&str>,
    ) -> Result<ArtifactRecord> {
        use base64::Engine;
        if b64.len() > MAX_BASE64_INPUT {
            return Err(KernelError::Tool(format!(
                "base64 payload exceeds max encoded size {MAX_BASE64_INPUT} bytes"
            )));
        }
        let encoded_chars = b64.chars().filter(|c| !c.is_whitespace()).count();
        if encoded_chars > ((MAX_BYTES + 2) / 3) * 4 {
            return Err(KernelError::Tool(format!(
                "base64 payload exceeds max decoded size {MAX_BYTES} bytes"
            )));
        }
        let cleaned: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(cleaned.as_bytes())
            .map_err(|e| KernelError::Tool(format!("invalid base64 artifact payload: {e}")))?;
        self.put_bytes(&bytes, media_type, source, label, session_id)
    }

    pub fn list(&self) -> Result<Vec<ArtifactRecord>> {
        let _lock = self.lock_shared()?;
        let mut rows = self.read_rows_unlocked()?;
        // Newest first; keep at most MAX_LIST unique sha256 (first wins after reverse).
        rows.reverse();
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for row in rows {
            if seen.insert(row.sha256.clone()) && !row.deleted {
                out.push(row);
            }
            if out.len() >= MAX_LIST {
                break;
            }
        }
        Ok(out)
    }

    pub fn get_bytes(&self, sha256: &str) -> Result<Vec<u8>> {
        validate_sha256(sha256)?;
        let _lock = self.lock_shared()?;
        let path = self.blob_path(sha256);
        self.read_verified_blob_unlocked(&path, sha256)
    }

    /// Latest index metadata for a sha256 (newest wins).
    pub fn get_meta(&self, sha256: &str) -> Result<ArtifactRecord> {
        validate_sha256(sha256)?;
        let _lock = self.lock_shared()?;
        self.get_meta_unlocked(sha256)
    }

    fn get_meta_unlocked(&self, sha256: &str) -> Result<ArtifactRecord> {
        let rows = self.read_rows_unlocked()?;
        let mut found: Option<ArtifactRecord> = None;
        for row in rows {
            if row.sha256 == sha256 {
                found = Some(row);
            }
        }
        match found {
            Some(row) if !row.deleted => Ok(row),
            _ => Err(KernelError::Tool(format!(
                "artifact metadata not found: {sha256}"
            ))),
        }
    }

    /// Encode blob as standard base64 for UI transport.
    pub fn get_base64(&self, sha256: &str) -> Result<String> {
        use base64::Engine;
        let bytes = self.get_bytes(sha256)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    /// Append a deletion tombstone and remove blob bytes for `sha256`.
    pub fn delete(&self, sha256: &str) -> Result<()> {
        let result = self.delete_many(std::slice::from_ref(&sha256.to_string()))?;
        if let Some(fail) = result.failed.first() {
            return Err(KernelError::Tool(fail.error.clone()));
        }
        if result.deleted.is_empty() {
            return Err(KernelError::Tool(format!("artifact not found: {sha256}")));
        }
        Ok(())
    }

    /// Delete up to [`MAX_BULK_DELETE`] artifacts under one exclusive lock.
    ///
    /// Duplicates are collapsed. Each sha is best-effort: missing items land in
    /// `failed` without aborting the rest of the batch.
    pub fn delete_many(&self, sha256s: &[String]) -> Result<BulkDeleteResult> {
        if sha256s.is_empty() {
            return Err(KernelError::Tool(
                "artifacts_delete_many requires at least one sha256".into(),
            ));
        }
        if sha256s.len() > MAX_BULK_DELETE {
            return Err(KernelError::Tool(format!(
                "artifacts_delete_many supports at most {MAX_BULK_DELETE} items"
            )));
        }
        let mut ordered = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for raw in sha256s {
            let sha = raw.trim();
            if sha.is_empty() || !seen.insert(sha.to_string()) {
                continue;
            }
            ordered.push(sha.to_string());
        }
        if ordered.is_empty() {
            return Err(KernelError::Tool(
                "artifacts_delete_many requires at least one sha256".into(),
            ));
        }
        let _lock = self.lock_exclusive()?;
        let mut deleted = Vec::new();
        let mut failed = Vec::new();
        for sha in ordered {
            match self.delete_unlocked(&sha) {
                Ok(()) => deleted.push(sha),
                Err(e) => failed.push(BulkDeleteFailure {
                    sha256: sha,
                    error: e.to_string(),
                }),
            }
        }
        Ok(BulkDeleteResult { deleted, failed })
    }

    fn delete_unlocked(&self, sha256: &str) -> Result<()> {
        validate_sha256(sha256)?;
        let meta = self.get_meta_unlocked(sha256).ok();
        let path = self.blob_path(sha256);
        let blob_existed = path.try_exists()?;
        if meta.is_none() && !blob_existed {
            return Err(KernelError::Tool(format!("artifact not found: {sha256}")));
        }
        let mut tombstone = meta.unwrap_or_else(|| ArtifactRecord {
            sha256: sha256.to_string(),
            media_type: "application/octet-stream".into(),
            size_bytes: 0,
            source: "delete".into(),
            label: "deleted artifact".into(),
            created_at_unix: now_unix(),
            session_id: None,
            deleted: true,
        });
        tombstone.deleted = true;
        tombstone.created_at_unix = now_unix();
        self.append_index_unlocked(&tombstone)?;
        if blob_existed {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(KernelError::Tool("refusing symlinked artifact blob".into()));
            }
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn blob_path(&self, sha256: &str) -> PathBuf {
        self.root
            .join("blobs")
            .join(&sha256[..2.min(sha256.len())])
            .join(sha256)
    }

    fn append_index_unlocked(&self, record: &ArtifactRecord) -> Result<()> {
        reject_symlink(&self.index_path, "artifact index")?;
        let mut file = OpenOptions::new().append(true).open(&self.index_path)?;
        let line = serde_json::to_string(record)?;
        writeln!(file, "{line}")?;
        file.sync_data()?;
        Ok(())
    }

    fn read_rows_unlocked(&self) -> Result<Vec<ArtifactRecord>> {
        reject_symlink(&self.index_path, "artifact index")?;
        let file = File::open(&self.index_path)?;
        let reader = BufReader::new(file);
        let mut rows = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(row) = serde_json::from_str::<ArtifactRecord>(line) {
                if validate_sha256(&row.sha256).is_ok() {
                    rows.push(row);
                }
            }
        }
        Ok(rows)
    }

    fn lock_shared(&self) -> Result<File> {
        let file = open_owned_file(&self.lock_path, false)?;
        FileExt::lock_shared(&file)?;
        Ok(file)
    }

    fn lock_exclusive(&self) -> Result<File> {
        let file = open_owned_file(&self.lock_path, false)?;
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }

    fn publish_blob_unlocked(&self, path: &Path, bytes: &[u8], sha256: &str) -> Result<()> {
        if path.try_exists()? {
            self.read_verified_blob_unlocked(path, sha256)?;
            return Ok(());
        }
        let parent = path
            .parent()
            .ok_or_else(|| KernelError::Tool("artifact blob has no parent".into()))?;
        let temp = parent.join(format!(".{sha256}.{}.tmp", Uuid::new_v4()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        match fs::hard_link(&temp, path) {
            Ok(()) => {
                fs::remove_file(&temp)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                fs::remove_file(&temp)?;
                self.read_verified_blob_unlocked(path, sha256)?;
            }
            Err(error) => {
                let _ = fs::remove_file(&temp);
                return Err(error.into());
            }
        }
        self.read_verified_blob_unlocked(path, sha256)?;
        Ok(())
    }

    fn read_verified_blob_unlocked(&self, path: &Path, sha256: &str) -> Result<Vec<u8>> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(KernelError::Tool(format!("artifact not found: {sha256}")));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(KernelError::Tool("refusing symlinked artifact blob".into()));
        }
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
            return Err(KernelError::Tool(
                "artifact blob is not a bounded regular file".into(),
            ));
        }
        let mut file = File::open(path)?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        std::io::Read::by_ref(&mut file)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err(KernelError::Tool(format!(
                "artifact exceeds max size {MAX_BYTES} bytes"
            )));
        }
        if hex_sha256(&bytes) != sha256 {
            return Err(KernelError::Tool("artifact digest mismatch".into()));
        }
        Ok(bytes)
    }
}

fn ensure_owned_directory(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(KernelError::Tool(format!(
                    "{label} must be a non-symlink directory"
                )));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match fs::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(path)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(KernelError::Tool(format!(
                        "{label} must be a non-symlink directory"
                    )));
                }
            }
            Err(error) => return Err(error.into()),
        },
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn reject_symlink(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(KernelError::Tool(format!(
            "{label} must be a non-symlink file"
        )));
    }
    Ok(())
}

fn open_owned_file(path: &Path, append: bool) -> Result<File> {
    loop {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(KernelError::Tool(
                        "artifact state file must not be a symlink".into(),
                    ));
                }
                return Ok(OpenOptions::new()
                    .read(true)
                    .write(true)
                    .append(append)
                    .open(path)?);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .append(append)
                    .create_new(true)
                    .open(path)
                {
                    Ok(file) => return Ok(file),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(KernelError::Tool(
            "artifact sha256 must be 64 hex characters".into(),
        ));
    }
    Ok(())
}

fn sanitize_field(value: &str, max: usize, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    trimmed.chars().take(max).collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn put_is_content_addressed_and_idempotent() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let a = store
            .put_bytes(b"hello artifact", "text/plain", "test", "hello", None)
            .unwrap();
        let b = store
            .put_bytes(b"hello artifact", "text/plain", "test", "hello-again", None)
            .unwrap();
        assert_eq!(a.sha256, b.sha256);
        assert_eq!(a.size_bytes, 14);
        assert!(store.blob_path(&a.sha256).is_file());
        let bytes = store.get_bytes(&a.sha256).unwrap();
        assert_eq!(bytes, b"hello artifact");
    }

    #[test]
    fn list_returns_newest_unique_first() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        store
            .put_bytes(b"one", "text/plain", "src", "one", None)
            .unwrap();
        store
            .put_bytes(b"two", "text/plain", "src", "two", None)
            .unwrap();
        // re-publish one with new label — still one unique sha
        store
            .put_bytes(b"one", "text/plain", "src", "one-relabel", None)
            .unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].label, "one-relabel");
        assert_eq!(list[1].label, "two");
    }

    #[test]
    fn rejects_empty_and_bad_sha() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        assert!(store
            .put_bytes(b"", "text/plain", "src", "empty", None)
            .is_err());
        assert!(store.get_bytes("not-a-hash").is_err());
    }

    #[test]
    fn get_meta_and_base64_roundtrip() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let rec = store
            .put_bytes(b"png-ish", "image/png", "browser.screenshot", "shot", None)
            .unwrap();
        let meta = store.get_meta(&rec.sha256).unwrap();
        assert_eq!(meta.label, "shot");
        assert_eq!(meta.media_type, "image/png");
        let b64 = store.get_base64(&rec.sha256).unwrap();
        assert!(!b64.is_empty());
        let again = store
            .put_base64(&b64, "image/png", "browser.screenshot", "shot2", None)
            .unwrap();
        assert_eq!(again.sha256, rec.sha256);
    }

    #[test]
    fn delete_removes_blob_and_index_row() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let a = store
            .put_bytes(b"keep-me", "text/plain", "src", "keep", None)
            .unwrap();
        let b = store
            .put_bytes(b"drop-me", "text/plain", "src", "drop", None)
            .unwrap();
        store.delete(&b.sha256).unwrap();
        assert!(store.get_bytes(&b.sha256).is_err());
        assert!(store.get_meta(&b.sha256).is_err());
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.list().unwrap()[0].sha256, a.sha256);
        assert!(store.delete(&b.sha256).is_err());
    }

    #[test]
    fn delete_many_removes_batch_and_reports_missing() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let a = store
            .put_bytes(b"bulk-a", "text/plain", "src", "a", None)
            .unwrap();
        let b = store
            .put_bytes(b"bulk-b", "text/plain", "src", "b", None)
            .unwrap();
        let keep = store
            .put_bytes(b"bulk-keep", "text/plain", "src", "keep", None)
            .unwrap();
        let missing = "0".repeat(64);
        let result = store
            .delete_many(&[
                a.sha256.clone(),
                b.sha256.clone(),
                a.sha256.clone(), // duplicate collapsed
                missing.clone(),
            ])
            .unwrap();
        assert_eq!(result.deleted.len(), 2);
        assert!(result.deleted.contains(&a.sha256));
        assert!(result.deleted.contains(&b.sha256));
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].sha256, missing);
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.list().unwrap()[0].sha256, keep.sha256);
        assert!(store
            .delete_many(&[])
            .unwrap_err()
            .to_string()
            .contains("at least one"));
        let too_many: Vec<String> = (0..MAX_BULK_DELETE + 1)
            .map(|i| format!("{i:064x}"))
            .collect();
        assert!(store
            .delete_many(&too_many)
            .unwrap_err()
            .to_string()
            .contains("at most"));
    }

    #[test]
    fn oversized_base64_is_rejected_before_decode() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let oversized = "A".repeat((MAX_BYTES * 4 / 3) + 16_384);
        let error = store
            .put_base64(
                &oversized,
                "application/octet-stream",
                "test",
                "large",
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("base64 payload exceeds"));
    }

    #[test]
    fn read_rejects_tampered_blob_contents() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let record = store
            .put_bytes(b"original", "text/plain", "test", "original", None)
            .unwrap();
        fs::write(store.blob_path(&record.sha256), b"tampered").unwrap();

        let error = store.get_bytes(&record.sha256).unwrap_err();
        assert!(error.to_string().contains("digest mismatch"));
    }

    #[cfg(unix)]
    #[test]
    fn read_rejects_symlinked_blob() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let record = store
            .put_bytes(b"original", "text/plain", "test", "original", None)
            .unwrap();
        let outside = dir.path().join("outside");
        fs::write(&outside, b"original").unwrap();
        fs::remove_file(store.blob_path(&record.sha256)).unwrap();
        symlink(&outside, store.blob_path(&record.sha256)).unwrap();

        let error = store.get_bytes(&record.sha256).unwrap_err();
        assert!(error.to_string().contains("symlink"));
    }

    #[test]
    fn concurrent_writers_preserve_every_index_record() {
        let dir = Arc::new(tempdir().unwrap());
        let mut workers = Vec::new();
        for worker in 0..4 {
            let dir = Arc::clone(&dir);
            workers.push(thread::spawn(move || {
                let store = ArtifactStore::open(dir.path()).unwrap();
                for item in 0..20 {
                    let body = format!("worker-{worker}-item-{item}");
                    store
                        .put_bytes(
                            body.as_bytes(),
                            "text/plain",
                            "concurrency-test",
                            &body,
                            None,
                        )
                        .unwrap();
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }

        let store = ArtifactStore::open(dir.path()).unwrap();
        assert_eq!(store.list().unwrap().len(), 80);
        let index = fs::read_to_string(store.root().join("index.jsonl")).unwrap();
        assert_eq!(index.lines().count(), 80);
        assert!(index
            .lines()
            .all(|line| serde_json::from_str::<ArtifactRecord>(line).is_ok()));
    }

    #[test]
    fn delete_appends_tombstone_instead_of_rewriting_index() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let record = store
            .put_bytes(b"delete", "text/plain", "test", "delete", None)
            .unwrap();
        store.delete(&record.sha256).unwrap();

        let index = fs::read_to_string(store.root().join("index.jsonl")).unwrap();
        let rows = index
            .lines()
            .map(|line| serde_json::from_str::<ArtifactRecord>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert!(rows.last().unwrap().deleted);
    }
}
