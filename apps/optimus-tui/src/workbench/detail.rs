//! What a tool call actually produced, read from its typed outcome.
//!
//! Phase 3 of ADR-0075. A tool row has always been one clipped line; the thing
//! a reader usually wants — what the command printed — was on the far side of
//! the kernel's `summary` budget and never reached the screen at all. The
//! kernel does carry it: `ToolLifecycleEvent::outcome` holds the whole
//! [`ToolOutcome`], whose `data` is the tool's own structured result parsed
//! back into JSON (`turn_loop.rs`). This module reads that result into typed
//! detail so a block can have a body to open.
//!
//! Read from the outcome, never from the rendered line. The summary is a
//! preview the kernel writes for the model and truncates at a byte budget;
//! deriving an exit code or a truncation flag from it would be exactly the
//! prose parsing the block contract prohibits.

use optimus_packs::ToolOutcome;

/// Output rows one block will hold. A command that printed a hundred thousand
/// lines must not become a hundred thousand rows of transcript to lay out on
/// every frame; what is dropped is said so, out loud, in [`CommandDetail`].
const MAX_KEPT_LINES: usize = 200;
/// A search result is evidence, not a browser dump. Keep enough titles to
/// identify what the call found while leaving the rest behind the fold.
const MAX_SEARCH_RESULTS: usize = 8;
/// Keep source rows readable in the terminal. The transcript wraps to its
/// current pane, but a bounded source row means a long tracking URL cannot
/// dominate several screens before wrapping even begins.
const MAX_SEARCH_TEXT_CELLS: usize = 64;

/// The typed body a block can open, when its tool produced one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToolDetail {
    /// Nothing this surface knows how to open. The one-line row is the whole
    /// truth for this call.
    #[default]
    None,
    Command(CommandDetail),
    WebSearch(SearchDetail),
}

impl ToolDetail {
    /// Read a call's outcome into typed detail.
    ///
    /// An outcome this surface has no reader for stays [`ToolDetail::None`] —
    /// an unopenable block is honest, and inventing a body from the preview
    /// would not be.
    pub fn read(outcome: Option<&ToolOutcome>) -> Self {
        let Some(outcome) = outcome else {
            return Self::None;
        };
        match CommandDetail::read(outcome) {
            Some(command) => Self::Command(command),
            None => SearchDetail::read(outcome)
                .map(Self::WebSearch)
                .unwrap_or(Self::None),
        }
    }

    /// Whether opening this block would show anything.
    pub fn has_body(&self) -> bool {
        match self {
            Self::None => false,
            Self::Command(command) => !command.body().is_empty(),
            Self::WebSearch(search) => !search.body().is_empty(),
        }
    }

    /// The rows opening this block shows.
    pub fn body(&self) -> Vec<String> {
        match self {
            Self::None => Vec::new(),
            Self::Command(command) => command.body(),
            Self::WebSearch(search) => search.body(),
        }
    }
}

/// The useful part of a `web_search` outcome once it is opened: source titles
/// and provenance URLs, kept under the call that produced them. Snippets are
/// deliberately not copied into the TUI body; they turn an expandable tool
/// call into a second answer and are where raw search noise starts to win.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDetail {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub omitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
}

