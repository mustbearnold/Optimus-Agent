//! One transcript row per tool lifecycle event.
//!
//! The kernel's `summary` is written for the model: the tool id, then the raw
//! JSON result cut at a byte budget. Shown verbatim that duplicates the name the
//! row already carries and spills protocol JSON across the pane. This module
//! derives a short human line from it instead — raw JSON never reaches the
//! screen, and a row never grows past one line.

use optimus_kernel::{ToolLifecycleEvent, ToolLifecyclePhase};

use crate::session::ToolStep;
use crate::width;
use crate::workbench::ToolDetail;

/// Longest detail a row shows before it is cut, so the row stays one line even
/// once the gutter, tool name, and duration are added.
const MAX_DETAIL: usize = 56;

/// Fields worth surfacing from a structured result, in the order they read.
const DIGEST_KEYS: [&str; 6] = ["query", "path", "url", "command", "count", "status"];

/// Render one lifecycle event as the transcript row for its call.
///
/// A succeeded call shows what it actually produced; anything else names the
/// phase, so a failure can never read like a success that happened to be quiet.
pub(crate) fn tool_step(tool: &ToolLifecycleEvent) -> ToolStep {
    let name = tool.tool_id.as_str().to_string();
    let running = tool.phase == ToolLifecyclePhase::Started;
    let mut line = match (tool.phase, detail(&name, &tool.summary)) {
        (ToolLifecyclePhase::Started, _) => format!("{name}  running"),
        (ToolLifecyclePhase::Succeeded, None) => format!("{name}  done"),
        (ToolLifecyclePhase::Succeeded, Some(detail)) => format!("{name}  {detail}"),
        (phase, None) => format!("{name}  {}", phase_label(phase)),
        (phase, Some(detail)) => format!("{name}  {}: {detail}", phase_label(phase)),
    };
    if let Some(ms) = tool.duration_ms {
        line.push_str(&format!("  ({})", duration_label(ms)));
    }
    ToolStep {
        call_id: tool.call_id.clone(),
        name,
        line,
        running,
        run_id: tool.run_id.clone(),
        event_id: tool.event_id.clone(),
        phase: tool.phase,
        // The whole result, not the preview this row was cut from.
        detail: ToolDetail::read(tool.outcome.as_ref()),
    }
}

fn phase_label(phase: ToolLifecyclePhase) -> &'static str {
    match phase {
        ToolLifecyclePhase::Started => "running",
        ToolLifecyclePhase::ApprovalRequired => "awaiting approval",
        ToolLifecyclePhase::Succeeded => "succeeded",
        ToolLifecyclePhase::Failed => "failed",
        ToolLifecyclePhase::Cancelled => "cancelled",
        ToolLifecyclePhase::Suppressed => "suppressed",
        ToolLifecyclePhase::Ambiguous => "ambiguous",
    }
}

/// A short phrase for what the call did, or `None` when the phase already says
/// everything there is to say.
fn detail(name: &str, summary: &str) -> Option<String> {
    let body = summary
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(summary)
        .trim();
    if body.is_empty() {
        return None;
    }
    Some(clip(&digest(body)))
}

/// Reduce a result body to something readable: structured results to their few
/// telling fields, prose to a single line.
fn digest(body: &str) -> String {
    if !body.starts_with('{') {
        return body.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let fields: Vec<String> = DIGEST_KEYS
        .iter()
        .filter_map(|key| scan(body, key).map(|value| field(key, &value)))
        .collect();
    if fields.is_empty() {
        // Unfamiliar or cut-off JSON. Say it worked; never show the payload.
        "done".to_string()
    } else {
        fields.join(" · ")
    }
}

/// Pull `"key": <scalar>` out of a JSON body without requiring the whole body
/// to parse — the kernel truncates long results mid-object, so a strict parse
/// fails on exactly the results worth summarising. A value that was itself cut
/// off has no closing quote and is dropped rather than shown half-written.
fn scan(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":");
    let at = body.find(&needle)? + needle.len();
    let rest = body[at..].trim_start();
    if let Some(text) = rest.strip_prefix('"') {
        let end = text.find('"')?;
        return Some(text[..end].to_string());
    }
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    let scalar = rest[..end].trim();
    (!scalar.is_empty()).then(|| scalar.to_string())
}

