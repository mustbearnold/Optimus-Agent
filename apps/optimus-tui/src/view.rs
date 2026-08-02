//! Layout for the terminal face: transcript, composer, status.
//!
//! Row content comes from [`crate::transcript`]; this module only decides where
//! rows go and what colour they are.

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

mod composer;
#[path = "view/sidebar.rs"]
mod rail;

use crate::mouse;
use crate::overlay;
use crate::session::{Role, TuiSession};
use crate::transcript::{self, Row};
use crate::width;

const BACKGROUND: Color = Color::Rgb(12, 12, 12);
const HAIRLINE: Color = Color::Rgb(42, 42, 42);
const TEXT: Color = Color::Rgb(228, 228, 228);
const MUTED: Color = Color::Rgb(126, 126, 126);
const ACCENT: Color = Color::Rgb(132, 164, 255);
const SUCCESS: Color = Color::Rgb(137, 201, 112);
const WARNING: Color = Color::Rgb(226, 190, 112);
const DANGER: Color = Color::Rgb(235, 116, 116);
const PROMPT_BACKGROUND: Color = Color::Rgb(27, 27, 29);
const HOVER_BACKGROUND: Color = Color::Rgb(34, 35, 40);
const COMPOSER_BACKGROUND: Color = Color::Rgb(25, 25, 27);
const SIDEBAR_BACKGROUND: Color = Color::Rgb(17, 17, 17);
const SIDEBAR_ACTION: Color = Color::Rgb(24, 27, 36);
const SIDEBAR_SELECTED: Color = Color::Rgb(23, 24, 29);

/// Rows the composer block occupies for this draft, borders included. One
/// arithmetic, shared by the layout below and by every caller that has to
/// subtract the composer from the frame.
pub fn composer_height(session: &TuiSession, width: u16) -> u16 {
    let content_width = mouse::content_width(width, session.sidebar.open, session.sidebar.width);
    composer::height(session.composer.text(), content_width)
}

/// Text columns available to transcript rows for this session's responsive
/// rail state.
pub fn transcript_width_for(session: &TuiSession, width: u16) -> u16 {
    mouse::content_width(width, session.sidebar.open, session.sidebar.width).saturating_sub(1)
}

pub fn draw(frame: &mut Frame, session: &TuiSession) {
    let areas = mouse::layout_with_sidebar(
        frame.area(),
        composer_height(session, frame.area().width),
        session.sidebar.open,
        session.sidebar.width,
    );
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND)),
        frame.area(),
    );

    if areas.sidebar.width > 0 {
        rail::draw(frame, areas.sidebar, session);
        rail::draw_divider(frame, areas.sidebar_divider);
    } else {
        rail::draw_collapsed_tab(frame, frame.area());
    }

    draw_context(frame, areas.context, session);

    // Rows are pre-wrapped to the inner width, so scrolling is exact and the
    // tail stays visible as text streams in.
    let inner_width = areas.transcript.width.saturating_sub(1);
    let rows = visible_rows(session, inner_width);
    let height = areas.transcript.height as usize;
    let offset = scroll_offset(rows.len(), height, session.scroll_back) as u16;

    let lines: Vec<Line> = rows
        .iter()
        .map(|row| paint_with_hover(row, session.hovered_block))
        .collect();
    frame.render_widget(Paragraph::new(lines).scroll((offset, 0)), areas.transcript);
    draw_scrollbar(frame, areas.transcript, rows.len(), height, offset as usize);

    let composer_title = session
        .workbench
        .inspecting()
        .then_some("Inspect · ↑↓ move · Space fold · Tab back");
    let composer_badge = session
        .model
        .as_ref()
        .map(|model| format!("{}/{model}", session.provider))
        .unwrap_or_else(|| session.provider.clone());
    composer::render(
        frame,
        areas.composer,
        &session.composer,
        composer_title,
        &composer_badge,
    );

    draw_status(frame, areas.status, session);
    draw_help(frame, areas.help, session);

    // Suggestions first, so a picker the user opened on purpose covers a list
    // that merely appeared because they typed a slash.
    draw_suggestions(frame, session, areas.composer);

    if let Some(picker) = session.picker.as_ref() {
        draw_picker(frame, picker);
    }
}

