//! Sandboxed Files + Artifacts IPC.

use std::path::Path;

use optimus_kernel::{ArtifactStore, FsRoots};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "fs_roots"
            | "fs_list"
            | "fs_read"
            | "artifacts_list"
            | "artifacts_put_text"
            | "artifacts_get"
            | "artifacts_delete"
            | "artifacts_delete_many"
    )
}

pub(super) fn handle(
    home: &Path,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "fs_roots" => fs_roots(home),
        "fs_list" => fs_list(home, params),
        "fs_read" => fs_read(home, params),
        "artifacts_list" => artifacts_list(home),
        "artifacts_put_text" => artifacts_put_text(home, params),
        "artifacts_get" => artifacts_get(home, params),
        "artifacts_delete" => artifacts_delete(home, params),
        "artifacts_delete_many" => artifacts_delete_many(home, params),
        _ => Err(format!("unknown method: {method}")),
    }
}

fn open_fs_roots(home: &Path) -> Result<FsRoots, String> {
    FsRoots::from_home(home).map_err(|e| e.to_string())
}

fn fs_roots(home: &Path) -> Result<serde_json::Value, String> {
    let roots = open_fs_roots(home)?;
    let rows: Vec<_> = roots
        .roots()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let id = if i == 0 {
                "home".to_string()
            } else {
                format!("root-{i}")
            };
            json!({
                "id": id,
                "path": p.display().to_string(),
            })
        })
        .collect();
    Ok(json!({ "roots": rows }))
}

fn fs_list(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let path = if path == "." { "" } else { path };
    let roots = open_fs_roots(home)?;
    let entries = roots.list_dir(path).map_err(|e| e.to_string())?;
    Ok(json!({ "entries": entries }))
}

fn fs_read(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "path required".to_string())?
        .to_string();
    let max_bytes = params
        .get("max_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(512_000) as usize;
    let roots = open_fs_roots(home)?;
    let result = roots
        .read_text(&path, max_bytes, false)
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "content": result.content,
        "truncated": result.truncated,
        "path": path,
    }))
}

fn artifacts_list(home: &Path) -> Result<serde_json::Value, String> {
    let store = ArtifactStore::open(home).map_err(|e| e.to_string())?;
    let artifacts = store.list().map_err(|e| e.to_string())?;
    Ok(json!({ "artifacts": artifacts }))
}

fn artifacts_put_text(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "text required".to_string())?;
    let label = params
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("note");
    let source = params
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("manual");
    let store = ArtifactStore::open(home).map_err(|e| e.to_string())?;
    let record = store
        .put_bytes(
            text.as_bytes(),
            "text/plain; charset=utf-8",
            source,
            label,
            None,
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({ "artifact": record }))
}

