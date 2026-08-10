//! How a tool call, its outcome and its failures are described.
//!
//! Split out of `lib.rs` under the module-size law. These are the functions
//! that turn what happened into something a human or a model reads: the
//! approval summary, the transcript preview, the timing and lifecycle events,
//! and the mapping from a kernel error to a reported one.

use super::*;

pub(crate) fn exact_action_summary(effect_json: &str) -> String {
    match serde_json::from_str::<Effect>(effect_json) {
        Ok(Effect::ProjectWriteFile {
            relative_path,
            contents,
            ..
        })
        | Ok(Effect::WriteFile {
            relative_path,
            contents,
        }) => format!("Write {relative_path} ({} bytes)", contents.len()),
        Ok(Effect::ProjectMkdir { relative_path, .. }) | Ok(Effect::Mkdir { relative_path }) => {
            format!("Create directory {relative_path}")
        }
        Ok(Effect::ProjectDeletePath { relative_path, .. })
        | Ok(Effect::DeletePath { relative_path }) => {
            format!("Delete {relative_path}")
        }
        Ok(Effect::ProjectRenamePath {
            from_relative_path,
            to_relative_path,
            ..
        })
        | Ok(Effect::RenamePath {
            from_relative_path,
            to_relative_path,
        }) => format!("Rename {from_relative_path} → {to_relative_path}"),
        Ok(Effect::ProjectPatchFile { relative_path, .. })
        | Ok(Effect::PatchFile { relative_path, .. }) => format!("Patch {relative_path}"),
        Ok(Effect::ProjectRunCommand { program, args, .. })
        | Ok(Effect::RunCommand { program, args }) => {
            let program = serde_json::to_string(&program).unwrap_or_else(|_| "<invalid>".into());
            let args = serde_json::to_string(&args).unwrap_or_else(|_| "<invalid>".into());
            format!("Run {program} with args {args}")
        }
        Ok(Effect::ProjectServe {
            program,
            args,
            port,
            ttl_seconds,
            ..
        }) => {
            // The port and the TTL are named because they are what the approval
            // actually buys: a lease letting this agent reach that listener for
            // that long (ADR-0060), not merely a process starting.
            let program = serde_json::to_string(&program).unwrap_or_else(|_| "<invalid>".into());
            let args = serde_json::to_string(&args).unwrap_or_else(|_| "<invalid>".into());
            format!("Serve {program} with args {args} on 127.0.0.1:{port} for up to {ttl_seconds}s")
        }
        Ok(Effect::AssertFileEquals { relative_path, .. }) => {
            format!("Verify {relative_path}")
        }
        Err(_) => "Exact action requires approval".into(),
    }
}

/// The `CommandClass` discriminator behind a pending command effect, when the
/// exact effect is a command (spec-014 R7, ADR-0081).
///
/// The approval card labels "Always allow <class> in this project (this
/// session)" from this value; it is derived here, from the exact effect the
/// runtime settled, never from free text. Non-command effects yield `None` —
/// session consent is command-class-scoped only.
pub(crate) fn command_class_of_effect(effect_json: &str) -> Option<String> {
    let effect = serde_json::from_str::<Effect>(effect_json).ok()?;
    let (program, args) = match effect {
        Effect::ProjectRunCommand { program, args, .. }
        | Effect::RunCommand { program, args }
        | Effect::ProjectServe { program, args, .. } => (program, args),
        _ => return None,
    };
    Some(
        optimus_policy::classify_command(&program, &args)
            .as_str()
            .to_string(),
    )
}

pub(crate) fn timing_event(
    kind: TimingEventKind,
    turn_started: std::time::Instant,
    duration_ms: Option<u64>,
    step: Option<u32>,
    call: Option<&ToolCall>,
) -> TimingEvent {
    TimingEvent {
        kind,
        step,
        call_id: call.map(|value| value.id.clone()),
        name: call.map(|value| value.name.clone()),
        duration_ms,
        elapsed_ms: turn_loop::elapsed_ms(turn_started),
        status: None,
        suppressed: false,
    }
}

pub(crate) fn evidence_tool_signature(call: &ToolCall) -> Option<String> {
    if !matches!(
        call.name.as_str(),
        "web_search" | "memory_recall" | "skill_resolve"
    ) {
        return None;
    }
    let arguments = canonical_json(&call.arguments);
    serde_json::to_string(&arguments)
        .ok()
        .map(|value| format!("{}:{value}", call.name))
}

