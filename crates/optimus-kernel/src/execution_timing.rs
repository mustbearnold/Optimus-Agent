//! Timing and token-accounting projections for execution manifests.

use rusqlite::params;
use uuid::Uuid;

use serde::{Deserialize, Serialize};

use super::{execution::ExecutionStore, CompletionUsage, KernelError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ExecutionTimingSummary {
    pub total_ms: u64,
    pub first_response_ms: Option<u64>,
    pub model_ms: u64,
    pub tool_ms: u64,
    pub model_call_count: usize,
    pub executed_tool_call_count: usize,
    pub suppressed_tool_call_count: usize,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub accounted_model_call_count: usize,
    pub unaccounted_model_call_count: usize,
    pub terminal_status: Option<String>,
}

/// Bounded projection of one model step for causal reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionModelCallSummary {
    pub step: u32,
    pub provider: String,
    pub model: String,
    pub request_sha256: String,
    pub response_sha256: String,
    pub replay_class: String,
    pub duration_ms: u64,
    pub usage: Option<CompletionUsage>,
}

fn add_usage_total(total: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *total = Some(total.unwrap_or_default().saturating_add(value));
    }
}

impl ExecutionStore {
    pub fn timing_summary(&self, manifest_id: Uuid) -> Result<ExecutionTimingSummary> {
        let mut summary = self
            .conn
            .query_row(
                "SELECT m.duration_ms,
                        (SELECT elapsed_ms FROM execution_timing_events WHERE manifest_id=m.id AND kind='first_response' ORDER BY sequence LIMIT 1),
                        COALESCE((SELECT sum(duration_ms) FROM execution_timing_events WHERE manifest_id=m.id AND kind='model_finished'),0),
                        COALESCE((SELECT sum(duration_ms) FROM execution_timing_events WHERE manifest_id=m.id AND kind='tool_finished' AND suppressed=0),0),
                        (SELECT count(*) FROM execution_timing_events WHERE manifest_id=m.id AND kind='model_finished'),
                        (SELECT count(*) FROM execution_timing_events WHERE manifest_id=m.id AND kind='tool_finished' AND suppressed=0),
                        (SELECT count(*) FROM execution_timing_events WHERE manifest_id=m.id AND kind='tool_finished' AND suppressed=1),
                        CASE WHEN m.status='running' THEN NULL ELSE m.status END
                 FROM execution_manifests m WHERE m.id=?1",
                params![manifest_id.to_string()],
                |row| {
                    Ok(ExecutionTimingSummary {
                        total_ms: row.get::<_, i64>(0)? as u64,
                        first_response_ms: row
                            .get::<_, Option<i64>>(1)?
                            .map(|value| value as u64),
                        model_ms: row.get::<_, i64>(2)? as u64,
                        tool_ms: row.get::<_, i64>(3)? as u64,
                        model_call_count: row.get::<_, i64>(4)? as usize,
                        executed_tool_call_count: row.get::<_, i64>(5)? as usize,
                        suppressed_tool_call_count: row.get::<_, i64>(6)? as usize,
                        input_tokens: None,
                        output_tokens: None,
                        total_tokens: None,
                        reasoning_tokens: None,
                        cached_input_tokens: None,
                        cache_write_tokens: None,
                        accounted_model_call_count: 0,
                        unaccounted_model_call_count: 0,
                        terminal_status: row.get(7)?,
                    })
                },
            )
            .map_err(KernelError::Sqlite)?;
        for call in self.list_model_calls(manifest_id)? {
            let Some(usage) = call.usage else {
                summary.unaccounted_model_call_count += 1;
                continue;
            };
            summary.accounted_model_call_count += 1;
            add_usage_total(&mut summary.input_tokens, usage.input_tokens);
            add_usage_total(&mut summary.output_tokens, usage.output_tokens);
            add_usage_total(&mut summary.total_tokens, usage.total_tokens);
            add_usage_total(&mut summary.reasoning_tokens, usage.reasoning_tokens);
            add_usage_total(&mut summary.cached_input_tokens, usage.cached_input_tokens);
            add_usage_total(&mut summary.cache_write_tokens, usage.cache_write_tokens);
        }
        Ok(summary)
    }

    /// R10 (ADR-0082): distinct model steps whose tool calls were suppressed —
    /// the durable engagement counter for the step-scoped tool-loop guard.
    /// COUNT(DISTINCT step) survives approval resume, and a step that
    /// suppressed several calls still counts once.
    pub fn suppressed_tool_step_count(&self, manifest_id: Uuid) -> Result<u32> {
        self.conn
            .query_row(
                "SELECT COUNT(DISTINCT step)
                 FROM execution_timing_events
                 WHERE manifest_id=?1 AND kind='tool_finished' AND suppressed=1",
                params![manifest_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as u32)
            .map_err(KernelError::Sqlite)
    }
}
