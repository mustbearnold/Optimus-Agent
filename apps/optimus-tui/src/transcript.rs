//! Transcript rows: message text turned into the exact screen lines to paint.
//!
//! Deliberately free of ratatui types. Wrapping, gutters, and inline markdown
//! are the part most likely to be wrong, so they stay unit-testable without
//! standing up a terminal; `view` maps the result onto spans.
//!
//! Two readability rules live here. Text wraps on word boundaries rather than
//! mid-word, and it wraps at [`READABLE_WIDTH`] even when the terminal is much
//! wider, because a 200-column paragraph is hard to track back to the next line.

use crate::session::{Message, Role};

/// Longest line the transcript will lay out, however wide the terminal is.
pub const READABLE_WIDTH: usize = 96;

/// Narrowest a container may be, so a one-word turn still looks deliberate.
const MIN_BOX: usize = 22;

/// How conversational turns are framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    /// Each turn sits in its own titled container.
    Boxed,
    /// Gutter markers only: two more rows of content per message, and a
    /// transcript that copies out of the terminal without border characters.
    Plain,
}

/// A run of characters sharing one emphasis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub bold: bool,
    /// Container edges are drawn dim, so the frame never competes with the
    /// text it is framing.
    pub dim: bool,
}

impl Segment {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            dim: false,
        }
    }

    fn edge(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            dim: true,
        }
    }
}

/// One screen row, already wrapped and gutter-prefixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub role: Role,
    pub segments: Vec<Segment>,
}

impl Row {
    fn blank() -> Self {
        Self {
            role: Role::Assistant,
            segments: Vec::new(),
        }
    }

