//! Painting the draft: wrapping, the prompt gutter, and where the terminal
//! cursor goes.
//!
//! The composer used to be one hard-coded row that silently clipped anything
//! longer, with no cursor drawn at all — you could not see where typing would
//! land. Height is derived from the draft here, and [`layout`] is a pure
//! function so both the wrap and the cursor position are assertable without a
//! terminal.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_segmentation::UnicodeSegmentation;

use crate::composer::Composer;
use crate::width;
use crate::wrapped;

use super::{ACCENT, COMPOSER_BACKGROUND, HAIRLINE, MUTED};

/// Columns reserved for the prompt gutter, on every visual row so wrapped
/// text stays aligned under the first character.
const GUTTER: u16 = 2;
/// Columns the block's border costs, one each side. Both [`height`] and
/// [`layout`] take the block's *outer* width and subtract this themselves —
/// the two disagreeing about whose width they were handed is precisely how the
/// box ended up sized for one row while the wrap produced two.
const BORDER: u16 = 2;
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

/// Total height of the composer block, borders included. `width` is the
/// block's outer width, the same one [`layout`] takes.
pub fn height(draft: &str, width: u16) -> u16 {
    let rows = wrap(draft, text_width(width)).len().clamp(1, MAX_ROWS);
    rows as u16 + 2
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    title: Option<&str>,
    provider: &str,
) {
    let inner_height = usize::from(area.height.saturating_sub(2));
    let layout = layout(composer.text(), composer.cursor(), area.width);

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

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(HAIRLINE))
        .style(Style::default().bg(COMPOSER_BACKGROUND));
    if let Some(title) = title {
        block = block
            .title(width::truncate(
                title,
                usize::from(area.width.saturating_sub(2)),
            ))
            .title_style(Style::default().fg(ACCENT));
    }
    let provider = width::truncate(provider, usize::from(area.width.saturating_sub(2)));
    block = block.title_bottom(
        Line::from(format!(" {provider} "))
            .right_aligned()
            .style(Style::default().fg(MUTED)),
    );
    let paragraph = if composer.text().is_empty() && title.is_none() {
        Paragraph::new(Line::from(vec![
            Span::styled("› ", Style::default().fg(ACCENT)),
            Span::styled(
                "Ask Optimus anything…",
                Style::default().fg(MUTED).add_modifier(Modifier::DIM),
            ),
        ]))
    } else {
        wrapped(visible.join("\n"))
    };
    frame.render_widget(paragraph.block(block), area);
    if area.width > 0 && area.height > 0 {
        let cursor_x = area
            .x
            .saturating_add(1)
            .saturating_add(layout.cursor_col as u16)
            .min(area.x + area.width - 1);
        let cursor_y = area
            .y
            .saturating_add(1)
            .saturating_add((layout.cursor_row - first) as u16)
            .min(area.y + area.height - 1);
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

/// Wrap the draft and locate the cursor within the wrapped rows. `width` is
/// the block's outer width.
pub fn layout(text: &str, cursor: usize, width: u16) -> Layout {
    let text_width = text_width(width);
    let rows = wrap(text, text_width);
    let gutter_width = gutter_width(width);
    if text_width == 0 {
        return Layout {
            rows: vec![width::take("› ", gutter_width)],
            cursor_row: 0,
            cursor_col: gutter_width,
        };
    }
    // Walk the same grapheme segmentation the wrap used, counting terminal
    // cells before the cursor, so both agree on where a row ends.
    let mut cursor = cursor.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let before = &text[..cursor];
    let mut cursor_row = 0;
    let mut cursor_col = 0;
    for grapheme in before.graphemes(true) {
        if grapheme == "\n" {
            cursor_row += 1;
            cursor_col = 0;
            continue;
        }
        let display = width::fit_grapheme(grapheme, text_width);
        let display_width = width::cells(&display);
        if width::grapheme_cells(grapheme) > text_width {
            if cursor_col > 0 {
                cursor_row += 1;
            }
            cursor_col = display_width.min(text_width);
        } else if cursor_col > 0 && cursor_col + display_width > text_width {
            cursor_row += 1;
            cursor_col = display_width;
        } else {
            cursor_col += display_width;
        }
    }
    Layout {
        rows: rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let gutter = if index == 0 { "› " } else { "  " };
                format!("{}{}", width::take(gutter, gutter_width), row)
            })
            .collect(),
        cursor_row,
        cursor_col: cursor_col + gutter_width,
    }
}

