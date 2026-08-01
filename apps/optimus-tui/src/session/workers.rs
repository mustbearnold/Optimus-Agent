//! Spawning the workers that carry a turn or a provider connection.
//!
//! Split out of `session.rs` under the module-size law, along the seam
//! ADR-0075 names. A child module for the same reason `approval` is one:
//! spawning fills [`ActiveTurn`] and the private fields of [`TuiSession`],
//! and privacy in Rust already extends to descendants. Every worker honours
//! the settlement contract (#108): send `Done` or `Failed` before the sender
//! drops, so a channel that vanishes without one reads as the crash it is.
//! The third worker — resolving a parked effect — lives in `approval`,
//! because there the user, not the model, is the authority.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use optimus_host::chat_turn_cancellable;
use optimus_kernel::codex_device_login::device_code_login_with;
use optimus_kernel::{CancellationToken, CodexAuthStore};
use serde_json::json;

use super::event_adapter::stream_sink;
use super::{reservation, ActiveTurn, Role, TuiSession, TurnUpdate, WorkerKind};

impl TuiSession {
    /// Send whatever is in the composer as one turn, on a worker thread.
    pub fn submit(&mut self) {
        let prompt = self.composer.text().trim().to_string();
        if prompt.is_empty() || self.busy() {
            return;
        }
        self.composer.take();
        // Remembered across launches: retyping a long prompt because the
        // process restarted is exactly the friction a terminal face should
        // not have.
        self.history.record(&prompt);
        self.history.save(&self.home);
        // Submitting anything is a commitment to watch the reply arrive, and
        // both command output and turns answer at the tail.
        self.scroll_back = 0;
        // Slash commands answer locally and never reach the model.
        if crate::commands::dispatch(self, &prompt) {
            return;
        }
        self.push(Role::User, prompt.clone());
        // No empty assistant bubble is opened: the first text delta creates it,
        // so a turn that calls tools first does not leave a blank row above them.
        self.answer_started = false;
        self.begin("working");

        let mut params = self.turn_params(&prompt);

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(CancellationToken::new());
        let worker_cancel = Arc::clone(&cancel);
        let home = self.home.clone();

        thread::spawn(move || {
            // Proving the crash arm needs a worker that really dies mid-turn
            // (#108). Debug builds only, same contract as
            // OPTIMUS_TUI_PANIC_ON_KEY: no released binary can be made to
            // panic by its environment; `tests/pty.rs` is the sole caller.
            #[cfg(debug_assertions)]
            if std::env::var_os("OPTIMUS_TUI_PANIC_IN_WORKER").is_some() {
                panic!("OPTIMUS_TUI_PANIC_IN_WORKER");
            }
            if !reservation::ensure(&home, &mut params, &tx) {
                return;
            }
            let mut sink = stream_sink(tx.clone());
            let outcome = chat_turn_cancellable(&home, params, Some(&mut sink), &worker_cancel);
            let final_update = match outcome {
                Ok(value) => TurnUpdate::Done {
                    session_id: value
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    text: value
                        .get("assistant_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                },
                Err(error) => TurnUpdate::Failed(error),
            };
            let _ = tx.send(final_update);
        });

        self.active = Some(ActiveTurn {
            updates: rx,
            cancel,
            kind: WorkerKind::Turn,
            awaiting_approval: false,
        });
    }

    /// Start Codex device-code sign-in on a worker.
    ///
    /// Selecting a provider that is not connected should *connect* it, not just
    /// relabel the session. The verification URL and code go into the transcript
    /// and the URL is opened in a browser; the worker polls until the user
    /// finishes or it times out.
    pub fn connect_codex(&mut self) {
        if self.busy() {
            return;
        }
        self.release_mouse_for_copying();
        self.push(Role::Assistant, "starting Codex sign-in…".into());
        self.begin("waiting for authorization");

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(CancellationToken::new());
        let home = self.home.clone();
        let prompt_tx = tx.clone();

        thread::spawn(move || {
            let outcome = CodexAuthStore::open(&home)
                .map_err(|e| e.to_string())
                .and_then(|store| {
                    device_code_login_with(&store, &mut |prompt| {
                        let _ = prompt_tx.send(TurnUpdate::Text(format!(
                            "\n  1. Open: {}\n  2. Enter code: {}\n",
                            prompt.verification_url, prompt.user_code
                        )));
                        // Best effort: the code above is enough on its own if no
                        // browser opens, so a failure here is not the flow failing.
                        let _ = optimus_host::handle_ipc(
                            &home,
                            "open_url",
                            json!({ "url": prompt.verification_url }),
                        );
                    })
                    .map_err(|e| e.to_string())
                });
            let _ = tx.send(match outcome {
                Ok(()) => TurnUpdate::Done {
                    session_id: String::new(),
                    text: "Codex connected. Tokens stored in this Optimus home.".into(),
                },
                Err(error) => TurnUpdate::Failed(format!("Codex sign-in failed: {error}")),
            });
        });

        self.active = Some(ActiveTurn {
            updates: rx,
            cancel,
            kind: WorkerKind::Connect,
            awaiting_approval: false,
        });
    }

    /// Build one turn's params. Separate from `submit` so tests can assert the
    /// wire shape — notably that /yolo rides the turn as `access: "yolo"`.
    /// `pub(super)` for the same reason as `approval_params`: those assertions
    /// live in the parent's test block, beside the rest of the surface.
    pub(super) fn turn_params(&self, prompt: &str) -> serde_json::Value {
        let mut params = json!({ "message": prompt });
        self.apply_model_choice(&mut params);
        if let Some(id) = &self.session_id {
            params["session"] = json!(id);
        }
        // /yolo applies to new effects too: the turn itself runs UnrestrictedHost.
        if self.yolo {
            params["access"] = json!("yolo");
        } else if let Some(profile) = self.access {
            params["access"] = json!(profile);
        }
        params
    }
}
