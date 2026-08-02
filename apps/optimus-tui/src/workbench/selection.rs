//! Where the keyboard is pointed, and which block it is pointed at.
//!
//! Phase 2 of ADR-0075. Selection is semantic — a [`BlockId`], never a row —
//! so it survives streaming appends, folding, reflow, and resize. Movement is
//! over *items* rather than blocks: a folded run is one thing to step past, and
//! stepping into it selects the run rather than a member nobody can see.
//!
//! Focus is two regions in this phase. The composer owns the keyboard by
//! default; Tab hands it to the transcript and Tab or Esc hands it back. The
//! picker overlay keeps whole-keyboard ownership above both, exactly as it did
//! (ADR-0074).

use super::grouping::{project, Item};
use super::{BlockId, WorkbenchState};

/// Which region the keyboard is talking to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FocusRegion {
    #[default]
    Composer,
    /// Inspecting the transcript: keys move and open blocks rather than typing.
    Scrollback,
}

/// How a selection move should land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStep {
    Next,
    Previous,
    First,
    Last,
}

impl WorkbenchState {
    pub fn focus(&self) -> FocusRegion {
        self.focus
    }

    pub fn inspecting(&self) -> bool {
        self.focus == FocusRegion::Scrollback
    }

    /// The transcript as the units that paint, computed fresh. Grouping is
    /// derived, so this is the one place that decides what a step moves past.
    pub fn items(&self) -> Vec<Item> {
        project(&self.blocks)
    }

    /// Point the keyboard at the transcript.
    ///
    /// An empty transcript has nothing to inspect, so focus stays where it is
    /// rather than moving somewhere with no cursor. Entering with nothing
    /// selected lands on the newest item — the one under the eye, beside the
    /// composer the human just left.
    pub fn inspect(&mut self) {
        let items = self.items();
        let Some(last) = items.last() else {
            return;
        };
        self.focus = FocusRegion::Scrollback;
        if self.resolve(&items).is_none() {
            self.selected_block = Some(last.id());
        }
    }

    /// Point the keyboard back at the composer, keeping the selection so
    /// returning lands where the human left off.
    pub fn leave_inspect(&mut self) {
        self.focus = FocusRegion::Composer;
    }

    /// Select the item holding `id`, whatever `id` was when it was captured —
    /// a click reports the block under the pointer, which may be a member of a
    /// run rather than the run itself.
    pub fn select_item(&mut self, id: BlockId) {
        let items = self.items();
        if let Some(item) = items.iter().find(|item| item.holds(id, &self.blocks)) {
            self.selected_block = Some(item.id());
            self.focus = FocusRegion::Scrollback;
        }
    }

    /// Move the selection one item, or to either end.
    ///
    /// Both ends stop rather than wrap: wrapping from the newest item to the
    /// oldest in a ten-thousand-block session is never what the hand meant.
    pub fn step(&mut self, step: SelectionStep) {
        let items = self.items();
        if items.is_empty() {
            return;
        }
        let at = self.resolve(&items);
        let landed = match (step, at) {
            (SelectionStep::First, _) => 0,
            (SelectionStep::Last, _) => items.len() - 1,
            (SelectionStep::Next, None) | (SelectionStep::Previous, None) => items.len() - 1,
            (SelectionStep::Next, Some(at)) => (at + 1).min(items.len() - 1),
            (SelectionStep::Previous, Some(at)) => at.saturating_sub(1),
        };
        self.selected_block = Some(items[landed].id());
    }

    /// Open or close the selected item. Returns whether anything folded, so a
    /// caller can tell "nothing here folds" from "folded".
    pub fn toggle_fold(&mut self) -> bool {
        let Some(id) = self.selected_block else {
            return false;
        };
        self.toggle_fold_of(id)
    }

    /// Open or close the item holding `id`.
    ///
    /// The flag records that a human touched this fold, which is what stops
    /// arriving output from ever reopening or closing it again (ADR-0075 §1).
    pub fn toggle_fold_of(&mut self, id: BlockId) -> bool {
        let items = self.items();
        let Some(item) = items.iter().find(|item| item.holds(id, &self.blocks)) else {
            return false;
        };
        if !item.foldable() {
            return false;
        }
        let open = item.expanded();
        let head = item.id();
        let Some(block) = self.blocks.iter_mut().find(|block| block.id == head) else {
            return false;
        };
        block.presentation.expanded = !open;
        block.presentation.user_changed_expansion = true;
        true
    }

