//! spec-014 R8 latency shaping — reasoning effort capping across steps.
//!
//! R8 (ADR-0082): the first step of a fresh turn keeps the user's effort
//! choice; every later step is capped at `low`; `off` is never upgraded;
//! `auto`/`None` resolves to `low`. R10 step-scoped tool-loop guard is
//! exercised in `kernel_turn.rs` (advertising on one suppressed step,
//! lockdown on two).

use optimus_kernel::{CompletionResponse, Kernel, KernelConfig, ScriptedModel, ToolCall};
use serde_json::json;
use tempfile::tempdir;

fn browser_tool_call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "activate_pack".into(),
        arguments: json!({"name":"browser"}),
    }
}

#[test]
fn first_step_keeps_user_effort_later_steps_cap_at_low() {
    let dir = tempdir().unwrap();
    let config = KernelConfig {
        thinking_level: Some("high".into()),
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open(dir.path(), config).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![browser_tool_call("one")],
            reasoning_content: None,
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![browser_tool_call("two")],
            reasoning_content: None,
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
            reasoning_content: None,
        },
    ]);
    kernel
        .turn_with_sink(&mut model, "first step keeps high", &mut |_| {})
        .unwrap();
    assert_eq!(model.seen.len(), 3);
    // Step 1 (fresh turn): the user's choice survives.
    assert_eq!(model.seen[0].reasoning_effort.as_deref(), Some("high"));
    // Steps 2+ : capped at low.
    assert_eq!(model.seen[1].reasoning_effort.as_deref(), Some("low"));
    assert_eq!(model.seen[2].reasoning_effort.as_deref(), Some("low"));
}

#[test]
fn off_is_never_upgraded_on_later_steps() {
    let dir = tempdir().unwrap();
    let config = KernelConfig {
        thinking_level: Some("off".into()),
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open(dir.path(), config).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![browser_tool_call("one")],
            reasoning_content: None,
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
            reasoning_content: None,
        },
    ]);
    kernel
        .turn_with_sink(&mut model, "off stays off", &mut |_| {})
        .unwrap();
    assert_eq!(model.seen.len(), 2);
    assert_eq!(model.seen[0].reasoning_effort.as_deref(), Some("off"));
    assert_eq!(model.seen[1].reasoning_effort.as_deref(), Some("off"));
}

#[test]
fn auto_effort_resolves_to_low_on_later_steps() {
    let dir = tempdir().unwrap();
    // `auto` normalizes to None (omit provider effort); the first step omits,
    // later steps resolve to an explicit `low` floor.
    let config = KernelConfig {
        thinking_level: Some("auto".into()),
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open(dir.path(), config).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![browser_tool_call("one")],
            reasoning_content: None,
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
            reasoning_content: None,
        },
    ]);
    kernel
        .turn_with_sink(&mut model, "auto resolves low", &mut |_| {})
        .unwrap();
    assert_eq!(model.seen.len(), 2);
    assert_eq!(model.seen[0].reasoning_effort, None);
    assert_eq!(model.seen[1].reasoning_effort.as_deref(), Some("low"));
}

#[test]
fn later_step_cap_applies_to_medium_too() {
    let dir = tempdir().unwrap();
    let config = KernelConfig {
        thinking_level: Some("medium".into()),
        ..KernelConfig::default()
    };
    let mut kernel = Kernel::open(dir.path(), config).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![browser_tool_call("one")],
            reasoning_content: None,
        },
        CompletionResponse {
            text: Some("done".into()),
            tool_calls: vec![],
            reasoning_content: None,
        },
    ]);
    kernel
        .turn_with_sink(&mut model, "medium caps low", &mut |_| {})
        .unwrap();
    assert_eq!(model.seen[0].reasoning_effort.as_deref(), Some("medium"));
    assert_eq!(model.seen[1].reasoning_effort.as_deref(), Some("low"));
}
