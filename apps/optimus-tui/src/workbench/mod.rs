//! The workbench block model: a block owns its lifecycle; rows only paint it.
//!
//! Phase 1 of ADR-0075. Every transcript row the session records is mirrored
//! here as a [`WorkbenchBlock`] with durable identity, a typed lifecycle, and
//! kernel provenance: `blocks()[i]` describes `messages[i]`. Nothing paints
//! from blocks yet — the `Message` vector remains the compatibility projection
//! and the PTY suite proves the pixels did not move. What this buys is the
//! structure every later phase builds on: identity minted once at adaptation
//! time and keyed by domain identity (the kernel `call_id` for tools), never
//! by row position, so it survives streaming, folding, resize, and redraw.
//!
//! Lifecycle moves only on typed events — the [`ToolStep`] phase carried from
//! `ToolLifecycleEvent`, and the turn's own settlement — never by parsing
//! rendered text. Selection is semantic: a [`BlockId`], not an index, so it
//! survives reflow and streaming appends.

use std::time::Instant;

use optimus_kernel::ToolLifecyclePhase;
use uuid::Uuid;

use crate::session::{Role, ToolStep};

mod detail;
mod effects;
mod grouping;
mod selection;

pub use detail::{CommandDetail, ToolDetail};
pub use grouping::{Body, Item};
pub use selection::{FocusRegion, SelectionStep};

#[cfg(test)]
pub(crate) use grouping::ungrouped;

/// Durable identity of one block: minted at adaptation time, stable for the
/// block's life across streaming, folding, resize, and redraw (ADR-0075 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub Uuid);

impl BlockId {
    pub(crate) fn mint() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Where an operation is in its life (ADR-0075 §1).
///
/// Phase 1 produces `Running`, `Blocked`, and the three settled states.
/// `Queued`, `Waiting`, and `PossiblyStalled` belong to the phases that own
/// plans, agents, and background jobs — `PossiblyStalled` in particular is a
/// warning derived from a real absence of events, never a declared fact, and
/// nothing in this phase has the evidence to raise it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockLifecycle {
    Queued,
    Running,
    Waiting,
    /// Paused on a human decision; the approval card is the phase-1 case.
    Blocked,
    Succeeded,
    Failed,
    Cancelled,
    PossiblyStalled,
}

impl BlockLifecycle {
    /// Settled states are terminal in the absence of new evidence. A later
    /// typed event may still move one — an approved call settles again when
    /// its continuation reports what actually ran.
    pub fn is_settled(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// What a block is, mirroring [`Role`] one-for-one while the `Message` vector
/// remains the projection that paints. Later phases grow richer kinds; phase 1
/// only wraps what already exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkbenchBlockKind {
    UserPrompt,
    AssistantAnswer,
    /// One tool call, keyed by the kernel's `call_id` so every lifecycle
    /// phase lands on the block that call already owns.
    ToolCall {
        call_id: String,
        /// The kernel's `tool_id`, carried because grouping has to ask the
        /// canonical contract what this call was allowed to do before it may
        /// fold the call away (ADR-0075 phase 2).
        tool: String,
    },
    /// An action note: an approval card, a recorded decision.
    StatusNote,
    ErrorNote,
}

impl WorkbenchBlockKind {
    /// The transcript role this kind mirrors. Total, so the row/block
    /// correspondence is checkable shape against shape.
    pub fn role(&self) -> Role {
        match self {
            Self::UserPrompt => Role::User,
            Self::AssistantAnswer => Role::Assistant,
            Self::ToolCall { .. } => Role::Tool,
            Self::StatusNote => Role::Action,
            Self::ErrorNote => Role::Error,
        }
    }
}

/// How a block is shown, owned by the human rather than the event stream.
/// `user_changed_expansion` is what makes a fold a human touched immune to
/// arriving output: nothing in the event path ever writes these fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPresentation {
    pub expanded: bool,
    /// Set once a human folds or unfolds the block; streaming must never
    /// reopen or close it after that.
    pub user_changed_expansion: bool,
    pub selected_tab: Option<String>,
    pub pinned: bool,
}

impl Default for BlockPresentation {
    fn default() -> Self {
        Self {
            expanded: true,
            user_changed_expansion: false,
            selected_tab: None,
            pinned: false,
        }
    }
}

