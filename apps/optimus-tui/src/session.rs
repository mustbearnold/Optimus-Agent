//! Session state for the terminal face.
//!
//! Turns go through `optimus_host::chat_turn_cancellable`, the same entry the
//! desktop uses, so the TUI cannot drift onto a private path.
//!
//! The turn runs on a worker thread and reports back over a channel. The screen
//! thread never blocks on a model call, which is what lets text stream in and
//! Ctrl-C interrupt a run in flight.
//!
//! Split along its seams under the module-size law (ADR-0075): this file keeps
//! session state and its small moves; [`event_adapter`] owns the typed wire
//! shape ([`TurnUpdate`]) and its consumption; [`workers`] spawns the turn and
//! connect workers; [`approval`] decides parked effects; [`reservation`]
//! secures durable identity before a provider is contacted.

use std::path::PathBuf;
#[cfg(test)]
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use optimus_host::handle_ipc;
use optimus_kernel::{CancellationToken, ToolApprovalBinding};
use serde_json::json;

use crate::composer::Composer;
use crate::history::History;
use crate::preferences::Preferences;
use crate::transcript::Chrome;
use crate::workbench::WorkbenchState;

mod approval;
mod event_adapter;
mod reservation;
mod workers;

pub use event_adapter::{ToolStep, TurnUpdate};

// All are exercised from this module's test block, beside the rest of the
// surface's behaviour; none is called from production code up here.
#[cfg(test)]
use crate::tool_line::tool_step;
#[cfg(test)]
pub(crate) use approval::approval_binding_fixture;
#[cfg(test)]
use approval::{decision_line, resolved_update};

/// Braille frames, the conventional terminal spinner.
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Render frames per spinner step. The loop polls at 40ms, so this animates at
/// roughly 12 steps a second — visible motion without strobing.
const SPINNER_EVERY: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Error,
    /// A pending exact action that needs a human decision.
    Action,
    /// One tool call's lifecycle, updated in place as it progresses.
    Tool,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub text: String,
    /// Set on [`Role::Tool`] rows so a later lifecycle phase for the same call
    /// rewrites the row it already owns instead of appending a duplicate.
    pub call_id: Option<String>,
}

/// What the worker thread is doing, which decides how its terminal update is
/// rendered: turns stream into an open assistant bubble, the other kinds settle
/// with a standalone message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerKind {
    Turn,
    Connect,
    Resolve,
}

struct ActiveTurn {
    updates: Receiver<TurnUpdate>,
    cancel: Arc<CancellationToken>,
    kind: WorkerKind,
    /// Set once this worker's stream reported an `Approval`, so its terminal
    /// failure is rendered as a park rather than an error.
    awaiting_approval: bool,
}

pub struct TuiSession {
    pub home: PathBuf,
    pub messages: Vec<Message>,
    /// The block mirror of `messages`: `workbench.blocks()[i]` describes
    /// `messages[i]` (ADR-0075 phase 1). Blocks own identity, lifecycle, and
    /// provenance; the rows stay the compatibility projection that paints.
    pub workbench: WorkbenchState,
    pub composer: Composer,
    /// Set by `/quit`; the event loop leaves on the next pass.
    pub quit: bool,
    /// Prompts already sent, for recall with Up/Down.
    pub history: History,
    pub provider: String,
    /// Model id to override the provider's default, or None to accept it.
    pub model: Option<String>,
    /// Reasoning effort, or None for the backend's own default.
    pub thinking: Option<String>,
    pub session_id: Option<String>,
    pub status: String,
    pub yolo: bool,
    /// Bounded ADR-0044 profile for new turns, chosen with /access. Canonical
    /// wire strings only; never the break-glass profile — that stays /yolo.
    pub access: Option<&'static str>,
    pub picker: Option<crate::picker::Picker>,
    /// Which command suggestion is highlighted; the list itself is derived.
    pub completion: crate::completion::Completion,
    /// The exact binding of a parked effect, held until a decision resolves it.
    pub pending_approval: Option<Box<ToolApprovalBinding>>,
    /// Rows scrolled up from the tail of the transcript; 0 follows new text.
    pub scroll_back: usize,
    /// Name of the tool currently in flight, shown beside the spinner.
    pub running_tool: Option<String>,
    /// How turns are framed. Containers read better; plain copies better.
    pub chrome: Chrome,
    /// True while the scrollbar thumb is held, so motion keeps scrolling even
    /// once the pointer wanders off the one-column track.
    pub dragging: bool,
    /// Whether the terminal's mouse is captured. Capture buys the scrollbar and
    /// the menus, and costs the terminal's own click-and-drag text selection,
    /// so it has to be surrenderable.
    pub mouse: bool,
    /// Render frames since start, driving the spinner animation.
    frame: usize,
    /// When the active worker began, for the elapsed counter.
    started: Option<Instant>,
    /// Whether this turn has produced any answer text yet, which decides
    /// whether a terminal `Done` still needs to place the settled answer.
    answer_started: bool,
    active: Option<ActiveTurn>,
}

impl TuiSession {
    pub fn new(home: PathBuf) -> Self {
        // Choosing a provider is a decision about how you want to work, not
        // about this run of the program. Re-asking every launch treats it as
        // the latter.
        let remembered = Preferences::load(&home);
        let history = History::load(&home);
        Self {
            home,
            messages: Vec::new(),
            workbench: WorkbenchState::default(),
            composer: Composer::new(),
            quit: false,
            history,
            provider: remembered.provider,
            model: remembered.model,
            thinking: remembered.thinking,
            session_id: None,
            status: String::new(),
            yolo: false,
            access: None,
            picker: None,
            completion: crate::completion::Completion::default(),
            pending_approval: None,
            scroll_back: 0,
            running_tool: None,
            chrome: Chrome::Boxed,
            dragging: false,
            mouse: true,
            frame: 0,
            started: None,
            answer_started: false,
            active: None,
        }
    }

