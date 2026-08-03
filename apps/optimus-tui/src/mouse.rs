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
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};

use crate::picker::Picker;
use crate::sidebar;
use crate::width;

/// Rows the wheel moves per notch.
const WHEEL: isize = 3;
pub const HORIZONTAL_GUTTER: u16 = 2;

/// The panes of the terminal face, in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regions {
    /// Flat transcript viewport inside the workbench's horizontal gutter.
    pub transcript: Rect,
    /// The scrollbar track at the transcript viewport's right edge.
    pub track: Rect,
}

/// The stable workbench frame. Rendering and hit-testing both consume this
/// geometry so adding the context rail or key rail cannot move the cursor away
/// from the cell the mouse thinks it is pointing at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutAreas {
    pub sidebar: Rect,
    pub sidebar_divider: Rect,
    pub context: Rect,
    pub transcript: Rect,
    pub composer: Rect,
    pub status: Rect,
    pub help: Rect,
}

/// Pointer-owned presentation state passed to the pure hit-test function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interaction {
    pub scrollbar_dragging: bool,
    pub sidebar_open: bool,
    pub sidebar_width: u16,
    pub sidebar_dragging: bool,
    pub sidebar_state: sidebar::HitState,
}

/// Split the frame into the workbench's five visual bands.
#[cfg(test)]
pub fn layout(area: Rect, composer_height: u16) -> LayoutAreas {
    layout_with_sidebar(area, composer_height, false, sidebar::DEFAULT_WIDTH)
}

/// Split the frame into the optional workspace rail and the workbench's five
/// visual bands. The same rectangles drive rendering and pointer hit-testing.
pub fn layout_with_sidebar(
    area: Rect,
    composer_height: u16,
    sidebar_open: bool,
    sidebar_width: u16,
) -> LayoutAreas {
    // A narrow horizontal gutter keeps the workbench from becoming a wall of
    // edge-to-edge glyphs. It also gives the command bar the breathing room
    // visible in the reference terminal while preserving one shared geometry
    // contract for drawing and hit-testing.
    let (sidebar, sidebar_divider, content) = horizontal_areas(area, sidebar_open, sidebar_width);
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(composer_height),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(content);
    LayoutAreas {
        sidebar,
        sidebar_divider,
        context: areas[0],
        transcript: areas[1],
        composer: areas[2],
        status: areas[3],
        help: areas[4],
    }
}

/// The content width after the outer gutter and optional rail have been
/// reserved. It is the width of the rectangle handed to the five-band layout,
/// not the width of the composer text inside its border.
pub fn content_width(area_width: u16, sidebar_open: bool, sidebar_width: u16) -> u16 {
    horizontal_areas(
        Rect {
            x: 0,
            y: 0,
            width: area_width,
            height: 1,
        },
        sidebar_open,
        sidebar_width,
    )
    .2
    .width
}

/// Whether the rail has enough room to leave a readable main workbench beside
/// it. At smaller sizes the requested open state is retained, but the visual
/// rail collapses so the prompt never becomes a postage stamp.
pub fn sidebar_visible(area_width: u16, sidebar_open: bool, sidebar_width: u16) -> bool {
    if !sidebar_open {
        return false;
    }
    let workbench_width = area_width.saturating_sub(HORIZONTAL_GUTTER.saturating_mul(2));
    let width = sidebar_width.clamp(sidebar::MIN_WIDTH, sidebar::MAX_WIDTH);
    workbench_width >= width + sidebar::DIVIDER_WIDTH + sidebar::MIN_CONTENT_WIDTH
}

fn horizontal_areas(area: Rect, sidebar_open: bool, sidebar_width: u16) -> (Rect, Rect, Rect) {
    let workbench = area.inner(Margin::new(HORIZONTAL_GUTTER, 0));
    if !sidebar_visible(area.width, sidebar_open, sidebar_width) {
        return (Rect::default(), Rect::default(), workbench);
    }

    let width = sidebar_width.clamp(sidebar::MIN_WIDTH, sidebar::MAX_WIDTH);
    let areas = Layout::horizontal([
        Constraint::Length(width),
        Constraint::Length(sidebar::DIVIDER_WIDTH),
        Constraint::Min(0),
    ])
    .split(workbench);
    (areas[0], areas[1], areas[2])
}