/// One operation's block. Fields per the ADR-0075 §1 contract; the text it
/// paints stays in the mirrored `Message`, because phase 1 changes what state
/// *is*, not what the screen shows.
#[derive(Debug, Clone)]
pub struct WorkbenchBlock {
    pub id: BlockId,
    pub kind: WorkbenchBlockKind,
    pub lifecycle: BlockLifecycle,
    /// The `run_id` of the owning turn, when the event carried a real one.
    /// Production run ids are UUIDs (the approval binding's typed `run_id`
    /// proves it); a fixture id like "run-1" stays `None` rather than being
    /// invented. Turn-level notes stay `None` in phase 1 — no typed event
    /// hands the screen thread a run id for them yet.
    pub turn_id: Option<Uuid>,
    /// Nesting, for the block kinds that own children directly. Grouping does
    /// not use it: a run of repeated calls is derived from the block list on
    /// every projection ([`grouping`]), so no block has to be re-parented as
    /// the run it belongs to grows.
    pub parent_id: Option<BlockId>,
    pub presentation: BlockPresentation,
    pub started_at: Option<Instant>,
    pub settled_at: Option<Instant>,
    /// Kernel event ids that drove this block, in arrival order. The kernel
    /// mints `ToolLifecycleEvent::event_id` as a `String`, so the ADR-0075
    /// sketch's `Vec<Uuid>` would force an invented parse; the honest type
    /// carries the ids verbatim.
    pub provenance: Vec<String>,
    /// What the call produced, read from its typed outcome — the body opening
    /// this block shows (ADR-0075 phase 3). Empty until an outcome arrives,
    /// which is why a running call has nothing to open.
    pub detail: ToolDetail,
}

impl WorkbenchBlock {
    fn born(kind: WorkbenchBlockKind, lifecycle: BlockLifecycle, turn_id: Option<Uuid>) -> Self {
        let now = Instant::now();
        Self {
            id: BlockId::mint(),
            kind,
            lifecycle,
            turn_id,
            parent_id: None,
            presentation: BlockPresentation::default(),
            started_at: Some(now),
            settled_at: lifecycle.is_settled().then_some(now),
            provenance: Vec::new(),
            detail: ToolDetail::None,
        }
    }
}

/// The block list and the semantic selection over it.
///
/// Fields are private so every mutation goes through the typed entry points
/// below — the same discipline that keeps lifecycle off the log-parsing path.
/// Rendered rows are a projection computed at draw time; nothing here reads
/// row indices back into state.
#[derive(Debug, Default)]
pub struct WorkbenchState {
    blocks: Vec<WorkbenchBlock>,
    selected_block: Option<BlockId>,
    /// Which region the keyboard is talking to; see [`selection`].
    focus: FocusRegion,
}