/// Compact project/provider context, deliberately quieter than the work area.
fn draw_context(frame: &mut Frame, area: Rect, session: &TuiSession) {
    let total = usize::from(area.width);
    if total == 0 {
        return;
    }
    let prefix = width::take("  ", total);
    let available_right = total.saturating_sub(width::cells(&prefix));
    let suffix = if available_right >= 2 {
        "  "
    } else if available_right == 1 {
        " "
    } else {
        ""
    };
    let provider_limit = available_right.saturating_sub(width::cells(suffix));
    let provider = width::truncate(&session.provider, provider_limit);
    let right = format!("{provider}{suffix}");
    let right_width = width::cells(&right);
    let gap = if total.saturating_sub(width::cells(&prefix) + right_width) >= 2 {
        2
    } else if total > width::cells(&prefix) + right_width {
        1
    } else {
        0
    };
    let path_width = total.saturating_sub(width::cells(&prefix) + gap + right_width);
    let path = compact_path(&session.home, path_width as u16);
    let left = format!("{prefix}{path}");
    let gap = total.saturating_sub(width::cells(&left) + right_width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, Style::default().fg(MUTED)),
            Span::raw(" ".repeat(gap)),
            Span::styled(right, Style::default().fg(TEXT)),
        ])),
        area,
    );
}

fn compact_path(path: &std::path::Path, width: u16) -> String {
    let value = path.to_string_lossy();
    let display = if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::Path::new(&home).to_string_lossy();
        if let Some(rest) = value.strip_prefix(home.as_ref()) {
            format!("~{rest}")
        } else {
            value.into_owned()
        }
    } else {
        value.into_owned()
    };
    let width = usize::from(width);
    if width::cells(&display) <= width {
        return display;
    }
    if width <= 7 {
        return width::take(&display, width);
    }
    let suffix_len = width - 7;
    let prefix = width::take(&display, 6);
    let suffix = width::take_end(&display, suffix_len);
    format!("{prefix}…{suffix}")
}

/// Colour a row by whose turn it is. Distinct hues per role are what make the
/// transcript scannable rather than one undifferentiated block of text.
///
/// The selected item is reversed rather than tinted: reverse video is the one
/// emphasis every terminal has, including the monochrome and `NO_COLOR` ones,
/// so which block the keyboard is pointed at never depends on a palette.
fn paint_with_hover(row: &Row, hovered: Option<crate::workbench::BlockId>) -> Line<'static> {
    let is_hovered = !row.selected && row.block.is_some() && row.block == hovered;
    let mut base = match row.role {
        Role::User => Style::default().fg(TEXT),
        Role::Assistant => Style::default().fg(TEXT),
        Role::Tool => Style::default().fg(MUTED),
        Role::Action => Style::default().fg(WARNING),
        Role::Error => Style::default().fg(DANGER),
    };
    if row.surface_width.is_some() {
        base = base.bg(PROMPT_BACKGROUND);
    }
    if row.selected {
        base = base
            .add_modifier(Modifier::REVERSED)
            .add_modifier(Modifier::BOLD);
    }
    let mut spans = row
        .segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let mut style = base;
            if index == 0 && !row.selected {
                let marker = segment.text.trim_start().chars().next();
                let semantic_marker =
                    matches!(marker, Some('›' | '✦' | '│' | '◇' | '×' | '▸' | '▾'));
                style = match (row.role, marker) {
                    (Role::User, Some('›')) | (Role::Assistant, Some('✦')) => {
                        style.fg(ACCENT).add_modifier(Modifier::BOLD)
                    }
                    (Role::Tool, Some('│')) => style.fg(SUCCESS),
                    (Role::Tool, Some('▸' | '▾')) => style.fg(ACCENT),
                    _ => style,
                };
                if is_hovered && semantic_marker {
                    style = style.bg(HOVER_BACKGROUND).add_modifier(Modifier::BOLD);
                }
            }
            if row.role == Role::Tool {
                style = match segment.text.trim() {
                    "[done]" => style.fg(SUCCESS),
                    "[failed]" | "[stopped]" => style.fg(DANGER),
                    "[queued]" | "[running]" | "[waiting]" | "[approval]" | "[stalled]" => {
                        style.fg(WARNING)
                    }
                    _ => style,
                };
            }
            if segment.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            if segment.dim {
                style = style.add_modifier(Modifier::DIM);
            }
            Span::styled(segment.text.clone(), style)
        })
        .collect::<Vec<_>>();
    if let Some(target) = row.surface_width {
        let used = spans
            .iter()
            .map(|span| width::cells(span.content.as_ref()))
            .sum::<usize>();
        if target > used {
            spans.push(Span::styled(" ".repeat(target - used), base));
        }
    }
    Line::from(spans)
}

/// The footer is the workbench's compact state rail. The text remains the
/// durable session summary, while the leading marker gives the eye a semantic
/// colour and shape to find before reading the details.
fn status_parts(session: &TuiSession) -> (&'static str, Style, String) {
    let (marker, marker_style) = if session.busy() {
        ("◌", Style::default().fg(Color::LightYellow))
    } else if session.pending_approval.is_some() {
        ("!", Style::default().fg(Color::LightRed))
    } else {
        ("●", Style::default().fg(Color::LightGreen))
    };
    (marker, marker_style, status_label(session))
}