fn artifacts_get(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let sha256 = params
        .get("sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "sha256 required".to_string())?;
    let store = ArtifactStore::open(home).map_err(|e| e.to_string())?;
    let meta = store.get_meta(sha256).map_err(|e| e.to_string())?;
    let bytes = store.get_bytes(sha256).map_err(|e| e.to_string())?;
    let media = meta.media_type.to_ascii_lowercase();
    let is_image = media.starts_with("image/");
    let is_text = media.starts_with("text/") || media.contains("json") || media.contains("xml");

    if is_image {
        let b64 = store.get_base64(sha256).map_err(|e| e.to_string())?;
        return Ok(json!({
            "artifact": meta,
            "kind": "image",
            "data_url": format!("data:{};base64,{}", meta.media_type, b64),
        }));
    }

    if is_text {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let truncated = text.len() > 200_000;
        let body = if truncated {
            let mut boundary = 200_000;
            while !text.is_char_boundary(boundary) {
                boundary -= 1;
            }
            format!("{}…[truncated]", &text[..boundary])
        } else {
            text
        };
        return Ok(json!({
            "artifact": meta,
            "kind": "text",
            "text": body,
            "truncated": truncated,
        }));
    }

    // Binary fallback: short hex preview only (no giant dump).
    let preview: String = bytes
        .iter()
        .take(64)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(json!({
        "artifact": meta,
        "kind": "binary",
        "hex_preview": preview,
        "size_bytes": bytes.len(),
    }))
}

fn artifacts_delete(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let sha256 = params
        .get("sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "sha256 required".to_string())?;
    let store = ArtifactStore::open(home).map_err(|e| e.to_string())?;
    store.delete(sha256).map_err(|e| e.to_string())?;
    Ok(json!({ "ok": true, "sha256": sha256 }))
}

fn artifacts_delete_many(
    home: &Path,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let sha256s: Vec<String> = params
        .get("sha256s")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "sha256s array required".to_string())?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    let store = ArtifactStore::open(home).map_err(|e| e.to_string())?;
    let result = store.delete_many(&sha256s).map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": result.failed.is_empty(),
        "deleted": result.deleted,
        "failed": result.failed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn artifacts_put_list_get_gallery_path() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let put = artifacts_put_text(
            home,
            json!({"text": "gallery hello", "label": "note-a", "source": "test"}),
        )
        .unwrap();
        let sha = put["artifact"]["sha256"].as_str().unwrap().to_string();
        let list = artifacts_list(home).unwrap();
        assert_eq!(list["artifacts"].as_array().unwrap().len(), 1);
        let got = artifacts_get(home, json!({"sha256": sha})).unwrap();
        assert_eq!(got["kind"], "text");
        assert_eq!(got["text"], "gallery hello");
        assert_eq!(got["artifact"]["label"], "note-a");
    }

    #[test]
    fn artifacts_delete_clears_list() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let put = artifacts_put_text(
            home,
            json!({"text": "bye", "label": "doomed", "source": "test"}),
        )
        .unwrap();
        let sha = put["artifact"]["sha256"].as_str().unwrap().to_string();
        let del = artifacts_delete(home, json!({"sha256": sha})).unwrap();
        assert_eq!(del["ok"], true);
        let list = artifacts_list(home).unwrap();
        assert!(list["artifacts"].as_array().unwrap().is_empty());
        assert!(artifacts_get(home, json!({"sha256": sha})).is_err());
    }

    #[test]
    fn artifacts_delete_many_clears_batch() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let a = artifacts_put_text(
            home,
            json!({"text": "one", "label": "a", "source": "test"}),
        )
        .unwrap()["artifact"]["sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let b = artifacts_put_text(
            home,
            json!({"text": "two", "label": "b", "source": "test"}),
        )
        .unwrap()["artifact"]["sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let keep = artifacts_put_text(
            home,
            json!({"text": "keep", "label": "k", "source": "test"}),
        )
        .unwrap()["artifact"]["sha256"]
            .as_str()
            .unwrap()
            .to_string();
        let res = artifacts_delete_many(home, json!({"sha256s": [a, b, "0".repeat(64)]})).unwrap();
        assert_eq!(res["deleted"].as_array().unwrap().len(), 2);
        assert_eq!(res["failed"].as_array().unwrap().len(), 1);
        assert_eq!(res["ok"], false);
        let list = artifacts_list(home).unwrap();
        let rows = list["artifacts"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["sha256"], keep);
    }

    #[test]
    fn artifacts_get_truncates_unicode_text_on_a_character_boundary() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let text = "€".repeat(70_000);
        let put = artifacts_put_text(
            home,
            json!({"text": text, "label": "unicode", "source": "test"}),
        )
        .unwrap();
        let sha = put["artifact"]["sha256"].as_str().unwrap();

        let got = artifacts_get(home, json!({"sha256": sha})).unwrap();
        assert_eq!(got["kind"], "text");
        assert_eq!(got["truncated"], true);
        assert!(got["text"].as_str().unwrap().ends_with("…[truncated]"));
    }
}