/// Split the frame the same way [`crate::view::draw`] does.
///
/// `composer_height` is the composer block's total height for the current
/// draft — it grows with a multiline prompt, so hit-testing cannot assume a
/// fixed bottom strip. See [`crate::view::composer_height`].
#[cfg(test)]
pub fn regions(area: Rect, composer_height: u16) -> Regions {
    regions_with_sidebar(area, composer_height, false, sidebar::DEFAULT_WIDTH)
}

/// Split the hit-test regions using the same optional rail as the renderer.
pub fn regions_with_sidebar(
    area: Rect,
    composer_height: u16,
    sidebar_open: bool,
    sidebar_width: u16,
) -> Regions {
    let transcript =
        layout_with_sidebar(area, composer_height, sidebar_open, sidebar_width).transcript;
    let track = Rect {
        x: transcript.x + transcript.width.saturating_sub(1),
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
        .map(|item| width::cells(&item.label) + width::cells(&item.detail) + 8)
        .max()
        .unwrap_or(40)
        .clamp(34, 72)
        .min(usize::from(area.width)) as u16;
    let height = (picker.items.len() as u16).saturating_add(2).min(14);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

/// The first item Ratatui's one-line picker can display at its current
/// selection. Keeping this calculation beside the rectangle means the mouse
/// can map a screen row back to the same item the list paints after it scrolls.
pub fn picker_scroll_offset(rect: Rect, picker: &Picker) -> usize {
    let count = picker.items.len();
    let visible = usize::from(rect.height.saturating_sub(2));
    if count == 0 || visible == 0 {
        return 0;
    }

    let selected = picker.selected().min(count.saturating_sub(1));
    selected
        .saturating_sub(visible.saturating_sub(1))
        .min(count.saturating_sub(visible))
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
    /// A click landed this many rows into the transcript viewport. The block
    /// underneath is resolved by the caller against the laid-out rows, because
    /// only the projection knows which block a screen row paints.
    Inspect(usize),
    /// The pointer moved to a transcript row, or left the hit-map entirely.
    /// The caller resolves the row to a durable block id for presentation.
    Hover(Option<usize>),
    /// Toggle the workspace rail, including the small reopen tab when it is
    /// collapsed.
    ToggleSidebar,
    /// Open the collapsed tab without toggling a rail that is only hidden by
    /// the responsive width rule.
    OpenSidebar,
    /// Start, continue, or finish a horizontal rail resize.
    SidebarResizeStart,
    SidebarResizeTo(u16),
    SidebarResizeEnd,
    /// Move the expanded sidebar section through rows that do not all fit.
    SidebarScroll(isize),
    /// The pointer crossed the left-dismiss threshold while resizing.
    SidebarClose,
    /// A first-class action in the workspace rail.
    NewSession,
    /// A section heading in the workspace rail was selected.
    SidebarSection(sidebar::Section),
    /// Open one of the durable sessions shown in the rail.
    SidebarSession(usize),
    /// Open one of the durable pinned sessions shown in the rail.
    SidebarPinnedSession(usize),
    /// Select a project scope for the session list and subsequent turns.
    SidebarProject(usize),
    Nothing,
}

/// Read one mouse event against the current layout.
///
/// `dragging` is held by the caller: once the thumb is grabbed, motion keeps
/// scrolling even when the pointer wanders off the one-column track, which is
/// what makes a drag usable at all.
#[cfg(test)]
pub fn intent(
    event: &MouseEvent,
    area: Rect,
    composer_height: u16,
    picker: Option<&Picker>,
    dragging: bool,
) -> Intent {
    intent_with_sidebar(
        event,
        area,
        composer_height,
        picker,
        Interaction {
            scrollbar_dragging: dragging,
            sidebar_open: false,
            sidebar_width: sidebar::DEFAULT_WIDTH,
            sidebar_dragging: false,
            sidebar_state: sidebar::HitState::default(),
        },
    )
}

/// Read one mouse event with the optional workspace rail present.
pub fn intent_with_sidebar(
    event: &MouseEvent,
    area: Rect,
    composer_height: u16,
    picker: Option<&Picker>,
    interaction: Interaction,
) -> Intent {
    let regions = regions_with_sidebar(
        area,
        composer_height,
        interaction.sidebar_open,
        interaction.sidebar_width,
    );
    let at = Position::new(event.column, event.row);
    let layout = layout_with_sidebar(
        area,
        composer_height,
        interaction.sidebar_open,
        interaction.sidebar_width,
    );

    match event.kind {
        // The wheel scrolls wherever the pointer is; hunting for the transcript
        // before it responds would just feel broken.
        MouseEventKind::ScrollUp if picker.is_none() && layout.sidebar.contains(at) => {
            return Intent::SidebarScroll(WHEEL)
        }
        MouseEventKind::ScrollDown if picker.is_none() && layout.sidebar.contains(at) => {
            return Intent::SidebarScroll(-WHEEL)
        }
        MouseEventKind::ScrollUp => return Intent::Scroll(WHEEL),
        MouseEventKind::ScrollDown => return Intent::Scroll(-WHEEL),
        MouseEventKind::Up(MouseButton::Left) if interaction.sidebar_dragging => {
            return Intent::SidebarResizeEnd
        }
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved
            if interaction.sidebar_dragging =>
        {
            let workbench = area.inner(Margin::new(HORIZONTAL_GUTTER, 0));
            let requested = event.column.saturating_sub(workbench.x);
            if requested <= sidebar::CLOSE_DRAG_WIDTH {
                return Intent::SidebarClose;
            }
            return Intent::SidebarResizeTo(requested);
        }
        MouseEventKind::Up(MouseButton::Left) if interaction.scrollbar_dragging => {
            return Intent::Release;
        }
        MouseEventKind::Drag(MouseButton::Left) if interaction.scrollbar_dragging => {
            return Intent::ScrollTo(fraction(regions.track, event.row));
        }
        _ => {}
    }

    // An open picker owns the pointer, exactly as it owns the keyboard.
    if let Some(picker) = picker {
        let rect = picker_rect(area, picker);
        return match event.kind {
            MouseEventKind::Down(MouseButton::Left) => match row_of(
                rect,
                at,
                picker.items.len(),
                picker_scroll_offset(rect, picker),
            ) {
                Some(index) => Intent::Choose(index),
                None => Intent::Nothing,
            },
            MouseEventKind::Moved => Intent::Hover(None),
            _ => Intent::Nothing,
        };
    }

    if layout.sidebar.width > 0 {
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && layout.sidebar_divider.contains(at)
        {
            return Intent::SidebarResizeStart;
        }
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && layout.sidebar.contains(at)
        {
            return match sidebar::row_at_for_height(
                interaction.sidebar_state,
                at.y.saturating_sub(layout.sidebar.y),
                layout.sidebar.height,
            ) {
                sidebar::Row::Close => Intent::ToggleSidebar,
                sidebar::Row::NewSession => Intent::NewSession,
                sidebar::Row::SessionsHeading => Intent::SidebarSection(sidebar::Section::Sessions),
                sidebar::Row::ProjectsHeading => Intent::SidebarSection(sidebar::Section::Projects),
                sidebar::Row::PinnedHeading => Intent::SidebarSection(sidebar::Section::Pinned),
                sidebar::Row::Session(index) => Intent::SidebarSession(index),
                sidebar::Row::PinnedSession(index) => Intent::SidebarPinnedSession(index),
                sidebar::Row::Project(index) => Intent::SidebarProject(index),
                _ => Intent::Nothing,
            };
        }
        if matches!(event.kind, MouseEventKind::Moved)
            && (layout.sidebar.contains(at) || layout.sidebar_divider.contains(at))
        {
            return Intent::Hover(None);
        }
    } else if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
        && event.column < area.x.saturating_add(HORIZONTAL_GUTTER)
    {
        // The collapsed tab lives in the outer gutter, which is otherwise
        // empty. It is an open action, not a toggle: a rail can be logically
        // open while temporarily hidden because the terminal is too narrow.
        return Intent::OpenSidebar;
    }

    match event.kind {
        MouseEventKind::Moved => {
            if regions.transcript.contains(at) {
                Intent::Hover(Some(usize::from(at.y - regions.transcript.y)))
            } else {
                Intent::Hover(None)
            }
        }
        MouseEventKind::Down(MouseButton::Right) => Intent::OpenMenu,
        // The track first: it is one column wide and sits on the transcript's
        // border, so a press there is a grab rather than a selection.
        MouseEventKind::Down(MouseButton::Left) if regions.track.contains(at) => Intent::GrabTrack,
        MouseEventKind::Down(MouseButton::Left) if regions.transcript.contains(at) => {
            Intent::Inspect(usize::from(at.y - regions.transcript.y))
        }
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
fn row_of(rect: Rect, at: Position, count: usize, offset: usize) -> Option<usize> {
    if !rect.contains(at) {
        return None;
    }
    let visible = usize::from(rect.height.saturating_sub(2));
    if at.y <= rect.y {
        return None;
    }
    let relative = usize::from(at.y - rect.y - 1);
    if visible == 0 || relative >= visible {
        return None;
    }
    let index = offset.saturating_add(relative);
    (index < count).then_some(index)
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

    fn sidebar_interaction(open: bool, dragging: bool) -> Interaction {
        Interaction {
            scrollbar_dragging: false,
            sidebar_open: open,
            sidebar_width: 28,
            sidebar_dragging: dragging,
            sidebar_state: sidebar::HitState::default(),
        }
    }

    #[test]
    fn the_track_sits_on_the_transcript_border_beside_the_viewport() {
        let regions = regions(AREA, 3);
        assert_eq!(regions.track.x, 77, "right workbench column");
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
        let event = at(MouseEventKind::Down(MouseButton::Left), 77, 5);
        assert_eq!(intent(&event, AREA, 3, None, false), Intent::GrabTrack);
    }

    #[test]
    fn pressing_inside_the_transcript_inspects_the_row_rather_than_grabbing_the_thumb() {
        let regions = regions(AREA, 3);
        let event = at(
            MouseEventKind::Down(MouseButton::Left),
            40,
            regions.transcript.y,
        );
        assert_eq!(intent(&event, AREA, 3, None, false), Intent::Inspect(0));
        let lower = at(
            MouseEventKind::Down(MouseButton::Left),
            40,
            regions.transcript.y + 6,
        );
        assert_eq!(intent(&lower, AREA, 3, None, false), Intent::Inspect(6));
    }

    #[test]
    fn a_press_on_the_border_or_the_composer_inspects_nothing() {
        // The transcript block's own border is chrome; below it is the draft.
        let top_border = at(MouseEventKind::Down(MouseButton::Left), 40, 0);
        assert_eq!(intent(&top_border, AREA, 3, None, false), Intent::Nothing);
        let composer = at(MouseEventKind::Down(MouseButton::Left), 40, 22);
        assert_eq!(intent(&composer, AREA, 3, None, false), Intent::Nothing);
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
        let event = at(MouseEventKind::Drag(MouseButton::Left), 77, 9);
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
    fn moving_over_the_transcript_reports_a_row_without_selecting_it() {
        let regions = regions(AREA, 3);
        let inside = at(MouseEventKind::Moved, 40, regions.transcript.y + 4);
        assert_eq!(
            intent(&inside, AREA, 3, None, false),
            Intent::Hover(Some(4))
        );

        let outside = at(MouseEventKind::Moved, 40, 22);
        assert_eq!(intent(&outside, AREA, 3, None, false), Intent::Hover(None));
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
    fn clicking_a_scrolled_picker_row_targets_the_item_that_is_visible() {
        let items = (0..10)
            .map(|i| PickerItem {
                id: format!("id{i}"),
                label: format!("item {i}"),
                detail: String::new(),
                current: false,
                connected: true,
            })
            .collect();
        let mut picker = Picker::new(PickerKind::Command, "Commands", items);
        picker.select(9);
        let area = Rect {
            width: 80,
            height: 10,
            ..AREA
        };
        let rect = picker_rect(area, &picker);

        assert_eq!(rect.height, 10);
        assert_eq!(picker_scroll_offset(rect, &picker), 2);

        let first_visible = at(
            MouseEventKind::Down(MouseButton::Left),
            rect.x + 2,
            rect.y + 1,
        );
        assert_eq!(
            intent(&first_visible, area, 3, Some(&picker), false),
            Intent::Choose(2)
        );

        let last_visible = at(
            MouseEventKind::Down(MouseButton::Left),
            rect.x + 2,
            rect.y + rect.height - 2,
        );
        assert_eq!(
            intent(&last_visible, area, 3, Some(&picker), false),
            Intent::Choose(9)
        );
    }

    #[test]
    fn clicking_the_scrolled_picker_bottom_border_selects_nothing() {
        let items = (0..10)
            .map(|i| PickerItem {
                id: format!("id{i}"),
                label: format!("item {i}"),
                detail: String::new(),
                current: false,
                connected: true,
            })
            .collect();
        let mut picker = Picker::new(PickerKind::Command, "Commands", items);
        picker.select(9);
        let area = Rect {
            width: 80,
            height: 10,
            ..AREA
        };
        let rect = picker_rect(area, &picker);
        let bottom_border = at(
            MouseEventKind::Down(MouseButton::Left),
            rect.x + 2,
            rect.y + rect.height - 1,
        );

        assert_eq!(
            intent(&bottom_border, area, 3, Some(&picker), false),
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
        assert_eq!(intent(&event, tiny, 3, None, true), Intent::ScrollTo(0.0));
    }

    #[test]
    fn the_open_sidebar_owns_the_left_columns_and_keeps_the_main_track_aligned() {
        let areas = layout_with_sidebar(AREA, 3, true, sidebar::DEFAULT_WIDTH);
        assert_eq!(areas.sidebar, Rect::new(2, 0, 28, 24));
        assert_eq!(areas.sidebar_divider, Rect::new(30, 0, 1, 24));
        assert_eq!(areas.context.x, 31);
        assert_eq!(areas.context.width, 47);
        assert_eq!(regions_with_sidebar(AREA, 3, true, 28).track.x, 77);
    }

    #[test]
    fn the_wheel_pages_the_open_sidebar_only_when_the_pointer_is_over_it() {
        assert_eq!(
            intent_with_sidebar(
                &at(MouseEventKind::ScrollDown, 4, 6),
                AREA,
                3,
                None,
                sidebar_interaction(true, false),
            ),
            Intent::SidebarScroll(-WHEEL)
        );
        assert_eq!(
            intent_with_sidebar(
                &at(MouseEventKind::ScrollDown, 60, 6),
                AREA,
                3,
                None,
                sidebar_interaction(true, false),
            ),
            Intent::Scroll(-WHEEL)
        );
    }

    #[test]
    fn the_sidebar_click_targets_cover_close_new_session_and_sections() {
        let areas = layout_with_sidebar(AREA, 3, true, sidebar::DEFAULT_WIDTH);
        for (row, expected) in [
            (sidebar::CLOSE_ROW, Intent::ToggleSidebar),
            (sidebar::NEW_SESSION_ROW, Intent::NewSession),
            (
                sidebar::SESSIONS_ROW,
                Intent::SidebarSection(sidebar::Section::Sessions),
            ),
            (
                sidebar::PROJECTS_ROW,
                Intent::SidebarSection(sidebar::Section::Projects),
            ),
            (
                sidebar::PINNED_ROW,
                Intent::SidebarSection(sidebar::Section::Pinned),
            ),
        ] {
            let event = at(
                MouseEventKind::Down(MouseButton::Left),
                areas.sidebar.x + 2,
                row,
            );
            assert_eq!(
                intent_with_sidebar(&event, AREA, 3, None, sidebar_interaction(true, false)),
                expected,
                "row {row}"
            );
        }
    }

    #[test]
    fn the_divider_resizes_until_a_far_left_drag_closes_the_sidebar() {
        let areas = layout_with_sidebar(AREA, 3, true, sidebar::DEFAULT_WIDTH);
        let start = at(
            MouseEventKind::Down(MouseButton::Left),
            areas.sidebar_divider.x,
            10,
        );
        assert_eq!(
            intent_with_sidebar(&start, AREA, 3, None, sidebar_interaction(true, false)),
            Intent::SidebarResizeStart
        );

        let resize = at(MouseEventKind::Drag(MouseButton::Left), 36, 10);
        assert_eq!(
            intent_with_sidebar(&resize, AREA, 3, None, sidebar_interaction(true, true)),
            Intent::SidebarResizeTo(34)
        );

        let close = at(MouseEventKind::Moved, 6, 10);
        assert_eq!(
            intent_with_sidebar(&close, AREA, 3, None, sidebar_interaction(true, true)),
            Intent::SidebarClose
        );
        let release = at(MouseEventKind::Up(MouseButton::Left), 6, 10);
        assert_eq!(
            intent_with_sidebar(&release, AREA, 3, None, sidebar_interaction(true, true)),
            Intent::SidebarResizeEnd
        );
    }

    #[test]
    fn the_collapsed_gutter_tab_reopens_the_rail() {
        let event = at(MouseEventKind::Down(MouseButton::Left), 0, 0);
        assert_eq!(
            intent_with_sidebar(&event, AREA, 3, None, sidebar_interaction(false, false)),
            Intent::OpenSidebar
        );
    }

    #[test]
    fn a_responsive_collapsed_tab_does_not_close_a_rail_hidden_by_width() {
        let narrow = Rect { width: 60, ..AREA };
        let event = at(MouseEventKind::Down(MouseButton::Left), 0, 0);
        assert_eq!(
            intent_with_sidebar(&event, narrow, 3, None, sidebar_interaction(true, false),),
            Intent::OpenSidebar
        );
    }
}