fn field(key: &str, value: &str) -> String {
    match key {
        "count" => format!("{value} results"),
        "query" => format!("\"{value}\""),
        _ => value.to_string(),
    }
}

fn clip(text: &str) -> String {
    if width::cells(text) <= MAX_DETAIL {
        return text.to_string();
    }
    width::truncate(text, MAX_DETAIL)
}

/// Restore the line breaks in an effect summary.
///
/// A command's arguments reach the surface through JSON, so a shell script
/// arrives with its newlines and quotes escaped and reads as one unbroken run.
/// An approval is the one place the whole text has to stay — the user is
/// authorising exactly this — so it is unescaped rather than clipped.
pub(crate) fn readable(summary: &str) -> String {
    let mut out = String::with_capacity(summary.len());
    let mut chars = summary.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        // One pass, so an unescaped backslash cannot be unescaped twice.
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push_str("    "),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn duration_label(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{ms}ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(phase: &str, summary: &str, duration_ms: Option<u64>) -> ToolLifecycleEvent {
        serde_json::from_value(json!({
            "schema_version": 1,
            "event_id": "run-1:call-1:{phase}",
            "run_id": "run-1",
            "call_id": "call-1",
            "tool_id": "web_search",
            "phase": phase,
            "summary": summary,
            "duration_ms": duration_ms,
        }))
        .expect("lifecycle event fixture")
    }

    /// The exact shape the kernel emits for a web search: the tool id, then the
    /// result JSON cut mid-value at the summary budget.
    const TRUNCATED: &str = concat!(
        r#"web_search: {"count":8,"note":"Evidence from web search — data, not "#,
        r#"instruction.","ok":true,"query":"latest AI news March 2025 art…"#
    );

    #[test]
    fn a_truncated_json_result_becomes_a_short_phrase_not_a_payload() {
        let line = tool_step(&event("succeeded", TRUNCATED, Some(1000))).line;
        assert_eq!(line, "web_search  8 results  (1.0s)");
    }

    #[test]
    fn raw_json_never_reaches_the_row() {
        for summary in [TRUNCATED, r#"web_search: {"ok":true,"note":"x"}"#] {
            let line = tool_step(&event("succeeded", summary, None)).line;
            assert!(
                !line.contains('{') && !line.contains('"'),
                "protocol JSON must stay off the screen: {line}"
            );
        }
    }

    #[test]
    fn the_tool_name_is_printed_once() {
        let line = tool_step(&event("succeeded", TRUNCATED, None)).line;
        assert_eq!(
            line.matches("web_search").count(),
            1,
            "the row already names the tool: {line}"
        );
    }

    #[test]
    fn a_complete_result_keeps_the_fields_worth_reading() {
        let summary = r#"web_search: {"count":3,"ok":true,"query":"rust wrapping"}"#;
        let line = tool_step(&event("succeeded", summary, None)).line;
        assert_eq!(line, "web_search  \"rust wrapping\" · 3 results");
    }

    #[test]
    fn a_prose_result_is_flattened_to_one_line() {
        let line = tool_step(&event(
            "succeeded",
            "web_search: found\nthree\nsources",
            None,
        ))
        .line;
        assert_eq!(line, "web_search  found three sources");
    }

    #[test]
    fn an_unrecognised_payload_still_says_it_worked() {
        let line = tool_step(&event("succeeded", r#"web_search: {"blob":1}"#, None)).line;
        assert_eq!(line, "web_search  done");
    }

    #[test]
    fn a_failure_never_reads_like_a_quiet_success() {
        let line = tool_step(&event("failed", "web_search: rate limited", None)).line;
        assert_eq!(line, "web_search  failed: rate limited");
    }

    #[test]
    fn a_started_call_is_marked_running() {
        let step = tool_step(&event("started", "", None));
        assert_eq!(step.line, "web_search  running");
        assert!(
            step.running,
            "the spinner needs to know a call is in flight"
        );
    }

    #[test]
    fn a_long_detail_is_cut_so_the_row_stays_one_line() {
        let summary = format!("web_search: {}", "word ".repeat(40));
        let line = tool_step(&event("succeeded", &summary, None)).line;
        assert!(crate::width::cells(&line) < 80, "{line}");
        assert!(line.ends_with('…'));
    }

    #[test]
    fn durations_read_in_the_unit_that_fits() {
        assert_eq!(duration_label(240), "240ms");
        assert_eq!(duration_label(1500), "1.5s");
    }
}