    pub fn busy(&self) -> bool {
        self.active.is_some()
    }

    /// Advance the spinner. Called once per render frame.
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// The activity line shown under the transcript while a worker runs.
    ///
    /// Returns `None` when idle, so the row simply is not laid out — an
    /// always-present placeholder would read as a stuck spinner.
    pub fn activity_line(&self, width: u16) -> Option<String> {
        if !self.busy() {
            return None;
        }
        let spinner = SPINNER[(self.frame / SPINNER_EVERY) % SPINNER.len()];
        let what = match (&self.running_tool, self.status.as_str()) {
            (Some(tool), _) => tool.as_str(),
            (None, "") => "working",
            (None, status) => status,
        };
        Some(crate::activity::text(
            spinner,
            what,
            self.elapsed_secs(),
            usize::from(width),
        ))
    }

    fn elapsed_secs(&self) -> u64 {
        self.started.map_or(0, |at| at.elapsed().as_secs())
    }

    /// Hand the mouse back before printing something the user must copy.
    ///
    /// Capture costs the terminal's own click-and-drag selection, so a flow
    /// whose whole purpose is moving a string from this pane into a browser
    /// cannot hold it — printing a code nobody can select is the same as not
    /// printing it. Announced, because a silent capability change is a bug
    /// report waiting to happen, and `/mouse` is how it comes back.
    ///
    /// Not restored automatically: capture is surrendered on the user's behalf,
    /// and taking it back without being asked undoes the surrender.
    fn release_mouse_for_copying(&mut self) {
        if !self.mouse {
            return;
        }
        self.mouse = false;
        self.push(
            Role::Assistant,
            "mouse released so you can select the code — `/mouse` takes it back".into(),
        );
    }

    /// Apply the highlighted picker row and close it.
    pub fn confirm_picker(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        let Some(item) = picker.confirm().cloned() else {
            return;
        };
        match picker.kind {
            crate::picker::PickerKind::Provider => {
                if item.id == "auto" && self.model.is_some() {
                    self.push(
                        Role::Error,
                        "choose model Auto before returning provider selection to Auto".into(),
                    );
                    return;
                }
                let changed = item.id != self.provider;
                if changed {
                    self.provider = item.id.clone();
                    self.remember_model_choice();
                    self.push(
                        Role::Assistant,
                        format!("provider is now {} — remembered for next launch", item.id),
                    );
                }
                // Picking a provider that is not connected should connect it.
                if !item.connected && item.id == "codex" {
                    self.connect_codex();
                } else if !item.connected && changed {
                    self.push(
                        Role::Error,
                        format!("{} is not connected — {}", item.id, item.detail),
                    );
                }
            }
            crate::picker::PickerKind::Approval => self.resolve_approval(&item.id),
            // Route back through dispatch so a menu row and the typed command
            // cannot drift apart.
            crate::picker::PickerKind::Command => {
                crate::commands::dispatch(self, &format!("/{}", item.id));
            }
        }
    }

    /// Move the transcript so the thumb sits `fraction` down the track, where
    /// 0.0 is the oldest row on screen and 1.0 is the live tail.
    pub fn scroll_to(&mut self, fraction: f64, max_back: usize) {
        let back = (1.0 - fraction.clamp(0.0, 1.0)) * max_back as f64;
        self.scroll_back = back.round() as usize;
    }

    /// Write the current provider, model and effort down for the next launch.
    ///
    /// Called by the commands that change them rather than on every turn: a
    /// preference is what the user chose, not what the last request happened to
    /// carry.
    pub fn remember_model_choice(&self) {
        Preferences {
            provider: self.provider.clone(),
            model: self.model.clone(),
            thinking: self.thinking.clone(),
        }
        .save(&self.home);
    }

    /// Stamp who should answer, and how hard, onto an outgoing request.
    ///
    /// Shared by a fresh turn and by the continuation of an approved one. The
    /// A resumed explicit-provider turn must keep that provider rather than
    /// silently falling back to automatic selection. Omitted model and thinking
    /// keys mean "no override", which is not the same as an empty string and
    /// must not be sent as one.
    fn apply_model_choice(&self, params: &mut serde_json::Value) {
        params["provider"] = json!(self.provider);
        if let Some(model) = &self.model {
            params["model"] = json!(model);
        }
        if let Some(thinking) = &self.thinking {
            params["thinking_level"] = json!(thinking);
        }
    }

    /// Ask the running turn to stop. The worker settles it as failed or done.
    pub fn cancel(&mut self) {
        if let Some(active) = &self.active {
            active.cancel.cancel();
            self.status = "cancelling".into();
        }
    }

    /// Put the session in the running state with no worker behind it, so render
    /// tests can photograph the spinner. The returned sender keeps the channel
    /// open; dropping it settles the turn.
    #[cfg(test)]
    pub(crate) fn busy_for_test(&mut self, status: &str) -> mpsc::Sender<TurnUpdate> {
        let (tx, rx) = mpsc::channel();
        self.active = Some(ActiveTurn {
            updates: rx,
            cancel: Arc::new(CancellationToken::new()),
            kind: WorkerKind::Turn,
            awaiting_approval: false,
        });
        self.begin(status);
        tx
    }

    /// Mark a worker as starting: reset the elapsed clock and say what it does.
    fn begin(&mut self, status: &str) {
        self.status = status.into();
        self.started = Some(Instant::now());
        self.running_tool = None;
    }

    pub fn push(&mut self, role: Role, text: String) {
        self.workbench.push_note(role, self.active.is_some());
        self.messages.push(Message {
            role,
            text,
            call_id: None,
        });
        debug_assert_eq!(
            self.workbench.len(),
            self.messages.len(),
            "every row has exactly one block (ADR-0075 phase 1)"
        );
    }

    /// Scroll the transcript `delta` rows away from the tail, clamped to
    /// `max_back` so PageUp cannot run past the top. Negative moves toward the
    /// tail; reaching it re-enables follow.
    pub fn scroll(&mut self, delta: isize, max_back: usize) {
        self.scroll_back = self.scroll_back.saturating_add_signed(delta).min(max_back);
    }

