//! State and stable row coordinates for the terminal workbench rail.
//!
//! The sidebar is intentionally presentation state. Sessions, projects, and
//! pinned work remain owned by the existing session/workbench models; this
//! module only owns whether the rail is visible, how wide it is, and which
//! section the pointer last touched.

use std::path::Path;

pub(crate) const DEFAULT_WIDTH: u16 = 28;
pub(crate) const MIN_WIDTH: u16 = 22;
pub(crate) const MAX_WIDTH: u16 = 40;
/// A drag that gets this close to the left gutter means "dismiss", not "make
/// the rail unusably thin".
pub(crate) const CLOSE_DRAG_WIDTH: u16 = 10;
pub(crate) const MIN_CONTENT_WIDTH: u16 = 34;
pub(crate) const DIVIDER_WIDTH: u16 = 1;

pub(crate) const CLOSE_ROW: u16 = 1;
pub(crate) const NEW_SESSION_ROW: u16 = 3;
pub(crate) const SESSIONS_ROW: u16 = 5;
pub(crate) const PROJECTS_ROW: u16 = 9;
pub(crate) const PINNED_ROW: u16 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Sessions,
    Projects,
    Pinned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) open: bool,
    pub(crate) width: u16,
    pub(crate) dragging: bool,
    pub(crate) section: Section,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: true,
            width: DEFAULT_WIDTH,
            dragging: false,
            section: Section::Sessions,
        }
    }
}

impl State {
    pub(crate) fn toggle(&mut self) {
        self.open = !self.open;
        self.dragging = false;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.dragging = false;
    }

    pub(crate) fn resize_to(&mut self, requested: u16) {
        if requested <= CLOSE_DRAG_WIDTH {
            self.close();
            return;
        }
        self.width = requested.clamp(MIN_WIDTH, MAX_WIDTH);
        self.open = true;
    }

    pub(crate) fn select(&mut self, section: Section) {
        self.section = section;
    }
}

/// A compact, stable project name for the rail. The full path remains in the
/// context rail; the sidebar should identify the workspace at a glance.
pub(crate) fn project_name(home: &Path) -> String {
    home.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "workspace".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizing_clamps_to_a_readable_rail_and_a_far_left_drag_closes_it() {
        let mut state = State::default();
        state.resize_to(MIN_WIDTH - 1);
        assert_eq!(state.width, MIN_WIDTH);
        assert!(state.open);

        state.resize_to(MAX_WIDTH + 1);
        assert_eq!(state.width, MAX_WIDTH);

        state.dragging = true;
        state.resize_to(CLOSE_DRAG_WIDTH);
        assert!(!state.open);
        assert!(!state.dragging);
    }
}
