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

/// The typed body a block can open, when its tool produced one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToolDetail {
    /// Nothing this surface knows how to open. The one-line row is the whole
    /// truth for this call.
    #[default]
    None,
    Command(CommandDetail),
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
            None => Self::None,
        }
    }

    /// Whether opening this block would show anything.
    pub fn has_body(&self) -> bool {
        match self {
            Self::None => false,
            Self::Command(command) => !command.body().is_empty(),
        }
    }

    /// The rows opening this block shows.
    pub fn body(&self) -> Vec<String> {
        match self {
            Self::None => Vec::new(),
            Self::Command(command) => command.body(),
        }
    }
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
