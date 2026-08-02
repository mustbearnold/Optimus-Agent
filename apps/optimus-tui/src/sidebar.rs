//! State and stable row coordinates for the terminal workbench rail.
//!
//! The rail is a real projection of durable session state. This module keeps
//! the presentation state and the small, shared row map used by both drawing
//! and mouse hit-testing; the session store remains the source of truth.

use std::path::Path;

use optimus_kernel::SessionMeta;

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

const SESSION_SLOTS: usize = 3;
const PROJECT_SLOTS: usize = 2;
const PINNED_SLOTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Section {
    #[default]
    Sessions,
    Projects,
    Pinned,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum ProjectFilter {
    #[default]
    All,
    Workspace,
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectEntry {
    pub(crate) id: Option<String>,
    pub(crate) label: String,
    pub(crate) session_count: usize,
    pub(crate) current: bool,
}

/// A semantic row in the rail. The renderer turns it into text and the mouse
/// layer turns it into an intent; neither has to copy the vertical arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Empty,
    Workspace,
    Close,
    NewSession,
    SessionsHeading,
    Session(usize),
    SessionsSummary,
    ProjectsHeading,
    Project(usize),
    ProjectsSummary,
    PinnedHeading,
    PinnedSession(usize),
    PinnedSummary,
}

/// The small copyable snapshot needed by pure hit-testing. It deliberately
/// carries counts, not session objects, so input handling never owns a DB row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HitState {
    pub(crate) section: Section,
    pub(crate) current_unsaved: bool,
    pub(crate) sessions: usize,
    pub(crate) projects: usize,
    pub(crate) pinned: usize,
}

