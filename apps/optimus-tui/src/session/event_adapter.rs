//! The typed event spine: kernel stream events in, transcript state out.
//!
//! Split out of `session.rs` under the module-size law, along the seam
//! ADR-0075 formalizes. [`TurnUpdate`] is deliberately a plain data enum
//! rather than a callback: it is the shape a future stdio or WebSocket
//! transport publishes, so a remote client and the terminal consume identical
//! events (ADR-0045). [`stream_sink`] is the only translation from kernel
//! stream events and [`pump`](TuiSession::pump) their only consumer, which
//! keeps lifecycle truth typed end to end — nothing here parses log or status
//! text to decide state.

use std::sync::mpsc::{self, TryRecvError};

use optimus_kernel::{StreamControl, StreamEvent, ToolApprovalBinding, ToolLifecyclePhase};

use crate::tool_line::{readable, tool_step};

use super::{Message, Role, TuiSession, WorkerKind};

/// One tool call's progress, already rendered for the transcript.
#[derive(Debug, Clone)]
pub struct ToolStep {
    /// Identity of the call, so successive phases update one row. The run id
    /// matters because providers may reuse the call id on a later turn.
    pub call_id: String,
    pub name: String,
    pub line: String,
    pub running: bool,
    /// The typed facts the block mirror consumes (ADR-0075 phase 1). Carried
    /// from the kernel event because deriving lifecycle from the rendered
    /// `line` would be exactly the log parsing the contract prohibits.
    pub run_id: String,
    pub event_id: String,
    pub phase: ToolLifecyclePhase,
    /// What the call produced, read from the typed outcome the kernel carries
    /// (ADR-0075 phase 3). The `line` above is a one-line summary; this is the
    /// body a reader opens the block to see.
    pub detail: crate::workbench::ToolDetail,
}

/// One observable step of a turn. The wire shape for any transport.
#[derive(Debug, Clone)]
pub enum TurnUpdate {
    /// A fresh turn reserved its durable identity before contacting a provider.
    SessionReserved(String),
    /// Assistant answer text, streamed.
    Text(String),
    /// Soft status for the footer; never mixed into the answer.
    Status(String),
    /// A tool call started, finished, or failed.
    Tool(ToolStep),
    /// An exact effect paused awaiting a human decision. The turn will park
    /// (settle as failed at the kernel API) while the job stays pending.
    Approval(Box<ToolApprovalBinding>),
    /// The decision was carried out, naming the call it settled. Not terminal:
    /// the turn it unblocked runs on, and whatever happens next belongs to that
    /// turn rather than to the decision. It is sent the moment the resolver
    /// returns, so a continuation that later fails cannot leave a spent card on
    /// the screen — and it names the call because that continuation may already
    /// have parked on an approval of its own, which must survive.
    ApprovalSettled(String),
    /// Terminal success: the durable session id and the settled answer.
    Done { session_id: String, text: String },
    /// Terminal failure, already rendered for a human.
    Failed(String),
}

/// Why a drain ended the turn (#108). Every clean worker path sends
/// `Done`/`Failed` before its sender drops; a channel that vanishes without
/// one is a worker that panicked past its final send, and the difference is
/// the difference between saying nothing and reporting a crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnEnd {
    Settled,
    WorkerGone,
}

/// Feed kernel stream events to the screen thread as transcript updates.
///
/// Shared by both workers. Resolving an approval resumes the turn it paused
/// (ADR-0046), so it is a streaming turn like any other and has to present the
/// same events the same way — a second translation would drift.
pub(super) fn stream_sink(
    stream: mpsc::Sender<TurnUpdate>,
) -> impl FnMut(StreamEvent) -> StreamControl {
    move |event: StreamEvent| {
        let update = match event {
            StreamEvent::TextDelta(text) => Some(TurnUpdate::Text(text)),
            StreamEvent::Status(text) => Some(TurnUpdate::Status(text)),
            // A paused exact effect needs a decision, so it becomes an action
            // row; every other tool phase is visible work and belongs in the
            // transcript. Thinking and timing detail do not — those belong in
            // a dock.
            StreamEvent::Tool(tool) => match (&tool.phase, &tool.approval) {
                (ToolLifecyclePhase::ApprovalRequired, Some(binding)) => {
                    Some(TurnUpdate::Approval(Box::new(binding.clone())))
                }
                _ => Some(TurnUpdate::Tool(tool_step(&tool))),
            },
            _ => None,
        };
        let Some(update) = update else {
            return StreamControl::Continue;
        };
        // A closed receiver means the screen is gone; stop the turn rather than
        // keep spending tokens into a void.
        if stream.send(update).is_err() {
            StreamControl::Cancel
        } else {
            StreamControl::Continue
        }
    }
}

