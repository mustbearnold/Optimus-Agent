//! Live command suggestions, offered while a slash command is being typed.
//!
//! This is deliberately not a [`crate::picker::Picker`]. A picker is modal — it
//! owns the keyboard the moment it opens, which is right for a list you chose to
//! open and wrong for one that appears because you typed a `/`. Suggestions have
//! to hover over a composer that is still accepting every keystroke, so they get
//! their own state and borrow only the three keys that would otherwise mean
//! something less useful over a half-written command.
//!
//! The list is never stored. [`suggestions`] recomputes it from the draft each
//! time it is needed, so the only thing [`Completion`] holds is which row is
//! highlighted — and that is clamped when it is read rather than when the draft
//! changes. A list that shrinks under the selection therefore cannot strand it,
//! because there is no second copy of the truth to fall out of step with.

use crate::commands::{Command, COMMANDS};

/// Which suggestion is highlighted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Completion {
    selected: usize,
}

/// The commands `draft` could still become, or empty when it is not naming one.
///
/// Suggesting needs the draft to be a lone `/word`. Once a space is typed the
/// name is settled and an argument is being written, and a menu of names over
/// the top of it would be in the way; a newline means this is a multi-line
/// prompt that happens to open with a slash. Leading whitespace is not trimmed
/// on purpose — the rule a user can see is the one that starts at column one.
pub fn suggestions(draft: &str) -> Vec<&'static Command> {
    let Some(partial) = draft.strip_prefix('/') else {
        return Vec::new();
    };
    if partial.contains(char::is_whitespace) {
        return Vec::new();
    }
    let partial = partial.to_ascii_lowercase();
    let mut found: Vec<&'static Command> = COMMANDS
        .iter()
        .filter(|command| command.name.starts_with(&partial))
        .collect();
    // A sole exact match is not a suggestion. The name is fully typed, and one
    // row repeating it back sits over the transcript saying nothing.
    if found.len() == 1 && found[0].name == partial {
        found.clear();
    }
    found
}

impl Completion {
    /// The highlighted row, folded into a list of `count` rows.
    pub fn selected(self, count: usize) -> usize {
        if count == 0 {
            0
        } else {
            self.selected.min(count - 1)
        }
    }

    /// Down one row, wrapping — the list is short enough that the far end is
    /// closer going backwards than holding the key down.
    pub fn down(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        self.selected = (self.selected(count) + 1) % count;
    }

    /// Up one row, wrapping.
    pub fn up(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        self.selected = (self.selected(count) + count - 1) % count;
    }

    /// Back to the top, for when the draft changed under the selection.
    pub fn reset(&mut self) {
        self.selected = 0;
    }

