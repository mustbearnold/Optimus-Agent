//! Painting the draft: wrapping, the prompt gutter, and where the terminal
//! cursor goes.
//!
//! The composer used to be one hard-coded row that silently clipped anything
//! longer, with no cursor drawn at all — you could not see where typing would
//! land. Height is derived from the draft here, and [`layout`] is a pure
//! function so both the wrap and the cursor position are assertable without a
//! terminal.

use ratatui::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

use crate::composer::Composer;
use crate::{bordered, wrapped};

/// Columns reserved for the prompt gutter, on every visual row so wrapped
/// text stays aligned under the first character.
const GUTTER: u16 = 2;
/// Visual rows the draft may occupy before it scrolls internally. A paste of
/// a hundred lines must not swallow the transcript.
const MAX_ROWS: usize = 10;

/// Where the draft's text and cursor land, in rows relative to the composer's
/// inner area.
#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    /// Visual rows, gutter included.
    pub rows: Vec<String>,
    /// Cursor row within `rows`.
    pub cursor_row: usize,
    /// Cursor column, gutter included.
    pub cursor_col: usize,
}

/// Total height of the composer block, borders included.
pub fn height(draft: &str, width: u16) -> u16 {
    let rows = wrap(draft, text_width(width)).len().clamp(1, MAX_ROWS);
    rows as u16 + 2
}

pub fn render(frame: &mut Frame, area: Rect, composer: &Composer) {
    let inner_width = area.width.saturating_sub(2);
    let inner_height = usize::from(area.height.saturating_sub(2));
    let layout = layout(composer.text(), composer.cursor(), inner_width);

    // Keep the cursor's row on screen: a long draft scrolls under it rather
    // than typing off the bottom edge.
    let first = layout
        .cursor_row
        .saturating_sub(inner_height.saturating_sub(1));
    let visible: Vec<String> = layout
        .rows
        .iter()
        .skip(first)
        .take(inner_height)
        .cloned()
        .collect();

    frame.render_widget(wrapped(visible.join("\n")).block(bordered("Message")), area);
    frame.set_cursor_position(Position::new(
        area.x + 1 + layout.cursor_col as u16,
        area.y + 1 + (layout.cursor_row - first) as u16,
    ));
}

/// Wrap the draft and locate the cursor within the wrapped rows.
pub fn layout(text: &str, cursor: usize, width: u16) -> Layout {
    let text_width = text_width(width);
    let rows = wrap(text, text_width);
    // Walk the same segmentation the wrap used, counting graphemes before the
    // cursor, so both agree on where a row ends.
    let before = &text[..cursor.min(text.len())];
    let mut cursor_row = 0;
    let mut cursor_col = 0;
    for line in before.split('\n') {
        let count = line.graphemes(true).count();
        // A cursor after exactly `text_width` graphemes belongs at the end of
        // that row, not at the start of a row the wrap never produced —
        // hence (count - 1), and the reserved column in `text_width`.
        let row = count.saturating_sub(1) / text_width;
        cursor_row += row + 1;
        cursor_col = count - row * text_width;
    }
    // The loop counts one row per logical line; the cursor sits on the last.
    let cursor_row = cursor_row.saturating_sub(1);
    Layout {
        rows: rows
            .iter()
            .enumerate()
            .map(|(index, row)| format!("{}{row}", if index == 0 { "› " } else { "  " }))
            .collect(),
        cursor_row,
        cursor_col: cursor_col + usize::from(GUTTER),
    }
}

/// Usable text columns: the gutter, and one column kept free so a cursor
/// sitting after a full row still paints inside the border.
fn text_width(width: u16) -> usize {
    usize::from(width.saturating_sub(GUTTER + 1)).max(1)
}

/// Hard-wrap each logical line at the available width, in graphemes. An empty
/// draft still occupies one row — that is where the cursor sits.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    for line in text.split('\n') {
        let clusters: Vec<&str> = line.graphemes(true).collect();
        if clusters.is_empty() {
            rows.push(String::new());
            continue;
        }
        for chunk in clusters.chunks(width) {
            rows.push(chunk.concat());
        }
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{height, layout as lay, MAX_ROWS};

    #[test]
    fn an_empty_draft_is_one_row_with_the_cursor_after_the_gutter() {
        let layout = lay("", 0, 20);
        assert_eq!(layout.rows, vec!["› ".to_string()]);
        assert_eq!((layout.cursor_row, layout.cursor_col), (0, 2));
        assert_eq!(height("", 20), 3, "one text row plus two borders");
    }

    #[test]
    fn the_cursor_follows_the_text_it_was_typed_after() {
        let layout = lay("hello", 5, 20);
        assert_eq!(layout.cursor_col, 7, "gutter (2) + five graphemes");
        assert_eq!(layout.cursor_row, 0);
        // Cursor moved left twice: it must not stay at the end.
        assert_eq!(lay("hello", 3, 20).cursor_col, 5);
    }

    #[test]
    fn a_multiline_draft_grows_the_box_and_moves_the_cursor_down() {
        let text = "one\ntwo\nthree";
        assert_eq!(height(text, 20), 5, "three rows plus borders");
        let layout = lay(text, text.len(), 20);
        assert_eq!(layout.cursor_row, 2);
        assert_eq!(layout.cursor_col, 7);
        assert_eq!(layout.rows[1], "  two", "continuation rows keep alignment");
    }

    #[test]
    fn long_lines_wrap_instead_of_clipping_silently() {
        // The old fixed 3-row box painted only the first visual row.
        let text = "abcdefghij";
        let layout = lay(text, text.len(), 7); // 4 usable columns
        assert_eq!(
            layout.rows,
            vec![
                "› abcd".to_string(),
                "  efgh".to_string(),
                "  ij".to_string()
            ]
        );
        assert_eq!((layout.cursor_row, layout.cursor_col), (2, 4));
        assert_eq!(height(text, 7), 5);
    }

    #[test]
    fn a_cursor_after_a_full_row_stays_on_that_row() {
        // Off-by-one here parks the cursor on a row the wrap never made.
        let layout = lay("abcd", 4, 7); // exactly one full row of 4
        assert_eq!(layout.rows, vec!["› abcd".to_string()]);
        assert_eq!((layout.cursor_row, layout.cursor_col), (0, 6));
    }

    #[test]
    fn a_huge_paste_stops_growing_and_scrolls_instead() {
        let text = "line\n".repeat(100);
        assert_eq!(height(&text, 20), MAX_ROWS as u16 + 2);
    }

    #[test]
    fn wrapping_counts_grapheme_clusters_not_bytes() {
        let text = "👍👍👍";
        let layout = lay(text, text.len(), 5); // 2 usable columns
        assert_eq!(layout.rows, vec!["› 👍👍".to_string(), "  👍".to_string()]);
    }
}