impl SearchDetail {
    fn read(outcome: &ToolOutcome) -> Option<Self> {
        if outcome.tool_id.as_str() != "web_search" {
            return None;
        }
        let data = outcome.data.as_object()?;
        let raw_results = data.get("results")?.as_array()?;
        let mut results = Vec::new();
        for result in raw_results {
            let Some(result) = result.as_object() else {
                continue;
            };
            let title = result
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim();
            let url = result
                .get("provenance_url")
                .or_else(|| result.get("url"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim();
            if title.is_empty() && url.is_empty() {
                continue;
            }
            results.push(SearchResult {
                title: title.to_string(),
                url: url.to_string(),
            });
            if results.len() == MAX_SEARCH_RESULTS {
                break;
            }
        }
        if results.is_empty() {
            return None;
        }
        Some(Self {
            query: data
                .get("query")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            omitted: raw_results.len().saturating_sub(results.len()),
            results,
        })
    }

    fn body(&self) -> Vec<String> {
        let mut rows = Vec::new();
        if !self.query.is_empty() {
            rows.push(format!("query: {}", compact_search_text(&self.query)));
        }
        for (index, result) in self.results.iter().enumerate() {
            if !result.title.is_empty() {
                rows.push(format!(
                    "{}. {}",
                    index + 1,
                    compact_search_text(&result.title)
                ));
            }
            if !result.url.is_empty() {
                rows.push(format!("   {}", compact_search_text(&result.url)));
            }
        }
        if self.omitted > 0 {
            rows.push(format!("… {} more sources", self.omitted));
        }
        rows
    }
}

fn compact_search_text(value: &str) -> String {
    crate::width::truncate(value, MAX_SEARCH_TEXT_CELLS)
}

/// What a command did: its streams, how it left, and what was dropped on the
/// way here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandDetail {
    pub exit_code: Option<i32>,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    /// The runtime cut the stream before this surface ever saw it.
    pub truncated_stdout: bool,
    pub truncated_stderr: bool,
    /// Lines this surface itself dropped past [`MAX_KEPT_LINES`].
    pub dropped_stdout: usize,
    pub dropped_stderr: usize,
    pub timed_out: bool,
}

impl CommandDetail {
    /// Read a terminal result, or `None` when the outcome is not one.
    ///
    /// Keyed on the fields a command result actually has rather than on the
    /// tool id, so a pack that runs a command through a different tool gets the
    /// same block for free, and a tool that merely happens to be named like one
    /// does not.
    fn read(outcome: &ToolOutcome) -> Option<Self> {
        let data = outcome.data.as_object()?;
        // `exit_code` is the field only a command result carries; every
        // terminal outcome has it, null included (`tool_dispatch.rs`).
        if !data.contains_key("exit_code") {
            return None;
        }
        let text = |key: &str| {
            data.get(key)
                .and_then(|value| value.as_str())
                .unwrap_or_default()
        };
        let flag = |key: &str| {
            data.get(key)
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        };
        let (stdout, dropped_stdout) = keep(text("stdout"));
        let (stderr, dropped_stderr) = keep(text("stderr"));
        Some(Self {
            exit_code: data
                .get("exit_code")
                .and_then(|value| value.as_i64())
                .and_then(|value| i32::try_from(value).ok()),
            stdout,
            stderr,
            truncated_stdout: flag("truncated_stdout"),
            truncated_stderr: flag("truncated_stderr"),
            dropped_stdout,
            dropped_stderr,
            timed_out: flag("timed_out"),
        })
    }

