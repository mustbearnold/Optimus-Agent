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
const COMPACT_HEIGHT: u16 = 16;
const COMPACT_CONTENT_ROW: u16 = 7;

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
    pub(crate) session_offset: usize,
    pub(crate) projects: usize,
    pub(crate) project_offset: usize,
    pub(crate) pinned: usize,
    pub(crate) pinned_offset: usize,
}

impl Default for HitState {
    fn default() -> Self {
        Self {
            section: Section::Sessions,
            current_unsaved: true,
            sessions: 1,
            session_offset: 0,
            projects: 1,
            project_offset: 0,
            pinned: 0,
            pinned_offset: 0,
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
    session_offset: usize,
    project_offset: usize,
    pinned_offset: usize,
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
            session_offset: 0,
            project_offset: 0,
            pinned_offset: 0,
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
        if section == Section::Sessions {
            self.project_filter = ProjectFilter::All;
            self.session_offset = 0;
        }
    }

    /// Move the expanded section's small viewport. The headings stay put, so
    /// every section remains one click away while wheel users can still reach
    /// every session, project and pin rather than only the first few rows.
    pub(crate) fn scroll(&mut self, rows: isize) {
        let (count, slots, offset) = match self.section {
            Section::Sessions => (self.session_count(), SESSION_SLOTS, self.session_offset),
            Section::Projects => (self.projects.len(), PROJECT_SLOTS, self.project_offset),
            Section::Pinned => (self.pinned_count(), PINNED_SLOTS, self.pinned_offset),
        };
        let maximum = count.saturating_sub(slots);
        let next = if rows >= 0 {
            offset.saturating_sub(rows as usize)
        } else {
            offset.saturating_add(rows.unsigned_abs()).min(maximum)
        };
        match self.section {
            Section::Sessions => self.session_offset = next,
            Section::Projects => self.project_offset = next,
            Section::Pinned => self.pinned_offset = next,
        }
    }

    pub(crate) fn replace_data(
        &mut self,
        sessions: Vec<SessionMeta>,
        mut projects: Vec<ProjectEntry>,
        current_unsaved: bool,
    ) {
        projects.sort_by(|a, b| {
            a.id.is_some()
                .cmp(&b.id.is_some())
                .then(a.label.cmp(&b.label))
        });
        self.sessions = sessions;
        self.projects = projects;
        self.current_unsaved = current_unsaved;
        if matches!(&self.project_filter, ProjectFilter::Named(id) if !self.projects.iter().any(|project| project.id.as_deref() == Some(id)))
        {
            self.project_filter = ProjectFilter::All;
        }
        self.clamp_offsets();
    }

    /// Keep the active row visible after refreshes that reorder the durable
    /// list (notably pinning), while leaving wheel-driven browsing alone until
    /// the underlying data actually changes.
    pub(crate) fn reveal_session(&mut self, current: Option<&str>) {
        let (session_index, pinned_index) = {
            let sessions = self.filtered_sessions();
            let session_index = match current {
                None if self.current_unsaved => Some(0),
                Some(id) => sessions
                    .iter()
                    .position(|session| session.id.to_string() == id)
                    .map(|index| index + usize::from(self.current_unsaved)),
                _ => None,
            };
            let pinned_index = current.and_then(|id| {
                sessions
                    .iter()
                    .filter(|session| session.pinned)
                    .position(|session| session.id.to_string() == id)
            });
            (session_index, pinned_index)
        };
        if let Some(index) = session_index {
            self.session_offset = reveal(self.session_offset, index, SESSION_SLOTS);
        }
        if let Some(index) = pinned_index {
            self.pinned_offset = reveal(self.pinned_offset, index, PINNED_SLOTS);
        }
        if let Some(index) = self.projects.iter().position(|project| project.current) {
            self.project_offset = reveal(self.project_offset, index, PROJECT_SLOTS);
        }
        self.clamp_offsets();
    }

    pub(crate) fn visible_window(&self, section: Section) -> (usize, usize, usize) {
        let (offset, count, slots) = match section {
            Section::Sessions => (self.session_offset, self.session_count(), SESSION_SLOTS),
            Section::Projects => (self.project_offset, self.projects.len(), PROJECT_SLOTS),
            Section::Pinned => (self.pinned_offset, self.pinned_count(), PINNED_SLOTS),
        };
        (offset, (offset + slots).min(count), count)
    }

    pub(crate) fn hit_state(&self) -> HitState {
        HitState {
            section: self.section,
            current_unsaved: self.current_unsaved,
            sessions: self.session_count(),
            session_offset: self.session_offset,
            projects: self.projects.len(),
            project_offset: self.project_offset,
            pinned: self.pinned_count(),
            pinned_offset: self.pinned_offset,
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
        self.session_offset = 0;
        Some(project.id)
    }

    fn clamp_offsets(&mut self) {
        self.session_offset = self
            .session_offset
            .min(self.session_count().saturating_sub(SESSION_SLOTS));
        self.project_offset = self
            .project_offset
            .min(self.projects.len().saturating_sub(PROJECT_SLOTS));
        self.pinned_offset = self
            .pinned_offset
            .min(self.pinned_count().saturating_sub(PINNED_SLOTS));
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
    if height < COMPACT_HEIGHT {
        return compact_rows(state, height);
    }

    let mut rows = vec![Row::Empty; usize::from(height.max(PINNED_ROW + 1))];
    put(&mut rows, 0, Row::Workspace);
    put(&mut rows, CLOSE_ROW, Row::Close);
    put(&mut rows, NEW_SESSION_ROW, Row::NewSession);
    put(&mut rows, SESSIONS_ROW, Row::SessionsHeading);
    put(&mut rows, PROJECTS_ROW, Row::ProjectsHeading);
    put(&mut rows, PINNED_ROW, Row::PinnedHeading);

    match state.section {
        Section::Sessions => {
            let count = display_slots(
                state.sessions.saturating_sub(state.session_offset),
                SESSION_SLOTS,
            );
            for slot in 0..count {
                put(
                    &mut rows,
                    SESSIONS_ROW + 1 + slot as u16,
                    Row::Session(state.session_offset + slot),
                );
            }
        }
        Section::Projects => {
            let count = display_slots(
                state.projects.saturating_sub(state.project_offset),
                PROJECT_SLOTS,
            );
            for slot in 0..count {
                put(
                    &mut rows,
                    PROJECTS_ROW + 1 + slot as u16,
                    Row::Project(state.project_offset + slot),
                );
            }
        }
        Section::Pinned => {
            let count = display_slots(
                state.pinned.saturating_sub(state.pinned_offset),
                PINNED_SLOTS,
            );
            for slot in 0..count {
                put(
                    &mut rows,
                    PINNED_ROW + 1 + slot as u16,
                    Row::PinnedSession(state.pinned_offset + slot),
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

pub(crate) fn row_at_for_height(state: HitState, row: u16, height: u16) -> Row {
    rows(state, height)
        .get(usize::from(row))
        .copied()
        .unwrap_or(Row::Empty)
}

fn compact_rows(state: HitState, height: u16) -> Vec<Row> {
    let mut rows = vec![Row::Empty; usize::from(height)];
    put(&mut rows, 0, Row::Workspace);
    put(&mut rows, CLOSE_ROW, Row::Close);
    put(&mut rows, NEW_SESSION_ROW, Row::NewSession);
    put(&mut rows, 4, Row::SessionsHeading);
    put(&mut rows, 5, Row::ProjectsHeading);
    put(&mut rows, 6, Row::PinnedHeading);

    let (count, offset, slots) = match state.section {
        Section::Sessions => (state.sessions, state.session_offset, SESSION_SLOTS),
        Section::Projects => (state.projects, state.project_offset, PROJECT_SLOTS),
        Section::Pinned => (state.pinned, state.pinned_offset, PINNED_SLOTS),
    };
    for slot in 0..display_slots(count.saturating_sub(offset), slots) {
        let row = COMPACT_CONTENT_ROW + slot as u16;
        let item = match state.section {
            Section::Sessions => Row::Session(offset + slot),
            Section::Projects => Row::Project(offset + slot),
            Section::Pinned => Row::PinnedSession(offset + slot),
        };
        put(&mut rows, row, item);
    }
    rows
}

fn display_slots(count: usize, slots: usize) -> usize {
    count.min(slots)
}

fn reveal(offset: usize, index: usize, slots: usize) -> usize {
    if index < offset {
        index
    } else if index >= offset + slots {
        index + 1 - slots
    } else {
        offset
    }
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
        assert_eq!(
            row_at_for_height(hit, CLOSE_ROW, COMPACT_HEIGHT),
            Row::Close
        );
        assert_eq!(
            row_at_for_height(hit, NEW_SESSION_ROW, COMPACT_HEIGHT),
            Row::NewSession
        );
        assert_eq!(
            row_at_for_height(hit, SESSIONS_ROW, COMPACT_HEIGHT),
            Row::SessionsHeading
        );
        assert_eq!(
            row_at_for_height(hit, PROJECTS_ROW, COMPACT_HEIGHT),
            Row::ProjectsHeading
        );
        assert_eq!(
            row_at_for_height(hit, PINNED_ROW, COMPACT_HEIGHT),
            Row::PinnedHeading
        );
        assert_eq!(row_at_for_height(hit, 6, COMPACT_HEIGHT), Row::Session(0));
    }

    #[test]
    fn compact_rows_keep_all_section_headings_and_active_items_reachable() {
        let hit = HitState {
            projects: 5,
            pinned: 3,
            ..HitState::default()
        };
        assert_eq!(row_at_for_height(hit, 4, 10), Row::SessionsHeading);
        assert_eq!(row_at_for_height(hit, 5, 10), Row::ProjectsHeading);
        assert_eq!(row_at_for_height(hit, 6, 10), Row::PinnedHeading);

        let projects = HitState {
            section: Section::Projects,
            projects: 5,
            ..hit
        };
        assert_eq!(row_at_for_height(projects, 7, 10), Row::Project(0));
        assert_eq!(row_at_for_height(projects, 8, 10), Row::Project(1));

        let pinned = HitState {
            section: Section::Pinned,
            ..hit
        };
        assert_eq!(row_at_for_height(pinned, 7, 10), Row::PinnedSession(0));
        assert_eq!(row_at_for_height(pinned, 9, 10), Row::PinnedSession(2));
    }

    #[test]
    fn overflowing_sections_scroll_without_moving_their_headings() {
        let mut state = State {
            current_unsaved: false,
            sessions: (0..6)
                .map(|index| SessionMeta {
                    id: uuid::Uuid::new_v4(),
                    title: format!("session-{index}"),
                    created_at: format!("ts:{index}"),
                    updated_at: format!("ts:{index}"),
                    message_count: 1,
                    packs: Vec::new(),
                    pinned: false,
                    archived: false,
                    project: None,
                    inbound_policy: "hold-approval".into(),
                    discoverable: false,
                    dialog_expiry_seconds: None,
                })
                .collect(),
            ..State::default()
        };

        state.scroll(-3);
        let hit = state.hit_state();
        assert_eq!(
            row_at_for_height(hit, SESSIONS_ROW, COMPACT_HEIGHT),
            Row::SessionsHeading
        );
        assert_eq!(
            row_at_for_height(hit, SESSIONS_ROW + 1, COMPACT_HEIGHT),
            Row::Session(3)
        );
        assert_eq!(
            row_at_for_height(hit, SESSIONS_ROW + 3, COMPACT_HEIGHT),
            Row::Session(5)
        );
        assert_eq!(state.visible_window(Section::Sessions), (3, 6, 6));

        state.scroll(3);
        assert_eq!(
            row_at_for_height(state.hit_state(), SESSIONS_ROW + 1, COMPACT_HEIGHT),
            Row::Session(0)
        );
    }

    #[test]
    fn selecting_the_sessions_heading_clears_a_project_filter() {
        let mut state = State::default();
        state.projects.push(ProjectEntry {
            id: Some("project-a".into()),
            label: "project-a".into(),
            session_count: 1,
            current: false,
        });
        state.sessions = [Some("project-a"), None]
            .into_iter()
            .map(|project| SessionMeta {
                id: uuid::Uuid::new_v4(),
                title: "session".into(),
                created_at: "ts:1".into(),
                updated_at: "ts:1".into(),
                message_count: 1,
                packs: Vec::new(),
                pinned: false,
                archived: false,
                project: project.map(str::to_owned),
                inbound_policy: "hold-approval".into(),
                discoverable: false,
                dialog_expiry_seconds: None,
            })
            .collect();
        state.select_project(0);
        assert_eq!(state.session_count(), 2, "project row plus current draft");

        state.select(Section::Sessions);
        assert!(matches!(state.project_filter, ProjectFilter::All));
        assert_eq!(state.session_count(), 3);
    }

    #[test]
    fn project_rows_keep_workspace_first_and_named_scopes_stable() {
        let entry = |id: Option<&str>, label: &str| ProjectEntry {
            id: id.map(str::to_owned),
            label: label.into(),
            session_count: 0,
            current: false,
        };
        let mut state = State::default();
        state.replace_data(
            Vec::new(),
            vec![
                entry(Some("project-4"), "project-4"),
                entry(None, "workspace"),
                entry(Some("project-1"), "project-1"),
            ],
            true,
        );
        assert_eq!(state.project_at(0).unwrap().label, "workspace");
        assert_eq!(state.project_at(1).unwrap().label, "project-1");
        assert_eq!(state.project_at(2).unwrap().label, "project-4");
    }
}
