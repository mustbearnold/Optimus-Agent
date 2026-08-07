//! Reopening a durable row over the host wire (spec-015 B1).
//!
//! The restore consumes the same projection the renderer's A11-oracle
//! snapshot uses (`get_session` → projected messages + tool_events), so a
//! reopened transcript cannot drift from what a fresh fetch would paint.
//! Calls still waiting on approval contribute no rows here: their blocked
//! row is restored from the exact durable binding, matching the pre-wire
//! restore (no `started` receipt either). A call whose approval later
//! settled replays fully — its `approval_required` phase is history, not a
//! parked job — so the terminal row still paints.

use optimus_kernel::{
    SessionMeta, ToolApprovalBinding, ToolCall, ToolLifecycleEvent, ToolLifecyclePhase,
};
use serde_json::json;

use super::{Message, Role, TuiSession};

impl TuiSession {
    /// Load one durable row into the compatibility transcript and its
    /// workbench mirror, both at launch and on sidebar clicks.
    pub(super) fn load_session_meta(&mut self, meta: &SessionMeta) -> Result<(), String> {
        let Some(client) = &self.client else {
            return Err("no host connection".into());
        };
        let value = client
            .call("get_session", json!({ "id": meta.id }))
            .map_err(|error| format!("could not load the session: {error}"))?;
        let messages = value
            .get("messages")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "get_session returned no messages".to_string())?;

        self.messages.clear();
        self.workbench.clear();
        let mut pending: Option<(ToolApprovalBinding, ToolCall)> = None;
        for row in messages {
            let role = row.get("role").and_then(|value| value.as_str());
            let content = row
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            match role {
                Some("user") => {
                    self.workbench.push_loaded(Role::User, None, None);
                    self.messages.push(Message {
                        role: Role::User,
                        text: content.into(),
                        call_id: None,
                        run_id: None,
                    });
                }
                Some("assistant") => {
                    // A turn that ended in a pending approval carries only a
                    // tool-protocol envelope; the projection rides its events
                    // on an empty assistant placeholder. The envelope is not
                    // conversation history (pre-wire restore parity: it was
                    // skipped), so the placeholder paints no row either —
                    // the pending call's blocked row is restored below.
                    let has_content = !content.is_empty();
                    if has_content {
                        self.workbench.push_loaded(Role::Assistant, None, None);
                        self.messages.push(Message {
                            role: Role::Assistant,
                            text: content.into(),
                            call_id: None,
                            run_id: None,
                        });
                    }
                    // The projection rides each turn's tool_events on its last
                    // assistant message. Replaying them in wire order paints
                    // the same receipts a live turn did. A call that is still
                    // waiting on approval contributes no rows here: its blocked
                    // row is restored from the exact binding below, matching
                    // the pre-wire restore (no `started` receipt either).
                    if let Some(events) = row.get("tool_events").and_then(|value| value.as_array())
                    {
                        // A call that is still waiting on approval contributes
                        // no rows here: its blocked row is restored from the
                        // exact binding below, matching the pre-wire restore
                        // (no `started` receipt either). A call whose
                        // approval later settled replays fully — its
                        // `approval_required` phase is history, not a parked
                        // job — so the terminal row still paints.
                        let pending_call_id = events.iter().find_map(|event| {
                            let lifecycle: ToolLifecycleEvent =
                                serde_json::from_value(event.clone()).ok()?;
                            if lifecycle.phase != ToolLifecyclePhase::ApprovalRequired {
                                return None;
                            }
                            let settled = events.iter().any(|later| {
                                serde_json::from_value::<ToolLifecycleEvent>(later.clone())
                                    .is_ok_and(|later| {
                                        later.call_id == lifecycle.call_id
                                            && matches!(
                                                later.phase,
                                                ToolLifecyclePhase::Succeeded
                                                    | ToolLifecyclePhase::Failed
                                                    | ToolLifecyclePhase::Cancelled
                                                    | ToolLifecyclePhase::Suppressed
                                                    | ToolLifecyclePhase::Ambiguous
                                            )
                                    })
                            });
                            (!settled)
                                .then(|| lifecycle.approval.map(|binding| binding.call_id))
                                .flatten()
                        });
                        for event in events {
                            let lifecycle: ToolLifecycleEvent =
                                serde_json::from_value(event.clone())
                                    .map_err(|error| error.to_string())?;
                            if Some(lifecycle.call_id.as_str()) == pending_call_id.as_deref() {
                                if lifecycle.phase == ToolLifecyclePhase::ApprovalRequired {
                                    if let Some(binding) = lifecycle.approval.clone() {
                                        // The wire does not carry the original
                                        // ToolCall arguments (renderer parity:
                                        // receipts restore from the binding).
                                        let call = ToolCall {
                                            id: binding.call_id.clone(),
                                            name: lifecycle.tool_id.as_str().to_string(),
                                            arguments: json!({}),
                                        };
                                        pending = Some((binding, call));
                                    }
                                }
                                continue;
                            }
                            let step = crate::tool_line::tool_step(&lifecycle);
                            self.workbench.apply_tool_step(&step);
                            // One row per call, exactly like the live adapter:
                            // a later phase rewrites the row that call already
                            // owns instead of appending a second message.
                            let existing = self.messages.iter_mut().rev().find(|m| {
                                m.call_id.as_deref() == Some(step.call_id.as_str())
                                    && m.run_id.as_deref() == Some(step.run_id.as_str())
                            });
                            match existing {
                                Some(message) => message.text = step.line,
                                None => self.messages.push(Message {
                                    role: Role::Tool,
                                    text: step.line,
                                    call_id: Some(step.call_id),
                                    run_id: Some(step.run_id),
                                }),
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.session_id = Some(meta.id.to_string());
        self.project_id = meta.project.clone();
        self.pending_approval = None;
        self.picker = None;
        self.completion.reset();
        self.scroll_back = 0;
        self.status.clear();
        self.running_tool = None;
        self.answer_started = false;
        self.refresh_sidebar();
        if let Some((binding, call)) = pending {
            self.restore_pending_approval(binding, &call);
        }
        Ok(())
    }

    /// Rebuild the human-facing approval card from the exact durable binding.
    /// The model transcript is deliberately not trusted to recreate authority
    /// after a restart; it only supplies context to the next kernel turn.
    fn restore_pending_approval(&mut self, binding: ToolApprovalBinding, call: &ToolCall) {
        let call_id = binding.call_id.clone();
        let tool = call.name.clone();
        self.workbench
            .restore_blocked_tool(&tool, &call_id, binding.run_id);
        self.messages.push(Message {
            role: Role::Tool,
            text: format!("{tool}  awaiting approval"),
            call_id: Some(call_id.clone()),
            run_id: Some(binding.run_id.to_string()),
        });
        self.pending_approval = Some(Box::new(binding.clone()));
        self.push(
            Role::Action,
            format!(
                "approval required:\n{}",
                crate::tool_line::readable(&binding.summary)
            ),
        );
        self.open_approval_picker();
    }
}