    /// Index of the item the selection points at, following a block that has
    /// since been swallowed by a run into the run that swallowed it.
    fn resolve(&self, items: &[Item]) -> Option<usize> {
        let id = self.selected_block?;
        items.iter().position(|item| item.holds(id, &self.blocks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Role;
    use crate::workbench::BlockLifecycle;
    use uuid::Uuid;

    const TURN: &str = "11111111-1111-4111-8111-111111111111";

    fn read(state: &mut WorkbenchState, id: &str) {
        state.push_call_for_test(
            "read_file",
            id,
            BlockLifecycle::Succeeded,
            Some(Uuid::parse_str(TURN).unwrap()),
        );
    }

    /// A prompt, a run of three reads, and an answer: three items over five
    /// blocks, which is what makes stepping over a run observable.
    fn scripted() -> WorkbenchState {
        let mut state = WorkbenchState::default();
        state.push_note(Role::User, false);
        for n in 0..3 {
            read(&mut state, &format!("r{n}"));
        }
        state.push_note(Role::Assistant, false);
        state
    }

    #[test]
    fn the_composer_owns_the_keyboard_until_it_is_handed_over() {
        let mut state = scripted();
        assert_eq!(state.focus(), FocusRegion::Composer);
        assert!(!state.inspecting());
        state.inspect();
        assert!(state.inspecting());
        state.leave_inspect();
        assert!(!state.inspecting());
        assert!(
            state.selected().is_some(),
            "returning to the composer keeps the place"
        );
    }

    #[test]
    fn inspecting_an_empty_transcript_does_not_move_focus_to_nothing() {
        let mut state = WorkbenchState::default();
        state.inspect();
        assert_eq!(state.focus(), FocusRegion::Composer);
        assert!(state.selected().is_none());
    }

    #[test]
    fn inspecting_lands_on_the_newest_item_beside_the_composer() {
        let mut state = scripted();
        state.inspect();
        assert_eq!(state.selected(), Some(state.blocks()[4].id));
    }

    #[test]
    fn a_run_is_one_thing_to_step_past() {
        let mut state = scripted();
        state.step(SelectionStep::First);
        assert_eq!(state.selected(), Some(state.blocks()[0].id), "the prompt");
        state.step(SelectionStep::Next);
        assert_eq!(
            state.selected(),
            Some(state.blocks()[1].id),
            "the run, keyed by its first read"
        );
        state.step(SelectionStep::Next);
        assert_eq!(
            state.selected(),
            Some(state.blocks()[4].id),
            "one step clears the whole run"
        );
    }

    #[test]
    fn both_ends_stop_rather_than_wrapping_around() {
        let mut state = scripted();
        state.step(SelectionStep::First);
        for _ in 0..5 {
            state.step(SelectionStep::Previous);
        }
        assert_eq!(state.selected(), Some(state.blocks()[0].id));
        state.step(SelectionStep::Last);
        for _ in 0..5 {
            state.step(SelectionStep::Next);
        }
        assert_eq!(state.selected(), Some(state.blocks()[4].id));
    }

    #[test]
    fn a_selection_survives_the_run_it_points_at_growing() {
        // A live turn: the prompt, then reads still arriving with nothing after
        // them, which is exactly when a run grows under a reader's cursor.
        let mut state = WorkbenchState::default();
        state.push_note(Role::User, true);
        for n in 0..3 {
            read(&mut state, &format!("r{n}"));
        }
        state.step(SelectionStep::Last);
        let run = state.selected().expect("the run");

        for n in 3..9 {
            read(&mut state, &format!("late{n}"));
        }
        assert_eq!(state.selected(), Some(run), "the run keeps its identity");
        let items = state.items();
        assert_eq!(items.len(), 2, "still a prompt and one run");
        assert!(items[1].holds(run, state.blocks()));
        assert_eq!(items[1].span().len(), 9, "and swallowed the new calls");
    }

    #[test]
    fn folding_is_the_humans_and_arriving_output_never_undoes_it() {
        let mut state = scripted();
        state.step(SelectionStep::First);
        state.step(SelectionStep::Next);
        assert!(state.toggle_fold(), "a run folds");
        assert!(matches!(&state.items()[1], Item::Group { expanded, .. } if *expanded));
        assert!(state.toggle_fold());
        assert!(matches!(&state.items()[1], Item::Group { expanded, .. } if !*expanded));
    }

    #[test]
    fn nothing_that_has_no_body_pretends_to_fold() {
        let mut state = scripted();
        state.step(SelectionStep::First);
        assert!(!state.toggle_fold(), "a prompt has nothing to hide");
        state.step(SelectionStep::Last);
        assert!(!state.toggle_fold(), "nor does an answer, in this phase");
    }

    #[test]
    fn folding_with_nothing_selected_does_nothing() {
        let mut state = scripted();
        assert!(!state.toggle_fold());
    }

    #[test]
    fn clicking_a_member_of_a_run_selects_the_run() {
        let mut state = scripted();
        let middle = state.blocks()[2].id;
        state.select_item(middle);
        assert_eq!(
            state.selected(),
            Some(state.blocks()[1].id),
            "the run is selected, not a row inside a closed fold"
        );
        assert!(state.inspecting(), "clicking a block inspects it");
    }

    #[test]
    fn clicking_a_block_that_is_gone_selects_nothing_new() {
        let mut state = scripted();
        state.step(SelectionStep::First);
        let before = state.selected();
        state.select_item(BlockId::mint());
        assert_eq!(state.selected(), before);
    }
}
