//! Local causal reconstruction of a turn from durable stores (not logs).
//!
//! Phase 5: given a trace id, manifest id, or turn id under an Optimus home,
//! assemble one reconstructible report from `execution.db` (and optional
//! session effect links). TraceStore remains optional offline evidence;
//! production turns bind identity in `execution_trace_links`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ExecutionManifest, ExecutionModelCallSummary, ExecutionStore, ExecutionTimingSummary,
    ExecutionToolCallSummary, ExecutionToolLifecycleSummary, KernelError, ReplayReport, Result,
    SessionEffectLink, SessionStore, TraceContext, TraceId,
};

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
    })
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
