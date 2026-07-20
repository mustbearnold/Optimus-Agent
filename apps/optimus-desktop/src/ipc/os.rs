//! Native/HTTP window-adjacent and OS integration IPC.

use std::path::PathBuf;

use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "window_minimize"
            | "window_maximize"
            | "window_close"
            | "window_drag"
            | "window_outer_position"
            | "window_set_outer_position"
            | "pick_folder"
            | "open_path"
    )
}

pub(super) fn handle(
    _home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        // HTTP/Playwright stubs — real window chrome is handled in the native event loop.
        "window_minimize"
        | "window_maximize"
        | "window_close"
        | "window_drag"
        | "window_outer_position"
        | "window_set_outer_position" => {
            Ok(json!({ "ok": true, "mode": "http-stub", "x": 0, "y": 0 }))
        }
        "pick_folder" => pick_folder(),
        "open_path" => open_path_in_os(params),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Native folder picker (main-thread). HTTP mode uses [`pick_folder`] stub.
pub(crate) fn pick_folder_dialog() -> Result<serde_json::Value, String> {
    let picked = rfd::FileDialog::new()
        .set_title("Add project folder")
        .pick_folder();
    match picked {
        Some(path) => {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "project".into());
            Ok(json!({
                "cancelled": false,
                "path": path.display().to_string(),
                "name": name,
            }))
        }
        None => Ok(json!({ "cancelled": true })),
    }
}

/// IPC entry used by HTTP/Playwright (no real dialog).
fn pick_folder() -> Result<serde_json::Value, String> {
    Ok(json!({
        "cancelled": true,
        "mode": "http-stub",
        "hint": "Folder picker is available in the native desktop window",
    }))
}
fn open_path_in_os(params: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "path required".to_string())?;
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(if p.is_file() {
                // Select file in folder
                format!("/select,{}", p.display())
            } else {
                p.display().to_string()
            })
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(json!({ "ok": true, "path": path }))
}