fn status_label(session: &TuiSession) -> String {
    if session.busy() {
        format!(
            "turn · {}",
            if session.status.is_empty() {
                "working"
            } else {
                &session.status
            }
        )
    } else if session.pending_approval.is_some() {
        "approval required · /approval to decide".into()
    } else {
        "turn · ready".into()
    }
}

fn draw_status(frame: &mut Frame, area: Rect, session: &TuiSession) {
    let total = usize::from(area.width);
    if total == 0 {
        return;
    }
    let (marker, marker_style, label) = status_parts(session);
    let prefix = format!("  {marker} ");
    let policy = compact_status(session);
    let right_full = if policy.is_empty() {
        String::new()
    } else {
        format!("{policy}  ")
    };
    let right_budget = width::cells(&right_full).min(total.saturating_sub(4));
    let right = width::truncate(&right_full, right_budget);
    let gap = if total >= width::cells(&prefix) + width::cells(&right) + 2 {
        2
    } else {
        0
    };
    let label_budget = total.saturating_sub(width::cells(&prefix) + width::cells(&right) + gap);
    let left_label = width::truncate(&label, label_budget);
    let mut spans = vec![Span::raw(width::take(&prefix, total))];
    spans.push(Span::styled(left_label, Style::default().fg(MUTED)));
    let used = spans
        .iter()
        .map(|span| width::cells(span.content.as_ref()))
        .sum::<usize>();
    let fill = total.saturating_sub(used + width::cells(&right));
    if fill > 0 {
        spans.push(Span::raw(" ".repeat(fill)));
    }
    spans.push(Span::styled(right, Style::default().fg(MUTED)));
    // Rebuild the marker span separately so truncating the label never loses
    // the state colour and shape at the left edge.
    let marker_width = width::cells(&prefix);
    let mut line = Line::from(spans);
    if marker_width <= total {
        line.spans[0] = Span::styled(
            width::take(&prefix, total),
            marker_style.add_modifier(Modifier::BOLD),
        );
    }
    frame.render_widget(Paragraph::new(line), area);
}

/// Only policy belongs opposite the turn state. Provider/model already lives
/// in the context and composer, and repeating `ready` on both sides made the
/// footer read like duplicated telemetry rather than a calm status rail.
fn compact_status(session: &TuiSession) -> String {
    let mut parts = Vec::new();
    if session.yolo {
        parts.push("YOLO".to_owned());
    }
    if let Some(thinking) = &session.thinking {
        parts.push(format!("think:{thinking}"));
    }
    if !session.yolo {
        if let Some(access) = session.access {
            parts.push(format!("access:{access}"));
        }
    }
    parts.join(" · ")
}

fn draw_help(frame: &mut Frame, area: Rect, session: &TuiSession) {
    let separator = if area.width < 44 { " │ " } else { "  │  " };
    let candidates: &[&[(&str, &str)]] = if session.busy() {
        &[
            &[
                ("Ctrl-C", "stop"),
                ("Ctrl-B", "sidebar"),
                ("Tab", "inspect"),
                ("Esc", "clear"),
            ],
            &[("Ctrl-C", "stop"), ("Tab", "inspect"), ("Esc", "clear")],
            &[("^C", "stop"), ("Tab", "inspect"), ("Esc", "clear")],
            &[("Esc", "clear")],
        ]
    } else {
        &[
            &[
                ("Enter", "send"),
                ("Ctrl-B", "sidebar"),
                ("Tab", "inspect"),
                ("Esc", "clear"),
            ],
            &[("Enter", "send"), ("Tab", "inspect"), ("Esc", "clear")],
            &[("↵", "send"), ("Tab", "inspect"), ("Esc", "clear")],
            &[("Esc", "clear")],
        ]
    };
    let required = |items: &[(&str, &str)]| {
        width::cells("  ")
            + items
                .iter()
                .map(|(key, action)| width::cells(key) + 1 + width::cells(action))
                .sum::<usize>()
            + width::cells(separator) * items.len().saturating_sub(1)
    };
    let compact = candidates
        .iter()
        .copied()
        .find(|items| required(items) <= usize::from(area.width));
    let prefix = width::take("  ", usize::from(area.width));
    let Some(compact) = compact else {
        let available = usize::from(area.width).saturating_sub(width::cells(&prefix));
        let text = width::truncate("Esc:clear", available);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(text, Style::default().fg(MUTED)),
            ])),
            area,
        );
        return;
    };
    let mut spans = vec![Span::raw(prefix)];
    for (index, (key, action)) in compact.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::styled(separator, Style::default().fg(HAIRLINE)));
        }
        spans.push(Span::styled(
            key,
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(":{action}"),
            Style::default().fg(MUTED),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
        // The track runs through the flat viewport; the same rows
        // `mouse::regions` hit-tests.
        block,
        &mut state,
    );
}

