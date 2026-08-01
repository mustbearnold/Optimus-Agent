//! Layout for the terminal face: transcript, composer, status.
//!
//! Row content comes from [`crate::transcript`]; this module only decides where
//! rows go and what colour they are.

use ratatui::prelude::*;
use ratatui::widgets::{
    Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

mod composer;

use crate::mouse;
use crate::session::{Role, TuiSession};
use crate::transcript::{self, Row};
use crate::{bordered, wrapped};

/// Rows the composer block occupies for this draft, borders included. One
/// arithmetic, shared by the layout below and by every caller that has to
/// subtract the composer from the frame.
pub fn composer_height(session: &TuiSession, width: u16) -> u16 {
    composer::height(session.composer.text(), width)
}

pub fn draw(frame: &mut Frame, session: &TuiSession) {
    let areas = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(composer_height(session, frame.area().width)),
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

    composer::render(frame, areas[1], &session.composer);

    frame.render_widget(
        wrapped(session.status_line()).style(Style::default().add_modifier(Modifier::DIM)),
        areas[2],
    );

    // Suggestions first, so a picker the user opened on purpose covers a list
    // that merely appeared because they typed a slash.
    draw_suggestions(frame, session, areas[1]);

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

/// Commands the half-typed draft could still become, floating just above it.
///
/// An overlay rather than a fourth row in the layout. `composer_height` is
/// load-bearing in the mouse hit-test and both scroll spans, so a list that
/// took height from the frame would shift the prompt out from under the cursor
/// the moment a `/` was typed, and shift it back on the next keystroke.
///
/// It grows upward from the composer's top edge for the same reason: the row
/// nearest the text being typed stays put as the list gets shorter, so the eye
/// tracks one edge instead of following the whole box.
fn draw_suggestions(frame: &mut Frame, session: &TuiSession, composer: Rect) {
    let found = crate::completion::suggestions(session.composer.text());
    if found.is_empty() {
        return;
    }

    // Columns measured from the rows actually on screen, so a filter down to
    // one short name does not leave a box sized for the ones it ruled out.
    let name_width = found
        .iter()
        .map(|command| command.typed_form().chars().count())
        .max()
        .unwrap_or(0);
    let summary_width = found
        .iter()
        .map(|command| command.summary.chars().count())
        .max()
        .unwrap_or(0);

    // Two columns of border, two for the selection marker, one between columns.
    let width = (name_width + summary_width + 5) as u16;
    // Borders included, capped by the room left above the composer. A cap that
    // bites just shows fewer rows; the `List` keeps the selected one in view.
    let height = (found.len() as u16 + 2).min(composer.y);
    if height < 3 || width < 3 {
        return;
    }
    let rect = Rect {
        x: composer.x,
        y: composer.y - height,
        width: width.min(composer.width),
        height,
    };

    let rows: Vec<ListItem> = found
        .iter()
        .map(|command| {
            ListItem::new(Line::from(vec![
                Span::raw(format!("{:<name_width$} ", command.typed_form())),
                Span::styled(
                    command.summary,
                    Style::default().add_modifier(Modifier::DIM),
                ),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(session.completion.selected(found.len())));

    frame.render_widget(Clear, rect);
    frame.render_stateful_widget(
        List::new(rows)
            // The title carries the key, because a list that appears on its own
            // has to say how to take a row from it.
            .block(bordered("Tab to complete"))
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
    // Two columns are reserved for the transcript gutter below.
    if let Some(activity) = session.activity_line(width.saturating_sub(2)) {
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
    fn a_narrow_activity_row_elides_detail_before_the_interrupt_hint() {
        let (_dir, mut session) = session_with(&[(Role::User, "do something")]);
        let _worker = session.busy_for_test("model step 1");

        let forty_columns = render(&session, 40, 12).join("\n");
        assert!(
            forty_columns.contains("Ctrl-C to interrupt"),
            "the actionable hint must survive intact at 40 columns:\n{forty_columns}"
        );
        assert!(
            forty_columns.contains("model step…"),
            "status should be deliberately elided rather than hard-cut:\n{forty_columns}"
        );

        let very_narrow = render(&session, 20, 12).join("\n");
        assert!(
            very_narrow.contains("Ctrl-C"),
            "even a tiny terminal must retain the compact interrupt cue:\n{very_narrow}"
        );
        assert!(
            !very_narrow.contains("to inter"),
            "the compact cue must not resemble a clipped sentence:\n{very_narrow}"
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

    #[test]
    fn a_half_typed_command_offers_the_names_it_could_still_become() {
        let (_dir, mut session) = session_with(&[(Role::Assistant, "hello")]);
        session.composer.set("/pro");
        let screen = render(&session, 60, 12);
        let joined = screen.join("\n");
        assert!(
            joined.contains("/providers") && joined.contains("/provider <id>"),
            "both names the draft could still become must be offered:\n{joined}"
        );
        assert!(
            joined.contains("Tab to complete"),
            "a list that appears on its own has to say how to take a row:\n{joined}"
        );
        // The list floats. If it took height from the layout instead, the draft
        // would drop a row the moment a `/` was typed and rise again on Enter.
        assert!(
            screen.iter().any(|row| row.contains("› /pro")),
            "the draft must stay put underneath the list:\n{joined}"
        );
    }

    #[test]
    fn ordinary_typing_is_never_covered_by_a_list() {
        let (_dir, mut session) = session_with(&[(Role::Assistant, "hello")]);
        session.composer.set("what is a slash for");
        let screen = render(&session, 60, 12).join("\n");
        assert!(
            !screen.contains("Tab to complete"),
            "a list over an ordinary prompt is just in the way:\n{screen}"
        );
    }

    /// What is highlighted on screen and what `Tab` would take are two separate
    /// reads of the same selection, and a user trusts the first to predict the
    /// second.
    #[test]
    fn the_highlighted_row_is_the_one_tab_would_take() {
        let (_dir, mut session) = session_with(&[(Role::Assistant, "hello")]);
        session.composer.set("/pro");
        let count = crate::completion::suggestions(session.composer.text()).len();
        session.completion.down(count);

        let screen = render(&session, 60, 12);
        let marked: Vec<&String> = screen.iter().filter(|row| row.contains("> /")).collect();
        assert_eq!(marked.len(), 1, "exactly one row is marked: {screen:?}");
        assert!(
            marked[0].contains("/provider <id>"),
            "the marker sits on the second row: {:?}",
            marked[0]
        );
        assert_eq!(
            session
                .completion
                .completed(session.composer.text())
                .as_deref(),
            Some("/provider "),
            "and Tab takes the row the marker is on"
        );
    }

    /// Nothing stops an overlay painting over the prompt except the arithmetic
    /// that places it, and the list is at its tallest exactly when the terminal
    /// has least room — so every small height gets checked, not one convenient
    /// one.
    #[test]
    fn the_list_stops_above_the_composer_at_every_height() {
        let (_dir, mut session) = session_with(&[(Role::Assistant, "hello")]);
        // The whole catalog, taller than most of these terminals.
        session.composer.set("/");
        for height in 6..16 {
            let screen = render(&session, 60, height);
            let composer = screen
                .iter()
                .position(|row| row.contains("Message"))
                .unwrap_or_else(|| panic!("no composer at {height} rows: {screen:?}"));
            // Below some height there is no room for a bordered list at all,
            // and standing down is the right answer rather than a squeeze.
            let Some(top) = screen
                .iter()
                .position(|row| row.contains("Tab to complete"))
            else {
                continue;
            };
            let bottom = top
                + screen[top..composer]
                    .iter()
                    .rposition(|row| row.starts_with('└'))
                    .unwrap_or_else(|| panic!("the list is open-bottomed at {height}: {screen:?}"));
            assert_eq!(
                bottom,
                composer - 1,
                "the list must close on the row above the composer at {height}: {screen:?}"
            );
        }
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
        let track = crate::mouse::regions(Rect::new(0, 0, 40, 14), 3).track;
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
