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
use crate::width;
use crate::workbench::{BlockId, BlockLifecycle, Item};
use unicode_segmentation::UnicodeSegmentation;

/// Longest line the transcript will lay out, however wide the terminal is.
pub const READABLE_WIDTH: usize = 96;

/// A provenance URL is useful as an identity, not as a paragraph. The
/// transcript keeps its beginning and uses a dim ellipsis for the query/path
/// tail so Google-style search URLs cannot take over the answer.
const MAX_SOURCE_URL_CELLS: usize = 64;

/// Fold markers: the run is open, and the run is closed.
const OPEN: char = '▾';
const SHUT: char = '▸';

/// How conversational turns are framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    /// The default app-like workbench: a filled task surface for the human,
    /// open assistant prose, and quiet operation rails.
    Workbench,
    /// Gutter markers only: two more rows of content per message, and a
    /// transcript that copies out of the terminal without border characters.
    Plain,
}

/// A run of characters sharing one emphasis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub bold: bool,
    /// Supporting runs can recede without losing their terminal cells.
    pub dim: bool,
}

/// A grapheme carrying the small amount of markdown emphasis the terminal
/// understands, plus whether a long source URL should recede into its fade.
#[derive(Debug, Clone)]
struct Marked {
    text: String,
    bold: bool,
    dim: bool,
}

impl Segment {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bold: false,
            dim: false,
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
    /// Fill this many cells with the row's background. Prompt surfaces use it
    /// to paint a full-width card without storing trailing spaces in the text
    /// projection that copy/paste and tests consume.
    pub surface_width: Option<usize>,
}

impl Row {
    pub(crate) fn blank() -> Self {
        Self {
            role: Role::Assistant,
            segments: Vec::new(),
            block: None,
            selected: false,
            surface_width: None,
        }
    }