/// Centred overlay. Drawn last so it sits above the transcript, and `Clear`ed
/// first so the text underneath cannot bleed through.
fn draw_picker(frame: &mut Frame, picker: &crate::picker::Picker) {
    // Same rectangle the hit-test uses, so a click lands on the row shown.
    let rect = mouse::picker_rect(frame.area(), picker);
    let inner_width = usize::from(rect.width.saturating_sub(2));

    let rows: Vec<ListItem> = picker
        .items
        .iter()
        .map(|item| {
            let mark = if item.current { "● " } else { "  " };
            // Labels identify the action; details are supporting context. At
            // narrow widths the old order preserved details and erased the
            // command name, turning a picker into a list of mysterious dots.
            let label_budget = inner_width
                .saturating_sub(width::cells(mark))
                .saturating_sub(1);
            let label = width::truncate(&item.label, label_budget);
            let detail_budget =
                inner_width.saturating_sub(width::cells(mark) + width::cells(&label));
            let detail = if detail_budget >= 2 {
                width::truncate(&format!("  {}", item.detail), detail_budget)
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{mark}{label}")),
                Span::styled(detail, Style::default().add_modifier(Modifier::DIM)),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(picker.selected()));

    overlay::prepare(frame, rect, overlay::Kind::Modal);
    let title = width::truncate(&picker.title, inner_width);
    frame.render_stateful_widget(
        List::new(rows)
            .block(overlay::panel(&title, overlay::Kind::Modal))
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
    let natural_name_width = found
        .iter()
        .map(|command| width::cells(&command.typed_form()))
        .max()
        .unwrap_or(0);
    let natural_summary_width = found
        .iter()
        .map(|command| width::cells(command.summary))
        .max()
        .unwrap_or(0);

    // Two columns of border, two for the selection marker, one between columns.
    let natural_width = natural_name_width
        .saturating_add(natural_summary_width)
        .saturating_add(5);
    let width = natural_width.min(usize::from(composer.width)) as u16;
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
    let inner_width = usize::from(rect.width.saturating_sub(2));
    let name_width = natural_name_width.min(inner_width);

    let rows: Vec<ListItem> = found
        .iter()
        .map(|command| {
            let name = width::truncate(&command.typed_form(), name_width);
            let name_used = width::cells(&name);
            let summary_start = name_used.saturating_add(1);
            let summary =
                width::truncate(command.summary, inner_width.saturating_sub(summary_start));
            let padding = " ".repeat(name_width.saturating_sub(name_used) + 1);
            ListItem::new(Line::from(vec![
                Span::raw(format!("{name}{padding}")),
                Span::styled(summary, Style::default().add_modifier(Modifier::DIM)),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(session.completion.selected(found.len())));

    overlay::prepare(frame, rect, overlay::Kind::Suggestions);
    frame.render_stateful_widget(
        List::new(rows)
            // The title carries the key, because a list that appears on its own
            // has to say how to take a row from it.
            .block(overlay::panel(
                "Tab to complete",
                overlay::Kind::Suggestions,
            ))
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
    let items = session.workbench.items();
    let mut rows = transcript::rows(
        &session.messages,
        &items,
        session.workbench.selected(),
        width,
        session.chrome,
    );
    // Two columns are reserved for the transcript gutter below.
    if let Some(activity) = session.activity_line(width.saturating_sub(2)) {
        rows.push(Row::blank());
        rows.push(Row::chrome(
            Role::Tool,
            vec![crate::transcript::Segment::plain(format!("  {activity}"))],
        ));
    }
    rows
}

/// Scroll-back that keeps the selected item on screen, moving as little as it
/// can.
///
/// Called while the transcript has the keyboard. Inside the range where the
/// selection is already fully visible the current position is kept exactly, so
/// arriving output does not jitter the view; outside it, the view moves only
/// as far as it must. An item taller than the viewport parks its first row at
/// the top, because that is the row the reader is looking for.
pub fn anchored(rows: &[Row], height: usize, scroll_back: usize) -> usize {
    let first = rows.iter().position(|row| row.selected);
    let (Some(first), Some(last)) = (first, rows.iter().rposition(|row| row.selected)) else {
        return scroll_back;
    };
    let max_back = max_scroll_back(rows.len(), height);
    // offset = rows - height - scroll_back, and the selection has to sit
    // inside [offset, offset + height).
    let low = rows
        .len()
        .saturating_sub(height)
        .saturating_sub(first)
        .min(max_back);
    let high = rows.len().saturating_sub(last + 1).min(max_back).max(low);
    scroll_back.clamp(low, high)
}

/// Transcript as plain lines. Separated from rendering so it can be asserted in
/// tests without standing up a terminal.
pub fn transcript_text(session: &TuiSession, width: u16) -> Vec<String> {
    visible_rows(session, width)
        .iter()
        .map(Row::plain)
        .collect()
}

/// What a click landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub block: crate::workbench::BlockId,
    /// Whether this is the item's first row — its header. Derived from where
    /// the row sits in its item's run, never from what the row says, so a
    /// header stays a header whatever it is captioned.
    pub head: bool,
}

/// The block painted at row `at` of the laid-out transcript, if any.
pub fn hit(rows: &[Row], at: usize) -> Option<Hit> {
    let block = rows.get(at)?.block?;
    let head = at == 0 || rows[at - 1].block != Some(block);
    Some(Hit { block, head })
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
            "  ✦ What should Optimus do?"
        );
    }

    #[test]
    fn narrow_chrome_keeps_complete_context_and_keyboard_labels() {
        let (_dir, session) = session_with(&[]);
        let screen = render(&session, 40, 20);
        let help = screen.last().expect("help rail");
        assert!(
            screen[0].contains("  auto"),
            "provider needs a breathing gap: {screen:?}"
        );
        assert!(
            help.contains("↵:send") && help.contains("Tab:inspect") && help.contains("Esc:clear"),
            "compact labels must remain complete: {screen:?}"
        );
    }

    #[test]
    fn a_wide_frame_paints_the_workspace_rail_without_widening_any_cell_row() {
        let (_dir, session) = session_with(&[]);
        let screen = render(&session, 80, 24);
        let joined = screen.join("\n");
        for label in ["WORKSPACE", "New session", "SESSIONS", "PROJECTS", "PINNED"] {
            assert!(
                joined.contains(label),
                "sidebar label {label} is missing:\n{joined}"
            );
        }
        assert!(
            screen.iter().all(|line| crate::width::cells(line) <= 80),
            "sidebar must keep every painted row inside the terminal: {screen:?}"
        );
        assert!(
            screen.iter().any(|line| line.contains('┊')),
            "the divider needs a visible drag handle: {screen:?}"
        );
    }

    #[test]
    fn a_narrow_frame_collapses_the_rail_but_keeps_the_reopen_tab() {
        let (_dir, session) = session_with(&[]);
        let screen = render(&session, 60, 20);
        let joined = screen.join("\n");
        assert!(
            !joined.contains("WORKSPACE"),
            "rail should collapse at 60 columns: {joined}"
        );
        assert!(
            screen[0].starts_with('›'),
            "collapsed rail needs a reopen tab: {screen:?}"
        );
        assert!(screen.iter().all(|line| crate::width::cells(line) <= 60));
    }

    #[test]
    fn busy_narrow_chrome_switches_to_a_complete_short_interrupt_label() {
        let (_dir, mut session) = session_with(&[]);
        let _worker = session.busy_for_test("working");
        let screen = render(&session, 40, 12);
        let help = screen.last().expect("help rail");
        assert!(help.contains("^C:stop"), "{screen:?}");
        assert!(
            !help.contains("Ctrl-C:stop"),
            "a narrow rail must not clip labels: {screen:?}"
        );
    }

    #[test]
    fn a_long_provider_is_truncated_at_a_cell_boundary_in_context_and_composer() {
        let (_dir, mut session) = session_with(&[]);
        session.provider = "provider界👍e\u{301}with-a-very-long-name".into();
        let screen = render(&session, 32, 12);
        assert!(
            screen[0].contains('…'),
            "context needs an explicit elision: {screen:?}"
        );
        assert!(
            screen
                .iter()
                .any(|row| row.contains('╰') && row.contains('…')),
            "composer badge needs an explicit elision: {screen:?}"
        );
        assert!(screen.iter().all(|line| crate::width::cells(line) <= 32));
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
    fn semantic_markers_keep_the_workbench_scannable() {
        for (role, marker, colour) in [
            (Role::User, "› ", ACCENT),
            (Role::Assistant, "✦ ", ACCENT),
            (Role::Tool, "│ ", SUCCESS),
            (Role::Action, "◇ ", WARNING),
            (Role::Error, "× ", DANGER),
        ] {
            let row = Row::chrome(role, vec![crate::transcript::Segment::plain(marker)]);
            assert_eq!(paint_with_hover(&row, None).spans[0].style.fg, Some(colour));
        }
    }

    #[test]
    fn the_task_surface_fills_its_visual_row_without_tinting_the_prompt_text() {
        let mut row = Row::chrome(
            Role::User,
            vec![
                crate::transcript::Segment::plain("  › "),
                crate::transcript::Segment::plain("trace the regression"),
            ],
        );
        row.surface_width = Some(40);
        let line = paint_with_hover(&row, None);
        assert_eq!(
            line.spans
                .iter()
                .map(|span| crate::width::cells(span.content.as_ref()))
                .sum::<usize>(),
            40
        );
        assert!(
            line.spans
                .iter()
                .all(|span| span.style.bg == Some(PROMPT_BACKGROUND)),
            "every cell of the task card needs the same surface"
        );
        assert_eq!(line.spans[0].style.fg, Some(ACCENT));
        assert_eq!(line.spans[1].style.fg, Some(TEXT));
    }

    #[test]
    fn bold_segments_survive_painting() {
        let row = Row::chrome(
            Role::Assistant,
            vec![
                crate::transcript::Segment::plain("plain"),
                crate::transcript::Segment {
                    text: "loud".into(),
                    bold: true,
                    dim: false,
                },
            ],
        );
        let line = paint_with_hover(&row, None);
        assert!(!line.spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn hovering_a_block_adds_a_non_destructive_pointer_affordance() {
        let id = crate::workbench::BlockId(uuid::Uuid::nil());
        let mut row = Row::chrome(
            Role::Assistant,
            vec![
                crate::transcript::Segment::plain("  ✦ "),
                crate::transcript::Segment::plain("answer"),
            ],
        );
        row.block = Some(id);
        let line = paint_with_hover(&row, Some(id));
        assert_eq!(line.spans[0].style.bg, Some(HOVER_BACKGROUND));
        assert_eq!(line.spans[1].style.bg, None);
        assert!(line
            .spans
            .iter()
            .all(|span| !span.style.add_modifier.contains(Modifier::UNDERLINED)));
        assert!(!row.selected, "hover must not mutate selection");
    }

    #[test]
    fn hovering_wrapped_prose_never_draws_rules_through_text() {
        let id = crate::workbench::BlockId(uuid::Uuid::nil());
        for text in ["wrapped prose", "• nested list item"] {
            let mut row = Row::chrome(
                Role::Assistant,
                vec![
                    crate::transcript::Segment::plain("    "),
                    crate::transcript::Segment::plain(text),
                ],
            );
            row.block = Some(id);
            let line = paint_with_hover(&row, Some(id));
            assert!(line
                .spans
                .iter()
                .all(|span| !span.style.add_modifier.contains(Modifier::UNDERLINED)));
            assert!(line
                .spans
                .iter()
                .all(|span| span.style.bg != Some(HOVER_BACKGROUND)));
        }
    }

    #[test]
    fn a_modal_picker_dims_the_frame_but_not_its_own_panel() {
        let (_dir, mut session) = session_with(&[]);
        session.picker = Some(crate::commands::menu());
        let backend = ratatui::backend::TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|f| draw(f, &session)).expect("draw");
        let area = Rect::new(0, 0, 60, 12);
        let rect = crate::mouse::picker_rect(area, session.picker.as_ref().unwrap());
        let buffer = terminal.backend().buffer();
        assert!(
            buffer[(0, 0)].style().add_modifier.contains(Modifier::DIM),
            "the frame behind a modal should recede"
        );
        assert!(
            !buffer[(rect.x + 1, rect.y + 1)]
                .style()
                .add_modifier
                .contains(Modifier::DIM),
            "the active panel should remain at full emphasis"
        );
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
        assert!(screen.contains("whats the ai news today"), "{screen}");
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
            forty_columns.contains("model st…"),
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
        session.push_call_for_test(
            "web_search",
            "call-1",
            "web_search  Found 3 sources  (1.2s)",
        );
        let screen = render(&session, 60, 12).join("\n");
        assert!(
            screen.contains("│ web_search  Found 3 sources  (1.2s)"),
            "tool work belongs on screen:\n{screen}"
        );
    }

    #[test]
    fn the_running_tool_is_named_in_the_activity_line() {
        let (_dir, mut session) = session_with(&[(Role::User, "search")]);
        let _worker = session.busy_for_test("working");
        session.running_tool = Some("web_search".into());
        let screen = render(&session, 60, 8).join("\n");
        assert!(screen.contains("web_search"), "{screen}");
    }

    #[test]
    fn the_footer_uses_a_semantic_marker_for_ready_and_busy_states() {
        let (_dir, session) = session_with(&[]);
        let ready = render(&session, 60, 8).join("\n");
        assert!(ready.contains("● turn · ready"), "{ready}");

        let (_dir, mut session) = session_with(&[]);
        let _worker = session.busy_for_test("working");
        let busy = render(&session, 60, 8).join("\n");
        assert!(busy.contains("◌ turn · working"), "{busy}");
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
            let composer = mouse::layout(Rect::new(0, 0, 60, height), composer_height(&session, 60))
                .composer
                .y as usize;
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
                    .rposition(|row| row.trim_start().starts_with('└'))
                    .unwrap_or_else(|| panic!("the list is open-bottomed at {height}: {screen:?}"));
            assert_eq!(
                bottom,
                composer - 1,
                "the list must close on the row above the composer at {height}: {screen:?}"
            );
        }
    }

    /// The rightmost column of each row, which is where the track is drawn.
    fn track_edge(screen: &[String], column: u16) -> Vec<char> {
        screen
            .iter()
            .map(|row| row.chars().nth(usize::from(column)).unwrap_or(' '))
            .collect()
    }

    #[test]
    fn a_transcript_taller_than_the_pane_grows_a_scrollbar() {
        let lines: Vec<(Role, &str)> = (0..30).map(|_| (Role::Assistant, "row")).collect();
        let (_dir, session) = session_with(&lines);
        let track = mouse::regions(Rect::new(0, 0, 40, 14), 3).track;
        let edge = track_edge(&render(&session, 40, 14), track.x);
        assert!(edge.contains(&'█'), "no thumb was painted: {edge:?}");
    }

    #[test]
    fn a_short_transcript_gets_no_scrollbar_at_all() {
        let (_dir, session) = session_with(&[(Role::Assistant, "just one")]);
        let track = mouse::regions(Rect::new(0, 0, 40, 14), 3).track;
        let edge = track_edge(&render(&session, 40, 14), track.x);
        assert!(
            !edge.contains(&'█'),
            "a thumb would imply history that is not there: {edge:?}"
        );
    }

    #[test]
    fn the_flat_scrollbar_has_no_pane_corners_to_compete_with_content() {
        let lines: Vec<(Role, &str)> = (0..30).map(|_| (Role::Assistant, "row")).collect();
        let (_dir, session) = session_with(&lines);
        let screen = render(&session, 40, 14);
        let track = mouse::regions(Rect::new(0, 0, 40, 14), 3).track;
        let edge = track_edge(&screen, track.x);
        assert_ne!(edge[0], '┐', "the context rail is flat: {}", screen[0]);
        assert_ne!(edge[9], '┘', "the transcript is flat: {}", screen[9]);
        assert!(
            edge.contains(&'█'),
            "the track still needs a visible thumb: {edge:?}"
        );
    }

    #[test]
    fn the_thumb_rides_up_the_track_as_the_transcript_scrolls_back() {
        let lines: Vec<(Role, &str)> = (0..40).map(|_| (Role::Assistant, "row")).collect();
        let (_dir, mut session) = session_with(&lines);
        let track = mouse::regions(Rect::new(0, 0, 40, 14), 3).track;
        let at_tail = track_edge(&render(&session, 40, 14), track.x)
            .iter()
            .position(|c| *c == '█')
            .expect("thumb at the tail");
        session.scroll_back = 30;
        let scrolled = track_edge(&render(&session, 40, 14), track.x)
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
            .filter(|(_, row)| row.chars().nth(usize::from(track.x)) == Some('█'))
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

    // ADR-0075 phase 2: folding, selection, and keeping the selection on screen.

    /// A prompt followed by four reads — one run, folded by default.
    fn with_a_run() -> (tempfile::TempDir, TuiSession) {
        let (dir, mut session) = session_with(&[(Role::User, "audit the auth code")]);
        for n in 0..4 {
            session.push_call_for_test(
                "read_file",
                &format!("r{n}"),
                &format!("read_file  src/auth/{n}.rs"),
            );
        }
        (dir, session)
    }

    #[test]
    fn repeated_reads_collapse_to_one_row_the_reader_can_open() {
        let (_dir, mut session) = with_a_run();
        let folded = render(&session, 60, 14).join("\n");
        assert!(
            folded.contains("▸ read_file · 4 calls"),
            "four reads should arrive as one row:\n{folded}"
        );
        assert!(
            !folded.contains("src/auth/2.rs"),
            "and the individual calls should be out of the way:\n{folded}"
        );

        session
            .workbench
            .step(crate::workbench::SelectionStep::Last);
        assert!(session.workbench.toggle_fold(), "the run opens");
        let open = render(&session, 60, 14).join("\n");
        assert!(open.contains("▾ read_file · 4 calls"), "{open}");
        assert!(
            open.contains("src/auth/2.rs"),
            "opening it must show what it was hiding:\n{open}"
        );
    }

    #[test]
    fn the_selected_block_is_visibly_marked_on_screen() {
        let (_dir, mut session) = with_a_run();
        let transcript = |session: &TuiSession| {
            render(session, 60, 14)
                .into_iter()
                .take_while(|row| {
                    let row = row.trim_start();
                    !row.starts_with('┌') && !row.starts_with('╭')
                })
                .collect::<Vec<_>>()
        };
        let quiet = transcript(&session);
        session.workbench.inspect();
        assert_eq!(
            quiet,
            transcript(&session),
            "selection changes emphasis, not the characters"
        );

        let backend = ratatui::backend::TestBackend::new(60, 14);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal.draw(|f| draw(f, &session)).expect("draw");
        let buffer = terminal.backend().buffer();
        let reversed = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter(|(x, y)| {
                buffer[(*x, *y)]
                    .style()
                    .add_modifier
                    .contains(Modifier::REVERSED)
            })
            .count();
        assert!(
            reversed > 0,
            "the block the keyboard is pointed at has to be visible"
        );
    }

    /// The projection is what a click reads back, so both directions of the
    /// mapping have to agree: the row a run paints belongs to that run.
    #[test]
    fn a_click_on_a_run_header_finds_the_run_and_knows_it_is_the_header() {
        let (_dir, session) = with_a_run();
        let rows = visible_rows(&session, 58);
        let at = rows
            .iter()
            .position(|row| row.plain().contains("read_file · 4 calls"))
            .expect("the run header");
        let found = hit(&rows, at).expect("a block under the header");
        assert!(found.head, "the header row heads its item");
        assert_eq!(
            found.block,
            session.workbench.items().last().unwrap().id(),
            "and names the run, not one of its members"
        );
        assert_eq!(hit(&rows, rows.len() + 5), None, "past the end is nothing");
    }

    #[test]
    fn a_click_inside_an_open_run_selects_the_run_without_closing_it() {
        let (_dir, mut session) = with_a_run();
        session
            .workbench
            .step(crate::workbench::SelectionStep::Last);
        session.workbench.toggle_fold();
        let rows = visible_rows(&session, 58);
        let member = rows
            .iter()
            .position(|row| row.plain().contains("src/auth/2.rs"))
            .expect("an opened member");
        let found = hit(&rows, member).expect("a block under the member");
        assert!(
            !found.head,
            "a member row is not the header, so clicking it must not fold"
        );
    }

    #[test]
    fn anchoring_leaves_a_selection_that_is_already_on_screen_exactly_where_it_is() {
        let mut rows: Vec<Row> = (0..20)
            .map(|_| {
                Row::chrome(
                    Role::Assistant,
                    vec![crate::transcript::Segment::plain("x")],
                )
            })
            .collect();
        rows[15].selected = true;
        // Viewport of 10 with 20 rows: scroll_back 0 shows rows 10..20.
        assert_eq!(anchored(&rows, 10, 0), 0, "row 15 is already visible");
        assert_eq!(anchored(&rows, 10, 3), 3, "and still visible three back");
    }

    #[test]
    fn anchoring_brings_a_selection_that_scrolled_away_back_into_view() {
        let mut rows: Vec<Row> = (0..40)
            .map(|_| {
                Row::chrome(
                    Role::Assistant,
                    vec![crate::transcript::Segment::plain("x")],
                )
            })
            .collect();
        rows[5].selected = true;
        // Following the tail shows 30..40; the selection is far above it.
        let back = anchored(&rows, 10, 0);
        assert!(back > 0, "the view must move to the selection");
        let offset = scroll_offset(rows.len(), 10, back);
        assert!(
            (offset..offset + 10).contains(&5),
            "row 5 must land inside the viewport, not near it"
        );
    }

    #[test]
    fn an_item_taller_than_the_viewport_parks_its_first_row_at_the_top() {
        let mut rows: Vec<Row> = (0..40)
            .map(|_| {
                Row::chrome(
                    Role::Assistant,
                    vec![crate::transcript::Segment::plain("x")],
                )
            })
            .collect();
        for row in &mut rows[10..30] {
            row.selected = true;
        }
        let back = anchored(&rows, 6, 0);
        assert_eq!(
            scroll_offset(rows.len(), 6, back),
            10,
            "the row the reader is looking for is the first one"
        );
    }

    #[test]
    fn anchoring_does_nothing_when_nothing_is_selected() {
        let rows: Vec<Row> = (0..40)
            .map(|_| {
                Row::chrome(
                    Role::Assistant,
                    vec![crate::transcript::Segment::plain("x")],
                )
            })
            .collect();
        assert_eq!(anchored(&rows, 10, 7), 7);
        assert_eq!(anchored(&[], 10, 4), 4);
    }

    #[test]
    fn inspect_mode_says_so_where_the_typing_would_have_gone() {
        let (_dir, mut session) = with_a_run();
        assert!(render(&session, 60, 14).join("\n").contains("›"));
        session.workbench.inspect();
        let screen = render(&session, 60, 14).join("\n");
        assert!(
            screen.contains("Inspect"),
            "a prompt that stopped taking letters has to say why:\n{screen}"
        );
    }
}
