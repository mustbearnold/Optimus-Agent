//! Local causal reconstruction of a turn from durable stores (not logs).
//!
//! Phase 5: given a trace id, manifest id, or turn id under an Optimus home,
//! assemble one reconstructible report from `execution.db` (and optional
//! session effect links). TraceStore remains optional offline evidence;
//! production turns bind identity in `execution_trace_links`.
//!
//! P14: versioned **local causal export** (JSON) with path redaction — not OTLP.
//! See ADR-0037.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ExecutionManifest, ExecutionModelCallSummary, ExecutionStore, ExecutionTimingSummary,
    ExecutionToolCallSummary, ExecutionToolLifecycleSummary, KernelError, ReplayReport, Result,
    SecurityDenialCode, SessionEffectLink, SessionStore, TraceContext, TraceId,
};

/// Version of the machine-readable causal export envelope (P14).
pub const CAUSAL_EXPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CausalQueryKind {
    TraceId,
    ManifestId,
    TurnId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CausalQuery {
    pub kind: CausalQueryKind,
    pub id: String,
}

/// Store-backed reconstruction of one turn's causal chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CausalTurnReport {
    pub home: String,
    pub query: CausalQuery,
    pub trace_context: Option<TraceContext>,
    pub manifest: ExecutionManifest,
    pub timings: ExecutionTimingSummary,
    pub replay: ReplayReport,
    pub model_calls: Vec<ExecutionModelCallSummary>,
    pub tool_calls: Vec<ExecutionToolCallSummary>,
    pub tool_lifecycle: Vec<ExecutionToolLifecycleSummary>,
    pub session_effect_links: Vec<SessionEffectLink>,
    /// True when every tool call that claims durable provenance has a matching
    /// session effect link (or no durable tools ran).
    pub effect_transcript_consistent: bool,
    /// Classified security/policy denials visible from durable tool outcomes
    /// (best-effort; empty when no classifiable fence fired).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_denials: Vec<String>,
}

/// Versioned, redacted causal export for operators and offline analysis (P14).
///
/// **Local-only S+++:** deterministic JSON of the store-backed causal graph.
/// Not OTLP/OpenTelemetry wire format; no network exporter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CausalExportDocument {
    pub export_version: u32,
    pub format: String,
    /// Always true for this format — reconstruction does not require stderr logs.
    pub store_backed: bool,
    /// Fixture/live-provider honesty: export never re-runs providers.
    pub live_provider_replay: bool,
    pub report: CausalTurnReport,
}

/// Load a causal turn report from durable execution (+ session) stores.
pub fn load_causal_turn(home: impl AsRef<Path>, query: CausalQuery) -> Result<CausalTurnReport> {
    let home = home.as_ref();
    let executions = ExecutionStore::open(home.join("execution.db"))?;
    let manifest_id = resolve_manifest_id(&executions, &query)?;
    let manifest = executions.manifest(manifest_id)?;
    let trace_context = executions.trace_context(manifest_id)?;
    let timings = executions.timing_summary(manifest_id)?;
    let replay = executions.replay_report(manifest_id)?;
    let model_calls = executions.list_model_calls(manifest_id)?;
    let tool_calls = executions.list_tool_calls(manifest_id)?;
    let tool_lifecycle = executions.list_tool_lifecycle_phases(manifest_id)?;

    let mut session_effect_links = Vec::new();
    let sessions_path = home.join("sessions.db");
    if sessions_path.exists() {
        let sessions = SessionStore::open(&sessions_path)?;
        if let Ok(links) = sessions.effect_links(manifest.session_id) {
            session_effect_links = links;
        }
    }

    let effect_transcript_consistent =
        effect_links_cover_tool_calls(&tool_calls, &session_effect_links);
    let security_denials = security_denials_from_lifecycle(&tool_lifecycle);

    Ok(CausalTurnReport {
        home: home.display().to_string(),
        query,
        trace_context,
        manifest,
        timings,
        replay,
        model_calls,
        tool_calls,
        tool_lifecycle,
        session_effect_links,
        effect_transcript_consistent,
        security_denials,
    })
}

/// Build a versioned export document with absolute home paths redacted.
pub fn export_causal_document(home: impl AsRef<Path>, query: CausalQuery) -> Result<CausalExportDocument> {
    let home = home.as_ref();
    let mut report = load_causal_turn(home, query)?;
    redact_causal_report(&mut report, home);
    Ok(CausalExportDocument {
        export_version: CAUSAL_EXPORT_VERSION,
        format: "optimus.causal.v1".into(),
        store_backed: true,
        live_provider_replay: false,
        report,
    })
}

/// Serialize export to pretty JSON bytes (UTF-8).
pub fn export_causal_json(home: impl AsRef<Path>, query: CausalQuery) -> Result<String> {
    let doc = export_causal_document(home, query)?;
    Ok(serde_json::to_string_pretty(&doc)?)
}

/// Write export JSON to `out_path` (parent dirs created).
pub fn write_causal_export(
    home: impl AsRef<Path>,
    query: CausalQuery,
    out_path: impl AsRef<Path>,
) -> Result<PathBuf> {
    let out_path = out_path.as_ref();
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = export_causal_json(home, query)?;
    std::fs::write(out_path, json.as_bytes())?;
    Ok(out_path.to_path_buf())
}

