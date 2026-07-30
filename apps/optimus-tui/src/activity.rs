//! Width-aware rendering policy for the live worker activity row.

/// Fit the live activity row deliberately instead of letting the terminal cut
/// the interrupt hint mid-word. Elapsed time is least important, followed by
/// status detail; the spinner and an actionable Ctrl-C cue survive longest.
pub(crate) fn text(spinner: char, what: &str, elapsed_secs: u64, width: usize) -> String {
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
    let fixed = prefix.chars().count() + suffix.chars().count();
    if width > fixed + 1 {
        let detail_width = width - fixed;
        let detail: String = what
            .chars()
            .take(detail_width - 1)
            .collect::<String>()
            .trim_end()
            .into();
        return format!("{prefix}{detail}…{suffix}");
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
    text.chars().count() <= width
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
}
