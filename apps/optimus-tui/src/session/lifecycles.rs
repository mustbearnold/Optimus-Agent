use std::path::Path;

use optimus_kernel::{
    PersistedToolLifecycle, SessionStore, ToolCall, ToolLifecycleEvent, ToolLifecyclePhase,
};
use serde_json::from_str;

pub(super) fn is_tool_call_envelope(content: &str) -> bool {
    from_str::<Vec<ToolCall>>(content)
        .map(|calls| !calls.is_empty())
        .unwrap_or(false)
}

/// Collapse each call's typed lifecycle into the latest phase for that call
/// occurrence, preserving repeated provider ids across turns. Providers may
/// reuse identifiers across turns, so only the current non-terminal occurrence
/// is replaced.
pub(super) fn collapse_tool_lifecycles(
    lifecycles: Vec<PersistedToolLifecycle>,
) -> Vec<ToolLifecycleEvent> {
    let mut occurrences = Vec::new();
    for lifecycle in lifecycles {
        let event = lifecycle.event;
        let can_update = occurrences
            .last()
            .is_some_and(|previous: &ToolLifecycleEvent| {
                previous.run_id == event.run_id
                    && previous.call_id == event.call_id
                    && !is_terminal_tool_phase(previous.phase)
            });
        if can_update {
            if let Some(previous) = occurrences.last_mut() {
                *previous = event;
            }
        } else {
            occurrences.push(event);
        }
    }
    occurrences
}

fn is_terminal_tool_phase(phase: ToolLifecyclePhase) -> bool {
    matches!(
        phase,
        ToolLifecyclePhase::Succeeded
            | ToolLifecyclePhase::Failed
            | ToolLifecyclePhase::Cancelled
            | ToolLifecyclePhase::Suppressed
            | ToolLifecyclePhase::Ambiguous
    )
}

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
pub(super) fn latest_session_id(home: &Path) -> Option<String> {
    let latest = SessionStore::open(home.join("sessions.db"))
        .ok()?
        .latest()
        .ok()??;
    Some(latest.id.to_string())
}
