//! What a key press means, as a pure function.
//!
//! The same reasoning as [`crate::mouse`]: keeping the decision out of the
//! event loop is what makes the whole key table assertable without standing
//! up a terminal. The loop's job shrinks to carrying the answer out.
//!
//! Precedence matters more than it looks. Several keys are overloaded —
//! `Up`/`Down` belong to an open picker before they mean history, `End`
//! means end-of-line while the draft is non-empty and follow-the-tail
//! otherwise, and `Esc` clears a draft before it closes anything. The
//! [`Mode`] passed in is exactly the state those rules need.
//!
//! Open suggestions are the one non-modal claim on the keyboard. A picker
//! returns early and answers every key; suggestions take three and let the rest
//! fall through, because the composer underneath them is still being typed into.
//!
//! Inspect focus (ADR-0075 §4) is the second whole-keyboard claim after the
//! picker: while the transcript has the keyboard, letters move and open blocks
//! rather than typing. `Tab` hands the keyboard over and `Tab` or `Esc` hands
//! it back, so the draft underneath is never lost and never typed into by
//! accident.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The state a key is interpreted against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    /// A picker overlay is open and owns the keyboard.
    pub picker: bool,
    /// A turn is in flight.
    pub busy: bool,
    /// The composer holds text.
    pub drafting: bool,
    /// Command suggestions are showing over a half-typed slash command.
    pub suggesting: bool,
    /// The transcript has the keyboard.
    pub inspecting: bool,
}