pub(crate) fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            let mut normalized = serde_json::Map::new();
            for key in keys {
                normalized.insert(key.clone(), canonical_json(&values[key]));
            }
            Value::Object(normalized)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

/// A one-line preview of a tool result, for the transcript row.
///
/// Bounded in chars, not bytes. Results carry arbitrary UTF-8 — a trending
/// repository whose description was `Node.js JavaScript runtime ✨🐢🚀✨` is
/// enough — and slicing at a byte index that lands inside a multi-byte character
/// panics, taking the turn with it.
///
/// The ellipsis is qualified because an unqualified one is read as a claim about
/// the data. Observed: a page that arrived complete, every repository present
/// and no truncation flag set, was reported to the user as "partially truncated
/// by the retrieval tool" — the model had taken the summary's `…` as evidence
/// about the result rather than about the preview. Whether a result is complete
/// is something the tool states in the data; a preview must not answer it too.
pub(crate) fn summarize(s: &str) -> String {
    const MAX: usize = 120;
    // A JSON result's identity lives in a few small fields; a raw prefix
    // buries them the moment any array (elements, matches, entries) grows.
    // Headline those fields so the 120-char trace names what happened; the
    // full payload always rides in `data`.
    if let Some(headline) = json_headline(s) {
        return truncate_preview(&headline, MAX);
    }
    truncate_preview(s, MAX)
}

