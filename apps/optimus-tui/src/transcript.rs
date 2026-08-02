//! Transcript rows: the workbench turned into the exact screen lines to paint.
//!
//! Deliberately free of ratatui types. Wrapping, gutters, and inline markdown
//! are the part most likely to be wrong, so they stay unit-testable without
//! standing up a terminal; `view` maps the result onto spans.
//!
//! Two readability rules live here. Text wraps on word boundaries rather than
//! mid-word, and it wraps at [`READABLE_WIDTH`] even when the terminal is much
//! wider, because a 200-column paragraph is hard to track back to the next line.
//!
//! Rows are a *projection* (ADR-0075 §1): [`rows`] is handed the workbench's
//! items and paints them, and every row it produces names the block it paints
//! so a click can find its way back to semantic state. Nothing here decides
//! what is grouped, what is open, or what is selected — those live in
//! [`crate::workbench`], and a row index is never read back into any of them.

use crate::session::{Message, Role};
use crate::workbench::{BlockId, Item};

/// Longest line the transcript will lay out, however wide the terminal is.
pub const READABLE_WIDTH: usize = 96;

/// Narrowest a container may be, so a one-word turn still looks deliberate.
const MIN_BOX: usize = 22;

/// Fold markers: the run is open, and the run is closed.
const OPEN: char = '▾';
const SHUT: char = '▸';

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
    /// The block this row paints. Chrome — blank separators, the greeting, the
    /// activity line — carries `None`, because clicking it means nothing.
    pub block: Option<BlockId>,
    /// Whether this row belongs to the item the keyboard is pointed at.
    pub selected: bool,
}

impl Row {
    pub(crate) fn blank() -> Self {
        Self {
            role: Role::Assistant,
            segments: Vec::new(),
            block: None,
            selected: false,
        }
    }

    /// Chrome that is not a block: the greeting, the activity line.
    pub(crate) fn chrome(role: Role, segments: Vec<Segment>) -> Self {
        Self {
            role,
            segments,
            block: None,
            selected: false,
        }
    }

