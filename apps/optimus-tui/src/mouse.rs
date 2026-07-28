//! Screen geometry, and what a click at a given cell means.
//!
//! The rectangles are computed here rather than inside the draw call so that
//! painting and hit-testing read from one arithmetic. Two copies would drift,
//! and a scrollbar that draws in one place but responds in another is worse
//! than no scrollbar at all.
//!
//! Only `Rect` is borrowed from ratatui; nothing here renders, so every rule
//! below is assertable without standing up a terminal.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::picker::Picker;

/// Rows the wheel moves per notch.
const WHEEL: isize = 3;

/// The panes of the terminal face, in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regions {
    /// Transcript viewport, inside the block border.
    pub transcript: Rect,
    /// The scrollbar track: the transcript block's right border column.
    pub track: Rect,
}

/// Split the frame the same way [`crate::view::draw`] does.
///
/// `composer_height` is the composer block's total height for the current
/// draft — it grows with a multiline prompt, so hit-testing cannot assume a
/// fixed bottom strip. See [`crate::view::composer_height`].
pub fn regions(area: Rect, composer_height: u16) -> Regions {
    // The composer and status (1) are pinned to the bottom; the transcript
    // takes the rest.
    let block = Rect {
        height: area.height.saturating_sub(composer_height + 1),
        ..area
    };
    let transcript = Rect {
        x: block.x + 1,
        y: block.y + 1,
        width: block.width.saturating_sub(2),
        height: block.height.saturating_sub(2),
    };
    let track = Rect {
        x: block.x + block.width.saturating_sub(1),
        y: transcript.y,
        width: 1,
        height: transcript.height,
    };
    Regions { transcript, track }
}

/// Where a picker overlay sits. Shared with the renderer so a click lands on
/// the row the user actually sees highlighted.
pub fn picker_rect(area: Rect, picker: &Picker) -> Rect {
    let width = picker
        .items
        .iter()
        .map(|item| item.label.chars().count() + item.detail.chars().count() + 8)
        .max()
        .unwrap_or(40)
        .clamp(34, 72) as u16;
    let height = (picker.items.len() as u16).saturating_add(2).min(14);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// What the event loop should do about a mouse event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Intent {
    /// Move the transcript by this many rows, positive being back into history.
    Scroll(isize),
    /// Jump the transcript so the thumb sits this far down the track, where
    /// 0.0 is the top of the history and 1.0 is the live tail.
    ScrollTo(f64),
    /// Start dragging the scrollbar thumb.
    GrabTrack,
    /// Stop dragging.
    Release,
    /// Choose the picker row at this index.
    Choose(usize),
    /// Open the command menu.
    OpenMenu,
    Nothing,
}

/// Read one mouse event against the current layout.
///
/// `dragging` is held by the caller: once the thumb is grabbed, motion keeps
/// scrolling even when the pointer wanders off the one-column track, which is
/// what makes a drag usable at all.
pub fn intent(
    event: &MouseEvent,
    area: Rect,
    composer_height: u16,
    picker: Option<&Picker>,
    dragging: bool,
) -> Intent {
    let regions = regions(area, composer_height);
    let at = Position::new(event.column, event.row);

    match event.kind {
        // The wheel scrolls wherever the pointer is; hunting for the transcript
        // before it responds would just feel broken.
        MouseEventKind::ScrollUp => return Intent::Scroll(WHEEL),
        MouseEventKind::ScrollDown => return Intent::Scroll(-WHEEL),
        MouseEventKind::Up(MouseButton::Left) if dragging => return Intent::Release,
        MouseEventKind::Drag(MouseButton::Left) if dragging => {
            return Intent::ScrollTo(fraction(regions.track, event.row));
        }
        _ => {}
    }

    // An open picker owns the pointer, exactly as it owns the keyboard.
    if let Some(picker) = picker {
        let rect = picker_rect(area, picker);
        return match event.kind {
            MouseEventKind::Down(MouseButton::Left) => match row_of(rect, at, picker.items.len()) {
                Some(index) => Intent::Choose(index),
                None => Intent::Nothing,
            },
            _ => Intent::Nothing,
        };
    }

    match event.kind {
        MouseEventKind::Down(MouseButton::Right) => Intent::OpenMenu,
        MouseEventKind::Down(MouseButton::Left) if regions.track.contains(at) => Intent::GrabTrack,
        _ => Intent::Nothing,
    }
}

/// How far down the track a row sits, clamped to its ends so a drag that
/// overshoots the pane still parks at the top or the bottom.
fn fraction(track: Rect, row: u16) -> f64 {
    if track.height <= 1 {
        return 1.0;
    }
    let offset = row.saturating_sub(track.y).min(track.height - 1);
    f64::from(offset) / f64::from(track.height - 1)
}

