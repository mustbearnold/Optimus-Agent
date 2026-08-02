//! What the transcript actually shows: blocks, and runs of blocks folded into
//! one header.
//!
//! Phase 2 of ADR-0075. Grouping is *derived*, never stored: [`project`] is a
//! pure function of the block list, so a run grows as more calls arrive without
//! anything having to be re-keyed, and nothing about the screen leaks back into
//! block state. The one thing a human owns — whether a group is open — lives on
//! the run's first block, which is the block whose identity the run inherits and
//! the only one guaranteed not to move as the run grows.
//!
//! The boundaries are deliberately narrow. Folding hides work, so a run breaks
//! at anything that changes what the reader would conclude: a different tool, a
//! different turn, anything that is not a clean success, and anything whose
//! declared policy says it could have changed something ([`super::effects`]).
//! Agent and plan boundaries join this list in the phases that give the terminal
//! agents and plans; until then there is nothing to break on and nothing is
//! claimed.

use super::effects;
use super::{BlockId, BlockLifecycle, WorkbenchBlock, WorkbenchBlockKind};

/// Fewest adjacent calls worth folding behind one header.
///
/// Two rows becoming one header saves a single row and costs the reader both
/// call summaries; three is where the summary starts paying for what it hides.
const MIN_GROUP: usize = 3;

/// One unit of the transcript: a block painted on its own, or a run of
/// repeated observations behind a header the reader can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// The block at `index` in the transcript, painted as itself.
    Single { index: usize, id: BlockId },
    /// A run of adjacent calls to one tool, all settled clean.
    Group {
        /// Identity of the run: the first member's, so it survives the run
        /// growing, the fold closing, and the transcript reflowing.
        id: BlockId,
        tool: String,
        /// Transcript indices of every member, in order. Never shorter than
        /// [`MIN_GROUP`].
        members: Vec<usize>,
        /// Whether the human opened it. Runs arrive closed.
        expanded: bool,
    },
}

impl Item {
    /// The identity this item is selected and folded by.
    pub fn id(&self) -> BlockId {
        match self {
            Self::Single { id, .. } => *id,
            Self::Group { id, .. } => *id,
        }
    }

    /// Whether `id` is this item or one of the blocks it swallowed, so a
    /// selection made before a run formed still points at the run.
    pub fn holds(&self, id: BlockId, blocks: &[WorkbenchBlock]) -> bool {
        match self {
            Self::Single { id: own, .. } => *own == id,
            Self::Group { members, .. } => members
                .iter()
                .any(|index| blocks.get(*index).is_some_and(|block| block.id == id)),
        }
    }

    /// Whether opening and closing this item means anything. Phase 2 folds
    /// groups; the block kinds that carry a body of their own fold from the
    /// phase that gives them one.
    pub fn foldable(&self) -> bool {
        matches!(self, Self::Group { .. })
    }

    /// Transcript indices this item paints, in order.
    pub fn span(&self) -> Vec<usize> {
        match self {
            Self::Single { index, .. } => vec![*index],
            Self::Group { members, .. } => members.clone(),
        }
    }
}

/// The transcript as units to paint, in order.
///
/// Every block appears exactly once, in its original position: grouping only
/// changes what is drawn around a run, never which blocks exist or where they
/// sit.
pub fn project(blocks: &[WorkbenchBlock]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut at = 0;
    while at < blocks.len() {
        let head = &blocks[at];
        let run = run_length(blocks, at);
        if run >= MIN_GROUP {
            items.push(Item::Group {
                id: head.id,
                tool: foldable_tool(head).unwrap_or_default().to_string(),
                members: (at..at + run).collect(),
                expanded: opened_by_hand(head),
            });
            at += run;
            continue;
        }
        items.push(Item::Single {
            index: at,
            id: head.id,
        });
        at += 1;
    }
    items
}

/// How many blocks the foldable run starting at `at` covers — one when the
/// block there cannot be folded with anything. Counted rather than collected,
/// so projecting a long transcript does not allocate a vector per block it
/// then throws away.
fn run_length(blocks: &[WorkbenchBlock], at: usize) -> usize {
    let head = &blocks[at];
    let Some(tool) = foldable_tool(head) else {
        return 1;
    };
    let mut run = 1;
    for block in &blocks[at + 1..] {
        if foldable_tool(block) != Some(tool) || block.turn_id != head.turn_id {
            break;
        }
        run += 1;
    }
    run
}