impl TuiSession {
    /// Drain whatever the worker has produced. Never blocks. Returns whether
    /// the screen state changed and therefore needs a repaint.
    pub fn pump(&mut self) -> bool {
        let Some(active) = &self.active else {
            return false;
        };
        let kind = active.kind;
        let mut awaiting = active.awaiting_approval;
        // Collect first: applying updates needs `&mut self`, which cannot overlap
        // the borrow on the receiver.
        let mut batch = Vec::new();
        // Not a bool, because *why* the turn ended decides what the user is
        // told (#108). A worker that settles sends `Done`/`Failed` before its
        // sender drops; a channel that disappears without one means a panic
        // unwound past that final send. Queued updates are delivered before
        // `Disconnected`, so a worker that settled and then died still reads
        // as settled.
        let mut finished: Option<TurnEnd> = None;
        loop {
            match active.updates.try_recv() {
                Ok(update) => {
                    let terminal =
                        matches!(update, TurnUpdate::Done { .. } | TurnUpdate::Failed(_));
                    batch.push(update);
                    if terminal {
                        finished = Some(TurnEnd::Settled);
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = Some(TurnEnd::WorkerGone);
                    break;
                }
            }
        }

        let changed = !batch.is_empty() || finished.is_some();
        let mut parked = false;
        for update in batch {
            match update {
                TurnUpdate::SessionReserved(session_id) => {
                    self.session_id = Some(session_id);
                    self.refresh_sidebar();
                }
                TurnUpdate::Text(delta) => self.append_assistant(&delta),
                TurnUpdate::Status(status) => self.status = status,
                TurnUpdate::Tool(step) => self.apply_tool_step(step),
                TurnUpdate::Approval(binding) => {
                    awaiting = true;
                    // The bound call's block waits on the human; the card row
                    // pushed below is the note that says so.
                    self.workbench.hold_for_approval(&binding.call_id);
                    self.push(
                        Role::Action,
                        format!("approval required:\n{}", readable(&binding.summary)),
                    );
                    self.pending_approval = Some(binding);
                }
                TurnUpdate::ApprovalSettled(call_id) => {
                    // Only the card that settled. The resumed turn parks on its
                    // own approval before the resolver returns, so clearing
                    // whatever is held here would swallow that newer binding
                    // and leave a card `/approval` can only answer with "no
                    // approval is pending".
                    if self
                        .pending_approval
                        .as_ref()
                        .is_some_and(|binding| binding.call_id == call_id)
                    {
                        self.pending_approval = None;
                    }
                }
                TurnUpdate::Done { session_id, text } => {
                    if !session_id.is_empty() {
                        self.session_id = Some(session_id);
                    }
                    match kind {
                        // Providers that never streamed settle their answer
                        // here; ones that did already hold this text. Resolving
                        // is a turn too — it finishes the one the approval
                        // paused (ADR-0046) — so it settles the same way.
                        WorkerKind::Turn | WorkerKind::Resolve => {
                            if !self.answer_started && !text.is_empty() {
                                self.append_assistant(&text);
                            }
                        }
                        // This flow ends with a standalone settlement message.
                        WorkerKind::Connect => {
                            if let Some(provider) = self.connecting_provider.take() {
                                self.provider = provider;
                                self.remember_model_choice();
                                self.push(
                                    Role::Action,
                                    format!(
                                        "provider is now {} — remembered for next launch",
                                        self.provider
                                    ),
                                );
                            }
                            self.push(Role::Assistant, text);
                        }
                    }
                    self.workbench.settle_success();
                }
                TurnUpdate::Failed(error) => {
                    if kind == WorkerKind::Connect {
                        self.connecting_provider = None;
                    }
                    if matches!(kind, WorkerKind::Turn | WorkerKind::Resolve) && awaiting {
                        // Not a failure to the user: the effect is parked and
                        // the decision card is the next step. A resumed turn
                        // can park again on a second effect, and that is a
                        // decision to make, not an error to report.
                        parked = true;
                    } else {
                        self.push(Role::Error, error);
                    }
                    // Whatever was still streaming was interrupted, park or
                    // not; only a block waiting on a human survives settlement.
                    self.workbench.settle_interrupted();
                }
            }
        }

        if let Some(end) = finished {
            if matches!(end, TurnEnd::WorkerGone) {
                // The same failure #104 fixed for the main thread — a crash
                // that reads as silence — arriving through a worker. Its
                // panic payload went to the log (stderr is redirected for
                // the whole run), so this row is the only thing standing
                // between the user and "the spinner stopped and nothing
                // came back".
                // The path on its own line: transcript rows wrap at the pane
                // width, and a pointer nobody can read back is no pointer.
                self.push(
                    Role::Error,
                    format!(
                        "the turn stopped unexpectedly before finishing. details:\n{}",
                        crate::logging::log_path(&self.home).display()
                    ),
                );
                // The worker died without settling, so nothing above closed
                // the blocks it stranded.
                self.workbench.settle_interrupted();
            }
            self.active = None;
            self.status.clear();
            self.running_tool = None;
            self.started = None;
            self.refresh_sidebar();
            if parked {
                self.open_approval_picker();
            } else if kind == WorkerKind::Resolve && self.pending_approval.is_some() {
                // Still holding a binding here means the decision never
                // settled, so the card is the way to try again. A decision that
                // *did* settle cleared it on `ApprovalSettled`, which is what
                // stops a failed continuation from leaving a prompt that can
                // only ever answer "session has no approval-paused turn".
                self.open_approval_picker();
            }
        } else if let Some(active) = self.active.as_mut() {
            active.awaiting_approval = awaiting;
        }
        changed
    }

    /// Place a tool's progress, rewriting the row that call already owns.
    fn apply_tool_step(&mut self, step: ToolStep) {
        // The block mirror first, from the typed phase the step carries; the
        // row below stays the projection that paints. Both key on the typed
        // `(run_id, call_id)` identity, so a provider reusing `call_1` on a
        // later turn cannot rewrite an older row.
        self.workbench.apply_tool_step(&step);
        self.running_tool = step.running.then(|| step.name.clone());
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
        debug_assert_eq!(
            self.workbench.len(),
            self.messages.len(),
            "every row has exactly one block (ADR-0075 phase 1)"
        );
    }

    fn append_assistant(&mut self, delta: &str) {
        self.answer_started = true;
        match self.messages.last_mut() {
            Some(message) if message.role == Role::Assistant => {
                message.text.push_str(delta);
                self.workbench.extend_assistant();
            }
            _ => self.push(Role::Assistant, delta.to_string()),
        }
    }
}