/// Index of the list row under `at`, or `None` outside the rows themselves —
/// the border is part of the overlay but is not a selectable row.
fn row_of(rect: Rect, at: Position, count: usize) -> Option<usize> {
    if !rect.contains(at) {
        return None;
    }
    let index = usize::from(at.y.saturating_sub(rect.y + 1));
    (at.y > rect.y && index < count).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::{PickerItem, PickerKind};

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };

    fn at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }
    }

    fn menu() -> Picker {
        let items = (0..3)
            .map(|i| PickerItem {
                id: format!("id{i}"),
                label: format!("item {i}"),
                detail: String::new(),
                current: false,
                connected: true,
            })
            .collect();
        Picker::new(PickerKind::Provider, "Pick", items)
    }

    #[test]
    fn the_track_sits_on_the_transcript_border_beside_the_viewport() {
        let regions = regions(AREA, 3);
        assert_eq!(regions.track.x, 79, "right border column");
        assert_eq!(regions.track.y, regions.transcript.y);
        assert_eq!(regions.track.height, regions.transcript.height);
        assert_eq!(
            regions.transcript.height, 18,
            "24 rows less composer, status, and two borders"
        );
    }

    #[test]
    fn the_wheel_scrolls_wherever_the_pointer_is() {
        for column in [0, 40, 79] {
            let up = intent(
                &at(MouseEventKind::ScrollUp, column, 5),
                AREA,
                3,
                None,
                false,
            );
            assert_eq!(up, Intent::Scroll(WHEEL));
        }
        let down = intent(&at(MouseEventKind::ScrollDown, 40, 5), AREA, 3, None, false);
        assert_eq!(down, Intent::Scroll(-WHEEL));
    }

    #[test]
    fn pressing_on_the_track_grabs_the_thumb() {
        let event = at(MouseEventKind::Down(MouseButton::Left), 79, 5);
        assert_eq!(intent(&event, AREA, 3, None, false), Intent::GrabTrack);
    }

    #[test]
    fn pressing_inside_the_transcript_does_not_grab_the_thumb() {
        let event = at(MouseEventKind::Down(MouseButton::Left), 40, 5);
        assert_eq!(intent(&event, AREA, 3, None, false), Intent::Nothing);
    }

    #[test]
    fn dragging_maps_the_track_ends_to_the_top_and_the_tail() {
        let top = at(MouseEventKind::Drag(MouseButton::Left), 79, 1);
        assert_eq!(intent(&top, AREA, 3, None, true), Intent::ScrollTo(0.0));
        let bottom = at(MouseEventKind::Drag(MouseButton::Left), 79, 18);
        assert_eq!(intent(&bottom, AREA, 3, None, true), Intent::ScrollTo(1.0));
    }

    #[test]
    fn a_drag_that_leaves_the_pane_clamps_instead_of_running_away() {
        let above = at(MouseEventKind::Drag(MouseButton::Left), 5, 0);
        assert_eq!(intent(&above, AREA, 3, None, true), Intent::ScrollTo(0.0));
        let below = at(MouseEventKind::Drag(MouseButton::Left), 5, 200);
        assert_eq!(intent(&below, AREA, 3, None, true), Intent::ScrollTo(1.0));
    }

    #[test]
    fn dragging_without_a_grab_is_ignored() {
        let event = at(MouseEventKind::Drag(MouseButton::Left), 79, 9);
        assert_eq!(intent(&event, AREA, 3, None, false), Intent::Nothing);
    }

    #[test]
    fn releasing_ends_the_drag() {
        let event = at(MouseEventKind::Up(MouseButton::Left), 40, 9);
        assert_eq!(intent(&event, AREA, 3, None, true), Intent::Release);
    }

    #[test]
    fn the_right_button_opens_the_command_menu() {
        let event = at(MouseEventKind::Down(MouseButton::Right), 40, 9);
        assert_eq!(intent(&event, AREA, 3, None, false), Intent::OpenMenu);
    }

    #[test]
    fn clicking_an_open_picker_chooses_the_row_under_the_pointer() {
        let picker = menu();
        let rect = picker_rect(AREA, &picker);
        for index in 0..3 {
            let event = at(
                MouseEventKind::Down(MouseButton::Left),
                rect.x + 2,
                rect.y + 1 + index as u16,
            );
            assert_eq!(
                intent(&event, AREA, 3, Some(&picker), false),
                Intent::Choose(index)
            );
        }
    }

    #[test]
    fn clicking_the_picker_border_selects_nothing() {
        let picker = menu();
        let rect = picker_rect(AREA, &picker);
        let event = at(MouseEventKind::Down(MouseButton::Left), rect.x + 2, rect.y);
        assert_eq!(
            intent(&event, AREA, 3, Some(&picker), false),
            Intent::Nothing
        );
    }

    #[test]
    fn an_open_picker_swallows_clicks_meant_for_the_track_behind_it() {
        let picker = menu();
        let event = at(MouseEventKind::Down(MouseButton::Left), 79, 5);
        assert_eq!(
            intent(&event, AREA, 3, Some(&picker), false),
            Intent::Nothing
        );
    }

    #[test]
    fn the_wheel_still_scrolls_while_a_picker_is_open() {
        let picker = menu();
        let event = at(MouseEventKind::ScrollUp, 40, 5);
        assert_eq!(
            intent(&event, AREA, 3, Some(&picker), false),
            Intent::Scroll(WHEEL)
        );
    }

    #[test]
    fn a_one_row_pane_does_not_divide_by_zero() {
        let tiny = Rect {
            width: 20,
            height: 6,
            ..AREA
        };
        let event = at(MouseEventKind::Drag(MouseButton::Left), 19, 1);
        assert_eq!(intent(&event, tiny, 3, None, true), Intent::ScrollTo(1.0));
    }
}
