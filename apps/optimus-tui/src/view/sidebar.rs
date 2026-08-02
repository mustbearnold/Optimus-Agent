//! Rendering for the persistent workspace rail and its collapsed tab.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::mouse;
use crate::session::TuiSession;
use crate::sidebar::{self, Section};
use crate::width;

use super::{ACCENT, HAIRLINE, MUTED, SIDEBAR_ACTION, SIDEBAR_BACKGROUND, TEXT};

pub(super) fn draw(frame: &mut Frame, area: Rect, session: &TuiSession) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(SIDEBAR_BACKGROUND)),
        area,
    );

    if sidebar::NEW_SESSION_ROW < area.height {
        frame.render_widget(
            Block::default().style(Style::default().bg(SIDEBAR_ACTION)),
            Rect {
                x: area.x,
                y: area.y + sidebar::NEW_SESSION_ROW,
                width: area.width,
                height: 1,
            },
        );
    }

    let lines = (0..area.height)
        .map(|row| line(area.width, row, session))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn line(width: u16, row: u16, session: &TuiSession) -> Line<'static> {
    let (marker, label, style) = match sidebar::row_at(session.sidebar.hit_state(), row) {
        sidebar::Row::Workspace => (
            "",
            "WORKSPACE".to_owned(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        sidebar::Row::Close => (
            "‹  ",
            "close sidebar".to_owned(),
            Style::default().fg(MUTED),
        ),
        sidebar::Row::NewSession => (
            "+  ",
            "New session".to_owned(),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        sidebar::Row::SessionsHeading => (
            "",
            "SESSIONS".to_owned(),
            section_style(session.sidebar.section == Section::Sessions),
        ),
        sidebar::Row::Session(index) => session_row(session, index),
        sidebar::Row::SessionsSummary => (
            "·  ",
            format!(
                "{} session{}",
                session.sidebar.session_count(),
                if session.sidebar.session_count() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            Style::default().fg(MUTED),
        ),
        sidebar::Row::ProjectsHeading => (
            "",
            "PROJECTS".to_owned(),
            section_style(session.sidebar.section == Section::Projects),
        ),
        sidebar::Row::Project(index) => project_row(session, index),
        sidebar::Row::ProjectsSummary => (
            "·  ",
            format!(
                "{} project{}",
                session.sidebar.projects.len(),
                if session.sidebar.projects.len() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            Style::default().fg(MUTED),
        ),
        sidebar::Row::PinnedHeading => (
            "",
            "PINNED".to_owned(),
            section_style(session.sidebar.section == Section::Pinned),
        ),
        sidebar::Row::PinnedSession(index) => pinned_session_row(session, index),
        sidebar::Row::PinnedSummary => (
            "·  ",
            format!(
                "{} pinned session{}",
                session.sidebar.pinned_count(),
                if session.sidebar.pinned_count() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            Style::default().fg(MUTED),
        ),
        sidebar::Row::Empty => ("", String::new(), Style::default()),
    };

    let marker = width::take(marker, usize::from(width));
    let label_budget = usize::from(width).saturating_sub(width::cells(&marker));
    let label = width::truncate(&label, label_budget);
    Line::from(vec![
        Span::styled(marker, style),
        Span::styled(label, style),
    ])
}

fn section_style(selected: bool) -> Style {
    let style = Style::default().fg(MUTED).add_modifier(Modifier::BOLD);
    if selected {
        style.fg(ACCENT)
    } else {
        style
    }
}

fn session_row(session: &TuiSession, index: usize) -> (&'static str, String, Style) {
    let Some(meta) = session.sidebar.session_at(index) else {
        return ("▸  ", "Current session".into(), Style::default().fg(ACCENT));
    };
    let id = meta.id.to_string();
    let current = session.session_id.as_deref() == Some(id.as_str());
    let marker = if current {
        "▸  "
    } else if meta.pinned {
        "◆  "
    } else {
        "·  "
    };
    let title = if meta.title.trim().is_empty() || meta.title == "session" {
        format!("Session {}", width::take(&id, 8))
    } else {
        meta.title
    };
    let style = if current {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };
    (marker, title, style)
}

fn project_row(session: &TuiSession, index: usize) -> (&'static str, String, Style) {
    let Some(project) = session.sidebar.project_at(index) else {
        return ("", String::new(), Style::default());
    };
    let marker = if project.current { "⌂  " } else { "·  " };
    let label = format!("{}  ({})", project.label, project.session_count);
    let style = if project.current {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(TEXT)
    };
    (marker, label, style)
}

fn pinned_session_row(session: &TuiSession, index: usize) -> (&'static str, String, Style) {
    let Some(meta) = session.sidebar.pinned_session_at(index) else {
        return ("", String::new(), Style::default());
    };
    let id = meta.id.to_string();
    let title = if meta.title.trim().is_empty() || meta.title == "session" {
        format!("Session {}", width::take(&id, 8))
    } else {
        meta.title
    };
    let current = session.session_id.as_deref() == Some(id.as_str());
    (
        if current { "▸  " } else { "★  " },
        title,
        if current {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        },
    )
}

pub(super) fn draw_divider(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let middle = area.height / 2;
    let lines = (0..area.height)
        .map(|row| {
            let glyph = if row == middle { "╋" } else { "│" };
            Line::from(Span::styled(glyph, Style::default().fg(HAIRLINE)))
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

pub(super) fn draw_collapsed_tab(frame: &mut Frame, area: Rect) {
    let width = mouse::HORIZONTAL_GUTTER.min(area.width);
    if width == 0 || area.height == 0 {
        return;
    }
    let tab = Rect {
        x: area.x,
        y: area.y,
        width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "›",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )))
        .style(Style::default().bg(super::BACKGROUND)),
        tab,
    );
}