/// What the event loop should do about a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    Ignore,
    /// Leave the application.
    Quit,
    /// Stop the turn in flight.
    Cancel,
    /// Send the draft.
    Submit,
    /// Insert a literal newline into the draft.
    Newline,
    Insert(char),
    Edit(Edit),
    Move(Motion),
    History(HistoryStep),
    Scroll(ScrollStep),
    /// Drop the draft without sending it.
    ClearDraft,
    Picker(PickerStep),
    /// Move the highlight through the open suggestions.
    Suggest(SuggestStep),
    /// Take the highlighted suggestion into the draft.
    Complete,
    /// Move the keyboard between the composer and the transcript.
    Focus(FocusStep),
    /// Act on the selected block.
    Block(BlockStep),
    /// Redraw from scratch (Ctrl-L), for a frame corrupted by another writer.
    Redraw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusStep {
    /// Hand the keyboard to the transcript.
    Inspect,
    /// Hand it back to the composer.
    Composer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStep {
    Next,
    Previous,
    First,
    Last,
    /// Open or close the selected block.
    Fold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    Backspace,
    Delete,
    KillToEnd,
    KillToStart,
    KillWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryStep {
    Older,
    Newer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollStep {
    PageUp,
    PageDown,
    /// Jump back to the live tail.
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerStep {
    Next,
    Previous,
    Confirm,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestStep {
    Next,
    Previous,
}

pub fn intent(key: &KeyEvent, mode: Mode) -> Intent {
    if mode.picker {
        return picker_intent(key);
    }
    // Before the suggestions: a half-typed command left in the draft must not
    // claim keys from a transcript the human has deliberately stepped into.
    if mode.inspecting {
        return inspect_intent(key, mode);
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    // Only three keys, and each is one the draft has a better use for while a
    // command is half-typed: `Tab` means nothing anywhere else, and `Up`/`Down`
    // would recall an old prompt over the top of the one being written. Enter
    // still sends and Esc still clears — a list that hijacked either would be
    // deciding something the user was in the middle of saying. Esc needs no
    // dismissal of its own: clearing the draft takes the `/` with it.
    if mode.suggesting && !ctrl && !alt {
        match key.code {
            KeyCode::Tab => return Intent::Complete,
            KeyCode::Down => return Intent::Suggest(SuggestStep::Next),
            KeyCode::Up => return Intent::Suggest(SuggestStep::Previous),
            _ => {}
        }
    }
    match key.code {
        // Ctrl-C stops a run in flight; with nothing running it exits. Pinned
        // by scripts/tui_e2e.py and advertised in /help.
        KeyCode::Char('c') if ctrl => {
            if mode.busy {
                Intent::Cancel
            } else {
                Intent::Quit
            }
        }
        // Ctrl-D exits only on an empty draft — on a draft it is
        // delete-forward, as every readline does.
        KeyCode::Char('d') if ctrl => {
            if mode.drafting {
                Intent::Edit(Edit::Delete)
            } else {
                Intent::Quit
            }
        }
        // Esc no longer quits: losing a typed prompt to a stray Esc is the
        // kind of thing that makes people distrust the tool. It clears the
        // draft; with nothing to clear it does nothing, and /quit exits.
        KeyCode::Esc => {
            if mode.drafting {
                Intent::ClearDraft
            } else {
                Intent::Ignore
            }
        }
        // Shift/Alt-Enter insert a newline where the terminal reports the
        // modifier; Ctrl-J is the fallback that works everywhere.
        KeyCode::Enter if shift || alt => Intent::Newline,
        KeyCode::Char('j') if ctrl => Intent::Newline,
        KeyCode::Enter => Intent::Submit,
        KeyCode::Delete => Intent::Edit(Edit::Delete),
        KeyCode::Char('k') if ctrl => Intent::Edit(Edit::KillToEnd),
        KeyCode::Char('u') if ctrl => Intent::Edit(Edit::KillToStart),
        KeyCode::Char('w') if ctrl => Intent::Edit(Edit::KillWord),
        KeyCode::Backspace if alt => Intent::Edit(Edit::KillWord),
        KeyCode::Backspace => Intent::Edit(Edit::Backspace),
        KeyCode::Char('a') if ctrl => Intent::Move(Motion::Home),
        KeyCode::Char('e') if ctrl => Intent::Move(Motion::End),
        KeyCode::Char('b') if alt => Intent::Move(Motion::WordLeft),
        KeyCode::Char('f') if alt => Intent::Move(Motion::WordRight),
        KeyCode::Left if ctrl || alt => Intent::Move(Motion::WordLeft),
        KeyCode::Right if ctrl || alt => Intent::Move(Motion::WordRight),
        KeyCode::Left => Intent::Move(Motion::Left),
        KeyCode::Right => Intent::Move(Motion::Right),
        KeyCode::Home => Intent::Move(Motion::Home),
        // End is overloaded: with a draft it is end-of-line, and only with an
        // empty composer does it keep its old meaning of following the tail.
        KeyCode::End => {
            if mode.drafting {
                Intent::Move(Motion::End)
            } else {
                Intent::Scroll(ScrollStep::Tail)
            }
        }
        KeyCode::Up => Intent::History(HistoryStep::Older),
        KeyCode::Down => Intent::History(HistoryStep::Newer),
        KeyCode::PageUp => Intent::Scroll(ScrollStep::PageUp),
        KeyCode::PageDown => Intent::Scroll(ScrollStep::PageDown),
        KeyCode::Char('l') if ctrl => Intent::Redraw,
        // Tab is unbound while nothing is being suggested, which is what makes
        // it free to mean "hand the keyboard to the transcript" (ADR-0075 §4).
        KeyCode::Tab => Intent::Focus(FocusStep::Inspect),
        // Control chords that reach here are unbound; typing their letter is
        // never what the human meant. Only unmodified (or shifted) characters
        // are text.
        KeyCode::Char(c) if !ctrl && !alt => Intent::Insert(c),
        _ => Intent::Ignore,
    }
}

/// What a key means while the transcript has the keyboard.
///
/// A whole-keyboard claim like the picker's, so nothing here can type into the
/// draft waiting underneath. Two ways out — `Tab` back the way in, and `Esc`
/// because that is what unwinds every other layer. The chords that stop and
/// repair a run keep working, since needing to inspect something is no reason
/// to lose the ability to interrupt it.
fn inspect_intent(key: &KeyEvent, mode: Mode) -> Intent {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Char('c') if ctrl => {
            if mode.busy {
                Intent::Cancel
            } else {
                Intent::Quit
            }
        }
        KeyCode::Char('l') if ctrl => Intent::Redraw,
        _ if ctrl || alt => Intent::Ignore,
        KeyCode::Tab | KeyCode::Esc => Intent::Focus(FocusStep::Composer),
        KeyCode::Down | KeyCode::Char('j') => Intent::Block(BlockStep::Next),
        KeyCode::Up | KeyCode::Char('k') => Intent::Block(BlockStep::Previous),
        KeyCode::Home | KeyCode::Char('g') => Intent::Block(BlockStep::First),
        KeyCode::End | KeyCode::Char('G') => Intent::Block(BlockStep::Last),
        KeyCode::Char(' ') | KeyCode::Enter => Intent::Block(BlockStep::Fold),
        KeyCode::PageUp => Intent::Scroll(ScrollStep::PageUp),
        KeyCode::PageDown => Intent::Scroll(ScrollStep::PageDown),
        _ => Intent::Ignore,
    }
}

fn picker_intent(key: &KeyEvent) -> Intent {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Down | KeyCode::Tab => Intent::Picker(PickerStep::Next),
        KeyCode::Up | KeyCode::BackTab => Intent::Picker(PickerStep::Previous),
        KeyCode::Enter => Intent::Picker(PickerStep::Confirm),
        KeyCode::Esc => Intent::Picker(PickerStep::Close),
        KeyCode::Char('c') if ctrl => Intent::Picker(PickerStep::Close),
        _ => Intent::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE: Mode = Mode {
        picker: false,
        busy: false,
        drafting: false,
        suggesting: false,
        inspecting: false,
    };
    const DRAFTING: Mode = Mode {
        drafting: true,
        ..IDLE
    };
    const BUSY: Mode = Mode { busy: true, ..IDLE };
    const PICKING: Mode = Mode {
        picker: true,
        ..IDLE
    };
    /// A slash command is half-typed, so the suggestions are on screen.
    const SUGGESTING: Mode = Mode {
        drafting: true,
        suggesting: true,
        ..IDLE
    };
    /// The transcript has the keyboard.
    const INSPECTING: Mode = Mode {
        inspecting: true,
        ..IDLE
    };

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn chord(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn a_control_chord_never_types_its_letter() {
        // The old wildcard arm meant Ctrl-A typed "a" into the prompt.
        for c in ['a', 'e', 'k', 'u', 'w'] {
            let got = intent(&chord(KeyCode::Char(c), KeyModifiers::CONTROL), DRAFTING);
            assert_ne!(got, Intent::Insert(c), "Ctrl-{c} typed its letter");
        }
        assert_eq!(
            intent(&chord(KeyCode::Char('q'), KeyModifiers::CONTROL), DRAFTING),
            Intent::Ignore,
            "an unbound chord is ignored, not typed"
        );
        assert_eq!(
            intent(&key(KeyCode::Char('a')), DRAFTING),
            Intent::Insert('a')
        );
        assert_eq!(
            intent(&chord(KeyCode::Char('A'), KeyModifiers::SHIFT), DRAFTING),
            Intent::Insert('A'),
            "shift is how capitals arrive"
        );
    }

    #[test]
    fn enter_submits_but_shift_alt_and_ctrl_j_insert_a_newline() {
        assert_eq!(intent(&key(KeyCode::Enter), DRAFTING), Intent::Submit);
        assert_eq!(
            intent(&chord(KeyCode::Enter, KeyModifiers::SHIFT), DRAFTING),
            Intent::Newline
        );
        assert_eq!(
            intent(&chord(KeyCode::Enter, KeyModifiers::ALT), DRAFTING),
            Intent::Newline
        );
        assert_eq!(
            intent(&chord(KeyCode::Char('j'), KeyModifiers::CONTROL), DRAFTING),
            Intent::Newline,
            "the fallback for terminals that cannot report Shift+Enter"
        );
    }

    #[test]
    fn escape_clears_the_draft_instead_of_throwing_the_session_away() {
        assert_eq!(intent(&key(KeyCode::Esc), DRAFTING), Intent::ClearDraft);
        assert_eq!(intent(&key(KeyCode::Esc), IDLE), Intent::Ignore);
        assert_ne!(intent(&key(KeyCode::Esc), IDLE), Intent::Quit);
    }

    #[test]
    fn ctrl_c_cancels_a_run_and_exits_when_idle() {
        // Pinned by scripts/tui_e2e.py; changing this breaks the pty gate.
        let ctrl_c = chord(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(intent(&ctrl_c, BUSY), Intent::Cancel);
        assert_eq!(intent(&ctrl_c, IDLE), Intent::Quit);
        assert_eq!(intent(&ctrl_c, PICKING), Intent::Picker(PickerStep::Close));
    }

    #[test]
    fn ctrl_d_deletes_forward_on_a_draft_and_exits_on_an_empty_one() {
        let ctrl_d = chord(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(intent(&ctrl_d, DRAFTING), Intent::Edit(Edit::Delete));
        assert_eq!(intent(&ctrl_d, IDLE), Intent::Quit);
    }

    #[test]
    fn end_is_end_of_line_while_drafting_and_follow_tail_when_empty() {
        assert_eq!(
            intent(&key(KeyCode::End), DRAFTING),
            Intent::Move(Motion::End)
        );
        assert_eq!(
            intent(&key(KeyCode::End), IDLE),
            Intent::Scroll(ScrollStep::Tail),
            "the old meaning survives where it cannot collide"
        );
    }

    #[test]
    fn an_open_picker_owns_the_arrows_before_history_does() {
        assert_eq!(
            intent(&key(KeyCode::Up), PICKING),
            Intent::Picker(PickerStep::Previous)
        );
        assert_eq!(
            intent(&key(KeyCode::Down), PICKING),
            Intent::Picker(PickerStep::Next)
        );
        assert_eq!(
            intent(&key(KeyCode::Up), IDLE),
            Intent::History(HistoryStep::Older)
        );
        assert_eq!(
            intent(&key(KeyCode::Down), IDLE),
            Intent::History(HistoryStep::Newer)
        );
        assert_eq!(
            intent(&key(KeyCode::Enter), PICKING),
            Intent::Picker(PickerStep::Confirm)
        );
        assert_eq!(intent(&key(KeyCode::Char('x')), PICKING), Intent::Ignore);
    }

    #[test]
    fn word_motions_arrive_by_both_the_alt_and_ctrl_spellings() {
        for modifier in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            assert_eq!(
                intent(&chord(KeyCode::Left, modifier), DRAFTING),
                Intent::Move(Motion::WordLeft)
            );
            assert_eq!(
                intent(&chord(KeyCode::Right, modifier), DRAFTING),
                Intent::Move(Motion::WordRight)
            );
        }
        assert_eq!(
            intent(&chord(KeyCode::Char('b'), KeyModifiers::ALT), DRAFTING),
            Intent::Move(Motion::WordLeft)
        );
        assert_eq!(
            intent(&chord(KeyCode::Backspace, KeyModifiers::ALT), DRAFTING),
            Intent::Edit(Edit::KillWord)
        );
    }

    #[test]
    fn open_suggestions_take_the_arrows_before_history_does() {
        assert_eq!(
            intent(&key(KeyCode::Down), SUGGESTING),
            Intent::Suggest(SuggestStep::Next)
        );
        assert_eq!(
            intent(&key(KeyCode::Up), SUGGESTING),
            Intent::Suggest(SuggestStep::Previous)
        );
        assert_eq!(
            intent(&key(KeyCode::Up), DRAFTING),
            Intent::History(HistoryStep::Older),
            "and give them straight back when nothing is being suggested"
        );
    }

    /// ADR-0075 §4: Tab completes while a list is open, and otherwise moves the
    /// keyboard between the composer and the transcript.
    #[test]
    fn tab_completes_while_suggesting_and_otherwise_moves_the_keyboard() {
        assert_eq!(intent(&key(KeyCode::Tab), SUGGESTING), Intent::Complete);
        assert_eq!(
            intent(&key(KeyCode::Tab), DRAFTING),
            Intent::Focus(FocusStep::Inspect)
        );
        assert_eq!(
            intent(&key(KeyCode::Tab), IDLE),
            Intent::Focus(FocusStep::Inspect)
        );
        assert_eq!(
            intent(&key(KeyCode::Tab), INSPECTING),
            Intent::Focus(FocusStep::Composer),
            "and back again"
        );
    }

    #[test]
    fn inspecting_moves_and_opens_blocks_instead_of_typing() {
        for (code, step) in [
            (KeyCode::Down, BlockStep::Next),
            (KeyCode::Char('j'), BlockStep::Next),
            (KeyCode::Up, BlockStep::Previous),
            (KeyCode::Char('k'), BlockStep::Previous),
            (KeyCode::Home, BlockStep::First),
            (KeyCode::Char('g'), BlockStep::First),
            (KeyCode::End, BlockStep::Last),
            (KeyCode::Char('G'), BlockStep::Last),
            (KeyCode::Char(' '), BlockStep::Fold),
            (KeyCode::Enter, BlockStep::Fold),
        ] {
            assert_eq!(
                intent(&key(code), INSPECTING),
                Intent::Block(step),
                "{code:?}"
            );
        }
    }

    #[test]
    fn inspecting_never_types_into_the_draft_underneath() {
        for c in ['a', 'q', 'z', '/', '1'] {
            assert_ne!(
                intent(&key(KeyCode::Char(c)), INSPECTING),
                Intent::Insert(c),
                "'{c}' must not reach a composer that does not have the keyboard"
            );
        }
        assert_eq!(intent(&key(KeyCode::Backspace), INSPECTING), Intent::Ignore);
    }

    #[test]
    fn escape_hands_the_keyboard_back_before_it_touches_the_draft() {
        let drafting_and_inspecting = Mode {
            drafting: true,
            ..INSPECTING
        };
        assert_eq!(
            intent(&key(KeyCode::Esc), drafting_and_inspecting),
            Intent::Focus(FocusStep::Composer),
            "unwind the outer layer first; the draft survives to be cleared next"
        );
    }

    #[test]
    fn inspecting_keeps_the_chords_that_stop_and_repair_a_run() {
        let ctrl_c = chord(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            intent(
                &ctrl_c,
                Mode {
                    busy: true,
                    ..INSPECTING
                }
            ),
            Intent::Cancel
        );
        assert_eq!(intent(&ctrl_c, INSPECTING), Intent::Quit);
        assert_eq!(
            intent(
                &chord(KeyCode::Char('l'), KeyModifiers::CONTROL),
                INSPECTING
            ),
            Intent::Redraw
        );
        assert_eq!(
            intent(
                &chord(KeyCode::Char('k'), KeyModifiers::CONTROL),
                INSPECTING
            ),
            Intent::Ignore,
            "a chord that means an edit must not be read as a move"
        );
    }

    #[test]
    fn the_transcript_outranks_a_half_typed_command_left_in_the_draft() {
        let both = Mode {
            inspecting: true,
            ..SUGGESTING
        };
        assert_eq!(
            intent(&key(KeyCode::Down), both),
            Intent::Block(BlockStep::Next)
        );
        assert_eq!(
            intent(&key(KeyCode::Tab), both),
            Intent::Focus(FocusStep::Composer)
        );
    }

    #[test]
    fn a_picker_still_outranks_the_transcript() {
        let both = Mode {
            picker: true,
            ..INSPECTING
        };
        assert_eq!(
            intent(&key(KeyCode::Down), both),
            Intent::Picker(PickerStep::Next)
        );
    }

    #[test]
    fn a_picker_still_outranks_the_suggestions() {
        // Both cannot be on screen, but precedence should not depend on that
        // staying true: the modal list is the one the user opened on purpose.
        let both = Mode {
            picker: true,
            ..SUGGESTING
        };
        assert_eq!(
            intent(&key(KeyCode::Down), both),
            Intent::Picker(PickerStep::Next)
        );
        assert_eq!(
            intent(&key(KeyCode::Tab), both),
            Intent::Picker(PickerStep::Next)
        );
    }

    #[test]
    fn suggestions_never_take_the_keys_that_end_a_thought() {
        // Enter sends and Esc clears, exactly as they do without a list up.
        // Hijacking either would decide something mid-sentence, and Esc taking
        // the draft with it is what dismisses the suggestions anyway.
        assert_eq!(intent(&key(KeyCode::Enter), SUGGESTING), Intent::Submit);
        assert_eq!(intent(&key(KeyCode::Esc), SUGGESTING), Intent::ClearDraft);
        assert_eq!(
            intent(&key(KeyCode::Char('p')), SUGGESTING),
            Intent::Insert('p'),
            "and typing carries on underneath, which a picker would not allow"
        );
        assert_eq!(
            intent(&key(KeyCode::Backspace), SUGGESTING),
            Intent::Edit(Edit::Backspace)
        );
        assert_eq!(
            intent(
                &chord(KeyCode::Char('c'), KeyModifiers::CONTROL),
                SUGGESTING
            ),
            Intent::Quit,
            "and the chords keep their meanings"
        );
    }
}