    /// The row as plain text — what scroll maths and tests measure.
    pub fn plain(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}

/// Stamp a freshly laid-out run of rows with the block they paint.
fn owned_by(rows: &mut [Row], block: BlockId, selected: bool) {
    for row in rows {
        row.block = Some(block);
        row.selected = selected;
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
///
/// `items` is the workbench's projection: what to paint, in order, with each
/// run of folded calls already resolved to its members and its open state.
/// `selected` is the block the keyboard is pointed at, or the one a click
/// landed on; a run highlights whole, members included, because the run is the
/// thing that is selected.
pub fn rows(
    messages: &[Message],
    items: &[Item],
    selected: Option<BlockId>,
    width: u16,
    chrome: Chrome,
) -> Vec<Row> {
    if messages.is_empty() {
        return greeting();
    }
    let mut rows = Vec::new();
    for (index, item) in items.iter().enumerate() {
        // One blank line between items, so turns are visually separable. A
        // run's members are one item and keep no blank between them.
        if index > 0 {
            rows.push(Row::blank());
        }
        let chosen = selected == Some(item.id());
        match item {
            Item::Single { index, id, body } => {
                let Some(message) = messages.get(*index) else {
                    continue;
                };
                let mut laid = match body {
                    // A block with a body wears the fold marker in place of
                    // its role's, so one glyph answers "is there more here"
                    // for a run and for a command alike.
                    Some(body) => laid_rows(
                        message.role,
                        &message.text,
                        width,
                        &format!("{} ", marker(body.expanded)),
                        "  ",
                    ),
                    None if chrome == Chrome::Boxed && contained(message.role) => {
                        boxed_rows(message, width)
                    }
                    None => message_rows(message, width),
                };
                if let Some(body) = body.as_ref().filter(|body| body.expanded) {
                    for line in &body.lines {
                        laid.extend(laid_rows(message.role, line, width, "  │ ", "  │ "));
                    }
                }
                owned_by(&mut laid, *id, chosen);
                rows.extend(laid);
            }
            Item::Group {
                id,
                tool,
                members,
                expanded,
            } => {
                let mut header = header_rows(tool, members.len(), *expanded, width);
                owned_by(&mut header, *id, chosen);
                rows.extend(header);
                if !*expanded {
                    continue;
                }
                for at in members {
                    let Some(message) = messages.get(*at) else {
                        continue;
                    };
                    // Indented under the header, so an open run reads as the
                    // header's contents rather than as loose rows beside it.
                    let mut laid = laid_rows(message.role, &message.text, width, "  ⏺ ", "    ");
                    owned_by(&mut laid, *id, chosen);
                    rows.extend(laid);
                }
            }
        }
    }
    rows
}

/// The one row a folded run shows: the marker, the tool, and how many calls.
///
/// Deliberately a count and nothing else. A run's wall time spans the model's
/// thinking between its calls, so presenting it as the tools' cost would be a
/// number that reads precise and is not; durations arrive with the typed
/// `Timing` events in the phase that consumes them.
fn header_rows(tool: &str, count: usize, expanded: bool, width: u16) -> Vec<Row> {
    let text = format!("{tool} · {count} calls");
    laid_rows(
        Role::Tool,
        &text,
        width,
        &format!("{} ", marker(expanded)),
        "  ",
    )
}

/// The glyph that says whether something can be opened, and whether it is.
fn marker(expanded: bool) -> char {
    if expanded {
        OPEN
    } else {
        SHUT
    }
}

/// Only conversational turns get a container. Tool, action, and error rows are
/// one-liners; a box around each would be more frame than content.
fn contained(role: Role) -> bool {
    matches!(role, Role::User | Role::Assistant)
}

fn greeting() -> Vec<Row> {
    vec![
        Row::chrome(
            Role::Assistant,
            vec![Segment {
                text: "  What should Optimus do?".into(),
                bold: true,
                dim: false,
            }],
        ),
        Row::blank(),
        Row::chrome(
            Role::Assistant,
            vec![Segment::plain(
                "  Describe a task and press Enter. Ctrl-C stops a run; Esc clears a draft.",
            )],
        ),
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
    laid_rows(message.role, &message.text, width, first, indent)
}

/// Wrap `text` for `width` columns behind an explicit gutter.
///
/// The gutter is a parameter rather than a lookup because a row's marker is not
/// always its role's: a folded run's header carries the fold marker, and its
/// members are indented under it while keeping the tool marker they had when
/// they stood alone.
fn laid_rows(role: Role, text: &str, width: u16, first: &str, indent: &str) -> Vec<Row> {
    let usable = usize::from(width).min(READABLE_WIDTH);
    // Never let a narrow pane drive the content width to zero.
    let content = usable.saturating_sub(first.chars().count()).max(8);

    laid_out(text, content)
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let lead = if index == 0 { first } else { indent };
            let mut segments = vec![Segment::plain(lead)];
            segments.extend(runs(line));
            Row {
                role,
                segments,
                block: None,
                selected: false,
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
        " YOU "
    } else {
        " OPTIMUS "
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
            block: None,
            selected: false,
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
        block: None,
        selected: false,
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

    use crate::workbench::{ungrouped, Body};

    fn message(role: Role, text: &str) -> Message {
        Message {
            role,
            text: text.into(),
            call_id: None,
        }
    }

    /// Lay out messages with nothing grouped and nothing selected — the shape
    /// every wrapping and framing rule below is about.
    fn painted(messages: &[Message], width: u16, chrome: Chrome) -> Vec<Row> {
        rows(messages, &ungrouped(messages.len()), None, width, chrome)
    }

    fn plain(rows: &[Row]) -> Vec<String> {
        rows.iter().map(Row::plain).collect()
    }

    #[test]
    fn greeting_describes_escape_as_draft_clear_not_exit() {
        let greeting = plain(&painted(&[], 80, Chrome::Plain)).join("\n");
        assert!(greeting.contains("Esc clears a draft"));
        assert!(!greeting.contains("Esc exits"));
    }

    #[test]
    fn a_user_line_gets_a_marker_and_the_assistant_is_indented_under_it() {
        let rows = painted(
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
        let rows = painted(&[message(Role::User, "hello")], 40, Chrome::Boxed);
        assert_eq!(plain(&rows), vec!["╭─ YOU ──╮", "│ hello  │", "╰────────╯"]);
    }

    #[test]
    fn every_edge_of_a_container_is_the_same_width() {
        for text in ["hi", "a much longer message that has to wrap somewhere", ""] {
            let rows = painted(&[message(Role::Assistant, text)], 40, Chrome::Boxed);
            let widths: Vec<usize> = rows.iter().map(|r| r.plain().chars().count()).collect();
            assert!(
                widths.iter().all(|w| *w == widths[0]),
                "a ragged box is a broken box for {text:?}: {widths:?}"
            );
        }
    }

    #[test]
    fn a_container_hugs_its_content_rather_than_spanning_the_pane() {
        let rows = painted(&[message(Role::User, "hi")], 90, Chrome::Boxed);
        assert!(
            rows[0].plain().chars().count() < 20,
            "a two-word turn must not stretch a full-width bar: {}",
            rows[0].plain()
        );
    }

    #[test]
    fn a_container_never_outgrows_the_readable_width() {
        let text = "word ".repeat(80);
        let rows = painted(&[message(Role::Assistant, text.trim())], 400, Chrome::Boxed);
        assert!(rows
            .iter()
            .all(|r| r.plain().chars().count() <= READABLE_WIDTH));
    }

    #[test]
    fn tool_rows_are_never_boxed_even_in_boxed_mode() {
        let rows = painted(
            &[message(Role::Tool, "web_search  8 results")],
            40,
            Chrome::Boxed,
        );
        assert_eq!(plain(&rows), vec!["⏺ web_search  8 results"]);
    }

    #[test]
    fn plain_chrome_draws_no_border_characters() {
        let rows = painted(&[message(Role::User, "hello")], 40, Chrome::Plain);
        assert_eq!(plain(&rows), vec!["› hello"]);
    }

    #[test]
    fn container_edges_are_dim_so_the_frame_never_shouts() {
        let rows = painted(&[message(Role::User, "hello")], 40, Chrome::Boxed);
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
            painted(&[], 80, Chrome::Boxed)[0].plain(),
            "  What should Optimus do?"
        );
    }

    #[test]
    fn a_narrow_pane_still_lays_out_without_panicking() {
        let rows = message_rows(&message(Role::Assistant, "some words here"), 1);
        assert!(!rows.is_empty());
    }

    // ADR-0075 phase 2: rows project items, and each names the block it paints.

    /// Three reads, folded into one run whose head is the first of them.
    fn run() -> (Vec<Message>, Vec<Item>, BlockId) {
        let messages: Vec<Message> = (0..3)
            .map(|n| message(Role::Tool, &format!("read_file  src/{n}.rs")))
            .collect();
        let id = BlockId::mint();
        let item = Item::Group {
            id,
            tool: "read_file".into(),
            members: vec![0, 1, 2],
            expanded: false,
        };
        (messages, vec![item], id)
    }

    fn opened(items: &[Item]) -> Vec<Item> {
        items
            .iter()
            .map(|item| match item {
                Item::Group {
                    id, tool, members, ..
                } => Item::Group {
                    id: *id,
                    tool: tool.clone(),
                    members: members.clone(),
                    expanded: true,
                },
                other => other.clone(),
            })
            .collect()
    }

    #[test]
    fn a_closed_run_is_one_row_that_says_what_it_is_hiding() {
        let (messages, items, _) = run();
        let painted = plain(&rows(&messages, &items, None, 80, Chrome::Plain));
        assert_eq!(painted, vec!["▸ read_file · 3 calls"]);
    }

    #[test]
    fn opening_a_run_shows_every_call_it_was_hiding() {
        let (messages, items, _) = run();
        let painted = plain(&rows(&messages, &opened(&items), None, 80, Chrome::Plain));
        assert_eq!(
            painted,
            vec![
                "▾ read_file · 3 calls",
                "  ⏺ read_file  src/0.rs",
                "  ⏺ read_file  src/1.rs",
                "  ⏺ read_file  src/2.rs",
            ],
            "the marker turns and the members appear underneath it"
        );
    }

    #[test]
    fn every_row_of_a_run_belongs_to_the_run_so_a_click_anywhere_finds_it() {
        let (messages, items, head) = run();
        for rows in [
            rows(&messages, &items, None, 80, Chrome::Plain),
            rows(&messages, &opened(&items), None, 80, Chrome::Plain),
        ] {
            assert!(
                rows.iter().all(|row| row.block == Some(head)),
                "a run's rows all name the run: {rows:?}"
            );
        }
    }

    #[test]
    fn a_selected_item_marks_all_of_its_rows_and_nothing_elses() {
        let messages = vec![message(Role::User, "hello"), message(Role::Assistant, "hi")];
        let items = ungrouped(2);
        let chosen = items[0].id();
        let rows = rows(&messages, &items, Some(chosen), 40, Chrome::Boxed);
        let marked: Vec<bool> = rows.iter().map(|row| row.selected).collect();
        assert!(
            marked.iter().filter(|m| **m).count() >= 3,
            "the whole container is selected, not one of its edges: {marked:?}"
        );
        assert!(
            rows.iter()
                .all(|row| !row.selected || row.block == Some(chosen)),
            "nothing outside the selected item may be marked"
        );
    }

    #[test]
    fn chrome_belongs_to_no_block_so_it_can_never_be_clicked_into_one() {
        let messages = vec![message(Role::User, "a"), message(Role::User, "b")];
        let rows = rows(&messages, &ungrouped(2), None, 40, Chrome::Plain);
        let blank = rows
            .iter()
            .find(|row| row.plain().is_empty())
            .expect("a separator between the two turns");
        assert_eq!(blank.block, None);
        assert!(painted(&[], 80, Chrome::Plain)
            .iter()
            .all(|row| row.block.is_none()));
    }

    #[test]
    fn a_run_header_stays_one_row_on_a_narrow_pane() {
        let (messages, items, _) = run();
        let painted = rows(&messages, &items, None, 24, Chrome::Plain);
        assert_eq!(painted.len(), 1, "{painted:?}");
        assert!(painted[0].plain().starts_with('▸'));
    }

    // ADR-0075 phase 3: a block with a body wears the same fold marker a run
    // does, and its output is set in from the summary line above it.

    fn with_body(expanded: bool) -> (Vec<Message>, Vec<Item>) {
        let messages = vec![message(Role::Tool, "terminal  47 passed  (8.3s)")];
        let items = vec![Item::Single {
            index: 0,
            id: BlockId::mint(),
            body: Some(Body {
                lines: vec!["running 47 tests".into(), "test result: ok".into()],
                expanded,
            }),
        }];
        (messages, items)
    }

    #[test]
    fn a_closed_command_is_the_summary_line_and_a_marker() {
        let (messages, items) = with_body(false);
        assert_eq!(
            plain(&rows(&messages, &items, None, 80, Chrome::Plain)),
            vec!["▸ terminal  47 passed  (8.3s)"]
        );
    }

    #[test]
    fn an_open_command_sets_its_output_in_under_the_line_it_belongs_to() {
        let (messages, items) = with_body(true);
        assert_eq!(
            plain(&rows(&messages, &items, None, 80, Chrome::Plain)),
            vec![
                "▾ terminal  47 passed  (8.3s)",
                "  │ running 47 tests",
                "  │ test result: ok",
            ],
            "the rule marks where the output starts and stops"
        );
    }

    #[test]
    fn every_row_of_an_open_command_belongs_to_the_command() {
        let (messages, items) = with_body(true);
        let painted = rows(&messages, &items, None, 80, Chrome::Plain);
        let owner = items[0].id();
        assert!(painted.iter().all(|row| row.block == Some(owner)));
        assert!(
            painted[1..].iter().all(|row| row.role == Role::Tool),
            "output keeps the call's own colour rather than reading as prose"
        );
    }

    /// An item naming a message that is not there must not panic or shift
    /// everything after it; it simply paints nothing.
    #[test]
    fn an_item_pointing_past_the_transcript_paints_nothing() {
        let messages = vec![message(Role::User, "only one")];
        let items = ungrouped(3);
        let rows = rows(&messages, &items, None, 40, Chrome::Plain);
        assert_eq!(plain(&rows), vec!["› only one", "", ""]);
    }
}
