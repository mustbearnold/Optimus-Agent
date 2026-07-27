//! Layout for the terminal face: transcript, composer, status.
//!
//! Row content comes from [`crate::transcript`]; this module only decides where
//! rows go and what colour they are.

use ratatui::prelude::*;
use ratatui::widgets::{
    Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::mouse;
use crate::session::{Role, TuiSession};
use crate::transcript::{self, Row};
use crate::{bordered, wrapped};

pub fn draw(frame: &mut Frame, session: &TuiSession) {
    let areas = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(frame.area());

    // Rows are pre-wrapped to the inner width, so scrolling is exact and the
    // tail stays visible as text streams in.
    let inner_width = areas[0].width.saturating_sub(2);
    let rows = visible_rows(session, inner_width);
    let height = areas[0].height.saturating_sub(2) as usize;
    let offset = scroll_offset(rows.len(), height, session.scroll_back) as u16;

    let lines: Vec<Line> = rows.iter().map(paint).collect();
    frame.render_widget(
        Paragraph::new(lines).scroll((offset, 0)).block(bordered(
            &session
                .running_tool
                .as_ref()
                .map_or_else(|| "Optimus".to_string(), |tool| format!("Optimus — {tool}")),
        )),
        areas[0],
    );
    draw_scrollbar(frame, areas[0], rows.len(), height, offset as usize);

    let composer = format!("› {}", session.composer);
    frame.render_widget(wrapped(composer).block(bordered("Message")), areas[1]);

    frame.render_widget(
        wrapped(session.status_line()).style(Style::default().add_modifier(Modifier::DIM)),
        areas[2],
    );

    if let Some(picker) = session.picker.as_ref() {
        draw_picker(frame, picker);
    }
}

/// Colour a row by whose turn it is. Distinct hues per role are what make the
/// transcript scannable rather than one undifferentiated block of text.
fn paint(row: &Row) -> Line<'static> {
    let base = match row.role {
        Role::User => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        Role::Assistant => Style::default(),
        Role::Tool => Style::default().fg(Color::Magenta),
        Role::Action => Style::default().fg(Color::Yellow),
        Role::Error => Style::default().fg(Color::Red),
    };
    Line::from(
        row.segments
            .iter()
            .map(|segment| {
                let mut style = base;
                if segment.bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if segment.dim {
                    style = style.add_modifier(Modifier::DIM);
                }
                Span::styled(segment.text.clone(), style)
            })
            .collect::<Vec<_>>(),
    )
}

/// The track on the transcript's right border.
///
/// Drawn only when there is something to scroll: a full-height thumb on a short
/// transcript would suggest history that is not there.
fn draw_scrollbar(frame: &mut Frame, block: Rect, rows: usize, height: usize, offset: usize) {
    if rows <= height {
        return;
    }
    let mut state = ScrollbarState::new(rows.saturating_sub(height)).position(offset);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█"),
        // Inset vertically so the track runs beside the viewport and leaves the
        // block's corners intact; the same rows `mouse::regions` hit-tests.
        block.inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut state,
    );
}

