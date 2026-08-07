//! Deciding a parked effect, and what the decision resumes.
//!
//! Split out of `session.rs` under the module-size law. A child module rather
//! than a sibling on purpose: this reaches into `ActiveTurn`, `WorkerKind` and
//! the private fields of [`TuiSession`], and privacy in Rust already extends to
//! descendants — so the split costs no widened visibility, which a sibling
//! module would have.
//!
//! What holds this together rather than the turn machinery next door is that
//! approval is the one path where the *user* is the authority. The binding is
//! carried verbatim from the kernel and sent back untouched (never re-derived
//! from what the renderer happens to be showing), and resolving it resumes the
//! turn it paused rather than ending it — ADR-0046.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use optimus_kernel::CancellationToken;
use serde_json::json;

use super::event_adapter::wire_update;
use super::{latest_session_id, ActiveTurn, Role, TuiSession, TurnUpdate, WorkerKind};

impl TuiSession {
    /// Resolve the pending approval with one explicit decision, on a worker.
    ///
    /// Approving executes the exact bound effect, so this must not run on the
    /// screen thread — a slow command effect would freeze the terminal.
    pub fn resolve_approval(&mut self, decision: &str) {
        if self.busy() {
            return;
        }
        let params = match self.approval_params(decision) {
            Ok(params) => params,
            Err(error) => {
                self.push(Role::Error, error);
                return;
            }
        };
        self.push(Role::Action, decision_line(decision).to_string());
        // Named now, while the held binding is still the one being decided. The
        // resumed turn streams through this same worker and can raise its own
        // approval before the resolver returns, so settlement has to say which
        // card it settled rather than clearing whatever is held at the end.
        let Some(settling_binding) = self
            .pending_approval
            .as_ref()
            .map(|binding| (**binding).clone())
        else {
            return;
        };
        // The continuation opens its own bubble, exactly as a fresh turn does.
        self.answer_started = false;
        self.begin("resolving approval");

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(CancellationToken::new());
        let Some(client) = self.client.clone() else {
            let _ = tx.send(TurnUpdate::Failed("no host connection".to_string()));
            self.active = Some(ActiveTurn {
                updates: rx,
                cancel,
                kind: WorkerKind::Resolve,
                awaiting_approval: false,
                stream_id: None,
            });
            return;
        };
        let stream_id = client.fresh_stream_id();
        let wire_stream_id = stream_id;
        let binding_for_worker = settling_binding.clone();
        let tx_for_worker = tx.clone();
        thread::spawn(move || {
            let stream = match client.resolve(wire_stream_id, params) {
                Ok(stream) => stream,
                Err(error) => {
                    let _ = tx.send(TurnUpdate::Failed(format!(
                        "approval resolution failed: {error}"
                    )));
                    return;
                }
            };
            let mut terminal = None;
            while let Some(event) = stream.next() {
                let kind = event.get("type").and_then(|v| v.as_str());
                if matches!(kind, Some("done" | "cancelled" | "error")) {
                    terminal = Some(event);
                    break;
                }
                if let Some(update) = wire_update(&event) {
                    if tx_for_worker.send(update).is_err() {
                        return;
                    }
                }
            }
            let update = match terminal {
                Some(event) if event.get("type").and_then(|v| v.as_str()) == Some("done") => {
                    // Ok means the decision itself was carried out, whatever
                    // the resumed turn goes on to do.
                    let _ = tx.send(TurnUpdate::ApprovalSettled(Box::new(binding_for_worker)));
                    resolved_update(event.get("result").unwrap_or(&json!({})))
                }
                Some(event) => TurnUpdate::Failed(format!(
                    "approval resolution failed: {}",
                    event
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("connection closed")
                )),
                None => TurnUpdate::Failed("approval resolution failed: connection lost".into()),
            };
            let _ = tx.send(update);
        });

        self.active = Some(ActiveTurn {
            updates: rx,
            cancel,
            kind: WorkerKind::Resolve,
            awaiting_approval: false,
            stream_id: Some(stream_id),
        });
    }