    /// Chrome that is not a block: the greeting, the activity line.
    pub(crate) fn chrome(role: Role, segments: Vec<Segment>) -> Self {
        Self {
            role,
            segments,
            block: None,
            selected: false,
            surface_width: None,
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
        return greeting(width);
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
            Item::Single {
                index,
                id,
                lifecycle,
                body,
            } => {
                let Some(message) = messages.get(*index) else {
                    continue;
                };
                let mut laid = match body {
                    // A block with a body wears the fold marker in place of
                    // its role's, so one glyph answers "is there more here"
                    // for a run and for a command alike.
                    Some(body) if chrome == Chrome::Workbench && message.role == Role::Tool => {
                        operation_rows(
                            message,
                            width,
                            *lifecycle,
                            &format!("{} ", marker(body.expanded)),
                            "  ",
                        )
                    }
                    Some(body) => laid_rows(
                        message.role,
                        &message.text,
                        width,
                        &format!("{} ", marker(body.expanded)),
                        "  ",
                    ),
                    None if chrome == Chrome::Workbench => {
                        workbench_rows(message, width, *lifecycle)
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
                member_bodies,
                expanded,
            } => {
                let mut header = header_rows(tool, members.len(), *expanded, width);
                owned_by(&mut header, *id, chosen);
                rows.extend(header);
                if !*expanded {
                    continue;
                }
                for (member_index, at) in members.iter().enumerate() {
                    let Some(message) = messages.get(*at) else {
                        continue;
                    };
                    // Indented under the header, so an open run reads as the
                    // header's contents rather than as loose rows beside it.
                    let mut laid = laid_rows(message.role, &message.text, width, "  ⏺ ", "    ");
                    owned_by(&mut laid, *id, chosen);
                    rows.extend(laid);
                    if let Some(body) = member_bodies
                        .get(member_index)
                        .and_then(|body| body.as_ref())
                    {
                        for line in &body.lines {
                            let mut detail =
                                laid_rows(message.role, line, width, "    │ ", "    │ ");
                            owned_by(&mut detail, *id, chosen);
                            rows.extend(detail);
                        }
                    }
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

fn greeting(width: u16) -> Vec<Row> {
    let mut title = laid_rows(
        Role::Assistant,
        "What should Optimus do?",
        width,
        "  ✦ ",
        "    ",
    );
    for row in &mut title {
        for segment in &mut row.segments {
            segment.bold = true;
        }
    }

    let mut rows = title;
    rows.push(Row::blank());
    rows.extend(laid_rows(
        Role::Assistant,
        "Describe a task and press Enter. Ctrl-C stops a run; Esc clears a draft.",
        width,
        "  ",
        "  ",
    ));
    rows
}

/// Wrap a message's text into styled terminal-cell rows at `content` columns.
///
/// Shared by both framings: the task surface and the bare gutter differ only
/// in what they put beside these lines.
fn laid_out(text: &str, content: usize, compact_urls: bool) -> Vec<Vec<Marked>> {
    let mut lines: Vec<Vec<Marked>> = Vec::new();
    let mut after_bullet = false;
    for line in text.split('\n') {
        let mut parsed = parse_inline(line);
        if compact_urls {
            parsed.marked = compact_long_urls(parsed.marked);
        }
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
    if usable == 0 {
        return vec![Row {
            role,
            segments: Vec::new(),
            block: None,
            selected: false,
            surface_width: None,
        }];
    }
    let first = width::truncate(first, usable);
    let indent = width::truncate(indent, usable);
    // Both the first-row marker and the continuation indent must fit. Never
    // use a fixed minimum here: that was the source of narrow-pane overflow.
    let chrome = width::cells(&first).max(width::cells(&indent));
    if usable <= chrome {
        return vec![Row {
            role,
            segments: vec![Segment::plain(first)],
            block: None,
            selected: false,
            surface_width: None,
        }];
    }
    let content = usable.saturating_sub(chrome).max(1);

    laid_out(text, content, matches!(role, Role::Assistant | Role::Tool))
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let lead = if index == 0 { &first } else { &indent };
            let mut segments = vec![Segment::plain(lead)];
            segments.extend(runs(line));
            Row {
                role,
                segments,
                block: None,
                selected: false,
                surface_width: None,
            }
        })
        .collect()
}

/// The reference-inspired default face. One strong filled surface belongs to
/// the user's task; everything Optimus does beneath it stays on the open
/// canvas. That produces hierarchy without turning a long run into nested
/// terminal boxes.
fn workbench_rows(message: &Message, width: u16, lifecycle: BlockLifecycle) -> Vec<Row> {
    match message.role {
        Role::User => prompt_rows(message, width),
        Role::Assistant => laid_rows(message.role, &message.text, width, "  ✦ ", "    "),
        Role::Tool => operation_rows(message, width, lifecycle, "  │ ", "  │ "),
        Role::Action => laid_rows(message.role, &message.text, width, "  ◇ ", "    "),
        Role::Error => laid_rows(message.role, &message.text, width, "  × ", "    "),
    }
}

/// A compact operation row with a typed lifecycle chip aligned to the right.
/// At narrow widths the chip stands down and the original human line remains,
/// so status never steals the tool name or causes overflow.
fn operation_rows(
    message: &Message,
    width: u16,
    lifecycle: BlockLifecycle,
    first: &str,
    indent: &str,
) -> Vec<Row> {
    let outer = usize::from(width);
    let status = format!("[{}]", lifecycle_label(lifecycle));
    let first_width = width::cells(first);
    let status_width = width::cells(&status);
    if outer < first_width.saturating_add(status_width).saturating_add(10) {
        return laid_rows(message.role, &message.text, width, first, indent);
    }

    let content = concise_operation_text(&message.text, lifecycle);
    let room = outer
        .saturating_sub(first_width + status_width + 2)
        .min(READABLE_WIDTH.saturating_sub(first_width))
        .max(1);
    let lines = laid_out(&content, room, true);
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let lead = if index == 0 { first } else { indent };
            let mut segments = vec![Segment::plain(lead)];
            segments.extend(runs(line));
            if index == 0 {
                let used = segments
                    .iter()
                    .map(|segment| width::cells(&segment.text))
                    .sum::<usize>();
                let gap = outer.saturating_sub(used + status_width);
                segments.push(Segment::plain(" ".repeat(gap)));
                segments.push(Segment::plain(status.clone()));
            }
            Row {
                role: message.role,
                segments,
                block: None,
                selected: false,
                surface_width: None,
            }
        })
        .collect()
}

fn lifecycle_label(lifecycle: BlockLifecycle) -> &'static str {
    match lifecycle {
        BlockLifecycle::Queued => "queued",
        BlockLifecycle::Running => "running",
        BlockLifecycle::Waiting => "waiting",
        BlockLifecycle::Blocked => "approval",
        BlockLifecycle::Succeeded => "done",
        BlockLifecycle::Failed => "failed",
        BlockLifecycle::Cancelled => "stopped",
        BlockLifecycle::PossiblyStalled => "stalled",
    }
}

/// Remove the phase word only when the typed lifecycle chip says exactly the
/// same thing. Details such as `failed: rate limited` remain intact.
fn concise_operation_text(text: &str, lifecycle: BlockLifecycle) -> String {
    let phase = match lifecycle {
        BlockLifecycle::Running => "running",
        BlockLifecycle::Blocked => "awaiting approval",
        BlockLifecycle::Succeeded => "done",
        BlockLifecycle::Failed => "failed",
        BlockLifecycle::Cancelled => "cancelled",
        _ => return text.to_owned(),
    };
    let duration_at = text
        .rfind("  (")
        .filter(|at| text[*at..].ends_with(')'))
        .unwrap_or(text.len());
    let (summary, duration) = text.split_at(duration_at);
    let suffix = format!("  {phase}");
    summary
        .strip_suffix(&suffix)
        .map(|summary| format!("{summary}{duration}"))
        .unwrap_or_else(|| text.to_owned())
}

/// A full-width task surface with one row of vertical padding. The readable
/// text measure remains bounded even on a cinema-wide terminal; the surface
/// itself spans the pane, matching the command-card hierarchy of the visual
/// reference.
fn prompt_rows(message: &Message, width: u16) -> Vec<Row> {
    let outer = usize::from(width);
    if outer < 6 {
        return message_rows(message, width);
    }
    let content = outer
        .saturating_sub(6)
        .min(READABLE_WIDTH.saturating_sub(6));
    let lines = laid_out(&message.text, content.max(1), false);
    let surface = || Row {
        role: Role::User,
        segments: Vec::new(),
        block: None,
        selected: false,
        surface_width: Some(outer),
    };
    let mut rows = vec![surface()];
    rows.extend(lines.iter().enumerate().map(|(index, line)| {
        let mut segments = vec![Segment::plain(if index == 0 { "  › " } else { "    " })];
        segments.extend(runs(line));
        Row {
            role: Role::User,
            segments,
            block: None,
            selected: false,
            surface_width: Some(outer),
        }
    }));
    rows.push(surface());
    rows
}

/// One logical line resolved into styled characters, plus how it lays out.
struct Parsed {
    marked: Vec<Marked>,
    bullet: bool,
    /// Columns to indent continuation lines by, so a wrapped bullet's later
    /// lines sit under its text rather than under the glyph.
    hang: usize,
}

/// Turn one logical line into grapheme clusters tagged with emphasis.
///
/// Only the markdown that actually shows up in answers is handled: `**bold**`,
/// setext-free `#` headings, and `-`/`*` bullets. Anything else is left alone
/// rather than half-rendered — a wrong transform reads worse than a literal one.
fn parse_inline(line: &str) -> Parsed {
    let trimmed = line.trim_start();
    let leading = width::cells(&line[..line.len() - trimmed.len()]);
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
    let hang = if bullet { width::cells(&prefix) } else { 0 };

    let mut marked: Vec<Marked> = prefix
        .graphemes(true)
        .map(|grapheme| Marked {
            text: grapheme.to_owned(),
            bold: heading,
            dim: false,
        })
        .collect();
    let graphemes: Vec<&str> = rest.graphemes(true).collect();
    let mut bold = false;
    let mut index = 0;
    while index < graphemes.len() {
        // `**` toggles emphasis and is not itself printed.
        if graphemes[index] == "*" && graphemes.get(index + 1).copied() == Some("*") {
            bold = !bold;
            index += 2;
            continue;
        }
        marked.push(Marked {
            text: graphemes[index].to_owned(),
            bold: bold || heading,
            dim: false,
        });
        index += 1;
    }
    Parsed {
        marked,
        bullet,
        hang,
    }
}

/// Replace an overlong URL token before wrapping. Doing this before the word
/// wrapper matters: otherwise a Google query is already split across several
/// rows and there is no honest way to tell the layout that the tail belongs to
/// one source identity.
fn compact_long_urls(marked: Vec<Marked>) -> Vec<Marked> {
    let mut compacted = Vec::with_capacity(marked.len());
    let mut at = 0;
    while at < marked.len() {
        if !starts_with_url(&marked, at) {
            compacted.push(marked[at].clone());
            at += 1;
            continue;
        }
        let mut end = at;
        while end < marked.len() && !marked[end].text.chars().any(char::is_whitespace) {
            end += 1;
        }
        let url = marked[at..end]
            .iter()
            .map(|grapheme| grapheme.text.as_str())
            .collect::<String>();
        if width::cells(&url) <= MAX_SOURCE_URL_CELLS {
            // Search-detail rows may already be clipped at the workbench
            // boundary. They still belong to this URL pass: preserve the
            // visible identity, but let the terminal recede the existing
            // ellipsis just like it does for a raw assistant URL.
            if width::cells(&url) == MAX_SOURCE_URL_CELLS && url.ends_with('…') {
                compacted.extend(marked[at..end - 1].iter().cloned());
                compacted.push(Marked {
                    text: marked[end - 1].text.clone(),
                    bold: marked[end - 1].bold,
                    dim: true,
                });
            } else {
                compacted.extend(marked[at..end].iter().cloned());
            }
        } else {
            compacted.push(Marked {
                text: width::take(&url, MAX_SOURCE_URL_CELLS - 1),
                bold: marked[at].bold,
                dim: false,
            });
            compacted.push(Marked {
                text: "…".into(),
                bold: false,
                dim: true,
            });
        }
        at = end;
    }
    compacted
}

fn starts_with_url(marked: &[Marked], at: usize) -> bool {
    let prefix = marked
        .iter()
        .skip(at)
        .take(8)
        .map(|grapheme| grapheme.text.as_str())
        .collect::<String>();
    prefix.starts_with("https://") || prefix.starts_with("http://")
}

/// Greedy word wrap that keeps each grapheme's emphasis attached to it.
///
/// `hang` indents every line after the first, which is what makes a wrapped
/// bullet read as one item rather than as two.
fn wrap_marked(marked: &[Marked], width: usize, hang: usize) -> Vec<Vec<Marked>> {
    if marked.is_empty() {
        return vec![Vec::new()];
    }
    if width == 0 {
        return vec![Vec::new()];
    }
    // A hang wider than the line would leave no room for text at all. Keep one
    // cell for the content so the row remains a valid terminal row.
    let hang = hang.min(width.saturating_sub(1));
    let mut rows = Vec::new();
    let mut start = 0;
    while start < marked.len() {
        let indent = if rows.is_empty() { 0 } else { hang };
        let room = width.saturating_sub(indent).max(1);
        let mut end = start;
        let mut used = 0;
        while end < marked.len() {
            let item_width = width::grapheme_cells(&marked[end].text);
            if item_width > room {
                if used == 0 {
                    end += 1;
                }
                break;
            }
            if used > 0 && used + item_width > room {
                break;
            }
            used += item_width;
            end += 1;
        }
        if end == start {
            end = (start + 1).min(marked.len());
        }
        if end == marked.len() {
            rows.push(indented(&marked[start..], indent, room));
            break;
        }
        // A space just beyond the measured edge is still a useful break point.
        if end < marked.len() && marked[end].text == " " {
            end += 1;
        }
        let take = (start..end)
            .rev()
            .find(|index| *index > start && marked[*index].text == " ")
            .unwrap_or(end);
        let take = take.max(start + 1).min(marked.len());
        rows.push(indented(&marked[start..take], indent, room));
        start = take;
        while start < marked.len() && marked[start].text == " " {
            start += 1;
        }
        if start == marked.len() {
            break;
        }
    }
    rows
}

fn indented(chunk: &[Marked], indent: usize, room: usize) -> Vec<Marked> {
    let mut row = vec![
        Marked {
            text: " ".to_owned(),
            bold: false,
            dim: false,
        };
        indent
    ];
    let mut used = 0;
    for marked in chunk {
        let remaining = room.saturating_sub(used);
        let fitted = width::fit_grapheme(&marked.text, remaining);
        if fitted.is_empty() && !marked.text.is_empty() {
            continue;
        }
        used += width::cells(&fitted);
        row.push(Marked {
            text: fitted,
            bold: marked.bold,
            dim: marked.dim,
        });
    }
    row
}

/// Collapse tagged graphemes back into the fewest segments that preserve style.
fn runs(marked: &[Marked]) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    for grapheme in marked {
        match segments.last_mut() {
            Some(last) if last.bold == grapheme.bold && last.dim == grapheme.dim => {
                last.text.push_str(&grapheme.text)
            }
            _ => segments.push(Segment {
                text: grapheme.text.clone(),
                bold: grapheme.bold,
                dim: grapheme.dim,
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
                .all(|r| width::cells(&r.plain()) <= READABLE_WIDTH),
            "a 400-column terminal must still wrap for readability"
        );
        assert!(rows.len() > 1);
    }

    #[test]
    fn a_long_source_url_fades_after_its_identity_without_wrapping_the_query() {
        let url = "https://www.google.com/search?q=latest+ai+news+today&source=web&client=optimus&hl=en-NZ&safe=active";
        let rows = message_rows(&message(Role::Assistant, url), 100);
        let shown = plain(&rows).join("\n");
        assert!(shown.contains("https://www.google.com/search"), "{shown}");
        assert!(
            shown.contains('…'),
            "the source needs a visible fade cue: {shown}"
        );
        assert!(
            !shown.contains("client=optimus&hl=en-NZ&safe=active"),
            "the query tail should recede: {shown}"
        );
        assert!(
            rows.iter()
                .flat_map(|row| row.segments.iter())
                .any(|segment| segment.dim && segment.text.contains('…')),
            "the fade cue should use the transcript's supporting style"
        );
    }

    #[test]
    fn a_preclipped_grouped_source_keeps_its_dim_fade() {
        let url = "https://www.google.com/search?q=latest+ai+news+today&source=web&client=optimus&hl=en-NZ&safe=active";
        let clipped = width::truncate(url, MAX_SOURCE_URL_CELLS);
        let rows = message_rows(&message(Role::Assistant, &format!("   {clipped}")), 100);
        assert!(
            rows.iter()
                .flat_map(|row| row.segments.iter())
                .any(|segment| segment.dim && segment.text == "…"),
            "a source row clipped by the detail view still needs a dim ellipsis: {rows:?}"
        );
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
    fn the_workbench_task_is_a_padded_full_width_surface() {
        let rows = painted(&[message(Role::User, "hello")], 40, Chrome::Workbench);
        assert_eq!(plain(&rows), vec!["", "  › hello", ""]);
        assert!(rows.iter().all(|row| row.surface_width == Some(40)));
    }

    #[test]
    fn assistant_output_stays_on_the_open_canvas() {
        let rows = painted(
            &[message(Role::Assistant, "a considered answer")],
            40,
            Chrome::Workbench,
        );
        assert_eq!(plain(&rows), vec!["  ✦ a considered answer"]);
        assert!(rows.iter().all(|row| row.surface_width.is_none()));
    }

    #[test]
    fn a_task_surface_spans_the_pane_without_padding_the_text_projection() {
        let rows = painted(&[message(Role::User, "hi")], 90, Chrome::Workbench);
        assert!(
            rows.iter().all(|row| row.surface_width == Some(90)),
            "the painter needs one shared card edge: {rows:?}"
        );
        assert_eq!(rows[1].plain(), "  › hi");
    }

    #[test]
    fn task_text_never_outgrows_the_readable_width() {
        let text = "word ".repeat(80);
        let rows = painted(&[message(Role::User, text.trim())], 400, Chrome::Workbench);
        assert!(rows
            .iter()
            .all(|r| width::cells(&r.plain()) <= READABLE_WIDTH));
    }

    #[test]
    fn tool_rows_use_a_quiet_operation_rail_in_workbench_mode() {
        let rows = painted(
            &[message(Role::Tool, "web_search  8 results")],
            40,
            Chrome::Workbench,
        );
        assert!(rows[0].plain().starts_with("  │ web_search  8 results"));
        assert!(rows[0].plain().ends_with("[done]"));
        assert_eq!(width::cells(&rows[0].plain()), 40);
    }

    #[test]
    fn operation_status_comes_from_lifecycle_and_does_not_duplicate_phase_text() {
        let message = message(Role::Tool, "terminal  running");
        let rows = operation_rows(&message, 40, BlockLifecycle::Running, "  │ ", "  │ ");
        assert!(rows[0].plain().contains("terminal"));
        assert_eq!(rows[0].plain().matches("running").count(), 1);
        assert!(rows[0].plain().ends_with("[running]"));
    }

    #[test]
    fn narrow_operations_keep_the_original_line_instead_of_clipping_for_a_chip() {
        let message = message(Role::Tool, "web_search  running");
        let rows = operation_rows(&message, 16, BlockLifecycle::Running, "  │ ", "  │ ");
        assert!(!rows.iter().any(|row| row.plain().contains("[running]")));
        assert_eq!(
            plain(&rows)
                .iter()
                .map(|row| row.trim_end())
                .collect::<Vec<_>>(),
            vec!["  │ web_search", "  │ running"]
        );
    }

    #[test]
    fn plain_chrome_draws_no_border_characters() {
        let rows = painted(&[message(Role::User, "hello")], 40, Chrome::Plain);
        assert_eq!(plain(&rows), vec!["› hello"]);
    }

    #[test]
    fn a_prompt_surface_does_not_store_visual_fill_as_copyable_text() {
        let rows = painted(&[message(Role::User, "hello")], 40, Chrome::Workbench);
        assert_eq!(rows[0].plain(), "");
        assert_eq!(rows[1].plain(), "  › hello");
        assert_eq!(rows[2].plain(), "");
        assert!(rows.iter().all(|row| row.surface_width == Some(40)));
    }

    #[test]
    fn the_empty_transcript_greets() {
        assert_eq!(
            painted(&[], 80, Chrome::Workbench)[0].plain(),
            "  ✦ What should Optimus do?"
        );
    }

    #[test]
    fn a_narrow_greeting_wraps_every_instruction_instead_of_clipping_it() {
        let screen = plain(&painted(&[], 34, Chrome::Plain))
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(screen.contains("What should Optimus do?"));
        assert!(screen
            .contains("Describe a task and press Enter. Ctrl-C stops a run; Esc clears a draft."));
    }

    #[test]
    fn a_narrow_pane_still_lays_out_without_panicking() {
        let rows = message_rows(&message(Role::Assistant, "some words here"), 1);
        assert!(!rows.is_empty());
    }

    #[test]
    fn every_chrome_mode_keeps_unicode_rows_inside_the_requested_cell_width() {
        let messages = vec![message(
            Role::Assistant,
            "界界 👍👍 e\u{301} ｶｶ and a deliberately long token",
        )];
        for width in [0_u16, 1, 2, 3, 4, 5, 8, 16, 24, 40, 96, 120] {
            for chrome in [Chrome::Plain, Chrome::Workbench] {
                let painted = painted(&messages, width, chrome);
                assert!(
                    painted
                        .iter()
                        .all(|row| width::cells(&row.plain()) <= usize::from(width)),
                    "{chrome:?} overflowed {width}: {painted:?}"
                );
            }
        }
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
            member_bodies: vec![None, None, None],
            expanded: false,
        };
        (messages, vec![item], id)
    }

    fn opened(items: &[Item]) -> Vec<Item> {
        items
            .iter()
            .map(|item| match item {
                Item::Group {
                    id,
                    tool,
                    members,
                    member_bodies,
                    ..
                } => Item::Group {
                    id: *id,
                    tool: tool.clone(),
                    members: members.clone(),
                    member_bodies: member_bodies.clone(),
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
    fn opening_a_search_group_keeps_its_sources_inside_the_group() {
        let messages = vec![
            message(Role::Tool, "web_search  2 results"),
            message(Role::Tool, "web_search  2 results"),
            message(Role::Tool, "web_search  2 results"),
        ];
        let id = BlockId::mint();
        let items = vec![Item::Group {
            id,
            tool: "web_search".into(),
            members: vec![0, 1, 2],
            member_bodies: vec![
                Some(Body {
                    lines: vec![
                        "1. First headline".into(),
                        "   https://example.com/first".into(),
                    ],
                    expanded: false,
                }),
                None,
                None,
            ],
            expanded: true,
        }];
        let painted = plain(&rows(&messages, &items, None, 100, Chrome::Plain));
        assert_eq!(painted[0], "▾ web_search · 3 calls");
        assert_eq!(painted[1], "  ⏺ web_search  2 results");
        assert_eq!(painted[2], "    │ 1. First headline");
        assert_eq!(painted[3], "    │    https://example.com/first");
        assert!(
            painted
                .iter()
                .all(|row| row.starts_with('▾') || row.starts_with("  ")),
            "search source rows must remain children of the group: {painted:?}"
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
        let rows = rows(&messages, &items, Some(chosen), 40, Chrome::Workbench);
        let marked: Vec<bool> = rows.iter().map(|row| row.selected).collect();
        assert!(
            marked.iter().filter(|m| **m).count() >= 3,
            "the whole task surface is selected, not one of its rows: {marked:?}"
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
            lifecycle: BlockLifecycle::Succeeded,
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
