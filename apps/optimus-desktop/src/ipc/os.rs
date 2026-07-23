//! Native/HTTP window-adjacent and OS integration IPC.

use std::path::PathBuf;
use std::process::{Command, Stdio};

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
            | "open_url"
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
        "open_url" => open_url_in_os(params),
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

fn spawn_and_reap(mut command: Command, context: &str) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{context}: {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
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
        let mut command = Command::new("explorer");
        command.arg(if p.is_file() {
            format!("/select,{}", p.display())
        } else {
            p.display().to_string()
        });
        spawn_and_reap(command, "open path")?;
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/usr/bin/open");
        command.arg(path);
        spawn_and_reap(command, "open path")?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("/usr/bin/xdg-open");
        command.arg(path);
        spawn_and_reap(command, "open path")?;
    }
    #[cfg(not(any(windows, unix)))]
    return Err("opening paths is unsupported on this platform".into());

    Ok(json!({ "ok": true, "path": path }))
}

fn validated_external_url(params: &serde_json::Value) -> Result<String, String> {
    let raw = params
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if raw.is_empty() || raw.len() > 8_192 {
        return Err("url must be between 1 and 8192 bytes".into());
    }
    let parsed = url::Url::parse(raw).map_err(|_| "invalid external url".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("only absolute http and https urls can be opened".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("credential-bearing urls are not allowed".into());
    }
    Ok(parsed.to_string())
}

fn open_url_in_os(params: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = validated_external_url(&params)?;
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", &url]);
        spawn_and_reap(command, "open external url")?;
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("/usr/bin/open");
        command.arg(&url);
        spawn_and_reap(command, "open external url")?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("/usr/bin/xdg-open");
        command.arg(&url);
        spawn_and_reap(command, "open external url")?;
    }
    #[cfg(not(any(windows, unix)))]
    return Err("opening external urls is unsupported on this platform".into());

    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validated_external_url;

    #[test]
    fn external_url_validation_is_http_only_and_rejects_credentials() {
        assert_eq!(
            validated_external_url(&json!({"url":"https://example.com/a?q=1"})).unwrap(),
            "https://example.com/a?q=1"
        );
        for denied in [
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///etc/passwd",
            "/relative/path",
            "https://user:pass@example.com/",
        ] {
            assert!(
                validated_external_url(&json!({"url":denied})).is_err(),
                "accepted {denied}"
            );
        }
    }
}