/// The tool this block may be folded under, or `None` when it may not be
/// folded at all.
///
/// A call folds only when it is a tool call that succeeded cleanly and whose
/// declared policy says it observed rather than changed. Everything else — a
/// failure, a cancellation, a call still running, one waiting on a human, a
/// write, a command, a browser step, a prompt, an answer, a note — stays a row
/// of its own, because those are exactly the rows a reader scrolls back to find.
fn foldable_tool(block: &WorkbenchBlock) -> Option<&str> {
    let WorkbenchBlockKind::ToolCall { tool, .. } = &block.kind else {
        return None;
    };
    if block.lifecycle != BlockLifecycle::Succeeded || !effects::is_observation(tool) {
        return None;
    }
    Some(tool.as_str())
}

/// Whether a human opened this run. Runs arrive closed, so an untouched fold is
/// closed and a touched one is whatever the human left it as — arriving output
/// never reopens or closes it (ADR-0075 §1).
fn opened_by_hand(head: &WorkbenchBlock) -> bool {
    head.presentation.user_changed_expansion && head.presentation.expanded
}

#[cfg(test)]
pub(crate) fn ungrouped(count: usize) -> Vec<Item> {
    (0..count)
        .map(|index| Item::Single {
            index,
            id: BlockId::mint(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Role;
    use crate::workbench::WorkbenchState;
    use uuid::Uuid;

    const TURN: &str = "11111111-1111-4111-8111-111111111111";

    fn state() -> WorkbenchState {
        WorkbenchState::default()
    }

    /// Push a settled tool call, the way `apply_tool_step` does for a real one.
    fn call(state: &mut WorkbenchState, tool: &str, id: &str, lifecycle: BlockLifecycle) {
        state.push_call_for_test(tool, id, lifecycle, Some(Uuid::parse_str(TURN).unwrap()));
    }

    fn read(state: &mut WorkbenchState, id: &str) {
        call(state, "read_file", id, BlockLifecycle::Succeeded);
    }

    fn kinds(state: &WorkbenchState) -> Vec<String> {
        project(state.blocks())
            .iter()
            .map(|item| match item {
                Item::Single { index, .. } => format!("single:{index}"),
                Item::Group { tool, members, .. } => format!("group:{tool}:{}", members.len()),
            })
            .collect()
    }

    #[test]
    fn a_run_of_repeated_reads_folds_into_one_item() {
        let mut state = state();
        for n in 0..4 {
            read(&mut state, &format!("r{n}"));
        }
        assert_eq!(kinds(&state), vec!["group:read_file:4"]);
    }

    #[test]
    fn a_run_shorter_than_the_threshold_stays_visible() {
        let mut state = state();
        read(&mut state, "r0");
        read(&mut state, "r1");
        assert_eq!(
            kinds(&state),
            vec!["single:0", "single:1"],
            "two rows becoming a header hides as much as it saves"
        );
    }

    #[test]
    fn a_group_keeps_the_identity_of_its_first_member_as_it_grows() {
        let mut state = state();
        for n in 0..3 {
            read(&mut state, &format!("r{n}"));
        }
        let first = project(state.blocks())[0].id();
        assert_eq!(first, state.blocks()[0].id);
        read(&mut state, "r3");
        let grown = project(state.blocks());
        assert_eq!(grown[0].id(), first, "a run growing does not re-key it");
        assert_eq!(grown[0].span().len(), 4);
    }

    #[test]
    fn every_block_is_painted_exactly_once_however_it_is_grouped() {
        let mut state = state();
        state.push_note(Role::User, false);
        for n in 0..5 {
            read(&mut state, &format!("r{n}"));
        }
        call(&mut state, "write_file", "w0", BlockLifecycle::Succeeded);
        for n in 0..3 {
            read(&mut state, &format!("s{n}"));
        }
        state.push_note(Role::Assistant, false);

        let painted: Vec<usize> = project(state.blocks())
            .iter()
            .flat_map(Item::span)
            .collect();
        let expected: Vec<usize> = (0..state.len()).collect();
        assert_eq!(painted, expected, "no block may be lost or duplicated");
    }

    #[test]
    fn a_write_never_joins_a_run_of_reads() {
        let mut state = state();
        read(&mut state, "r0");
        read(&mut state, "r1");
        call(&mut state, "write_file", "w0", BlockLifecycle::Succeeded);
        read(&mut state, "r2");
        read(&mut state, "r3");
        read(&mut state, "r4");
        assert_eq!(
            kinds(&state),
            vec!["single:0", "single:1", "single:2", "group:read_file:3"],
            "the write breaks the run and keeps its own row"
        );
    }

    #[test]
    fn a_failure_breaks_the_run_and_is_never_hidden() {
        let mut state = state();
        for n in 0..3 {
            read(&mut state, &format!("a{n}"));
        }
        call(&mut state, "read_file", "boom", BlockLifecycle::Failed);
        for n in 0..3 {
            read(&mut state, &format!("b{n}"));
        }
        assert_eq!(
            kinds(&state),
            vec!["group:read_file:3", "single:3", "group:read_file:3"]
        );
    }

    #[test]
    fn a_call_waiting_on_a_human_or_still_running_is_never_folded_away() {
        for lifecycle in [
            BlockLifecycle::Running,
            BlockLifecycle::Blocked,
            BlockLifecycle::Cancelled,
            BlockLifecycle::PossiblyStalled,
        ] {
            let mut state = state();
            read(&mut state, "r0");
            read(&mut state, "r1");
            call(&mut state, "read_file", "live", lifecycle);
            read(&mut state, "r2");
            assert_eq!(
                kinds(&state),
                vec!["single:0", "single:1", "single:2", "single:3"],
                "{lifecycle:?} must stay visible"
            );
        }
    }

    #[test]
    fn different_tools_do_not_share_a_run() {
        let mut state = state();
        read(&mut state, "r0");
        read(&mut state, "r1");
        call(&mut state, "web_search", "s0", BlockLifecycle::Succeeded);
        call(&mut state, "web_search", "s1", BlockLifecycle::Succeeded);
        assert_eq!(
            kinds(&state),
            vec!["single:0", "single:1", "single:2", "single:3"],
            "\"four lookups\" would not say what any of them found"
        );
    }

    #[test]
    fn a_run_never_crosses_a_turn_boundary() {
        let mut state = state();
        for n in 0..3 {
            read(&mut state, &format!("a{n}"));
        }
        for n in 0..3 {
            state.push_call_for_test(
                "read_file",
                &format!("b{n}"),
                BlockLifecycle::Succeeded,
                Some(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()),
            );
        }
        assert_eq!(
            kinds(&state),
            vec!["group:read_file:3", "group:read_file:3"],
            "two turns' work must not read as one run"
        );
    }

    #[test]
    fn a_run_arrives_closed_and_stays_however_the_human_left_it() {
        let mut state = state();
        for n in 0..3 {
            read(&mut state, &format!("r{n}"));
        }
        let head = project(state.blocks())[0].id();
        assert!(
            !matches!(project(state.blocks())[0], Item::Group { expanded, .. } if expanded),
            "runs arrive closed"
        );

        state.toggle_fold_of(head);
        assert!(
            matches!(project(state.blocks())[0], Item::Group { expanded, .. } if expanded),
            "the human opened it"
        );

        // More output arriving must not close what a human opened.
        read(&mut state, "r3");
        assert!(
            matches!(project(state.blocks())[0], Item::Group { expanded, .. } if expanded),
            "arriving output must never close a fold a human opened"
        );
    }

    #[test]
    fn a_selection_made_before_a_run_formed_still_points_at_the_run() {
        let mut state = state();
        read(&mut state, "r0");
        read(&mut state, "r1");
        let second = state.blocks()[1].id;
        read(&mut state, "r2");

        let items = project(state.blocks());
        assert_eq!(items.len(), 1, "the three reads folded together");
        assert!(items[0].holds(second, state.blocks()));
        assert_ne!(
            items[0].id(),
            second,
            "the run is keyed by its first member"
        );
    }
}
