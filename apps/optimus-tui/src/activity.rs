//! Terminal-cell-aware rendering policy for the live worker activity row.

use crate::width;

/// Fit the live activity row deliberately instead of letting the terminal cut
/// the interrupt hint mid-word. Elapsed time is least important, followed by
/// status detail; the spinner and an actionable Ctrl-C cue survive longest.
pub(crate) fn text(spinner: char, what: &str, elapsed_secs: u64, width: usize) -> String {
    // Status is a transport boundary. Keep a malformed or future provider
    // message from injecting a newline/tab into a row the layout treats as
    // one terminal line; whitespace between words remains readable.
    let normalized = what.split_whitespace().collect::<Vec<_>>().join(" ");
    let what = if normalized.is_empty() {
        "working"
    } else {
        normalized.as_str()
    };
    let full = format!("{spinner} {what}… {elapsed_secs}s · Ctrl-C to interrupt");
    if fits(&full, width) {
        return full;
    }

    let without_elapsed = format!("{spinner} {what}… · Ctrl-C to interrupt");
    if fits(&without_elapsed, width) {
        return without_elapsed;
    }

    let prefix = format!("{spinner} ");
    let suffix = " · Ctrl-C to interrupt";
    let fixed = width::cells(&prefix) + width::cells(suffix);
    if width > fixed {
        let detail_width = width - fixed;
        let ellipsis_width = width::grapheme_cells("…");
        if detail_width > ellipsis_width {
            let detail = width::take(what, detail_width - ellipsis_width);
            return format!("{prefix}{}…{suffix}", detail.trim_end());
        }
    }

    for fallback in [
        format!("{spinner}{suffix}"),
        format!("{spinner} · Ctrl-C"),
        "Ctrl-C".to_string(),
        spinner.to_string(),
    ] {
        if fits(&fallback, width) {
            return fallback;
        }
    }
    String::new()
}

fn fits(text: &str, width: usize) -> bool {
    width::cells(text) <= width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_is_dropped_in_priority_order_without_clipping_the_hint() {
        assert_eq!(
            text('⠼', "working", 7, 38),
            "⠼ working… 7s · Ctrl-C to interrupt"
        );
        assert_eq!(
            text('⠼', "model step 1", 7, 36),
            "⠼ model step… · Ctrl-C to interrupt"
        );
        assert_eq!(text('⠼', "model step 1", 7, 24), "⠼ · Ctrl-C to interrupt");
        assert_eq!(text('⠼', "model step 1", 7, 10), "⠼ · Ctrl-C");
        assert_eq!(text('⠼', "model step 1", 7, 6), "Ctrl-C");
    }

    #[test]
    fn wide_status_text_is_measured_in_terminal_cells() {
        let rendered = text('⠼', "解析作業を実行中", 7, 30);
        assert!(width::cells(&rendered) <= 30, "{rendered}");
        assert!(rendered.contains("Ctrl-C"), "{rendered}");
    }

    #[test]
    fn status_text_cannot_inject_extra_terminal_rows() {
        let rendered = text('⠼', "searching\nprimary\t sources", 7, 80);
        assert!(!rendered.contains('\n'), "{rendered:?}");
        assert!(!rendered.contains('\t'), "{rendered:?}");
        assert!(rendered.contains("searching primary sources"), "{rendered}");
        assert!(width::cells(&rendered) <= 80, "{rendered}");
    }
}
