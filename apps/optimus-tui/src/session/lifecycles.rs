use optimus_host::client::HostClient;
use optimus_kernel::SessionMeta;
use serde_json::json;

#[cfg(test)]
use optimus_kernel::ToolLifecycleEvent;

#[cfg(test)]
pub(super) fn next_tool_lifecycle<'a>(
    lifecycles: &'a [ToolLifecycleEvent],
    call_id: &str,
    cursor: &mut usize,
) -> Option<&'a ToolLifecycleEvent> {
    let offset = lifecycles
        .get(*cursor..)?
        .iter()
        .position(|event| event.call_id == call_id)?;
    *cursor += offset + 1;
    lifecycles.get(*cursor - 1)
}

/// Resolve the newest durable session after an approval parks a turn.
pub(super) fn latest_session_id(client: Option<&HostClient>) -> Option<String> {
    let client = client?;
    let value = client.call("sessions", json!({})).ok()?;
    let rows = value.get("sessions")?.as_array()?;
    rows.iter()
        .filter_map(|row| serde_json::from_value::<SessionMeta>(row.clone()).ok())
        .filter(|meta| !meta.archived)
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at))
        .map(|meta| meta.id.to_string())
}