    /// Wire params for `chat_approval_resolve`. Separate so tests can assert
    /// that the exact binding — not renderer-invented authority — is sent.
    ///
    /// `pub(super)` rather than private for that reason: the assertions live in
    /// the parent's test module beside the rest of the surface's behaviour.
    pub(super) fn approval_params(&self, decision: &str) -> Result<serde_json::Value, String> {
        let binding = self
            .pending_approval
            .as_ref()
            .ok_or("no approval is pending")?;
        let session_id = self
            .session_id
            .clone()
            .or_else(|| latest_session_id(self.client.as_deref()))
            .ok_or("no session to resolve against yet")?;
        let mut params = json!({
            "session_id": session_id,
            "run_id": binding.run_id.to_string(),
            "call_id": binding.call_id,
            "job_id": binding.job_id.to_string(),
            "node_id": binding.node_id.to_string(),
            "node_index": binding.node_index,
            "effect_sha256": binding.effect_sha256,
            "decision": decision,
        });
        // Resolving now resumes the turn (ADR-0046), so the continuation is a
        // model call and needs the same answerer the paused turn had.
        self.apply_model_choice(&mut params);
        if self.yolo {
            params["access"] = json!("yolo");
        } else if let Some(profile) = self.access {
            params["access"] = json!(profile);
        }
        Ok(params)
    }

    /// Open (or reopen) the decision picker for the pending approval.
    pub fn open_approval_picker(&mut self) {
        let Some(binding) = self.pending_approval.as_ref() else {
            return;
        };
        let summary = binding.summary.clone();
        self.picker = Some(crate::picker::Picker::new(
            crate::picker::PickerKind::Approval,
            "Approval required",
            vec![
                crate::picker::PickerItem {
                    id: "approve".into(),
                    label: "Approve and continue".into(),
                    detail: summary,
                    current: false,
                    connected: true,
                },
                crate::picker::PickerItem {
                    id: "deny".into(),
                    label: "Deny".into(),
                    detail: "records a denial; nothing runs".into(),
                    current: false,
                    connected: true,
                },
            ],
        ));
    }
}

/// Marks the decision in the transcript, under the card it answers.
///
/// Written on the screen thread the moment the user decides, not when the host
/// replies: resolving now resumes the paused turn (ADR-0046), so the reply is
/// the agent's answer and arrives after. Recording the decision late would print
/// it beneath the answer it led to.
pub(super) fn decision_line(decision: &str) -> &'static str {
    match decision {
        "approve" => "approved — running the exact action…",
        _ => "denied — it will not run",
    }
}

/// Turn a settled approval into the worker's last update.
///
/// The decision itself always succeeds or fails on its own terms; what follows
/// is the resumed turn, and that can fail separately or park on a second
/// approval. `resume_error` carries that, so it becomes a failure the screen
/// thread handles exactly as it handles one from a fresh turn — including
/// recognising a park as a park rather than an error.
pub(super) fn resolved_update(value: &serde_json::Value) -> TurnUpdate {
    let session_id = value
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    match value.get("resume_error").and_then(|v| v.as_str()) {
        Some(error) if !error.is_empty() => TurnUpdate::Failed(error.to_string()),
        _ => TurnUpdate::Done {
            session_id,
            text: value
                .get("assistant_text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        },
    }
}

/// A structurally valid binding for surface tests. The runtime rejects it on
/// contact, which is exactly what the fail-closed tests want.
#[cfg(test)]
pub(crate) fn approval_binding_fixture() -> Box<optimus_kernel::ToolApprovalBinding> {
    Box::new(
        serde_json::from_value(json!({
            "run_id": "11111111-1111-4111-8111-111111111111",
            "call_id": "write-1",
            "tool_id": "write_file",
            "job_id": "22222222-2222-4222-8222-222222222222",
            "node_id": "33333333-3333-4333-8333-333333333333",
            "node_index": 3,
            "effect_sha256": "ab".repeat(32),
            "summary": "Write src/proof.txt (4 bytes)",
        }))
        .expect("fixture binding deserializes"),
    )
}
