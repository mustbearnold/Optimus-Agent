//! Deterministic trajectory eval harness — offline scripted turns with expected traces.

use std::path::Path;

use optimus_packs::ToolId;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    CompletionResponse, Kernel, KernelConfig, KernelError, ScriptedModel, ToolCall, TurnResult,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub user: String,
    /// Scripted model steps (tool calls / final text).
    pub steps: Vec<CompletionResponse>,
    /// Canonical tool identities that must be invoked (any order).
    #[serde(default)]
    pub expect_tools: Vec<ToolId>,
    /// Substring that must appear in assistant_text.
    #[serde(default)]
    pub expect_text_contains: Option<String>,
    /// Disable stream chunking for offline scripted model.
    #[serde(default = "default_true")]
    pub stream_chunks: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCaseResult {
    pub id: String,
    pub ok: bool,
    pub detail: String,
    #[serde(default)]
    pub tool_trace: Vec<String>,
    #[serde(default)]
    pub assistant_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub passed: usize,
    pub failed: usize,
    pub cases: Vec<EvalCaseResult>,
}

impl EvalReport {
    pub fn all_ok(&self) -> bool {
        self.failed == 0
    }
}

/// Built-in offline suite (no network). Extensible later via JSON files.
pub fn builtin_suite() -> Vec<EvalCase> {
    vec![
        EvalCase {
            id: "offline-echo".into(),
            user: "ping".into(),
            steps: vec![CompletionResponse {
                text: Some("pong".into()),
                tool_calls: vec![],
            }],
            expect_tools: vec![],
            expect_text_contains: Some("pong".into()),
            stream_chunks: false,
        },
        EvalCase {
            id: "memory-then-answer".into(),
            user: "what editor?".into(),
            steps: vec![
                CompletionResponse {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "t1".into(),
                        name: "memory_recall".into(),
                        arguments: json!({
                            "subject": "user",
                            "predicate": "prefers_editor"
                        }),
                    }],
                },
                CompletionResponse {
                    text: Some("You prefer helix.".into()),
                    tool_calls: vec![],
                },
            ],
            expect_tools: vec!["memory_recall".into()],
            expect_text_contains: Some("helix".into()),
            stream_chunks: false,
        },
        EvalCase {
            id: "pack-activate-browser".into(),
            user: "need browser".into(),
            steps: vec![
                CompletionResponse {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "a1".into(),
                        name: "activate_pack".into(),
                        arguments: json!({"name": "browser"}),
                    }],
                },
                CompletionResponse {
                    text: Some("browser pack ready".into()),
                    tool_calls: vec![],
                },
            ],
            expect_tools: vec!["activate_pack".into()],
            expect_text_contains: Some("browser".into()),
            stream_chunks: false,
        },
        EvalCase {
            id: "write-file-job".into(),
            user: "write note".into(),
            steps: vec![
                CompletionResponse {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "w1".into(),
                        name: "write_file".into(),
                        arguments: json!({
                            "path": "notes/eval.txt",
                            "contents": "deterministic-write"
                        }),
                    }],
                },
                CompletionResponse {
                    text: Some("wrote notes/eval.txt".into()),
                    tool_calls: vec![],
                },
            ],
            expect_tools: vec!["write_file".into()],
            expect_text_contains: Some("wrote".into()),
            stream_chunks: false,
        },
    ]
}

pub fn run_case(home: impl AsRef<Path>, case: &EvalCase) -> Result<EvalCaseResult, KernelError> {
    let mut k = Kernel::open(home.as_ref(), KernelConfig::default())?;
    // Seed memory for the recall case (deterministic fixture).
    if case.id == "memory-then-answer" {
        k.remember_demo("user", "prefers_editor", "helix")?;
    }
    let mut model = ScriptedModel::new(case.steps.clone());
    model.stream_chunks = case.stream_chunks;
    let result: TurnResult = k.turn(&mut model, &case.user)?;

    let mut problems = Vec::new();
    for tool in &case.expect_tools {
        if !result.invoked_tools.contains(tool) {
            problems.push(format!("missing canonical tool invocation {tool:?}"));
        }
    }
    if let Some(sub) = &case.expect_text_contains {
        if !result.assistant_text.contains(sub) {
            problems.push(format!(
                "assistant_text missing {sub:?}: got {:?}",
                result.assistant_text
            ));
        }
    }

    Ok(EvalCaseResult {
        id: case.id.clone(),
        ok: problems.is_empty(),
        detail: if problems.is_empty() {
            "ok".into()
        } else {
            problems.join("; ")
        },
        tool_trace: result.tool_trace,
        assistant_text: result.assistant_text,
    })
}

pub fn run_suite(home: impl AsRef<Path>, cases: &[EvalCase]) -> EvalReport {
    let mut cases_out = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    for (i, case) in cases.iter().enumerate() {
        // Isolate each case under a subdir for determinism / no cross-talk.
        let case_home = home.as_ref().join(format!("case-{i}-{}", case.id));
        let _ = std::fs::create_dir_all(&case_home);
        match run_case(&case_home, case) {
            Ok(r) => {
                if r.ok {
                    passed += 1;
                } else {
                    failed += 1;
                }
                cases_out.push(r);
            }
            Err(e) => {
                failed += 1;
                cases_out.push(EvalCaseResult {
                    id: case.id.clone(),
                    ok: false,
                    detail: format!("kernel error: {e}"),
                    tool_trace: vec![],
                    assistant_text: String::new(),
                });
            }
        }
    }
    EvalReport {
        passed,
        failed,
        cases: cases_out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn builtin_suite_passes_offline() {
        let d = tempdir().unwrap();
        let report = run_suite(d.path(), &builtin_suite());
        assert!(
            report.all_ok(),
            "eval failed: {}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        assert_eq!(report.passed, 4);
    }
}