    /// Whether the command left the way a caller would call success. `None` is
    /// not success: a command whose exit code never arrived is a command
    /// nothing can vouch for.
    pub fn succeeded(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// The rows opening this block shows: what it printed, what it complained
    /// about, and every place something was left out.
    ///
    /// Truncation is always stated. A tail with no notice reads as the whole
    /// output, and a reader who believes that will conclude the wrong thing
    /// about what the command did.
    pub fn body(&self) -> Vec<String> {
        let mut rows = Vec::new();
        if self.timed_out {
            rows.push("timed out before it finished".to_string());
        }
        push_stream(
            &mut rows,
            "",
            &self.stdout,
            self.dropped_stdout,
            self.truncated_stdout,
        );
        push_stream(
            &mut rows,
            "stderr: ",
            &self.stderr,
            self.dropped_stderr,
            self.truncated_stderr,
        );
        if rows.is_empty() && self.exit_code.is_some() {
            rows.push("no output".to_string());
        }
        rows
    }
}

/// Keep at most [`MAX_KEPT_LINES`] lines, reporting how many were dropped.
/// The *tail* is kept: a command that failed says why at the end.
fn keep(text: &str) -> (Vec<String>, usize) {
    if text.is_empty() {
        return (Vec::new(), 0);
    }
    let lines: Vec<&str> = text.lines().collect();
    let dropped = lines.len().saturating_sub(MAX_KEPT_LINES);
    (
        lines
            .iter()
            .skip(dropped)
            .map(|line| line.to_string())
            .collect(),
        dropped,
    )
}

fn push_stream(rows: &mut Vec<String>, prefix: &str, lines: &[String], dropped: usize, cut: bool) {
    if lines.is_empty() {
        return;
    }
    if dropped > 0 {
        rows.push(format!("… {dropped} earlier lines not shown"));
    }
    rows.extend(lines.iter().map(|line| format!("{prefix}{line}")));
    if cut {
        rows.push("… the runtime cut this stream at its capture limit".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optimus_packs::ReplayClass;
    use serde_json::json;

    fn command(data: serde_json::Value) -> ToolOutcome {
        ToolOutcome::succeeded("call-1", "terminal", "ran", data, ReplayClass::Ambiguous)
    }

    fn detail(data: serde_json::Value) -> CommandDetail {
        match ToolDetail::read(Some(&command(data))) {
            ToolDetail::Command(detail) => detail,
            other => panic!("expected a command: {other:?}"),
        }
    }

    #[test]
    fn a_command_result_is_read_from_the_outcome_not_the_preview() {
        let read = detail(json!({
            "ok": true,
            "stdout": "test result: ok. 47 passed\n",
            "stderr": "",
            "exit_code": 0,
            "truncated_stdout": false,
            "truncated_stderr": false,
            "timed_out": false,
        }));
        assert_eq!(read.exit_code, Some(0));
        assert!(read.succeeded());
        assert_eq!(read.stdout, vec!["test result: ok. 47 passed".to_string()]);
        assert_eq!(read.body(), vec!["test result: ok. 47 passed".to_string()]);
    }

    #[test]
    fn an_outcome_that_is_not_a_command_opens_onto_nothing() {
        let read = ToolDetail::read(Some(&ToolOutcome::succeeded(
            "call-1",
            "web_search",
            "found",
            json!({"count": 3, "ok": true}),
            ReplayClass::ExternalNondeterministic,
        )));
        assert_eq!(read, ToolDetail::None);
        assert!(!read.has_body());
        assert!(ToolDetail::read(None).body().is_empty());
    }

    #[test]
    fn a_web_search_opens_onto_compact_sources_under_its_call() {
        let read = ToolDetail::read(Some(&ToolOutcome::succeeded(
            "call-1",
            "web_search",
            "Found 2 sources",
            json!({
                "ok": true,
                "query": "AI news today",
                "count": 2,
                "results": [
                    {
                        "title": "A useful headline",
                        "url": "https://example.com/article",
                        "provenance_url": "https://example.com/article",
                        "snippet": "not copied into the TUI body"
                    },
                    {
                        "title": "Another headline",
                        "url": "https://example.com/another"
                    }
                ]
            }),
            ReplayClass::ExternalNondeterministic,
        )));
        let ToolDetail::WebSearch(search) = read else {
            panic!("expected search detail: {read:?}");
        };
        assert_eq!(search.query, "AI news today");
        assert_eq!(
            search.body(),
            vec![
                "query: AI news today",
                "1. A useful headline",
                "   https://example.com/article",
                "2. Another headline",
                "   https://example.com/another",
            ]
        );
        assert!(!search.body().join("\n").contains("not copied"));
    }

    #[test]
    fn long_search_text_fades_with_a_cell_safe_ellipsis() {
        let read = ToolDetail::read(Some(&ToolOutcome::succeeded(
            "call-1",
            "web_search",
            "Found 1 source",
            json!({
                "query": "q".repeat(MAX_SEARCH_TEXT_CELLS + 20),
                "results": [{
                    "title": format!("Headline {}", "t".repeat(MAX_SEARCH_TEXT_CELLS + 20)),
                    "provenance_url": format!(
                        "https://google.example/search?q={}",
                        "x".repeat(MAX_SEARCH_TEXT_CELLS + 40)
                    )
                }]
            }),
            ReplayClass::ExternalNondeterministic,
        )));
        let ToolDetail::WebSearch(search) = read else {
            panic!("expected search detail: {read:?}");
        };
        let body = search.body();
        assert!(body
            .iter()
            .any(|line| { line.starts_with("query: ") && line.ends_with('…') }));
        assert!(body
            .iter()
            .any(|line| line.starts_with("1. ") && line.ends_with('…')));
        assert!(body
            .iter()
            .any(|line| line.starts_with("   https://") && line.ends_with('…')));
        assert!(body
            .iter()
            .all(|line| crate::width::cells(line) <= MAX_SEARCH_TEXT_CELLS + 8));
    }

    #[test]
    fn a_search_body_caps_the_number_of_sources_it_opens() {
        let results = (0..MAX_SEARCH_RESULTS + 2)
            .map(|index| {
                json!({
                    "title": format!("Headline {index}"),
                    "url": format!("https://example.com/{index}"),
                })
            })
            .collect::<Vec<_>>();
        let read = ToolDetail::read(Some(&ToolOutcome::succeeded(
            "call-1",
            "web_search",
            "Found sources",
            json!({"ok": true, "results": results}),
            ReplayClass::ExternalNondeterministic,
        )));
        let ToolDetail::WebSearch(search) = read else {
            panic!("expected search detail: {read:?}");
        };
        assert_eq!(search.results.len(), MAX_SEARCH_RESULTS);
        assert_eq!(search.omitted, 2);
        assert!(search
            .body()
            .last()
            .is_some_and(|line| line.contains("2 more")));
    }

    #[test]
    fn a_failure_keeps_its_exit_code_and_its_complaint() {
        let read = detail(json!({
            "stdout": "",
            "stderr": "error: could not compile `optimus-tui`\n",
            "exit_code": 101,
            "timed_out": false,
        }));
        assert!(!read.succeeded());
        assert_eq!(read.exit_code, Some(101));
        assert_eq!(
            read.body(),
            vec!["stderr: error: could not compile `optimus-tui`".to_string()],
            "a failed command stays inspectable"
        );
    }

    #[test]
    fn an_exit_code_that_never_arrived_is_not_read_as_success() {
        let read = detail(json!({ "stdout": "", "stderr": "", "exit_code": null }));
        assert_eq!(read.exit_code, None);
        assert!(
            !read.succeeded(),
            "nothing vouched for this command; it must not read as passing"
        );
    }

    #[test]
    fn the_runtimes_own_truncation_is_stated_rather_than_hidden() {
        let read = detail(json!({
            "stdout": "one\ntwo\n",
            "stderr": "",
            "exit_code": 0,
            "truncated_stdout": true,
        }));
        assert_eq!(
            read.body().last().unwrap(),
            "… the runtime cut this stream at its capture limit"
        );
    }

    #[test]
    fn output_this_surface_drops_is_counted_and_the_tail_is_what_is_kept() {
        let flood = (0..MAX_KEPT_LINES + 50)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let read = detail(json!({
            "stdout": flood,
            "stderr": "",
            "exit_code": 0,
        }));
        assert_eq!(read.dropped_stdout, 50);
        assert_eq!(read.stdout.len(), MAX_KEPT_LINES);
        let body = read.body();
        assert_eq!(body[0], "… 50 earlier lines not shown");
        assert_eq!(
            body.last().unwrap(),
            &format!("line {}", MAX_KEPT_LINES + 49),
            "the tail is kept, because that is where a failure says why"
        );
    }

    #[test]
    fn a_timeout_is_the_first_thing_the_body_says() {
        let read = detail(json!({
            "stdout": "starting\n",
            "stderr": "",
            "exit_code": null,
            "timed_out": true,
        }));
        assert_eq!(read.body()[0], "timed out before it finished");
    }

    #[test]
    fn a_command_that_printed_nothing_says_so_rather_than_opening_onto_a_blank() {
        let read = detail(json!({ "stdout": "", "stderr": "", "exit_code": 0 }));
        assert_eq!(read.body(), vec!["no output".to_string()]);
        assert!(ToolDetail::Command(read).has_body());
    }

    #[test]
    fn both_streams_are_kept_and_stderr_is_marked_as_such() {
        let read = detail(json!({
            "stdout": "building\n",
            "stderr": "warning: unused import\n",
            "exit_code": 0,
        }));
        assert_eq!(
            read.body(),
            vec![
                "building".to_string(),
                "stderr: warning: unused import".to_string()
            ]
        );
    }
}