    /// The row as plain text — what scroll maths and tests measure.
    pub fn plain(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Marker printed before a message's first row, and the indent that keeps its
/// wrapped continuation lines aligned underneath.
fn gutter(role: Role) -> (&'static str, &'static str) {
    match role {
        Role::User => ("› ", "  "),
        Role::Assistant => ("  ", "  "),
        Role::Tool => ("⏺ ", "  "),
        Role::Action => ("⚠ ", "  "),
        Role::Error => ("✗ ", "  "),
    }
}

/// Lay the whole transcript out for `width` columns.
pub fn rows(messages: &[Message], width: u16, chrome: Chrome) -> Vec<Row> {
    if messages.is_empty() {
        return greeting();
    }
    let mut rows = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        // One blank line between messages, so turns are visually separable.
        if index > 0 {
            rows.push(Row::blank());
        }
        if chrome == Chrome::Boxed && contained(message.role) {
            rows.extend(boxed_rows(message, width));
        } else {
            rows.extend(message_rows(message, width));
        }
    }
    rows
}

/// Only conversational turns get a container. Tool, action, and error rows are
/// one-liners; a box around each would be more frame than content.
fn contained(role: Role) -> bool {
    matches!(role, Role::User | Role::Assistant)
}

fn greeting() -> Vec<Row> {
    vec![
        Row {
            role: Role::Assistant,
            segments: vec![Segment {
                text: "  What should Optimus do?".into(),
                bold: true,
                dim: false,
            }],
        },
        Row::blank(),
        Row {
            role: Role::Assistant,
            segments: vec![Segment::plain(
                "  Describe a task and press Enter. Ctrl-C stops a run; Esc exits.",
            )],
        },
    ]
}

/// Wrap a message's text into styled character rows at `content` columns.
///
/// Shared by both framings: the container and the bare gutter differ only in
/// what they put either side of these lines.
fn laid_out(text: &str, content: usize) -> Vec<Vec<(char, bool)>> {
    let mut lines: Vec<Vec<(char, bool)>> = Vec::new();
    let mut after_bullet = false;
    for line in text.split('\n') {
        let parsed = parse_inline(line);
        // Bullets in a run get a blank line between them. A dense list is the
        // hardest thing to read back in a terminal, where there is no leading.
        if parsed.bullet && after_bullet && !lines.is_empty() {
            lines.push(Vec::new());
        }
        after_bullet = parsed.bullet;
        lines.extend(wrap_marked(&parsed.marked, content, parsed.hang));
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

fn message_rows(message: &Message, width: u16) -> Vec<Row> {
    let (first, indent) = gutter(message.role);
    let usable = usize::from(width).min(READABLE_WIDTH);
    // Never let a narrow pane drive the content width to zero.
    let content = usable.saturating_sub(first.chars().count()).max(8);

    laid_out(&message.text, content)
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let lead = if index == 0 { first } else { indent };
            let mut segments = vec![Segment::plain(lead)];
            segments.extend(runs(line));
            Row {
                role: message.role,
                segments,
            }
        })
        .collect()
}

/// One turn drawn inside a titled container.
///
/// The box hugs its content rather than spanning the pane, so a two-word reply
/// reads as a small card instead of a full-width bar.
fn boxed_rows(message: &Message, width: u16) -> Vec<Row> {
    let title = if message.role == Role::User {
        " you "
    } else {
        " optimus "
    };
    // Two columns of border and one of padding on each side.
    let outer = usize::from(width).clamp(MIN_BOX, READABLE_WIDTH);
    let room = outer - 4;

    let lines = laid_out(&message.text, room);
    let widest = lines.iter().map(Vec::len).max().unwrap_or(0);
    let inner = widest.max(title.chars().count() + 1).min(room);

    let mut rows = vec![edge_row(message.role, title, inner, true)];
    rows.extend(lines.iter().map(|line| {
        let mut segments = vec![Segment::edge("│ ")];
        segments.extend(runs(line));
        let pad = " ".repeat(inner.saturating_sub(line.len()));
        segments.push(Segment::edge(format!("{pad} │")));
        Row {
            role: message.role,
            segments,
        }
    }));
    rows.push(edge_row(message.role, "", inner, false));
    rows
}

/// Top or bottom of a container. Both come to `inner + 4` columns so the box
/// closes squarely however long the title is.
fn edge_row(role: Role, title: &str, inner: usize, top: bool) -> Row {
    let (left, right) = if top { ('╭', '╮') } else { ('╰', '╯') };
    let fill = "─".repeat(inner + 1 - title.chars().count());
    Row {
        role,
        segments: vec![Segment::edge(format!("{left}─{title}{fill}{right}"))],
    }
}

/// One logical line resolved into styled characters, plus how it lays out.
struct Parsed {
    marked: Vec<(char, bool)>,
    bullet: bool,
    /// Columns to indent continuation lines by, so a wrapped bullet's later
    /// lines sit under its text rather than under the glyph.
    hang: usize,
}

/// Turn one logical line into characters tagged with emphasis.
///
/// Only the markdown that actually shows up in answers is handled: `**bold**`,
/// setext-free `#` headings, and `-`/`*` bullets. Anything else is left alone
/// rather than half-rendered — a wrong transform reads worse than a literal one.
fn parse_inline(line: &str) -> Parsed {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();
    let mut prefix = " ".repeat(leading);
    let mut rest = trimmed;
    let mut heading = false;
    let mut bullet = false;

    if let Some(after) = trimmed.strip_prefix("### ") {
        rest = after;
        heading = true;
    } else if let Some(after) = trimmed.strip_prefix("## ") {
        rest = after;
        heading = true;
    } else if let Some(after) = trimmed.strip_prefix("# ") {
        rest = after;
        heading = true;
    } else if let Some(after) = trimmed.strip_prefix("- ").or(trimmed.strip_prefix("* ")) {
        prefix.push_str("• ");
        rest = after;
        bullet = true;
    }
    let hang = if bullet { prefix.chars().count() } else { 0 };

    let mut marked: Vec<(char, bool)> = prefix.chars().map(|c| (c, heading)).collect();
    let chars: Vec<char> = rest.chars().collect();
    let mut bold = false;
    let mut index = 0;
    while index < chars.len() {
        // `**` toggles emphasis and is not itself printed.
        if chars[index] == '*' && chars.get(index + 1) == Some(&'*') {
            bold = !bold;
            index += 2;
            continue;
        }
        marked.push((chars[index], bold || heading));
        index += 1;
    }
    Parsed {
        marked,
        bullet,
        hang,
    }
}

/// Greedy word wrap that keeps each character's emphasis attached to it.
///
/// `hang` indents every line after the first, which is what makes a wrapped
/// bullet read as one item rather than as two.
fn wrap_marked(marked: &[(char, bool)], width: usize, hang: usize) -> Vec<Vec<(char, bool)>> {
    if marked.is_empty() {
        return vec![Vec::new()];
    }
    // A hang wider than the line would leave no room for text at all.
    let hang = if hang < width { hang } else { 0 };
    let mut rows = Vec::new();
    let mut start = 0;
    while start < marked.len() {
        let indent = if rows.is_empty() { 0 } else { hang };
        let room = width - indent;
        if marked.len() - start <= room {
            rows.push(indented(&marked[start..], indent));
            break;
        }
        // Look one past the edge: a space exactly there still breaks cleanly.
        let window = &marked[start..(start + room + 1).min(marked.len())];
        let take = match window.iter().rposition(|(c, _)| *c == ' ') {
            // A token longer than the line has no break point; cut it.
            Some(0) | None => room,
            Some(at) => at,
        };
        rows.push(indented(&marked[start..start + take], indent));
        start += take;
        while start < marked.len() && marked[start].0 == ' ' {
            start += 1;
        }
    }
    rows
}

fn indented(chunk: &[(char, bool)], indent: usize) -> Vec<(char, bool)> {
    if indent == 0 {
        return chunk.to_vec();
    }
    let mut row = vec![(' ', false); indent];
    row.extend_from_slice(chunk);
    row
}

/// Collapse tagged characters back into the fewest segments that preserve style.
fn runs(marked: &[(char, bool)]) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    for (character, bold) in marked {
        match segments.last_mut() {
            Some(last) if last.bold == *bold => last.text.push(*character),
            _ => segments.push(Segment {
                text: character.to_string(),
                bold: *bold,
                dim: false,
            }),
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: Role, text: &str) -> Message {
        Message {
            role,
            text: text.into(),
            call_id: None,
        }
    }

    fn plain(rows: &[Row]) -> Vec<String> {
        rows.iter().map(Row::plain).collect()
    }

    #[test]
    fn a_user_line_gets_a_marker_and_the_assistant_is_indented_under_it() {
        let rows = rows(
            &[
                message(Role::User, "hello"),
                message(Role::Assistant, "hi back"),
            ],
            80,
            Chrome::Plain,
        );
        assert_eq!(plain(&rows), vec!["› hello", "", "  hi back"]);
    }

    #[test]
    fn wrapping_breaks_on_words_not_mid_word() {
        let rows = message_rows(&message(Role::Assistant, "alpha beta gamma delta"), 14);
        assert_eq!(plain(&rows), vec!["  alpha beta", "  gamma delta"]);
    }

    #[test]
    fn a_token_longer_than_the_line_is_cut_rather_than_dropped() {
        let rows = message_rows(&message(Role::Assistant, "aaaaaaaaaaaaaaaaaa"), 12);
        let joined: String = plain(&rows).join("").replace("  ", "");
        assert_eq!(joined, "aaaaaaaaaaaaaaaaaa", "no characters may be lost");
        assert!(rows.len() > 1);
    }

    #[test]
    fn long_lines_stop_at_the_readable_width_on_a_very_wide_terminal() {
        let text = "word ".repeat(60);
        let rows = message_rows(&message(Role::Assistant, text.trim()), 400);
        assert!(
            rows.iter()
                .all(|r| r.plain().chars().count() <= READABLE_WIDTH),
            "a 400-column terminal must still wrap for readability"
        );
        assert!(rows.len() > 1);
    }

    #[test]
    fn bold_markers_are_styled_and_never_printed() {
        let rows = message_rows(&message(Role::Assistant, "see **Google** today"), 80);
        let row = &rows[0];
        assert_eq!(
            row.plain(),
            "  see Google today",
            "** must not reach the screen"
        );
        let bolded: String = row
            .segments
            .iter()
            .filter(|s| s.bold)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(bolded, "Google");
    }

    #[test]
    fn bold_survives_a_wrap_in_the_middle_of_the_emphasis() {
        let rows = message_rows(&message(Role::Assistant, "aa **bbbb cccc** dd"), 12);
        let bolded: String = rows
            .iter()
            .flat_map(|r| r.segments.iter())
            .filter(|s| s.bold)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(
            bolded, "bbbbcccc",
            "emphasis must not be lost at the line break"
        );
    }

    #[test]
    fn bullets_become_a_glyph_and_headings_lose_their_hashes() {
        let rows = message_rows(
            &message(Role::Assistant, "## Themes\n- first\n* second"),
            80,
        );
        assert_eq!(
            plain(&rows),
            vec!["  Themes", "  • first", "  ", "  • second"]
        );
        assert!(
            rows[0].segments.iter().any(|s| s.bold),
            "a heading reads bold"
        );
    }

    #[test]
    fn bullets_in_a_run_are_separated_by_a_blank_line() {
        let rows = message_rows(&message(Role::Assistant, "- first\n- second\n- third"), 80);
        assert_eq!(
            plain(&rows),
            vec!["  • first", "  ", "  • second", "  ", "  • third"]
        );
    }

    #[test]
    fn a_list_the_model_already_spaced_does_not_get_doubled_blanks() {
        let rows = message_rows(&message(Role::Assistant, "- first\n\n- second"), 80);
        assert_eq!(plain(&rows), vec!["  • first", "  ", "  • second"]);
    }

    #[test]
    fn prose_around_a_list_keeps_its_own_spacing() {
        let rows = message_rows(&message(Role::Assistant, "intro\n- only\noutro"), 80);
        assert_eq!(plain(&rows), vec!["  intro", "  • only", "  outro"]);
    }

    /// Column the first word of a bullet starts at, which is where every later
    /// line of that bullet has to start too.
    fn text_column(row: &Row) -> usize {
        row.plain()
            .chars()
            .position(|c| c.is_alphabetic())
            .expect("a word")
    }

    #[test]
    fn a_wrapped_bullet_hangs_under_its_text_not_under_the_glyph() {
        let rows = message_rows(&message(Role::Assistant, "- alpha beta gamma"), 14);
        assert_eq!(plain(&rows), vec!["  • alpha beta", "    gamma"]);
        assert_eq!(text_column(&rows[1]), text_column(&rows[0]));
    }

    #[test]
    fn a_nested_bullet_hangs_under_its_own_indent() {
        let rows = message_rows(&message(Role::Assistant, "  - alpha beta gamma"), 16);
        assert_eq!(plain(&rows), vec!["    • alpha beta", "      gamma"]);
        assert_eq!(text_column(&rows[1]), text_column(&rows[0]));
    }

    #[test]
    fn a_wrapped_paragraph_is_not_given_a_hanging_indent() {
        let rows = message_rows(&message(Role::Assistant, "alpha beta gamma"), 14);
        assert_eq!(plain(&rows), vec!["  alpha beta", "  gamma"]);
    }

    #[test]
    fn blank_lines_inside_an_answer_are_kept() {
        let rows = message_rows(&message(Role::Assistant, "one\n\ntwo"), 80);
        assert_eq!(plain(&rows), vec!["  one", "  ", "  two"]);
    }

    #[test]
    fn each_role_is_distinguishable_by_its_marker() {
        for (role, marker) in [
            (Role::User, "› "),
            (Role::Tool, "⏺ "),
            (Role::Action, "⚠ "),
            (Role::Error, "✗ "),
        ] {
            let rows = message_rows(&message(role, "x"), 80);
            assert_eq!(rows[0].plain(), format!("{marker}x"), "{role:?}");
        }
    }

    #[test]
    fn a_turn_is_drawn_in_a_titled_container() {
        let rows = rows(&[message(Role::User, "hello")], 40, Chrome::Boxed);
        assert_eq!(plain(&rows), vec!["╭─ you ──╮", "│ hello  │", "╰────────╯"]);
    }

    #[test]
    fn every_edge_of_a_container_is_the_same_width() {
        for text in ["hi", "a much longer message that has to wrap somewhere", ""] {
            let rows = rows(&[message(Role::Assistant, text)], 40, Chrome::Boxed);
            let widths: Vec<usize> = rows.iter().map(|r| r.plain().chars().count()).collect();
            assert!(
                widths.iter().all(|w| *w == widths[0]),
                "a ragged box is a broken box for {text:?}: {widths:?}"
            );
        }
    }

    #[test]
    fn a_container_hugs_its_content_rather_than_spanning_the_pane() {
        let rows = rows(&[message(Role::User, "hi")], 90, Chrome::Boxed);
        assert!(
            rows[0].plain().chars().count() < 20,
            "a two-word turn must not stretch a full-width bar: {}",
            rows[0].plain()
        );
    }

    #[test]
    fn a_container_never_outgrows_the_readable_width() {
        let text = "word ".repeat(80);
        let rows = rows(&[message(Role::Assistant, text.trim())], 400, Chrome::Boxed);
        assert!(rows
            .iter()
            .all(|r| r.plain().chars().count() <= READABLE_WIDTH));
    }

    #[test]
    fn tool_rows_are_never_boxed_even_in_boxed_mode() {
        let rows = rows(
            &[message(Role::Tool, "web_search  8 results")],
            40,
            Chrome::Boxed,
        );
        assert_eq!(plain(&rows), vec!["⏺ web_search  8 results"]);
    }

    #[test]
    fn plain_chrome_draws_no_border_characters() {
        let rows = rows(&[message(Role::User, "hello")], 40, Chrome::Plain);
        assert_eq!(plain(&rows), vec!["› hello"]);
    }

    #[test]
    fn container_edges_are_dim_so_the_frame_never_shouts() {
        let rows = rows(&[message(Role::User, "hello")], 40, Chrome::Boxed);
        assert!(rows[0].segments.iter().all(|s| s.dim), "top edge");
        assert!(
            rows[1].segments.first().is_some_and(|s| s.dim)
                && rows[1].segments.iter().any(|s| !s.dim),
            "the text inside the box keeps its own emphasis"
        );
    }

    #[test]
    fn the_empty_transcript_greets() {
        assert_eq!(
            rows(&[], 80, Chrome::Boxed)[0].plain(),
            "  What should Optimus do?"
        );
    }

    #[test]
    fn a_narrow_pane_still_lays_out_without_panicking() {
        let rows = message_rows(&message(Role::Assistant, "some words here"), 1);
        assert!(!rows.is_empty());
    }
}
