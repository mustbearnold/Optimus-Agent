//! Terminal-cell measurements shared by every layout path.
//!
//! Rust string lengths, character counts, and grapheme counts are all useful
//! for different jobs, but none of them is the width a terminal paints. The
//! workbench treats this module as its small geometry boundary: callers may
//! split only between grapheme clusters and may never emit more cells than the
//! rectangle they were given.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Number of terminal cells occupied by `text`.
pub(crate) fn cells(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Number of cells occupied by one grapheme cluster.
pub(crate) fn grapheme_cells(grapheme: &str) -> usize {
    UnicodeWidthStr::width(grapheme)
}

/// Keep complete grapheme clusters while fitting a string into `limit` cells.
///
/// A cluster that cannot fit is left out rather than split. This matters for
/// combining marks and emoji sequences, and also makes it impossible for a
/// renderer to create a dangling half-glyph at the edge of a pane.
pub(crate) fn take(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut used: usize = 0;
    for grapheme in text.graphemes(true) {
        let width = grapheme_cells(grapheme);
        if used.saturating_add(width) > limit {
            break;
        }
        out.push_str(grapheme);
        used += width;
    }
    out
}

/// Keep the last complete grapheme clusters that fit in `limit` cells.
pub(crate) fn take_end(text: &str, limit: usize) -> String {
    let mut picked = Vec::new();
    let mut used: usize = 0;
    for grapheme in text.graphemes(true).rev() {
        let width = grapheme_cells(grapheme);
        if used.saturating_add(width) > limit {
            break;
        }
        picked.push(grapheme);
        used += width;
    }
    picked.into_iter().rev().collect()
}

/// Truncate at a cell boundary, preserving the end marker where possible.
pub(crate) fn truncate(text: &str, limit: usize) -> String {
    if cells(text) <= limit {
        return text.to_owned();
    }
    if limit == 0 {
        return String::new();
    }
    let ellipsis = "…";
    let ellipsis_width = grapheme_cells(ellipsis);
    if limit <= ellipsis_width {
        return take(ellipsis, limit);
    }
    let mut out = take(text, limit - ellipsis_width);
    out.push_str(ellipsis);
    out
}

/// A display-safe representation of a single grapheme for a narrow row.
///
/// A double-width cluster cannot physically fit in a one-cell row. The
/// ellipsis is the least surprising visible indication that it was elided.
pub(crate) fn fit_grapheme(grapheme: &str, limit: usize) -> String {
    if grapheme_cells(grapheme) <= limit {
        grapheme.to_owned()
    } else {
        truncate(grapheme, limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_terminal_cells_not_rust_string_units() {
        assert_eq!(cells("界"), 2);
        assert_eq!(cells("👍"), 2);
        assert_eq!(cells("e\u{301}"), 1);
        assert_eq!(cells("ｶ"), 1);
    }

    #[test]
    fn truncation_never_splits_a_grapheme_or_exceeds_the_limit() {
        for limit in 0..=8 {
            let clipped = truncate("界 e\u{301} 👍", limit);
            assert!(cells(&clipped) <= limit, "{clipped:?} > {limit}");
        }
        assert_eq!(take("e\u{301}界", 1), "e\u{301}");
        assert_eq!(take("e\u{301}界", 2), "e\u{301}");
    }
}
