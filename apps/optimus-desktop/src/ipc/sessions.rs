//! Durable session IPC.

use std::path::{Path, PathBuf};

use optimus_kernel::{get_session, list_sessions, Kernel, KernelConfig, SessionStore};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "sessions" | "delete_session" | "rename_session" | "new_session" | "get_session"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "sessions" => Ok(json!({ "sessions": sessions_json(home) })),
        "delete_session" => delete_session(home, params),
        "rename_session" => rename_session(home, params),
        "new_session" => {
            let k = Kernel::open(home, KernelConfig::default()).map_err(|e| e.to_string())?;
            Ok(json!({
                "id": k.session_id().to_string(),
                "title": k.session_title(),
            }))
        }
        "get_session" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id required".to_string())?;
            let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
            let detail = get_session(home, id).map_err(|e| e.to_string())?;
            let messages: Vec<_> = detail
                .messages
                .iter()
                .filter(|m| {
                    !matches!(
                        m.role,
                        optimus_kernel::Role::System | optimus_kernel::Role::Tool
                    )
                })
                .map(|m| {
                    json!({
                        "role": match m.role {
                            optimus_kernel::Role::User => "user",
                            optimus_kernel::Role::Assistant => "assistant",
                            _ => "other",
                        },
                        "content": m.content,
                    })
                })
                .collect();
            Ok(json!({
                "id": detail.id.to_string(),
                "title": detail.title,
                "packs": detail.packs,
                "messages": messages,
            }))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

pub fn sessions_json(home: &PathBuf) -> serde_json::Value {
    match list_sessions(home) {
        Ok(list) => {
            let rows: Vec<_> = list
                .into_iter()
                .map(|s| {
                    json!({
                        "id": s.id.to_string(),
                        "title": s.title,
                        "message_count": s.message_count,
                        "packs": s.packs,
                        "updated_at": s.updated_at,
                        "created_at": s.created_at,
                    })
                })
                .collect();
            json!(rows)
        }
        Err(_) => json!([]),
    }
}
fn delete_session(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "id required".to_string())?;
    let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
    let path = home.join("sessions.db");
    let store = SessionStore::open(&path).map_err(|e| e.to_string())?;
    let deleted = store.delete(id).map_err(|e| e.to_string())?;
    Ok(json!({ "deleted": deleted, "id": id.to_string() }))
}

fn rename_session(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "id required".to_string())?;
    let title = params
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "title required".to_string())?;
    let title = title.trim();
    if title.is_empty() {
        return Err("title required".into());
    }
    if title.chars().count() > 200 {
        return Err("title too long (max 200)".into());
    }
    let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
    let path = home.join("sessions.db");
    let store = SessionStore::open(&path).map_err(|e| e.to_string())?;
    let ok = store.rename(id, title).map_err(|e| e.to_string())?;
    if !ok {
        return Err("session not found".into());
    }
    Ok(json!({ "id": id.to_string(), "title": title }))
}