fn redact_causal_report(report: &mut CausalTurnReport, home: &Path) {
    let home_str = home.display().to_string();
    if home_str.is_empty() {
        report.home = "$OPTIMUS_HOME".into();
        return;
    }
    if report.home == home_str
        || report.home.starts_with(&home_str)
        || Path::new(&report.home)
            .canonicalize()
            .ok()
            .and_then(|p| home.canonicalize().ok().map(|h| p.starts_with(h)))
            .unwrap_or(false)
    {
        report.home = "$OPTIMUS_HOME".into();
    } else {
        report.home = report.home.replace(&home_str, "$OPTIMUS_HOME");
    }
}

fn security_denials_from_lifecycle(
    lifecycle: &[ExecutionToolLifecycleSummary],
) -> Vec<String> {
    let mut codes = Vec::new();
    for row in lifecycle {
        let phase = row.phase.to_ascii_lowercase();
        // Lifecycle phase strings may embed tool error text on failure paths.
        if let Some(code) = classify_message_for_export(&phase) {
            let s = code.to_string();
            if !codes.contains(&s) {
                codes.push(s);
            }
        }
    }
    codes
}

fn classify_message_for_export(message: &str) -> Option<&'static str> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("path not allowed") || lower.contains("outside root") {
        return Some(SecurityDenialCode::FsSandboxDeny.as_str());
    }
    if lower.contains("secret") && lower.contains("denied") {
        return Some(SecurityDenialCode::SecretBasenameDeny.as_str());
    }
    if lower.contains("path escape") {
        return Some(SecurityDenialCode::PathEscape.as_str());
    }
    if lower.contains("approval") || lower.contains("smartdeny") {
        return Some(SecurityDenialCode::ApprovalRequired.as_str());
    }
    if lower.contains("ssrf") {
        return Some(SecurityDenialCode::NetworkSsrfDeny.as_str());
    }
    if lower.contains("permission") && lower.contains("skill") {
        return Some(SecurityDenialCode::SkillPermissionDeny.as_str());
    }
    if lower.contains("unavailable") || lower.contains("not advertised") {
        return Some(SecurityDenialCode::ToolUnavailable.as_str());
    }
    None
}

/// List recent execution manifests (newest first) for operator browsing.
pub fn list_recent_causal_turns(
    home: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<ExecutionManifest>> {
    let executions = ExecutionStore::open(home.as_ref().join("execution.db"))?;
    executions.list_recent_manifests(limit)
}

fn resolve_manifest_id(executions: &ExecutionStore, query: &CausalQuery) -> Result<Uuid> {
    match query.kind {
        CausalQueryKind::ManifestId => {
            let id = Uuid::parse_str(query.id.trim()).map_err(KernelError::Uuid)?;
            let _ = executions.manifest(id)?;
            Ok(id)
        }
        CausalQueryKind::TurnId => {
            let turn = Uuid::parse_str(query.id.trim()).map_err(KernelError::Uuid)?;
            executions.find_by_turn(turn)?.ok_or_else(|| {
                KernelError::Model(format!("no execution manifest for turn {turn}"))
            })
        }
        CausalQueryKind::TraceId => {
            let trace = TraceId::parse(query.id.trim())?;
            executions.find_by_trace_id(trace)?.ok_or_else(|| {
                KernelError::Model(format!(
                    "no execution_trace_links row for trace {}",
                    query.id.trim()
                ))
            })
        }
    }
}

fn effect_links_cover_tool_calls(
    tool_calls: &[ExecutionToolCallSummary],
    links: &[SessionEffectLink],
) -> bool {
    for call in tool_calls {
        if call.suppressed || call.effect_attempt_id.is_none() {
            continue;
        }
        let Some(attempt) = call.effect_attempt_id.as_deref() else {
            continue;
        };
        let matched = links.iter().any(|link| {
            link.tool_call_id == call.call_id
                && link.effect_attempt_id.to_string() == attempt
                && call
                    .effect_sha256
                    .as_deref()
                    .is_none_or(|hash| hash == link.effect_hash)
        });
        if !matched {
            return false;
        }
    }
    true
}

/// Parse a free-form id into a causal query (trace / manifest / turn UUID).
///
/// Prefer an explicit kind when the same string could be ambiguous; all three
/// are UUID-shaped. Heuristic: try manifest, then turn, then trace lookup only
/// at load time — here we only classify by optional prefix.
pub fn parse_causal_query(raw: &str) -> Result<CausalQuery> {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("trace:") {
        return Ok(CausalQuery {
            kind: CausalQueryKind::TraceId,
            id: rest.trim().into(),
        });
    }
    if let Some(rest) = raw.strip_prefix("manifest:") {
        return Ok(CausalQuery {
            kind: CausalQueryKind::ManifestId,
            id: rest.trim().into(),
        });
    }
    if let Some(rest) = raw.strip_prefix("turn:") {
        return Ok(CausalQuery {
            kind: CausalQueryKind::TurnId,
            id: rest.trim().into(),
        });
    }
    // Default: treat bare UUID as trace id (TurnResult.trace_context.trace_id).
    let _ = Uuid::parse_str(raw).map_err(KernelError::Uuid)?;
    Ok(CausalQuery {
        kind: CausalQueryKind::TraceId,
        id: raw.into(),
    })
}
