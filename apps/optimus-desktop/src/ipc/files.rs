//! Sandboxed Files IPC.

use std::path::Path;

use optimus_kernel::FsRoots;
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(method, "fs_roots" | "fs_list" | "fs_read")
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