    /// Status line contents, kept here so it is assertable without a terminal.
    pub fn status_line(&self) -> String {
        let state = if self.busy() {
            if self.status.is_empty() {
                "working"
            } else {
                self.status.as_str()
            }
        } else if self.pending_approval.is_some() {
            "approval required — /approval to decide"
        } else {
            "ready"
        };
        // A remembered choice the user cannot see is indistinguishable from a
        // forgotten one, and the next turn spends their tokens on it either
        // way. Overrides are shown; the provider's own defaults stay quiet.
        let model = self
            .model
            .as_ref()
            .map(|model| format!("/{model}"))
            .unwrap_or_default();
        let thinking = self
            .thinking
            .as_ref()
            .map(|level| format!(" · think:{level}"))
            .unwrap_or_default();
        format!(
            "{}{}{} · {} · {}{}",
            self.provider,
            model,
            thinking,
            self.session_id.as_deref().unwrap_or("new session"),
            state,
            if self.yolo { " · YOLO" } else { "" }
        )
    }
}

/// The most recently touched durable session in this home.
///
/// A turn that parks on an approval settles as an error, which carries no
/// session id — but resolution needs one. The turn that just parked is by
/// construction the newest session, so recover the id from the durable list.
fn latest_session_id(home: &PathBuf) -> Option<String> {
    let value = handle_ipc(home, "sessions", json!({})).ok()?;
    let sessions = value.get("sessions")?.as_array()?.clone();
    sessions
        .iter()
        .max_by_key(|row| {
            row.get("updated_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        })
        .and_then(|row| row.get("id").and_then(|v| v.as_str()).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{Duration, Instant};

    use optimus_kernel::ToolLifecycleEvent;
    use tempfile::tempdir;

    fn session() -> (tempfile::TempDir, TuiSession) {
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        (dir, TuiSession::new(home))
    }

    /// Install a worker whose updates the test scripts by hand.
    fn install_worker(session: &mut TuiSession, kind: WorkerKind) -> mpsc::Sender<TurnUpdate> {
        let (tx, rx) = mpsc::channel();
        session.active = Some(ActiveTurn {
            updates: rx,
            cancel: Arc::new(CancellationToken::new()),
            kind,
            awaiting_approval: false,
        });
        tx
    }

    /// Pump until the turn settles, the way the render loop does.
    fn settle(session: &mut TuiSession) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while session.busy() && Instant::now() < deadline {
            session.pump();
            thread::sleep(Duration::from_millis(5));
        }
        session.pump();
    }

    #[test]
    fn auto_is_the_default_for_a_first_run() {
        let (_dir, session) = session();
        assert_eq!(session.provider, "auto");
        assert_eq!(session.status_line(), "auto · new session · ready");
    }

    #[test]
    fn a_remembered_model_and_effort_are_visible_before_the_turn_spends_them() {
        let (_dir, mut session) = session();
        session.provider = "codex".into();
        session.model = Some("gpt-5-codex".into());
        session.thinking = Some("high".into());
        assert_eq!(
            session.status_line(),
            "codex/gpt-5-codex · think:high · new session · ready"
        );
    }

    #[test]
    fn submit_returns_immediately_so_the_screen_never_blocks() {
        let (_dir, mut session) = session();
        session.composer.set("hello from the tui");
        let before = Instant::now();
        session.submit();
        assert!(
            before.elapsed() < Duration::from_millis(200),
            "submit must hand off to the worker, not run the turn inline"
        );
        settle(&mut session);
    }

    #[test]
    fn a_turn_records_both_roles_and_settles() {
        let (_dir, mut session) = session();
        session.composer.set("hello from the tui");
        session.submit();
        settle(&mut session);

        assert!(!session.busy());
        assert_eq!(session.composer.text(), "");
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, Role::User);
        assert_eq!(session.messages[0].text, "hello from the tui");
        assert_eq!(session.messages[1].role, Role::Assistant);
        assert_eq!(
            session.messages[1].text, "offline echo: hello from the tui",
            "fresh Auto routing resolved to the deterministic offline provider"
        );
    }

    #[test]
    fn the_session_id_is_captured_so_a_second_turn_continues_it() {
        let (_dir, mut session) = session();
        session.composer.set("first");
        session.submit();
        settle(&mut session);
        let first = session.session_id.clone().expect("session id");

        session.composer.set("second");
        session.submit();
        settle(&mut session);
        assert_eq!(session.session_id.as_deref(), Some(first.as_str()));
        assert_eq!(session.messages.len(), 4);
    }

    #[test]
    fn a_failed_first_turn_keeps_its_session_for_the_follow_up() {
        let (_dir, mut session) = session();
        session.provider = "missing-provider".into();
        session.composer.set("first request");
        session.submit();
        settle(&mut session);
        let first = session
            .session_id
            .clone()
            .expect("the durable id exists before provider failure");
        assert_eq!(
            session.turn_params("continue that request")["session"],
            serde_json::json!(first),
            "the follow-up is bound before another provider attempt"
        );

        session.composer.set("continue that request");
        session.submit();
        settle(&mut session);

        assert_eq!(session.session_id.as_deref(), Some(first.as_str()));
        let store = optimus_kernel::SessionStore::open(session.home.join("sessions.db")).unwrap();
        let sessions = store.list().unwrap();
        assert_eq!(sessions.len(), 1, "follow-up must not fork");
        assert_eq!(sessions[0].id.to_string(), first);
    }

    #[test]
    fn blank_input_is_not_a_turn() {
        let (_dir, mut session) = session();
        session.composer.set("   ");
        session.submit();
        assert!(!session.busy());
        assert!(session.messages.is_empty());
    }

    #[test]
    fn a_second_submit_is_refused_while_a_turn_is_running() {
        let (_dir, mut session) = session();
        session.composer.set("first");
        session.submit();
        session.composer.set("second");
        session.submit();
        assert_eq!(
            session.composer.text(),
            "second",
            "the refused input stays in the composer"
        );
        settle(&mut session);
    }

    #[test]
    fn yolo_rides_the_turn_params_as_unrestricted_access() {
        let (_dir, mut session) = session();
        assert!(session.turn_params("hi").get("access").is_none());
        session.yolo = true;
        assert_eq!(
            session.turn_params("hi")["access"],
            serde_json::json!("yolo"),
            "after /yolo, new effects must run under the unrestricted profile"
        );
    }

    #[test]
    fn a_selected_access_profile_rides_the_turn_params() {
        let (_dir, mut session) = session();
        session.access = Some("standard");
        assert_eq!(
            session.turn_params("hi")["access"],
            serde_json::json!("standard"),
            "the chosen bounded profile must reach the host"
        );
    }

    #[test]
    fn yolo_outranks_a_selected_access_profile() {
        let (_dir, mut session) = session();
        session.access = Some("standard");
        session.yolo = true;
        assert_eq!(
            session.turn_params("hi")["access"],
            serde_json::json!("yolo"),
            "break-glass, once released, is the wider explicit grant"
        );
    }

    #[test]
    fn a_selected_access_profile_rides_the_resumed_turn_too() {
        let (_dir, mut session) = session();
        session.session_id = Some("11111111-1111-4111-8111-111111111111".into());
        session.pending_approval = Some(approval_binding_fixture());
        session.access = Some("full_project");
        let params = session.approval_params("approve").expect("resolves");
        assert_eq!(params["access"], serde_json::json!("full_project"));
    }

    #[test]
    fn an_unset_model_or_effort_is_absent_rather_than_empty() {
        // The host reads these as overrides. An empty string is an override to
        // nothing, which is not what "I did not choose" means.
        let (_dir, session) = session();
        let params = session.turn_params("hi");
        assert!(params.get("model").is_none());
        assert!(params.get("thinking_level").is_none());
        assert_eq!(params["provider"], serde_json::json!("auto"));
    }

    #[test]
    fn a_chosen_model_and_effort_ride_the_turn() {
        let (_dir, mut session) = session();
        session.model = Some("gpt-5".into());
        session.thinking = Some("high".into());
        let params = session.turn_params("hi");
        assert_eq!(params["model"], serde_json::json!("gpt-5"));
        assert_eq!(params["thinking_level"], serde_json::json!("high"));
    }

    /// Resumption keeps an explicit provider/model choice instead of asking the
    /// automatic selector to make a different decision mid-turn.
    #[test]
    fn a_resumed_turn_answers_on_the_provider_the_paused_one_used() {
        let (_dir, mut session) = session();
        session.provider = "offline".into();
        session.model = Some("gpt-5".into());
        session.thinking = Some("low".into());
        session.session_id = Some("11111111-1111-4111-8111-111111111111".into());
        session.pending_approval = Some(approval_binding_fixture());

        let params = session
            .approval_params("approve")
            .expect("a pending approval resolves");
        assert_eq!(params["provider"], serde_json::json!("offline"));
        assert_eq!(params["model"], serde_json::json!("gpt-5"));
        assert_eq!(params["thinking_level"], serde_json::json!("low"));
    }

    #[test]
    fn yolo_rides_the_resumed_turn_too() {
        let (_dir, mut session) = session();
        session.session_id = Some("11111111-1111-4111-8111-111111111111".into());
        session.pending_approval = Some(approval_binding_fixture());
        session.yolo = true;
        let params = session.approval_params("approve").expect("resolves");
        assert_eq!(params["access"], serde_json::json!("yolo"));
    }

    #[test]
    fn cancelling_settles_the_turn_instead_of_hanging() {
        let (_dir, mut session) = session();
        session.composer.set("cancel me");
        session.submit();
        session.cancel();
        settle(&mut session);
        assert!(
            !session.busy(),
            "a cancelled turn must reach a terminal state"
        );
    }

    #[test]
    fn an_approval_parks_the_turn_as_a_decision_not_an_error() {
        let (_dir, mut session) = session();
        // Mirror the state submit() leaves behind: the prompt, nothing more.
        session.push(Role::User, "write the proof".into());
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Approval(super::approval_binding_fixture()))
            .unwrap();
        tx.send(TurnUpdate::Failed("needs approval: job parked".into()))
            .unwrap();
        settle(&mut session);

        assert!(!session.busy());
        assert!(session.pending_approval.is_some(), "binding must be held");
        assert!(
            !session.messages.iter().any(|m| m.role == Role::Error),
            "a parked effect is a decision, not an error: {:?}",
            session.messages
        );
        let last = session.messages.last().unwrap();
        assert_eq!(last.role, Role::Action);
        assert_eq!(
            last.text,
            "approval required:\nWrite src/proof.txt (4 bytes)"
        );
        assert_eq!(
            session.messages.len(),
            2,
            "no blank assistant row may linger above the card"
        );
        let picker = session.picker.as_ref().expect("decision picker opens");
        assert_eq!(picker.kind, crate::picker::PickerKind::Approval);
        assert!(session.status_line().contains("approval required"));
    }

    #[test]
    fn text_that_streamed_before_the_park_is_kept() {
        let (_dir, mut session) = session();
        session.push(Role::User, "write the proof".into());
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Text("working on it".into())).unwrap();
        tx.send(TurnUpdate::Approval(super::approval_binding_fixture()))
            .unwrap();
        tx.send(TurnUpdate::Failed("needs approval".into()))
            .unwrap();
        settle(&mut session);

        assert_eq!(session.messages[1].role, Role::Assistant);
        assert_eq!(session.messages[1].text, "working on it");
        assert_eq!(session.messages[2].role, Role::Action);
    }

    #[test]
    fn approval_params_carry_the_exact_binding_and_nothing_invented() {
        let (_dir, mut session) = session();
        session.pending_approval = Some(super::approval_binding_fixture());
        session.session_id = Some("44444444-4444-4444-8444-444444444444".into());
        let params = session.approval_params("approve").unwrap();
        assert_eq!(params["session_id"], "44444444-4444-4444-8444-444444444444");
        assert_eq!(params["run_id"], "11111111-1111-4111-8111-111111111111");
        assert_eq!(params["call_id"], "write-1");
        assert_eq!(params["job_id"], "22222222-2222-4222-8222-222222222222");
        assert_eq!(params["node_id"], "33333333-3333-4333-8333-333333333333");
        assert_eq!(params["node_index"], 3);
        assert_eq!(params["effect_sha256"], "ab".repeat(32));
        assert_eq!(params["decision"], "approve");
        assert!(
            params.get("project_id").is_none(),
            "this surface must not invent project authority"
        );
    }

    /// The answer to a resumed turn is the agent's; the surface only records
    /// which decision was taken, and does so in the agent's absence.
    #[test]
    fn a_decision_is_recorded_without_speaking_for_the_agent() {
        assert!(super::decision_line("approve").starts_with("approved"));
        assert!(super::decision_line("deny").starts_with("denied"));
        assert!(
            !super::decision_line("approve").contains("receipt recorded"),
            "the surface reports the decision, not the outcome it has not seen"
        );
    }

    /// The turn no longer ends when an approval settles (ADR-0046), so a
    /// settled decision carries the resumed answer.
    #[test]
    fn a_settled_approval_carries_the_resumed_answer() {
        let update = super::resolved_update(&json!({
            "session_id": "55555555-5555-4555-8555-555555555555",
            "status": "approved",
            "assistant_text": "Wrote it.",
        }));
        match update {
            TurnUpdate::Done { session_id, text } => {
                assert_eq!(session_id, "55555555-5555-4555-8555-555555555555");
                assert_eq!(text, "Wrote it.");
            }
            other => panic!("a settled approval must finish the turn: {other:?}"),
        }
    }

    /// The decision succeeded; the continuation did not. Reporting that as a
    /// failed approval would be a lie — the effect ran and is receipted.
    #[test]
    fn a_failed_continuation_is_reported_as_a_turn_failure() {
        let update = super::resolved_update(&json!({
            "status": "approved",
            "resume_error": "provider unreachable",
        }));
        match update {
            TurnUpdate::Failed(error) => assert_eq!(error, "provider unreachable"),
            other => panic!("a broken continuation must surface: {other:?}"),
        }
    }

    /// A resumed turn can reach a second held effect. That is another decision
    /// to make, not an error to show.
    #[test]
    fn a_second_park_during_a_resumed_turn_is_a_card_not_an_error() {
        let (_dir, mut session) = session();
        let tx = install_worker(&mut session, WorkerKind::Resolve);
        tx.send(TurnUpdate::Approval(super::approval_binding_fixture()))
            .unwrap();
        tx.send(TurnUpdate::Failed("needs approval: job parked".into()))
            .unwrap();
        settle(&mut session);

        assert!(session.pending_approval.is_some(), "binding must be held");
        assert!(
            !session
                .messages
                .iter()
                .any(|message| message.role == Role::Error),
            "a park is not an error: {:?}",
            session.messages
        );
    }

    #[test]
    fn resolving_with_no_session_anywhere_fails_closed_with_a_message() {
        let (_dir, mut session) = session();
        session.pending_approval = Some(super::approval_binding_fixture());
        session.resolve_approval("approve");
        assert!(!session.busy(), "nothing to resolve against, no worker");
        assert!(session.pending_approval.is_some(), "card stays actionable");
        let last = session.messages.last().unwrap();
        assert_eq!(last.role, Role::Error);
        assert!(last.text.contains("no session"), "{}", last.text);
    }

    #[test]
    fn a_resolution_success_clears_the_card_and_lands_the_answer() {
        let (_dir, mut session) = session();
        session.pending_approval = Some(super::approval_binding_fixture());
        let tx = install_worker(&mut session, WorkerKind::Resolve);
        tx.send(TurnUpdate::ApprovalSettled("write-1".into()))
            .unwrap();
        tx.send(TurnUpdate::Done {
            session_id: "55555555-5555-4555-8555-555555555555".into(),
            text: "Wrote src/proof.txt as asked.".into(),
        })
        .unwrap();
        settle(&mut session);

        assert!(session.pending_approval.is_none());
        assert!(session.picker.is_none(), "nothing left to decide");
        let last = session.messages.last().unwrap();
        assert_eq!(last.role, Role::Assistant);
        assert!(last.text.starts_with("Wrote src/proof.txt"));
        assert_eq!(
            session.session_id.as_deref(),
            Some("55555555-5555-4555-8555-555555555555")
        );
    }

    #[test]
    fn settling_one_card_never_swallows_the_one_the_continuation_raised() {
        // Observed live on `github trending`: the approved bash call settled,
        // the resumed turn immediately parked on a second command, and the
        // settlement — which arrives last, because the resolver returns after
        // the continuation has already streamed — cleared the new binding.
        // `/approval` then answered "no approval is pending" against a card
        // sitting in plain sight.
        let (_dir, mut session) = session();
        session.pending_approval = Some(super::approval_binding_fixture());
        let tx = install_worker(&mut session, WorkerKind::Resolve);

        let mut second = super::approval_binding_fixture();
        second.call_id = "curl-2".into();
        second.summary = "Run \"bash\" with args [\"-lc\",\"curl …\"]".into();
        tx.send(TurnUpdate::Approval(second)).unwrap();
        tx.send(TurnUpdate::ApprovalSettled("write-1".into()))
            .unwrap();
        tx.send(TurnUpdate::Failed("parked".into())).unwrap();
        settle(&mut session);

        let held = session
            .pending_approval
            .as_ref()
            .expect("the continuation's own card must survive settlement");
        assert_eq!(held.call_id, "curl-2");
        assert!(session.picker.is_some(), "and must be decidable");
    }

    #[test]
    fn a_settled_approval_leaves_no_card_when_the_resumed_turn_dies() {
        // Observed live: the decision ran, the continuation hit the step
        // budget, and the spent card was re-offered. Answering it could only
        // ever return "session has no approval-paused turn" — a prompt with no
        // way out.
        let (_dir, mut session) = session();
        session.pending_approval = Some(super::approval_binding_fixture());
        let tx = install_worker(&mut session, WorkerKind::Resolve);
        tx.send(TurnUpdate::ApprovalSettled("write-1".into()))
            .unwrap();
        tx.send(TurnUpdate::Failed("max steps exceeded (32)".into()))
            .unwrap();
        settle(&mut session);

        assert!(
            session.pending_approval.is_none(),
            "the decision was already carried out; the card is spent"
        );
        assert!(session.picker.is_none(), "nothing left to decide");
        assert!(
            session
                .messages
                .iter()
                .any(|m| m.role == Role::Error && m.text.contains("max steps")),
            "the real failure still has to reach the user"
        );
    }

    #[test]
    fn a_failed_resolution_keeps_the_card_actionable() {
        let (_dir, mut session) = session();
        session.pending_approval = Some(super::approval_binding_fixture());
        let tx = install_worker(&mut session, WorkerKind::Resolve);
        tx.send(TurnUpdate::Failed(
            "approval resolution failed: binding mismatch".into(),
        ))
        .unwrap();
        settle(&mut session);

        assert!(
            session.pending_approval.is_some(),
            "canonical pending state is retained on failure"
        );
        let picker = session.picker.as_ref().expect("picker reopens for retry");
        assert_eq!(picker.kind, crate::picker::PickerKind::Approval);
        assert!(session
            .messages
            .iter()
            .any(|m| m.role == Role::Error && m.text.contains("resolution failed")));
    }

    #[test]
    fn a_real_resolution_against_an_empty_home_fails_closed_and_keeps_the_card() {
        let (_dir, mut session) = session();
        session.pending_approval = Some(super::approval_binding_fixture());
        session.session_id = Some("44444444-4444-4444-8444-444444444444".into());
        session.resolve_approval("approve");
        assert!(session.busy(), "resolution runs on a worker");
        assert_eq!(session.status, "resolving approval");
        settle(&mut session);
        assert!(
            session.pending_approval.is_some(),
            "no durable job exists, so the runtime must refuse and the card stays"
        );
        assert!(session
            .messages
            .iter()
            .any(|m| m.role == Role::Error && m.text.contains("resolution failed")));
    }

    #[test]
    fn connect_completion_text_reaches_the_transcript() {
        let (_dir, mut session) = session();
        session.push(Role::Assistant, "starting Codex sign-in…".into());
        let tx = install_worker(&mut session, WorkerKind::Connect);
        tx.send(TurnUpdate::Done {
            session_id: String::new(),
            text: "Codex connected. Tokens stored in this Optimus home.".into(),
        })
        .unwrap();
        settle(&mut session);

        let last = session.messages.last().unwrap();
        assert_eq!(last.role, Role::Assistant);
        assert_eq!(
            last.text, "Codex connected. Tokens stored in this Optimus home.",
            "the settlement must not be swallowed because an earlier bubble has text"
        );
    }

    #[test]
    fn submitting_snaps_the_view_back_to_the_tail() {
        let (_dir, mut session) = session();
        session.scroll_back = 7;
        session.composer.set("hello");
        session.submit();
        assert_eq!(session.scroll_back, 0);
        settle(&mut session);
    }

    /// Built through serde rather than the struct literal: `ToolId` belongs to
    /// `optimus-packs`, which this surface deliberately does not depend on.
    fn tool_event(phase: &str, summary: &str, ms: Option<u64>) -> ToolLifecycleEvent {
        serde_json::from_value(json!({
            "schema_version": 1,
            "event_id": format!("run-1:call-1:{phase}"),
            "run_id": "run-1",
            "call_id": "call-1",
            "tool_id": "web_search",
            "phase": phase,
            "summary": summary,
            "duration_ms": ms,
        }))
        .expect("fixture tool event deserializes")
    }

    #[test]
    fn a_tool_call_occupies_one_row_that_updates_through_its_lifecycle() {
        let (_dir, mut session) = session();
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Tool(super::tool_step(&tool_event(
            "started", "", None,
        ))))
        .unwrap();
        session.pump();
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, Role::Tool);
        assert_eq!(session.messages[0].text, "web_search  running");
        assert_eq!(session.running_tool.as_deref(), Some("web_search"));

        tx.send(TurnUpdate::Tool(super::tool_step(&tool_event(
            "succeeded",
            "Found 3 sources",
            Some(1240),
        ))))
        .unwrap();
        session.pump();
        assert_eq!(
            session.messages.len(),
            1,
            "the finished call must rewrite its own row, not append a second"
        );
        assert_eq!(
            session.messages[0].text,
            "web_search  Found 3 sources  (1.2s)"
        );
        assert!(session.running_tool.is_none(), "nothing is in flight now");
    }

    #[test]
    fn a_failed_tool_never_reads_like_a_success() {
        let (_dir, mut session) = session();
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Tool(super::tool_step(&tool_event(
            "failed",
            "host unreachable",
            Some(80),
        ))))
        .unwrap();
        session.pump();
        assert_eq!(
            session.messages[0].text,
            "web_search  failed: host unreachable  (80ms)"
        );
    }

    #[test]
    fn tools_from_different_calls_get_their_own_rows() {
        let (_dir, mut session) = session();
        let mut second = tool_event("started", "", None);
        second.call_id = "call-2".into();
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Tool(super::tool_step(&tool_event(
            "started", "", None,
        ))))
        .unwrap();
        tx.send(TurnUpdate::Tool(super::tool_step(&second)))
            .unwrap();
        session.pump();
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn the_spinner_animates_and_names_the_running_tool() {
        let (_dir, mut session) = session();
        assert!(
            session.activity_line(80).is_none(),
            "an idle session shows no spinner"
        );

        let _tx = install_worker(&mut session, WorkerKind::Turn);
        session.begin("working");
        let first = session.activity_line(80).expect("a running turn spins");
        assert!(first.contains("working"), "{first}");
        assert!(first.contains("Ctrl-C to interrupt"), "{first}");

        // Advancing frames must change the glyph, or it reads as frozen.
        let mut glyphs = std::collections::HashSet::new();
        for _ in 0..40 {
            session.tick();
            glyphs.insert(session.activity_line(80).unwrap().chars().next().unwrap());
        }
        assert!(glyphs.len() > 1, "the spinner must actually animate");

        session.running_tool = Some("web_search".into());
        assert!(session.activity_line(80).unwrap().contains("web_search"));
    }

    #[test]
    fn settling_stops_the_spinner_and_forgets_the_running_tool() {
        let (_dir, mut session) = session();
        let tx = install_worker(&mut session, WorkerKind::Turn);
        session.begin("working");
        session.running_tool = Some("web_search".into());
        tx.send(TurnUpdate::Done {
            session_id: String::new(),
            text: "done".into(),
        })
        .unwrap();
        settle(&mut session);
        assert!(session.activity_line(80).is_none());
        assert!(session.running_tool.is_none());
    }

    #[test]
    fn scrolling_is_clamped_at_both_ends() {
        let (_dir, mut session) = session();
        session.scroll(10, 3);
        assert_eq!(session.scroll_back, 3, "cannot scroll past the top");
        session.scroll(-1, 3);
        assert_eq!(session.scroll_back, 2);
        session.scroll(-10, 3);
        assert_eq!(session.scroll_back, 0, "cannot scroll below the tail");
    }

    #[test]
    fn dragging_the_thumb_maps_the_track_onto_the_transcript() {
        let (_dir, mut session) = session();
        session.scroll_to(0.0, 40);
        assert_eq!(
            session.scroll_back, 40,
            "the top of the track is the oldest"
        );
        session.scroll_to(1.0, 40);
        assert_eq!(
            session.scroll_back, 0,
            "the bottom of the track is the tail"
        );
        session.scroll_to(0.5, 40);
        assert_eq!(session.scroll_back, 20, "halfway is halfway");
    }

    #[test]
    fn a_drag_outside_the_track_cannot_scroll_past_either_end() {
        let (_dir, mut session) = session();
        session.scroll_to(-3.0, 10);
        assert_eq!(session.scroll_back, 10);
        session.scroll_to(9.0, 10);
        assert_eq!(session.scroll_back, 0);
    }

    #[test]
    fn a_transcript_that_fits_on_screen_cannot_be_dragged_anywhere() {
        let (_dir, mut session) = session();
        session.scroll_to(0.0, 0);
        assert_eq!(session.scroll_back, 0);
    }

    #[test]
    fn a_code_the_user_must_copy_is_not_printed_under_mouse_capture() {
        let (_dir, mut session) = session();
        assert!(session.mouse, "captured by default");
        session.release_mouse_for_copying();
        assert!(!session.mouse, "the terminal needs it to select the code");
        assert!(
            session.messages[0].text.contains("/mouse"),
            "a silent capability change strands the user: {}",
            session.messages[0].text
        );
    }

    #[test]
    fn releasing_an_already_released_mouse_says_nothing() {
        let (_dir, mut session) = session();
        session.mouse = false;
        session.release_mouse_for_copying();
        assert!(!session.mouse);
        assert!(
            session.messages.is_empty(),
            "a no-op must not narrate itself"
        );
    }

    /// Pins the wiring, not the helper. `connect_codex` spawns a real device
    /// poll against OpenAI, so it cannot be called here — this asserts the
    /// release happens *before* the first line sign-in prints, which is the
    /// ordering the whole fix depends on.
    #[test]
    fn sign_in_releases_the_mouse_before_it_says_anything() {
        let (_dir, mut session) = session();
        session.release_mouse_for_copying();
        session.push(Role::Assistant, "starting Codex sign-in…".into());
        assert!(
            session.messages[0].text.contains("mouse released"),
            "the release must precede the sign-in banner, or the code scrolls \
             past before capture is dropped"
        );
    }

    // ADR-0075 phase 1: the block mirror.
    use crate::workbench::{BlockLifecycle, WorkbenchBlockKind};

    /// The phase-1 lockstep invariant: block `i` describes row `i`, shape for
    /// shape. This is the differential check for the mirror — drop any one
    /// mirror call from the session and a scripted turn below fails here.
    fn assert_blocks_mirror_rows(session: &TuiSession) {
        assert_eq!(session.workbench.len(), session.messages.len());
        for (block, message) in session.workbench.blocks().iter().zip(&session.messages) {
            assert_eq!(block.kind.role(), message.role);
            if let WorkbenchBlockKind::ToolCall { call_id } = &block.kind {
                assert_eq!(Some(call_id.as_str()), message.call_id.as_deref());
            }
        }
    }

    #[test]
    fn every_row_has_exactly_one_block_of_the_same_shape() {
        let (_dir, mut session) = session();
        session.push(Role::User, "find the sources".into());
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Text("Looking".into())).unwrap();
        tx.send(TurnUpdate::Tool(tool_step(&tool_event(
            "started", "", None,
        ))))
        .unwrap();
        session.pump();
        assert_blocks_mirror_rows(&session);
        tx.send(TurnUpdate::Tool(tool_step(&tool_event(
            "succeeded",
            "Found 3 sources",
            Some(1200),
        ))))
        .unwrap();
        tx.send(TurnUpdate::Done {
            session_id: "s-1".into(),
            text: String::new(),
        })
        .unwrap();
        settle(&mut session);
        assert_blocks_mirror_rows(&session);
        let lifecycles: Vec<_> = session
            .workbench
            .blocks()
            .iter()
            .map(|block| block.lifecycle)
            .collect();
        assert_eq!(
            lifecycles,
            vec![
                BlockLifecycle::Succeeded,
                BlockLifecycle::Succeeded,
                BlockLifecycle::Succeeded
            ],
            "prompt, answer, and tool all settled clean"
        );
    }

    #[test]
    fn a_tool_block_keeps_one_identity_through_its_lifecycle() {
        let (_dir, mut session) = session();
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Tool(tool_step(&tool_event(
            "started", "", None,
        ))))
        .unwrap();
        session.pump();
        let born = session.workbench.blocks()[0].id;
        tx.send(TurnUpdate::Tool(tool_step(&tool_event(
            "succeeded",
            "Found 3 sources",
            Some(1200),
        ))))
        .unwrap();
        session.pump();
        assert_eq!(session.workbench.len(), 1, "one call, one block");
        let block = &session.workbench.blocks()[0];
        assert_eq!(block.id, born, "identity survives streaming updates");
        assert_eq!(block.lifecycle, BlockLifecycle::Succeeded);
        assert_eq!(
            block.provenance,
            vec![
                "run-1:call-1:started".to_string(),
                "run-1:call-1:succeeded".to_string()
            ],
            "the block cites the kernel events that drove it"
        );
        tx.send(TurnUpdate::Done {
            session_id: String::new(),
            text: String::new(),
        })
        .unwrap();
        settle(&mut session);
    }

    #[test]
    fn selection_made_mid_stream_survives_the_rest_of_the_turn() {
        let (_dir, mut session) = session();
        session.push(Role::User, "hello".into());
        let chosen = session.workbench.blocks()[0].id;
        session.workbench.select(Some(chosen));
        let tx = install_worker(&mut session, WorkerKind::Turn);
        for delta in ["Hel", "lo ", "back"] {
            tx.send(TurnUpdate::Text(delta.into())).unwrap();
        }
        tx.send(TurnUpdate::Tool(tool_step(&tool_event(
            "started", "", None,
        ))))
        .unwrap();
        tx.send(TurnUpdate::Done {
            session_id: String::new(),
            text: String::new(),
        })
        .unwrap();
        settle(&mut session);
        assert_eq!(session.workbench.selected(), Some(chosen));
        assert_eq!(
            session.workbench.index_of(chosen),
            Some(0),
            "selection is semantic identity, not a row index"
        );
        assert_blocks_mirror_rows(&session);
    }

    #[test]
    fn a_failed_turn_cancels_the_answer_it_interrupted() {
        let (_dir, mut session) = session();
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Text("half an answ".into())).unwrap();
        tx.send(TurnUpdate::Failed("provider went away".into()))
            .unwrap();
        settle(&mut session);
        assert_blocks_mirror_rows(&session);
        assert_eq!(
            session.workbench.blocks()[0].lifecycle,
            BlockLifecycle::Cancelled,
            "an interrupted stream is not blessed as a success"
        );
        assert_eq!(
            session.workbench.blocks()[1].lifecycle,
            BlockLifecycle::Failed,
            "the error row is a settled failure"
        );
    }

    /// The park, scripted the way production produces it: the call starts,
    /// its exact binding arrives, the turn parks. The started event names the
    /// fixture binding's call and carries a real run id, so this also proves
    /// the owning turn is recorded.
    #[test]
    fn a_parked_call_stays_blocked_while_the_rest_settles() {
        let (_dir, mut session) = session();
        let held: ToolLifecycleEvent = serde_json::from_value(json!({
            "schema_version": 1,
            "event_id": "run:write-1:started",
            "run_id": "11111111-1111-4111-8111-111111111111",
            "call_id": "write-1",
            "tool_id": "write_file",
            "phase": "started",
            "summary": "",
        }))
        .expect("fixture tool event deserializes");
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Text("writing the proof".into()))
            .unwrap();
        tx.send(TurnUpdate::Tool(tool_step(&held))).unwrap();
        tx.send(TurnUpdate::Approval(approval_binding_fixture()))
            .unwrap();
        tx.send(TurnUpdate::Failed("needs approval".into()))
            .unwrap();
        settle(&mut session);
        assert_blocks_mirror_rows(&session);
        let call = &session.workbench.blocks()[1];
        assert_eq!(
            call.lifecycle,
            BlockLifecycle::Blocked,
            "the held binding outlives the worker, so the block keeps waiting"
        );
        assert_eq!(
            call.turn_id.map(|id| id.to_string()).as_deref(),
            Some("11111111-1111-4111-8111-111111111111"),
            "a real run id is carried as the owning turn"
        );
        assert_eq!(
            session.workbench.blocks()[0].lifecycle,
            BlockLifecycle::Cancelled,
            "the parked turn's stream was interrupted, not completed"
        );
        assert!(session.pending_approval.is_some());
    }

    #[test]
    fn a_dead_worker_cancels_the_blocks_it_stranded() {
        let (_dir, mut session) = session();
        let tx = install_worker(&mut session, WorkerKind::Turn);
        tx.send(TurnUpdate::Text("half".into())).unwrap();
        session.pump();
        drop(tx);
        settle(&mut session);
        assert_blocks_mirror_rows(&session);
        assert_eq!(
            session.workbench.blocks()[0].lifecycle,
            BlockLifecycle::Cancelled,
            "the stranded answer is not blessed"
        );
        let crash = session.workbench.blocks().last().unwrap();
        assert_eq!(crash.kind.role(), Role::Error);
        assert_eq!(crash.lifecycle, BlockLifecycle::Failed);
    }

    #[test]
    fn a_fresh_session_clears_the_blocks_with_the_rows() {
        let (_dir, mut session) = session();
        session.push(Role::User, "hello".into());
        assert!(!session.workbench.is_empty());
        crate::commands::dispatch(&mut session, "/new");
        assert_eq!(session.messages.len(), 1, "only the fresh-session note");
        assert_blocks_mirror_rows(&session);
    }
}