    /// `draft` with its half-typed name replaced by the highlighted suggestion,
    /// or `None` when there is nothing to complete.
    ///
    /// A completed command that reads an argument gains the space that
    /// separates it, because the next thing typed is that argument — and that
    /// space also ends the suggestions, which is the honest signal that the
    /// name is now settled. One that reads no argument is left bare, since a
    /// trailing space there would only have to be deleted before Enter.
    pub fn completed(&self, draft: &str) -> Option<String> {
        let found = suggestions(draft);
        let command = found.get(self.selected(found.len()))?;
        Some(match command.argument {
            Some(_) => format!("/{} ", command.name),
            None => format!("/{}", command.name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(draft: &str) -> Vec<&'static str> {
        suggestions(draft).iter().map(|c| c.name).collect()
    }

    #[test]
    fn a_bare_slash_offers_the_whole_catalog() {
        assert_eq!(names("/").len(), COMMANDS.len());
    }

    #[test]
    fn typing_narrows_to_the_names_that_still_fit() {
        assert_eq!(names("/pro"), vec!["providers", "projects", "provider"]);
        assert_eq!(names("/th"), vec!["thinking"]);
    }

    #[test]
    fn ordinary_prompts_are_never_interrupted() {
        for draft in ["", "search the web", "what does / mean", "a /slash midway"] {
            assert!(names(draft).is_empty(), "{draft:?} must not suggest");
        }
    }

    #[test]
    fn the_list_closes_once_the_name_is_settled() {
        // A space means an argument is being written, and a newline means this
        // is a prompt that merely opens with a slash.
        assert!(names("/provider ").is_empty());
        assert!(names("/provider codex").is_empty());
        assert!(names("/help\nand more").is_empty());
        // A fully typed name with nothing else matching is settled too.
        assert!(names("/help").is_empty());
        assert!(names("/yolo").is_empty());
    }

    #[test]
    fn a_fully_typed_prefix_of_a_longer_name_keeps_suggesting() {
        // `/provider` is complete, but `/providers` is one keystroke away and
        // does something different. Closing the list here would hide that.
        assert_eq!(names("/provider"), vec!["providers", "provider"]);
    }

    #[test]
    fn an_unknown_name_offers_nothing_rather_than_everything() {
        assert!(names("/banana").is_empty());
    }

    #[test]
    fn case_does_not_matter_because_dispatch_ignores_it_too() {
        assert_eq!(names("/YOL"), vec!["yolo"]);
    }

    #[test]
    fn the_selection_survives_a_list_that_shrinks_under_it() {
        let mut completion = Completion::default();
        completion.down(COMMANDS.len());
        completion.down(COMMANDS.len());
        assert_eq!(completion.selected(2), 1, "clamped into the shorter list");
        assert_eq!(completion.selected(0), 0, "and into an empty one");
        assert_eq!(
            completion.selected(COMMANDS.len()),
            2,
            "without having lost where it was"
        );
    }

    #[test]
    fn moving_wraps_at_both_ends() {
        let mut completion = Completion::default();
        completion.up(3);
        assert_eq!(completion.selected(3), 2);
        completion.down(3);
        assert_eq!(completion.selected(3), 0);
    }

    #[test]
    fn moving_over_an_empty_list_is_not_a_panic() {
        let mut completion = Completion::default();
        completion.down(0);
        completion.up(0);
        assert_eq!(completion.selected(0), 0);
    }

    #[test]
    fn tab_takes_the_highlighted_row_not_the_first() {
        let mut completion = Completion::default();
        assert_eq!(completion.completed("/pro").as_deref(), Some("/providers"));
        completion.down(suggestions("/pro").len());
        completion.down(suggestions("/pro").len());
        assert_eq!(completion.completed("/pro").as_deref(), Some("/provider "));
    }

    #[test]
    fn completing_a_command_that_reads_an_argument_leaves_room_for_it() {
        let completion = Completion::default();
        // The trailing space is also what ends the suggestions, so completing
        // and then typing the argument never fights the list.
        let completed = completion.completed("/mod").expect("a match");
        assert_eq!(completed, "/model ");
        assert!(suggestions(&completed).is_empty());
    }

    #[test]
    fn completing_a_command_that_reads_nothing_is_ready_to_send() {
        let completion = Completion::default();
        let completed = completion.completed("/yo").expect("a match");
        assert_eq!(completed, "/yolo");
        assert!(
            suggestions(&completed).is_empty(),
            "and the list stands down"
        );
    }

    #[test]
    fn there_is_nothing_to_complete_when_nothing_matches() {
        assert!(Completion::default().completed("/banana").is_none());
        assert!(Completion::default().completed("hello").is_none());
    }

    #[test]
    fn every_suggestion_is_a_command_that_runs() {
        // The same law the menu is held to: what is offered must dispatch.
        // `commands::every_catalogued_command_dispatches` proves the catalog
        // does, and this proves suggestions never come from anywhere else.
        for name in names("/") {
            assert!(
                crate::commands::lookup(name).is_some(),
                "/{name} is suggested but is not in the catalog"
            );
        }
    }
}
