//! Durable session IPC.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use optimus_kernel::{
    get_session, list_sessions, ExecutionStore, Kernel, KernelConfig, Message, Role, SessionDetail,
    SessionStore, StreamEvent, ToolCall, ToolLifecycleEvent,
};
use serde_json::json;

use crate::stream_event_to_json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "sessions"
            | "delete_session"
            | "rename_session"
            | "new_session"
            | "get_session"
            | "session_search"
            | "archive_session"
            | "pin_session"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "sessions" => {
            let project = crate::scope::optional_project_id(&params)?;
            Ok(json!({ "sessions": sessions_json_scoped(home, project.as_deref()) }))
        }
        "session_search" => session_search(home, params),
        "archive_session" => archive_session(home, params),
        "pin_session" => pin_session(home, params),
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
            let messages = project_session_messages(home, &detail)?;
            let store = SessionStore::open(home.join("sessions.db")).map_err(|e| e.to_string())?;
            let run_status = store
                .turns(id)
                .map_err(|e| e.to_string())?
                .last()
                .map(|turn| turn.status);
            let meta = store
                .list_filtered(optimus_kernel::ListFilter {
                    include_archived: true,
                })
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|s| s.id == id);
            Ok(json!({
                "id": detail.id.to_string(),
                "title": detail.title,
                "packs": detail.packs,
                "messages": messages,
                "run_status": run_status,
                "pinned": meta.as_ref().map(|m| m.pinned).unwrap_or(false),
                "archived": meta.as_ref().map(|m| m.archived).unwrap_or(false),
            }))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