fn json_headline(s: &str) -> Option<String> {
    let value: Value = serde_json::from_str(s).ok()?;
    let object = value.as_object()?;
    let mut parts = Vec::new();
    for key in [
        "ok",
        "final_url",
        "page_title",
        "path",
        "status",
        "exit_code",
        "count",
        "element_count",
        "found",
        "stdout",
        "text",
        "body_text",
    ] {
        match object.get(key) {
            Some(Value::String(text)) if !text.is_empty() => {
                parts.push(format!("{key}={text}"));
            }
            Some(field) if field.is_number() || field.is_boolean() => {
                parts.push(format!("{key}={field}"));
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn truncate_preview(s: &str, max: usize) -> String {
    let total = s.chars().count();
    if total <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}… [preview truncated at {max} of {total} chars; the full result is in `data`]")
}

pub(crate) fn tool_lifecycle_event(
    run_id: Uuid,
    call: &ToolCall,
    tool_id: ToolId,
    phase: ToolLifecyclePhase,
    summary: impl Into<String>,
    duration_ms: Option<u64>,
    outcome: Option<ToolOutcome>,
) -> ToolLifecycleEvent {
    let run_id = run_id.to_string();
    ToolLifecycleEvent {
        schema_version: TOOL_LIFECYCLE_SCHEMA_VERSION,
        event_id: format!("{run_id}:{}:{}", call.id, phase.as_str()),
        run_id,
        call_id: call.id.clone(),
        tool_id,
        phase,
        summary: summary.into(),
        duration_ms,
        outcome,
        approval: None,
    }
}

pub(crate) fn is_control_plane_tool_error(error: &KernelError) -> bool {
    match error {
        KernelError::Tool(message)
            if message.contains("path not allowed") || message.contains("secret path denied") =>
        {
            true
        }
        // Pack budget / catalog failures must surface as typed ToolOutcomes, not turn aborts.
        KernelError::Packs(
            PackError::SchemaBudget { .. }
            | PackError::PackLimit { .. }
            | PackError::CorePinned
            | PackError::UnknownPack(_)
            | PackError::NotLoaded(_)
            | PackError::ToolUnavailable(_)
            | PackError::ToolNotLoaded { .. }
            | PackError::UnknownTool(_)
            | PackError::InvalidArguments { .. }
            | PackError::SchemaTokenOverflow { .. },
        ) => false,
        KernelError::Packs(_) => true,
        _ => false,
    }
}

pub(crate) fn pack_error_tool_outcome(
    call: &ToolCall,
    descriptor: &optimus_packs::ToolDesc,
    error: &PackError,
) -> ToolOutcome {
    ToolOutcome::failed(
        call.id.clone(),
        descriptor.id.clone(),
        format!("{}: {error}", descriptor.id.as_str()),
        ToolErrorDetail {
            code: error.outcome_error_code().into(),
            message: error.to_string(),
            retryable: error.outcome_retryable(),
        },
        descriptor.replay,
    )
}

/// Builds a failed `ToolOutcome` that carries the real dispatch error instead
/// of a generic placeholder — the catch-all in `turn_loop::run_turn_loop` for
/// every tool-execution error that is not a pack error, a control-plane
/// denial, or a pending approval (those have their own typed handling).
/// Mirrors `pack_error_tool_outcome` above: the code stays the fixed
/// `tool_execution_failed` taxonomy, only the message gets richer.
pub(crate) fn dispatch_error_tool_outcome(
    call: &ToolCall,
    descriptor: &optimus_packs::ToolDesc,
    error: &KernelError,
) -> ToolOutcome {
    // Comfortably under `ToolOutcome::validate`'s hard 4096-byte cap (in
    // optimus_packs) even after the "{tool_id}: " prefix `summary` adds and
    // the truncation suffix below — a verbose underlying error must shrink a
    // single tool's failure, never grow it into a turn-aborting validation error.
    const MAX_ERROR_BYTES: usize = 2000;
    let cause = truncate_error_detail(&error.to_string(), MAX_ERROR_BYTES);
    ToolOutcome::failed(
        call.id.clone(),
        descriptor.id.clone(),
        format!("{}: {cause}", descriptor.id.as_str()),
        ToolErrorDetail {
            code: "tool_execution_failed".into(),
            message: cause,
            retryable: false,
        },
        descriptor.replay,
    )
}

/// Truncates on a UTF-8 char boundary, like `truncate_preview` above, but for
/// error text rather than a result preview — and without implying the
/// missing tail lives anywhere, since a failed tool call has no `data` to
/// point to.
fn truncate_error_detail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!(
        "{}… [error truncated at {boundary} of {} bytes]",
        &s[..boundary],
        s.len()
    )
}

pub(crate) fn kernel_error_code(error: &KernelError) -> &'static str {
    security_denial::kernel_or_security_code(error)
}

#[cfg(test)]
mod summarize_tests {
    use super::summarize;

    #[test]
    fn a_multibyte_character_on_the_boundary_does_not_take_the_turn_down() {
        // Byte-sliced, this panicked: the 120th byte lands inside an emoji.
        // github.com/trending carries several, `nodejs/node` among them.
        let result = format!("{}✨🐢🚀✨ trailing", "a".repeat(118));
        let summary = summarize(&result);
        assert!(summary.starts_with("aaa"));
        assert!(
            summary.contains(&format!("of {} chars", result.chars().count())),
            "{summary}"
        );
    }

    #[test]
    fn the_preview_says_it_is_the_preview_that_stopped() {
        // Observed: a complete page reported to the user as "partially truncated
        // by the retrieval tool". The model read a bare `…` on the summary as a
        // statement about the data.
        let summary = summarize(&"x".repeat(500));
        assert!(summary.contains("preview truncated"), "{summary}");
        assert!(
            summary.contains("in `data`"),
            "the preview must point at the thing that is complete"
        );
    }

    #[test]
    fn a_result_that_fits_is_its_own_summary_with_nothing_added() {
        assert_eq!(summarize("Found 3 sources"), "Found 3 sources");
    }
}

#[cfg(test)]
mod truncate_error_detail_tests {
    use super::truncate_error_detail;

    #[test]
    fn a_short_error_passes_through_unchanged() {
        let cause = "ssrf blocked: ip 127.0.0.1";
        assert_eq!(truncate_error_detail(cause, 2000), cause);
    }

    #[test]
    fn a_multibyte_character_on_the_truncation_boundary_does_not_panic() {
        // Same failure mode as summarize's boundary test: the cut point must
        // never land inside a multi-byte character's encoding. The kept
        // prefix must respect the byte budget even though the explanatory
        // suffix this function appends does not — that suffix is what makes
        // a truncated cause legible, so it is exempt by design.
        let cause = format!("{}✨🐢🚀✨ trailing", "a".repeat(18));
        let truncated = truncate_error_detail(&cause, 20);
        assert!(truncated.starts_with("aaaaaaaaaaaaaaaaaa"));
        assert!(truncated.contains("error truncated"), "{truncated}");
    }

    #[test]
    fn a_long_cause_is_actually_shortened_once_the_budget_dominates_the_suffix() {
        // At the real budget (2000 bytes) the explanatory suffix is noise
        // against a genuinely oversized cause, so truncation must shrink it.
        let cause = "e".repeat(10_000);
        let truncated = truncate_error_detail(&cause, 2000);
        assert!(truncated.len() < cause.len(), "{}", truncated.len());
        assert!(truncated.contains("error truncated at 2000 of 10000 bytes"));
    }
}