impl Default for HitState {
    fn default() -> Self {
        Self {
            section: Section::Sessions,
            current_unsaved: true,
            sessions: 1,
            projects: 1,
            pinned: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct State {
    pub(crate) open: bool,
    pub(crate) width: u16,
    pub(crate) dragging: bool,
    pub(crate) section: Section,
    /// Durable active sessions, sorted by the kernel store's pinned/updated
    /// order. The current unsaved draft is represented separately.
    pub(crate) sessions: Vec<SessionMeta>,
    pub(crate) projects: Vec<ProjectEntry>,
    pub(crate) current_unsaved: bool,
    pub(crate) project_filter: ProjectFilter,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: true,
            width: DEFAULT_WIDTH,
            dragging: false,
            section: Section::Sessions,
            sessions: Vec::new(),
            projects: Vec::new(),
            current_unsaved: true,
            project_filter: ProjectFilter::All,
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

    pub(crate) fn replace_data(
        &mut self,
        sessions: Vec<SessionMeta>,
        projects: Vec<ProjectEntry>,
        current_unsaved: bool,
    ) {
        self.sessions = sessions;
        self.projects = projects;
        self.current_unsaved = current_unsaved;
        if matches!(&self.project_filter, ProjectFilter::Named(id) if !self.projects.iter().any(|project| project.id.as_deref() == Some(id)))
        {
            self.project_filter = ProjectFilter::All;
        }
    }

    pub(crate) fn hit_state(&self) -> HitState {
        HitState {
            section: self.section,
            current_unsaved: self.current_unsaved,
            sessions: self.session_count(),
            projects: self.projects.len(),
            pinned: self.pinned_count(),
        }
    }

    pub(crate) fn session_count(&self) -> usize {
        self.filtered_sessions().len() + usize::from(self.current_unsaved)
    }

    pub(crate) fn pinned_count(&self) -> usize {
        self.filtered_sessions()
            .into_iter()
            .filter(|session| session.pinned)
            .count()
    }

    pub(crate) fn session_at(&self, index: usize) -> Option<SessionMeta> {
        if self.current_unsaved {
            if index == 0 {
                return None;
            }
            self.filtered_sessions().get(index - 1).cloned().cloned()
        } else {
            self.filtered_sessions().get(index).cloned().cloned()
        }
    }

    pub(crate) fn pinned_session_at(&self, index: usize) -> Option<SessionMeta> {
        self.filtered_sessions()
            .into_iter()
            .filter(|session| session.pinned)
            .nth(index)
            .cloned()
    }

    pub(crate) fn project_at(&self, index: usize) -> Option<ProjectEntry> {
        self.projects.get(index).cloned()
    }

    pub(crate) fn select_project(&mut self, index: usize) -> Option<Option<String>> {
        let project = self.projects.get(index)?.clone();
        self.project_filter = match &project.id {
            Some(id) => ProjectFilter::Named(id.clone()),
            None => ProjectFilter::Workspace,
        };
        self.section = Section::Sessions;
        Some(project.id)
    }

    fn filtered_sessions(&self) -> Vec<&SessionMeta> {
        self.sessions
            .iter()
            .filter(|session| match &self.project_filter {
                ProjectFilter::All => true,
                ProjectFilter::Workspace => session.project.is_none(),
                ProjectFilter::Named(id) => session.project.as_deref() == Some(id.as_str()),
            })
            .collect()
    }
}

/// Return the complete stable row map, cropped only by the terminal height.
/// Section headers keep their established coordinates so a user can build
/// muscle memory, while selecting a section expands its actual contents.
pub(crate) fn rows(state: HitState, height: u16) -> Vec<Row> {
    let mut rows = vec![Row::Empty; usize::from(height.max(PINNED_ROW + 1))];
    put(&mut rows, 0, Row::Workspace);
    put(&mut rows, CLOSE_ROW, Row::Close);
    put(&mut rows, NEW_SESSION_ROW, Row::NewSession);
    put(&mut rows, SESSIONS_ROW, Row::SessionsHeading);
    put(&mut rows, PROJECTS_ROW, Row::ProjectsHeading);
    put(&mut rows, PINNED_ROW, Row::PinnedHeading);

    match state.section {
        Section::Sessions => {
            let count = display_slots(state.sessions, SESSION_SLOTS);
            for index in 0..count {
                put(
                    &mut rows,
                    SESSIONS_ROW + 1 + index as u16,
                    Row::Session(index),
                );
            }
        }
        Section::Projects => {
            let count = display_slots(state.projects, PROJECT_SLOTS);
            for index in 0..count {
                put(
                    &mut rows,
                    PROJECTS_ROW + 1 + index as u16,
                    Row::Project(index),
                );
            }
        }
        Section::Pinned => {
            let count = display_slots(state.pinned, PINNED_SLOTS);
            for index in 0..count {
                put(
                    &mut rows,
                    PINNED_ROW + 1 + index as u16,
                    Row::PinnedSession(index),
                );
            }
        }
    }

    // Inactive sections still explain what they contain; this makes the rail
    // useful before the user has clicked every heading once.
    if state.section != Section::Sessions {
        put(&mut rows, SESSIONS_ROW + 1, Row::SessionsSummary);
    }
    if state.section != Section::Projects {
        put(&mut rows, PROJECTS_ROW + 1, Row::ProjectsSummary);
    }
    if state.section != Section::Pinned {
        put(&mut rows, PINNED_ROW + 1, Row::PinnedSummary);
    }

    rows.truncate(usize::from(height));
    rows
}

pub(crate) fn row_at(state: HitState, row: u16) -> Row {
    rows(state, row.saturating_add(1))
        .get(usize::from(row))
        .copied()
        .unwrap_or(Row::Empty)
}

fn display_slots(count: usize, slots: usize) -> usize {
    count.min(slots)
}

fn put(rows: &mut [Row], row: u16, value: Row) {
    if let Some(slot) = rows.get_mut(usize::from(row)) {
        *slot = value;
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

    #[test]
    fn default_rows_keep_the_original_heading_coordinates() {
        let hit = HitState::default();
        assert_eq!(row_at(hit, CLOSE_ROW), Row::Close);
        assert_eq!(row_at(hit, NEW_SESSION_ROW), Row::NewSession);
        assert_eq!(row_at(hit, SESSIONS_ROW), Row::SessionsHeading);
        assert_eq!(row_at(hit, PROJECTS_ROW), Row::ProjectsHeading);
        assert_eq!(row_at(hit, PINNED_ROW), Row::PinnedHeading);
        assert_eq!(row_at(hit, 6), Row::Session(0));
    }
}