fn project_session_messages(
    home: &Path,
    detail: &SessionDetail,
) -> Result<Vec<serde_json::Value>, String> {
    let session_store = SessionStore::open(home.join("sessions.db")).map_err(|e| e.to_string())?;
    let turns = session_store.turns(detail.id).map_err(|e| e.to_string())?;
    let execution_store =
        ExecutionStore::open(home.join("execution.db")).map_err(|e| e.to_string())?;
    let mut events_by_turn: BTreeMap<uuid::Uuid, Vec<ToolLifecycleEvent>> = BTreeMap::new();
    for persisted in execution_store
        .tool_lifecycle_for_session(detail.id)
        .map_err(|e| e.to_string())?
    {
        events_by_turn
            .entry(persisted.turn_id)
            .or_default()
            .push(persisted.event);
    }

    if turns.is_empty() {
        return Ok(project_turn(&detail.messages, &[]));
    }

    let mut projected = Vec::new();
    for (index, turn) in turns.iter().enumerate() {
        let start = turn.start_message_count.min(detail.messages.len());
        let end = turns
            .get(index + 1)
            .map(|next| next.start_message_count)
            .unwrap_or(detail.messages.len())
            .clamp(start, detail.messages.len());
        projected.extend(project_turn(
            &detail.messages[start..end],
            events_by_turn
                .get(&turn.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        ));
    }
    Ok(projected)
}

fn project_turn(messages: &[Message], events: &[ToolLifecycleEvent]) -> Vec<serde_json::Value> {
    let mut projected = Vec::new();
    let mut last_assistant = None;
    for message in messages {
        match message.role {
            Role::User => projected.push(json!({"role":"user","content":message.content})),
            Role::Assistant if !is_tool_protocol_message(&message.content) => {
                projected.push(json!({"role":"assistant","content":message.content}));
                last_assistant = Some(projected.len() - 1);
            }
            Role::System | Role::Tool | Role::Assistant => {}
        }
    }
    if !events.is_empty() {
        let assistant_index = last_assistant.unwrap_or_else(|| {
            projected.push(json!({"role":"assistant","content":""}));
            projected.len() - 1
        });
        let tool_events = events
            .iter()
            .cloned()
            .map(|event| stream_event_to_json(&StreamEvent::Tool(Box::new(event))))
            .collect::<Vec<_>>();
        projected[assistant_index]["tool_events"] = json!(tool_events);
    }
    projected
}

fn is_tool_protocol_message(content: &str) -> bool {
    serde_json::from_str::<Vec<ToolCall>>(content)
        .map(|calls| !calls.is_empty())
        .unwrap_or(false)
}

pub fn sessions_json(home: &PathBuf) -> serde_json::Value {
    sessions_json_scoped(home, None)
}

/// `project = Some(id)` is the C1 project-scoped view: only sessions bound to
/// that project (session/project.rs in the kernel). `None` is the unscoped
/// host list, unchanged.
pub fn sessions_json_scoped(home: &PathBuf, project: Option<&str>) -> serde_json::Value {
    let list = SessionStore::open(home.join("sessions.db"))
        .and_then(|s| {
            s.list_filtered(optimus_kernel::ListFilter {
                include_archived: true,
            })
        })
        .or_else(|_| list_sessions(home))
        .unwrap_or_default();
    let rows: Vec<_> = list
        .into_iter()
        .filter(|meta| project.is_none_or(|p| meta.project.as_deref() == Some(p)))
        .map(session_meta_json)
        .collect();
    json!(rows)
}

fn session_meta_json(s: optimus_kernel::SessionMeta) -> serde_json::Value {
    json!({
        "id": s.id.to_string(),
        "title": s.title,
        "message_count": s.message_count,
        "packs": s.packs,
        "updated_at": s.updated_at,
        "created_at": s.created_at,
        "pinned": s.pinned,
        "archived": s.archived,
        "project": s.project,
    })
}

fn session_search(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let q = params
        .get("q")
        .or_else(|| params.get("query"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let include_archived = params
        .get("include_archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    // Same C1 scoping as the `sessions` view — search must not be the side
    // door that shows another project's sessions.
    let project = crate::scope::optional_project_id(&params)?;
    let store = SessionStore::open(home.join("sessions.db")).map_err(|e| e.to_string())?;
    let rows = store
        .search(q, include_archived)
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|meta| {
            project
                .as_deref()
                .is_none_or(|p| meta.project.as_deref() == Some(p))
        })
        .map(session_meta_json)
        .collect::<Vec<_>>();
    Ok(json!({ "sessions": rows, "q": q }))
}

fn archive_session(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "id required".to_string())?;
    let archived = params
        .get("archived")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "archived bool required".to_string())?;
    let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
    let store = SessionStore::open(home.join("sessions.db")).map_err(|e| e.to_string())?;
    let ok = store
        .set_archived(id, archived)
        .map_err(|e| e.to_string())?;
    if !ok {
        return Err("session not found".into());
    }
    Ok(json!({ "id": id.to_string(), "archived": archived }))
}

fn pin_session(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "id required".to_string())?;
    let pinned = params
        .get("pinned")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "pinned bool required".to_string())?;
    let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
    let store = SessionStore::open(home.join("sessions.db")).map_err(|e| e.to_string())?;
    let ok = store.set_pinned(id, pinned).map_err(|e| e.to_string())?;
    if !ok {
        return Err("session not found".into());
    }
    Ok(json!({ "id": id.to_string(), "pinned": pinned }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use optimus_kernel::{ExecutionStatus, ToolLifecyclePhase, TurnStatus};
    use optimus_packs::{ReplayClass, ToolId, ToolOutcome};
    use tempfile::tempdir;

    #[test]
    fn session_reload_projects_durable_tool_cards_without_protocol_json() {
        let directory = tempdir().unwrap();
        let home = directory.path();
        let sessions = SessionStore::open(home.join("sessions.db")).unwrap();
        let session_id = sessions.create("reload").unwrap();
        let mut messages = vec![Message {
            role: Role::System,
            content: "system".into(),
            ..Message::default()
        }];
        sessions.save(session_id, "reload", &[], &messages).unwrap();
        let start_message_count = messages.len();
        messages.push(Message {
            role: Role::User,
            content: "inspect".into(),
            ..Message::default()
        });
        let turn_id = sessions
            .begin_turn(session_id, "reload", &[], &messages, start_message_count)
            .unwrap();
        messages.push(Message {
            role: Role::Assistant,
            content: serde_json::to_string(&vec![ToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: json!({"path":"README.md"}),
            }])
            .unwrap(),
            ..Message::default()
        });
        messages.push(Message {
            role: Role::Tool,
            content: "receipt".into(),
            tool_call_id: Some("call-1".into()),
            name: Some("read_file".into()),
            reasoning_content: None,
        });
        messages.push(Message {
            role: Role::Assistant,
            content: "Done.".into(),
            ..Message::default()
        });
        sessions
            .finish_turn(
                turn_id,
                session_id,
                "reload",
                &[],
                &messages,
                TurnStatus::Succeeded,
                None,
            )
            .unwrap();

        let executions = ExecutionStore::open(home.join("execution.db")).unwrap();
        let manifest = executions
            .begin(
                session_id,
                turn_id,
                "offline",
                "offline-scripted",
                b"inspect",
                b"tools",
                b"policy",
            )
            .unwrap();
        let outcome = ToolOutcome::succeeded(
            "call-1",
            "read_file",
            "Read README.md",
            json!({"text":"ok"}),
            ReplayClass::Deterministic,
        );
        for event in [
            ToolLifecycleEvent {
                schema_version: 1,
                event_id: format!("{manifest}:call-1:started"),
                run_id: manifest.to_string(),
                call_id: "call-1".into(),
                tool_id: ToolId::new("read_file"),
                phase: ToolLifecyclePhase::Started,
                summary: "Reading".into(),
                duration_ms: None,
                outcome: None,
                approval: None,
            },
            ToolLifecycleEvent {
                schema_version: 1,
                event_id: format!("{manifest}:call-1:succeeded"),
                run_id: manifest.to_string(),
                call_id: "call-1".into(),
                tool_id: ToolId::new("read_file"),
                phase: ToolLifecyclePhase::Succeeded,
                summary: "Read README.md".into(),
                duration_ms: Some(8),
                outcome: Some(outcome),
                approval: None,
            },
        ] {
            executions
                .record_tool_lifecycle_event(manifest, &event)
                .unwrap();
        }
        executions
            .finish(manifest, ExecutionStatus::Succeeded)
            .unwrap();

        let detail = get_session(home, session_id).unwrap();
        let projected = project_session_messages(home, &detail).unwrap();
        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0]["content"], "inspect");
        assert_eq!(projected[1]["content"], "Done.");
        assert_eq!(projected[1]["tool_events"].as_array().unwrap().len(), 2);
        assert_eq!(projected[1]["tool_events"][1]["phase"], "succeeded");
        assert!(!projected.iter().any(|message| message["content"]
            .as_str()
            .is_some_and(|content| content.contains("call-1"))));
    }
}