impl WorkbenchState {
    pub fn blocks(&self) -> &[WorkbenchBlock] {
        &self.blocks
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Select a block — or clear the selection — by identity, never by row.
    pub fn select(&mut self, id: Option<BlockId>) {
        self.selected_block = id;
    }

    pub fn selected(&self) -> Option<BlockId> {
        self.selected_block
    }

    pub fn index_of(&self, id: BlockId) -> Option<usize> {
        self.blocks.iter().position(|block| block.id == id)
    }

    pub(crate) fn clear(&mut self) {
        self.blocks.clear();
        self.selected_block = None;
        self.focus = FocusRegion::Composer;
    }

    /// Mirror one non-tool row as it is pushed. Birth states follow what the
    /// push provably means at its call sites:
    /// - a user prompt or an action note is complete the moment it is written;
    /// - an error is a settled failure;
    /// - an assistant bubble born under a live worker is streaming, one born
    ///   idle (the sign-in banner) is complete until a delta reopens it.
    pub(crate) fn push_note(&mut self, role: Role, turn_active: bool) {
        debug_assert!(
            role != Role::Tool,
            "tool rows are born in apply_tool_step, keyed by call_id"
        );
        let (kind, lifecycle) = match role {
            Role::User => (WorkbenchBlockKind::UserPrompt, BlockLifecycle::Succeeded),
            Role::Assistant if turn_active => {
                (WorkbenchBlockKind::AssistantAnswer, BlockLifecycle::Running)
            }
            Role::Assistant => (
                WorkbenchBlockKind::AssistantAnswer,
                BlockLifecycle::Succeeded,
            ),
            Role::Action => (WorkbenchBlockKind::StatusNote, BlockLifecycle::Succeeded),
            Role::Error => (WorkbenchBlockKind::ErrorNote, BlockLifecycle::Failed),
            // Kept total so the mirror can never desync the shape, even if a
            // future caller pushes a tool row past the debug assertion above.
            Role::Tool => (
                WorkbenchBlockKind::ToolCall {
                    call_id: String::new(),
                    tool: String::new(),
                },
                BlockLifecycle::Succeeded,
            ),
        };
        self.blocks
            .push(WorkbenchBlock::born(kind, lifecycle, None));
    }

    /// A delta reached the last assistant bubble. A settled bubble reopens:
    /// the delta is a typed event, and more text arriving is the stream saying
    /// the answer was not finished after all.
    pub(crate) fn extend_assistant(&mut self) {
        let Some(block) = self.blocks.last_mut() else {
            return;
        };
        debug_assert_eq!(
            block.kind,
            WorkbenchBlockKind::AssistantAnswer,
            "append_assistant only extends when the last row is an answer"
        );
        if block.kind == WorkbenchBlockKind::AssistantAnswer && block.lifecycle.is_settled() {
            block.lifecycle = BlockLifecycle::Running;
            block.settled_at = None;
        }
    }

    /// Mirror one tool lifecycle event: every phase for a `call_id` lands on
    /// the one block that call owns, exactly as the transcript rewrites the
    /// one row it owns. Lifecycle comes from the typed phase, never from the
    /// rendered line.
    pub(crate) fn apply_tool_step(&mut self, step: &ToolStep) {
        let lifecycle = lifecycle_for(step.phase);
        let existing = self.blocks.iter_mut().rev().find(|block| {
            matches!(&block.kind, WorkbenchBlockKind::ToolCall { call_id, .. } if *call_id == step.call_id)
        });
        match existing {
            Some(block) => {
                if block.lifecycle != lifecycle {
                    block.lifecycle = lifecycle;
                    // Stamped on the move into a settled state; cleared when a
                    // later typed event proves the call is live again (an
                    // approved continuation re-reporting its call).
                    block.settled_at = lifecycle.is_settled().then(Instant::now);
                }
                if !block.provenance.contains(&step.event_id) {
                    block.provenance.push(step.event_id.clone());
                }
                // A later phase carries the outcome the earlier ones had
                // nothing to say about. An arriving event never erases a body
                // the call already reported.
                if step.detail.has_body() {
                    block.detail = step.detail.clone();
                }
            }
            None => {
                let mut block = WorkbenchBlock::born(
                    WorkbenchBlockKind::ToolCall {
                        call_id: step.call_id.clone(),
                        tool: step.name.clone(),
                    },
                    lifecycle,
                    Uuid::parse_str(&step.run_id).ok(),
                );
                block.provenance.push(step.event_id.clone());
                block.detail = step.detail.clone();
                self.blocks.push(block);
            }
        }
    }

    /// The kernel holds the effect; the block waits on the human. Finds the
    /// live block for the bound call — a binding for a call this session never
    /// saw start (a resumed park) has no block to hold, and that is fine: the
    /// card row itself is mirrored as a note.
    pub(crate) fn hold_for_approval(&mut self, call_id: &str) {
        let held = self.blocks.iter_mut().rev().find(|block| {
            !block.lifecycle.is_settled()
                && matches!(&block.kind, WorkbenchBlockKind::ToolCall { call_id: own, .. } if own == call_id)
        });
        if let Some(block) = held {
            block.lifecycle = BlockLifecycle::Blocked;
        }
    }

    /// Append a settled tool-call block directly, for tests that script a
    /// transcript shape rather than a stream. Production code reaches every
    /// tool block through [`Self::apply_tool_step`] and its typed phase.
    #[cfg(test)]
    pub(crate) fn push_call_for_test(
        &mut self,
        tool: &str,
        call_id: &str,
        lifecycle: BlockLifecycle,
        turn_id: Option<Uuid>,
    ) {
        self.blocks.push(WorkbenchBlock::born(
            WorkbenchBlockKind::ToolCall {
                call_id: call_id.into(),
                tool: tool.into(),
            },
            lifecycle,
            turn_id,
        ));
    }

    /// Append a settled call that produced a body, for tests about folding
    /// rather than about the stream that filled it.
    #[cfg(test)]
    pub(crate) fn push_body_for_test(&mut self, tool: &str, call_id: &str, detail: ToolDetail) {
        self.push_call_for_test(tool, call_id, BlockLifecycle::Succeeded, None);
        if let Some(block) = self.blocks.last_mut() {
            block.detail = detail;
        }
    }

    /// The turn settled cleanly: its streaming answer succeeded.
    pub(crate) fn settle_success(&mut self) {
        self.settle_live(BlockLifecycle::Succeeded);
    }

    /// The turn stopped without settling its work — a failure, a park, or a
    /// worker crash. Whatever was still streaming was interrupted, and saying
    /// "cancelled" is the honest reading; saying "succeeded" would bless work
    /// the stream never finished.
    pub(crate) fn settle_interrupted(&mut self) {
        self.settle_live(BlockLifecycle::Cancelled);
    }

    /// Close every block the worker left open. `Blocked` survives: its truth
    /// is the held binding, which outlives the worker — every other live
    /// state's evidence *was* the worker's event stream.
    fn settle_live(&mut self, assistant_outcome: BlockLifecycle) {
        let now = Instant::now();
        for block in &mut self.blocks {
            if block.lifecycle.is_settled() || block.lifecycle == BlockLifecycle::Blocked {
                continue;
            }
            block.lifecycle = if block.kind == WorkbenchBlockKind::AssistantAnswer {
                assistant_outcome
            } else {
                BlockLifecycle::Cancelled
            };
            block.settled_at = Some(now);
        }
    }
}

/// The block lifecycle a typed tool phase proves. Two rows are judgments and
/// say so: `Suppressed` means policy stopped the call before it ran — a
/// cancellation, not a tool failure — and `Ambiguous` means the kernel cannot
/// vouch for what happened, and an outcome that cannot be vouched for must
/// never read as success.
fn lifecycle_for(phase: ToolLifecyclePhase) -> BlockLifecycle {
    match phase {
        ToolLifecyclePhase::Started => BlockLifecycle::Running,
        ToolLifecyclePhase::ApprovalRequired => BlockLifecycle::Blocked,
        ToolLifecyclePhase::Succeeded => BlockLifecycle::Succeeded,
        ToolLifecyclePhase::Failed | ToolLifecyclePhase::Ambiguous => BlockLifecycle::Failed,
        ToolLifecyclePhase::Cancelled | ToolLifecyclePhase::Suppressed => BlockLifecycle::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(phase: ToolLifecyclePhase, call_id: &str, event_id: &str, run_id: &str) -> ToolStep {
        ToolStep {
            call_id: call_id.into(),
            name: "web_search".into(),
            line: "web_search  running".into(),
            running: phase == ToolLifecyclePhase::Started,
            run_id: run_id.into(),
            event_id: event_id.into(),
            phase,
            detail: ToolDetail::None,
        }
    }

    #[test]
    fn every_kind_names_the_role_it_mirrors() {
        let call = WorkbenchBlockKind::ToolCall {
            call_id: "call-1".into(),
            tool: "web_search".into(),
        };
        assert_eq!(WorkbenchBlockKind::UserPrompt.role(), Role::User);
        assert_eq!(WorkbenchBlockKind::AssistantAnswer.role(), Role::Assistant);
        assert_eq!(call.role(), Role::Tool);
        assert_eq!(WorkbenchBlockKind::StatusNote.role(), Role::Action);
        assert_eq!(WorkbenchBlockKind::ErrorNote.role(), Role::Error);
    }

    #[test]
    fn every_tool_phase_maps_to_an_honest_lifecycle() {
        use ToolLifecyclePhase as Phase;
        assert_eq!(lifecycle_for(Phase::Started), BlockLifecycle::Running);
        assert_eq!(
            lifecycle_for(Phase::ApprovalRequired),
            BlockLifecycle::Blocked
        );
        assert_eq!(lifecycle_for(Phase::Succeeded), BlockLifecycle::Succeeded);
        assert_eq!(lifecycle_for(Phase::Failed), BlockLifecycle::Failed);
        assert_eq!(
            lifecycle_for(Phase::Ambiguous),
            BlockLifecycle::Failed,
            "an outcome the kernel cannot vouch for must not read as success"
        );
        assert_eq!(lifecycle_for(Phase::Cancelled), BlockLifecycle::Cancelled);
        assert_eq!(
            lifecycle_for(Phase::Suppressed),
            BlockLifecycle::Cancelled,
            "policy stopped it before it ran; the tool did not fail"
        );
    }

    #[test]
    fn a_tool_call_keeps_its_identity_and_cites_every_event() {
        let mut state = WorkbenchState::default();
        state.apply_tool_step(&step(
            ToolLifecyclePhase::Started,
            "call-1",
            "run-1:call-1:started",
            "run-1",
        ));
        let born = state.blocks()[0].id;
        let began = state.blocks()[0].started_at;
        assert_eq!(state.blocks()[0].lifecycle, BlockLifecycle::Running);
        assert!(state.blocks()[0].settled_at.is_none());

        state.apply_tool_step(&step(
            ToolLifecyclePhase::Succeeded,
            "call-1",
            "run-1:call-1:succeeded",
            "run-1",
        ));
        assert_eq!(state.len(), 1, "one call, one block, whatever the phase");
        let block = &state.blocks()[0];
        assert_eq!(block.id, born, "identity survives the phase change");
        assert_eq!(block.started_at, began, "birth time is not rewritten");
        assert_eq!(block.lifecycle, BlockLifecycle::Succeeded);
        assert!(block.settled_at.is_some(), "settlement is stamped");
        assert_eq!(
            block.provenance,
            vec![
                "run-1:call-1:started".to_string(),
                "run-1:call-1:succeeded".to_string()
            ]
        );
    }

    #[test]
    fn a_call_carries_the_tool_it_ran_so_grouping_can_ask_what_it_did() {
        let mut state = WorkbenchState::default();
        state.apply_tool_step(&step(
            ToolLifecyclePhase::Started,
            "call-1",
            "run-1:call-1:started",
            "run-1",
        ));
        assert_eq!(
            state.blocks()[0].kind,
            WorkbenchBlockKind::ToolCall {
                call_id: "call-1".into(),
                tool: "web_search".into(),
            },
            "the kernel's tool_id is carried, not re-derived from the row"
        );
    }

    #[test]
    fn a_repeated_event_is_cited_once() {
        let mut state = WorkbenchState::default();
        let event = step(
            ToolLifecyclePhase::Started,
            "call-1",
            "run-1:call-1:started",
            "run-1",
        );
        state.apply_tool_step(&event);
        state.apply_tool_step(&event);
        assert_eq!(state.blocks()[0].provenance.len(), 1);
    }

    #[test]
    fn settlement_spares_the_block_waiting_on_a_human() {
        let mut state = WorkbenchState::default();
        state.push_note(Role::Assistant, true);
        state.apply_tool_step(&step(
            ToolLifecyclePhase::Started,
            "write-1",
            "run-1:write-1:started",
            "run-1",
        ));
        state.hold_for_approval("write-1");
        state.hold_for_approval("no-such-call");
        assert_eq!(state.blocks()[1].lifecycle, BlockLifecycle::Blocked);

        state.settle_interrupted();
        assert_eq!(
            state.blocks()[0].lifecycle,
            BlockLifecycle::Cancelled,
            "the interrupted stream is not blessed"
        );
        assert_eq!(
            state.blocks()[1].lifecycle,
            BlockLifecycle::Blocked,
            "the held binding outlives the worker, so the block keeps waiting"
        );
        assert!(state.blocks()[1].settled_at.is_none());
    }

    #[test]
    fn a_settled_answer_reopens_when_more_text_streams() {
        let mut state = WorkbenchState::default();
        state.push_note(Role::Assistant, false);
        assert_eq!(state.blocks()[0].lifecycle, BlockLifecycle::Succeeded);
        state.extend_assistant();
        assert_eq!(
            state.blocks()[0].lifecycle,
            BlockLifecycle::Running,
            "a delta is the stream saying the answer was not finished"
        );
        assert!(state.blocks()[0].settled_at.is_none());
    }

    #[test]
    fn selection_survives_new_blocks_arriving() {
        let mut state = WorkbenchState::default();
        state.push_note(Role::User, false);
        let chosen = state.blocks()[0].id;
        state.select(Some(chosen));
        for _ in 0..5 {
            state.push_note(Role::Assistant, true);
        }
        assert_eq!(state.selected(), Some(chosen));
        assert_eq!(state.index_of(chosen), Some(0));
    }

    #[test]
    fn a_real_run_id_is_kept_and_a_fixture_one_is_not_invented() {
        let mut state = WorkbenchState::default();
        state.apply_tool_step(&step(
            ToolLifecyclePhase::Started,
            "write-1",
            "run:write-1:started",
            "11111111-1111-4111-8111-111111111111",
        ));
        state.apply_tool_step(&step(
            ToolLifecyclePhase::Started,
            "call-1",
            "run-1:call-1:started",
            "run-1",
        ));
        assert_eq!(
            state.blocks()[0].turn_id,
            Some(Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap())
        );
        assert_eq!(
            state.blocks()[1].turn_id,
            None,
            "an id that is not a UUID is carried nowhere, not parsed into one"
        );
    }

    #[test]
    fn an_error_is_born_settled() {
        let mut state = WorkbenchState::default();
        state.push_note(Role::Error, true);
        assert_eq!(state.blocks()[0].lifecycle, BlockLifecycle::Failed);
        assert!(state.blocks()[0].settled_at.is_some());
    }
}