/// Usable text columns inside a block `width` columns wide: the border, the
/// gutter, and one column kept free so a cursor sitting after a full row still
/// paints inside the border.
fn text_width(width: u16) -> usize {
    let inner = usize::from(width.saturating_sub(BORDER));
    inner.saturating_sub(gutter_width(width) + 1)
}

fn gutter_width(width: u16) -> usize {
    usize::from(width.saturating_sub(BORDER)).min(usize::from(GUTTER))
}

/// Hard-wrap each logical line at the available terminal-cell width. An empty
/// draft still occupies one row — that is where the cursor sits.
fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let width = width.max(1);
    let mut rows = Vec::new();
    for line in text.split('\n') {
        if line.is_empty() {
            rows.push(String::new());
            continue;
        }
        let mut row = String::new();
        let mut used = 0;
        for grapheme in line.graphemes(true) {
            let grapheme_width = width::grapheme_cells(grapheme);
            if grapheme_width > width {
                if !row.is_empty() {
                    rows.push(std::mem::take(&mut row));
                    used = 0;
                }
                let fitted = width::fit_grapheme(grapheme, width);
                if !fitted.is_empty() {
                    rows.push(fitted);
                }
            } else if used > 0 && used + grapheme_width > width {
                rows.push(std::mem::take(&mut row));
                row.push_str(grapheme);
                used = grapheme_width;
            } else {
                row.push_str(grapheme);
                used += grapheme_width;
            }
        }
        if !row.is_empty() {
            rows.push(row);
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
        let layout = lay(text, text.len(), 9); // 4 usable columns
        assert_eq!(
            layout.rows,
            vec![
                "› abcd".to_string(),
                "  efgh".to_string(),
                "  ij".to_string()
            ]
        );
        assert_eq!((layout.cursor_row, layout.cursor_col), (2, 4));
        assert_eq!(height(text, 9), 5);
    }

    #[test]
    fn a_cursor_after_a_full_row_stays_on_that_row() {
        // Off-by-one here parks the cursor on a row the wrap never made.
        let layout = lay("abcd", 4, 9); // exactly one full row of 4
        assert_eq!(layout.rows, vec!["› abcd".to_string()]);
        assert_eq!((layout.cursor_row, layout.cursor_col), (0, 6));
    }

    /// The regression test for the defect this convention change fixes:
    /// `height` was handed the block's outer width by `view::draw` while
    /// `render` handed `layout` the inner width, so at 100 columns a 96-grapheme
    /// draft wrapped to two rows inside a box sized for one, and the first row
    /// scrolled out from under the person typing it.
    #[test]
    fn height_and_layout_agree_on_how_many_rows_a_draft_needs() {
        for width in [9_u16, 20, 40, 100] {
            for len in [1_usize, 4, 5, 15, 16, 34, 35, 36, 95, 96, 97] {
                let text = "a".repeat(len);
                let rows = lay(&text, text.len(), width).rows.len().min(MAX_ROWS);
                assert_eq!(
                    usize::from(height(&text, width)),
                    rows + 2,
                    "width {width}, {len} graphemes"
                );
            }
        }
    }

    #[test]
    fn a_huge_paste_stops_growing_and_scrolls_instead() {
        let text = "line\n".repeat(100);
        assert_eq!(height(&text, 20), MAX_ROWS as u16 + 2);
    }

    #[test]
    fn wrapping_counts_terminal_cells_without_splitting_graphemes() {
        let text = "👍👍👍";
        let layout = lay(text, text.len(), 7); // 2 usable columns
        assert_eq!(
            layout.rows,
            vec!["› 👍".to_string(), "  👍".to_string(), "  👍".to_string()]
        );
    }

    #[test]
    fn every_composer_row_stays_inside_its_outer_width() {
        for width in 0_u16..=24 {
            let text = "界👍e\u{301}ｶ";
            let layout = lay(text, text.len(), width);
            assert!(
                layout
                    .rows
                    .iter()
                    .all(|row| crate::width::cells(row) <= usize::from(width)),
                "composer overflowed {width}: {:?}",
                layout.rows
            );
        }
    }
}