/// Centred overlay. Drawn last so it sits above the transcript, and `Clear`ed
/// first so the text underneath cannot bleed through.
fn draw_picker(frame: &mut Frame, picker: &crate::picker::Picker) {
    // Same rectangle the hit-test uses, so a click lands on the row shown.
    let rect = mouse::picker_rect(frame.area(), picker);

    let rows: Vec<ListItem> = picker
        .items
        .iter()
        .map(|item| {
            let mark = if item.current { "● " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{mark}{}", item.label)),
                Span::styled(
                    format!("  {}", item.detail),
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(picker.selected()));

    frame.render_widget(Clear, rect);
    frame.render_stateful_widget(
        List::new(rows)
            .block(bordered(&picker.title))
            .highlight_symbol("> ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        rect,
        &mut state,
    );
}

/// Every row the transcript pane shows, including the live activity row.
///
/// The spinner is part of the scrollable content rather than fixed chrome, so
/// it sits directly under the last message where the eye already is.
pub fn visible_rows(session: &TuiSession, width: u16) -> Vec<Row> {
    let mut rows = transcript::rows(&session.messages, width, session.chrome);
    if let Some(activity) = session.activity_line() {
        rows.push(Row {
            role: Role::Assistant,
            segments: Vec::new(),
        });
        rows.push(Row {
            role: Role::Tool,
            segments: vec![crate::transcript::Segment::plain(format!("  {activity}"))],
        });
    }
    rows
}

/// Transcript as plain lines. Separated from rendering so it can be asserted in
/// tests without standing up a terminal.
pub fn transcript_text(session: &TuiSession, width: u16) -> Vec<String> {
    visible_rows(session, width)
        .iter()
        .map(Row::plain)
        .collect()
}

/// First row hidden above the viewport, keeping the tail visible by default
/// and honouring how far the user scrolled back.
pub fn scroll_offset(rows: usize, height: usize, scroll_back: usize) -> usize {
    rows.saturating_sub(height).saturating_sub(scroll_back)
}

/// Rows the user can scroll back before the top row is already on screen.
pub fn max_scroll_back(rows: usize, height: usize) -> usize {
    rows.saturating_sub(height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn session_with(lines: &[(Role, &str)]) -> (tempfile::TempDir, TuiSession) {
        let dir = tempdir().unwrap();
        let mut session = TuiSession::new(dir.path().join("home"));
        for (role, text) in lines {
            session.push(*role, (*text).into());
        }
        (dir, session)
    }

    #[test]
    fn the_empty_transcript_still_greets() {
        let (_dir, session) = session_with(&[]);
        assert_eq!(
            transcript_text(&session, 80)[0],
            "  What should Optimus do?"
        );
    }

    #[test]
    fn an_idle_session_shows_no_spinner_row() {
        let (_dir, session) = session_with(&[(Role::User, "hi")]);
        let rows = transcript_text(&session, 80);
        assert!(
            !rows.iter().any(|r| r.contains("Ctrl-C to interrupt")),
            "an idle spinner would read as a stuck run: {rows:?}"
        );
    }

    #[test]
    fn each_role_paints_a_distinct_colour() {
        let mut seen = Vec::new();
        for role in [
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Action,
            Role::Error,
        ] {
            let row = Row {
                role,
                segments: vec![crate::transcript::Segment::plain("x")],
            };
            seen.push(paint(&row).spans[0].style.fg);
        }
        let unique: std::collections::HashSet<_> = seen.iter().collect();
        assert_eq!(
            unique.len(),
            seen.len(),
            "roles must not share a colour, or the transcript cannot be scanned"
        );
    }

    #[test]
    fn bold_segments_survive_painting() {
        let row = Row {
            role: Role::Assistant,
            segments: vec![
                crate::transcript::Segment::plain("plain"),
                crate::transcript::Segment {
                    text: "loud".into(),
                    bold: true,
                    dim: false,
                },
            ],
        };
        let line = paint(&row);
        assert!(!line.spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    /// Render a real frame and read it back as text, so layout is asserted the
    /// way a user sees it rather than through the row model alone.
    fn render(session: &TuiSession, width: u16, height: u16) -> Vec<String> {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|f| draw(f, session)).expect("draw");
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn a_running_turn_paints_a_spinner_under_the_transcript() {
        let (_dir, mut session) = session_with(&[(Role::User, "whats the ai news today")]);
        let _worker = session.busy_for_test("working");
        let screen = render(&session, 60, 12).join("\n");
        assert!(screen.contains("│ whats the ai news today │"), "{screen}");
        assert!(
            screen.contains("Ctrl-C to interrupt"),
            "the spinner must be on screen while a turn runs:\n{screen}"
        );
    }

    #[test]
    fn a_tool_call_is_painted_in_the_transcript_not_hidden_in_the_footer() {
        let (_dir, mut session) = session_with(&[(Role::User, "search the web")]);
        session.messages.push(crate::session::Message {
            role: Role::Tool,
            text: "web_search  Found 3 sources  (1.2s)".into(),
            call_id: Some("call-1".into()),
        });
        let screen = render(&session, 60, 12).join("\n");
        assert!(
            screen.contains("⏺ web_search  Found 3 sources  (1.2s)"),
            "tool work belongs on screen:\n{screen}"
        );
    }

    #[test]
    fn the_running_tool_is_named_in_the_pane_title() {
        let (_dir, mut session) = session_with(&[(Role::User, "search")]);
        let _worker = session.busy_for_test("working");
        session.running_tool = Some("web_search".into());
        let screen = render(&session, 60, 8).join("\n");
        assert!(screen.contains("Optimus — web_search"), "{screen}");
    }

    /// The rightmost column of each row, which is where the track is drawn.
    fn right_edge(screen: &[String]) -> Vec<char> {
        screen
            .iter()
            .map(|row| row.chars().last().unwrap_or(' '))
            .collect()
    }

    #[test]
    fn a_transcript_taller_than_the_pane_grows_a_scrollbar() {
        let lines: Vec<(Role, &str)> = (0..30).map(|_| (Role::Assistant, "row")).collect();
        let (_dir, session) = session_with(&lines);
        let edge = right_edge(&render(&session, 40, 14));
        assert!(edge.contains(&'█'), "no thumb was painted: {edge:?}");
    }

    #[test]
    fn a_short_transcript_gets_no_scrollbar_at_all() {
        let (_dir, session) = session_with(&[(Role::Assistant, "just one")]);
        let edge = right_edge(&render(&session, 40, 14));
        assert!(
            !edge.contains(&'█'),
            "a thumb would imply history that is not there: {edge:?}"
        );
    }

    #[test]
    fn the_scrollbar_leaves_the_panes_corners_intact() {
        let lines: Vec<(Role, &str)> = (0..30).map(|_| (Role::Assistant, "row")).collect();
        let (_dir, session) = session_with(&lines);
        let screen = render(&session, 40, 14);
        let edge = right_edge(&screen);
        assert_eq!(edge[0], '┐', "top-right corner: {}", screen[0]);
        assert_eq!(edge[9], '┘', "bottom-right corner: {}", screen[9]);
    }

    #[test]
    fn the_thumb_rides_up_the_track_as_the_transcript_scrolls_back() {
        let lines: Vec<(Role, &str)> = (0..40).map(|_| (Role::Assistant, "row")).collect();
        let (_dir, mut session) = session_with(&lines);
        let at_tail = right_edge(&render(&session, 40, 14))
            .iter()
            .position(|c| *c == '█')
            .expect("thumb at the tail");
        session.scroll_back = 30;
        let scrolled = right_edge(&render(&session, 40, 14))
            .iter()
            .position(|c| *c == '█')
            .expect("thumb after scrolling");
        assert!(
            scrolled < at_tail,
            "scrolling into history must move the thumb up: {scrolled} vs {at_tail}"
        );
    }

    /// The renderer and the hit-test have to agree on where the track is, or a
    /// drag moves a bar the user is not pointing at.
    #[test]
    fn the_painted_track_is_the_column_the_hit_test_reads() {
        let lines: Vec<(Role, &str)> = (0..30).map(|_| (Role::Assistant, "row")).collect();
        let (_dir, session) = session_with(&lines);
        let screen = render(&session, 40, 14);
        let track = crate::mouse::regions(Rect::new(0, 0, 40, 14)).track;
        let painted: Vec<usize> = screen
            .iter()
            .enumerate()
            .filter(|(_, row)| row.ends_with('█'))
            .map(|(y, _)| y)
            .collect();
        for y in painted {
            assert!(
                y >= usize::from(track.y) && y < usize::from(track.y + track.height),
                "row {y} is painted outside the track {track:?}"
            );
        }
    }

    #[test]
    fn the_offset_keeps_the_tail_visible_until_the_user_scrolls() {
        assert_eq!(scroll_offset(10, 4, 0), 6, "tail visible by default");
        assert_eq!(scroll_offset(10, 4, 2), 4, "scrolled two rows up");
        assert_eq!(scroll_offset(10, 4, 99), 0, "over-scroll stops at the top");
        assert_eq!(scroll_offset(3, 10, 0), 0, "short transcripts never scroll");
    }

    #[test]
    fn max_scroll_back_is_the_rows_above_the_viewport() {
        assert_eq!(max_scroll_back(10, 4), 6);
        assert_eq!(max_scroll_back(3, 10), 0);
    }
}
