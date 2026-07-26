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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, ArtifactError>;

fn tool_err(msg: impl AsRef<str>) -> ArtifactError {
    ArtifactError::Msg(msg.as_ref().to_string())
}

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
            return Err(tool_err("artifact bytes must be non-empty"));
        }
        if bytes.len() > MAX_BYTES {
            return Err(tool_err(format!(
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
            return Err(tool_err(format!(
                "base64 payload exceeds max encoded size {MAX_BASE64_INPUT} bytes"
            )));
        }
        let encoded_chars = b64.chars().filter(|c| !c.is_whitespace()).count();
        if encoded_chars > ((MAX_BYTES + 2) / 3) * 4 {
            return Err(tool_err(format!(
                "base64 payload exceeds max decoded size {MAX_BYTES} bytes"
            )));
        }
        let cleaned: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(cleaned.as_bytes())
            .map_err(|e| tool_err(format!("invalid base64 artifact payload: {e}")))?;
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
            _ => Err(tool_err(format!("artifact metadata not found: {sha256}"))),
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
            return Err(tool_err(fail.error.clone()));
        }
        if result.deleted.is_empty() {
            return Err(tool_err(format!("artifact not found: {sha256}")));
        }
        Ok(())
    }

    /// Delete up to [`MAX_BULK_DELETE`] artifacts under one exclusive lock.
    ///
    /// Duplicates are collapsed. Each sha is best-effort: missing items land in
    /// `failed` without aborting the rest of the batch.
    pub fn delete_many(&self, sha256s: &[String]) -> Result<BulkDeleteResult> {
        if sha256s.is_empty() {
            return Err(tool_err(
                "artifacts_delete_many requires at least one sha256",
            ));
        }
        if sha256s.len() > MAX_BULK_DELETE {
            return Err(tool_err(format!(
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
            return Err(tool_err(
                "artifacts_delete_many requires at least one sha256",
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
            return Err(tool_err(format!("artifact not found: {sha256}")));
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
                return Err(tool_err("refusing symlinked artifact blob"));
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
            .ok_or_else(|| tool_err("artifact blob has no parent"))?;
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
                return Err(tool_err(format!("artifact not found: {sha256}")));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(tool_err("refusing symlinked artifact blob"));
        }
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
            return Err(tool_err("artifact blob is not a bounded regular file"));
        }
        let mut file = File::open(path)?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        std::io::Read::by_ref(&mut file)
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_BYTES {
            return Err(tool_err(format!(
                "artifact exceeds max size {MAX_BYTES} bytes"
            )));
        }
        if hex_sha256(&bytes) != sha256 {
            return Err(tool_err("artifact digest mismatch"));
        }
        Ok(bytes)
    }

    /// Export one artifact under `{root}/exports/` only (program P25 confinement).
    /// Optional `filename` is basename-only; absolute/host paths are refused.
    pub fn export_file(&self, sha256: &str, filename: Option<&str>) -> Result<PathBuf> {
        validate_sha256(sha256)?;
        let meta = self.get_meta(sha256)?;
        let bytes = self.get_bytes(sha256)?;
        let dest = self.resolve_export_path(
            filename,
            &safe_export_basename(&meta.label, sha256, &meta.media_type),
        )?;
        write_regular_file(&dest, &bytes)?;
        Ok(dest)
    }

    /// Default export path under `{root}/exports/<safe-label>-<sha8>.<ext>`.
    pub fn default_export_path(&self, sha256: &str) -> Result<PathBuf> {
        validate_sha256(sha256)?;
        let meta = self.get_meta(sha256)?;
        self.resolve_export_path(
            None,
            &safe_export_basename(&meta.label, sha256, &meta.media_type),
        )
    }

    /// Bulk zip under `{root}/exports/` only. Entries use safe basenames
    /// (no directories) — prevents zip-slip on extract.
    pub fn export_zip(
        &self,
        sha256s: &[String],
        filename: Option<&str>,
    ) -> Result<(PathBuf, usize)> {
        if sha256s.is_empty() {
            return Err(tool_err("export_zip requires at least one sha256"));
        }
        if sha256s.len() > MAX_BULK_DELETE {
            return Err(tool_err(format!(
                "export_zip supports at most {MAX_BULK_DELETE} items"
            )));
        }
        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for raw in sha256s {
            let sha = raw.trim();
            if sha.is_empty() || !seen.insert(sha.to_string()) {
                continue;
            }
            validate_sha256(sha)?;
            let meta = self.get_meta(sha)?;
            let bytes = self.get_bytes(sha)?;
            let name = safe_export_basename(&meta.label, sha, &meta.media_type);
            let mut final_name = name.clone();
            let mut n = 1u32;
            while entries.iter().any(|(e, _)| e == &final_name) {
                n += 1;
                final_name = format!("{n}-{name}");
            }
            entries.push((final_name, bytes));
        }
        if entries.is_empty() {
            return Err(tool_err("export_zip requires at least one sha256"));
        }
        let count = entries.len();
        let stamp = now_unix();
        let dest = self.resolve_export_path(filename, &format!("artifacts-{stamp}.zip"))?;
        let zip_bytes = write_store_zip(&entries)?;
        write_regular_file(&dest, &zip_bytes)?;
        Ok((dest, count))
    }

    pub fn default_zip_path(&self) -> Result<PathBuf> {
        let stamp = now_unix();
        self.resolve_export_path(None, &format!("artifacts-{stamp}.zip"))
    }

    /// Resolve a path strictly under `{root}/exports/`.
    fn resolve_export_path(&self, filename: Option<&str>, default_name: &str) -> Result<PathBuf> {
        let exports = self.root.join("exports");
        ensure_owned_directory(&exports, "artifact exports")?;
        let name = match filename {
            Some(raw) => {
                let base = Path::new(raw)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .trim();
                if base.is_empty() || base != raw.trim() {
                    return Err(tool_err(
                        "export filename must be a plain basename under artifacts/exports",
                    ));
                }
                if base.contains("..") || base.contains('/') || base.contains('\\') {
                    return Err(tool_err("export filename invalid"));
                }
                validate_export_basename(base)?;
                base.to_string()
            }
            None => {
                validate_export_basename(default_name)?;
                default_name.to_string()
            }
        };
        let dest = exports.join(&name);
        // Canonical confinement: dest must stay under exports (no symlink escape).
        let exports_canon = exports.canonicalize().unwrap_or(exports.clone());
        if let Ok(parent) = dest.parent().unwrap_or(&exports).canonicalize() {
            if !parent.starts_with(&exports_canon) {
                return Err(tool_err("export path escapes artifacts/exports"));
            }
        }
        Ok(dest)
    }
}

fn validate_export_basename(name: &str) -> Result<()> {
    let lower = name.to_ascii_lowercase();
    if lower.is_empty() || lower.contains('\0') {
        return Err(tool_err("export basename invalid"));
    }
    if is_secret_export_basename(&lower) {
        return Err(tool_err(format!(
            "refusing export basename that looks secret: {name}"
        )));
    }
    Ok(())
}

fn is_secret_export_basename(name: &str) -> bool {
    const BLOCKED: &[&str] = &[
        ".env",
        ".env.local",
        "credentials",
        "credentials.json",
        "auth.json",
        ".netrc",
        "id_rsa",
        "id_ed25519",
        "secret",
        "secrets",
        "token",
        "tokens.json",
    ];
    for blocked in BLOCKED {
        if name == *blocked || name.starts_with(&format!("{blocked}.")) {
            return true;
        }
    }
    name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
}

fn safe_export_basename(label: &str, sha256: &str, media_type: &str) -> String {
    let mut base: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(48)
        .collect();
    while base.ends_with('-') {
        base.pop();
    }
    if base.is_empty() {
        base = "artifact".into();
    }
    let ext = media_ext(media_type);
    let short = &sha256[..8.min(sha256.len())];
    format!("{base}-{short}{ext}")
}

fn media_ext(media_type: &str) -> &'static str {
    let m = media_type.to_ascii_lowercase();
    if m.starts_with("image/png") {
        ".png"
    } else if m.starts_with("image/jpeg") || m.starts_with("image/jpg") {
        ".jpg"
    } else if m.starts_with("image/gif") {
        ".gif"
    } else if m.starts_with("image/webp") {
        ".webp"
    } else if m.starts_with("text/") || m.contains("json") {
        ".txt"
    } else {
        ".bin"
    }
}

fn write_regular_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Err(tool_err("refusing to overwrite symlink export path"));
        }
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Minimal ZIP (store method only) — entry names must not contain `/` or `..`.
fn write_store_zip(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(tool_err("zip entry name must be a plain basename"));
        }
        let name_bytes = name.as_bytes();
        let offset = out.len() as u32;
        let crc = crc32_ieee(data);
        let size = data.len() as u32;
        // Local file header
        out.extend_from_slice(&0x04034b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // store
        out.extend_from_slice(&0u16.to_le_bytes()); // time
        out.extend_from_slice(&0u16.to_le_bytes()); // date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);
        // Central directory header
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }
    let central_offset = out.len() as u32;
    let central_size = central.len() as u32;
    out.extend_from_slice(&central);
    // End of central directory
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn ensure_owned_directory(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(tool_err(format!("{label} must be a non-symlink directory")));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match fs::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(path)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(tool_err(format!("{label} must be a non-symlink directory")));
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
        return Err(tool_err(format!("{label} must be a non-symlink file")));
    }
    Ok(())
}

fn open_owned_file(path: &Path, append: bool) -> Result<File> {
    loop {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(tool_err("artifact state file must not be a symlink"));
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
        return Err(tool_err("artifact sha256 must be 64 hex characters"));
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

    #[test]
    fn export_file_and_zip_are_confined_under_exports() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let a = store
            .put_bytes(b"one", "text/plain", "test", "Note One", None)
            .unwrap();
        let b = store
            .put_bytes(b"two", "text/plain", "test", "Note Two", None)
            .unwrap();
        let dest = store.export_file(&a.sha256, None).unwrap();
        assert!(dest.starts_with(store.root().join("exports")));
        assert_eq!(fs::read(&dest).unwrap(), b"one");

        let (zip_path, count) = store
            .export_zip(
                &[a.sha256.clone(), b.sha256.clone(), a.sha256.clone()],
                None,
            )
            .unwrap();
        assert_eq!(count, 2);
        assert!(zip_path.starts_with(store.root().join("exports")));
        let zip = fs::read(&zip_path).unwrap();
        assert!(zip.starts_with(&[0x50, 0x4b])); // PK
        assert!(zip.windows(8).any(|w| w == b"Note-One") || zip.len() > 40);

        // Absolute host paths and secret basenames refused.
        assert!(store.export_file(&a.sha256, Some("/tmp/evil.txt")).is_err());
        assert!(store.export_file(&a.sha256, Some("../escape.txt")).is_err());
        assert!(store.export_file(&a.sha256, Some(".env")).is_err());
        assert!(store.export_file(&a.sha256, Some("auth.json")).is_err());
        assert!(store.export_file(&a.sha256, Some("key.pem")).is_err());
    }
}
