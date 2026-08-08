//! Spec-026 acceptance suite: durable session goals with budget enforcement.
//!
//! A1  restart survival            A2  token-budget stop
//! A3  time-budget stop            A4  pause freezes accounting
//! A5  closed status machine       A6  input validation
//! A7  tool dispatch               A8  status JSON surface
//! A9  restart-mid-goal gate

use std::time::Duration;

use optimus_kernel::{
    list_sessions, CancellationToken, CompletionRequest, CompletionResponse, CompletionUsage,
    GoalBudgetReason, GoalStatus, Kernel, KernelConfig, KernelError, ModelProvider, ScriptedModel,
    StreamEvent, ToolCall,
};
use serde_json::json;
use tempfile::tempdir;

/// Scripted model that reports a fixed per-call token count, so token budgets
/// are deterministically testable (the real providers report usage on the
/// wire; the offline script reports none by default).
struct UsageModel {
    inner: ScriptedModel,
    tokens_per_call: u64,
}

impl UsageModel {
    fn new(script: Vec<CompletionResponse>, tokens_per_call: u64) -> Self {
        Self {
            inner: ScriptedModel::new(script),
            tokens_per_call,
        }
    }
}

impl ModelProvider for UsageModel {
    fn complete(
        &mut self,
        request: CompletionRequest,
    ) -> optimus_kernel::Result<CompletionResponse> {
        self.inner.complete(request)
    }

    fn complete_streaming(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> optimus_kernel::Result<CompletionResponse> {
        self.inner.complete_streaming(request, sink)
    }

    fn complete_streaming_cancellable(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
    ) -> optimus_kernel::Result<CompletionResponse> {
        self.inner
            .complete_streaming_cancellable(request, sink, cancellation)
    }

    fn last_usage(&self) -> Option<CompletionUsage> {
        Some(CompletionUsage {
            input_tokens: Some(self.tokens_per_call),
            output_tokens: Some(0),
            total_tokens: Some(self.tokens_per_call),
            ..Default::default()
        })
    }
}

fn answer(text: &str) -> CompletionResponse {
    CompletionResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        reasoning_content: None,
    }
}

fn goal_tool(action: &str, extra: serde_json::Value) -> ToolCall {
    let mut args = json!({ "action": action });
    if let Some(object) = extra.as_object() {
        for (key, value) in object {
            args[key] = value.clone();
        }
    }
    ToolCall {
        id: format!("goal-{action}"),
        name: "goal".into(),
        arguments: args,
    }
}

fn assert_budget_limited(error: &KernelError) -> (uuid::Uuid, GoalBudgetReason) {
    match error {
        KernelError::GoalBudgetLimited { goal_id, reason } => (*goal_id, *reason),
        other => panic!("expected GoalBudgetLimited, got {other:?}"),
    }
}

#[test]
fn a1_goal_survives_host_restart() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel = Kernel::open(home, KernelConfig::default()).unwrap();
    let goal = kernel
        .goal_set("write the quarterly report".into(), Some(500), Some(120))
        .unwrap();
    kernel.goal_start().unwrap();
    let session_id = list_sessions(home).unwrap()[0].id;

    // Restart: reopen the same session id from the same home.
    let mut reopened =
        Kernel::open_session(home, KernelConfig::default(), Some(session_id)).unwrap();
    let loaded = reopened.goal().unwrap().expect("goal survives restart");
    assert_eq!(loaded.id, goal.id);
    assert_eq!(loaded.objective, "write the quarterly report");
    assert_eq!(loaded.status, GoalStatus::Active);
    assert_eq!(loaded.token_budget, Some(500));
    assert_eq!(loaded.time_budget_seconds, Some(120));
    assert!(reopened.goal_pause().is_ok());
}

#[test]
fn a2_token_budget_stops_the_turn_and_no_further_call_starts() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    kernel
        .goal_set("two tokens only".into(), Some(2), None)
        .unwrap();
    kernel.goal_start().unwrap();
    let goal_id = kernel.goal().unwrap().unwrap().id;

    // Tool-call steps keep the loop alive; the third answer must never run.
    let mut model = UsageModel::new(
        vec![
            CompletionResponse {
                text: None,
                tool_calls: vec![goal_tool("status", json!({}))],
                reasoning_content: None,
            },
            CompletionResponse {
                text: None,
                tool_calls: vec![goal_tool("status", json!({}))],
                reasoning_content: None,
            },
            answer("three"),
        ],
        1,
    );
    let error = kernel.turn(&mut model, "work").unwrap_err();
    let (stopped, reason) = assert_budget_limited(&error);
    assert_eq!(stopped, goal_id);
    assert_eq!(reason, GoalBudgetReason::TokenBudget);

    let goal = kernel.goal().unwrap().unwrap();
    assert_eq!(goal.status, GoalStatus::BudgetLimited);
    assert_eq!(goal.tokens_used, 2);
    // Exactly two model calls; the third scripted answer was never consumed.
    assert_eq!(model.inner.seen.len(), 2);

    // The turn record carries the distinct terminal outcome.
    let store = optimus_kernel::SessionStore::open(dir.path().join("sessions.db")).unwrap();
    let turns = store.turns(kernel.session_id()).unwrap();
    let last = turns.last().unwrap();
    assert_eq!(last.status, optimus_kernel::TurnStatus::Failed);
    assert_eq!(last.error_code.as_deref(), Some("goal_budget_limited"));
}

#[test]
fn a3_time_budget_stops_before_the_next_step() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    kernel
        .goal_set("one second of work".into(), None, Some(1))
        .unwrap();
    kernel.goal_start().unwrap();

    // The single tool-call step paces 1.2 s of wall clock; the step-2
    // pre-step gate must fire before the second model call starts.
    // The first response carries text, so the pace lands inside model call 1;
    // the step-2 pre-step gate then fires before any second call.
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: Some("slow".into()),
            tool_calls: vec![goal_tool("status", json!({}))],
            reasoning_content: None,
        },
        answer("never"),
    ])
    .paced(Duration::from_millis(1200));
    let error = kernel.turn(&mut model, "work").unwrap_err();
    let (_, reason) = assert_budget_limited(&error);
    assert_eq!(reason, GoalBudgetReason::TimeBudget);
    assert_eq!(model.seen.len(), 1);
    assert_eq!(
        kernel.goal().unwrap().unwrap().status,
        GoalStatus::BudgetLimited
    );
}

#[test]
fn a4_pause_freezes_token_and_time_accounting() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    kernel.goal_set("pausable work".into(), None, None).unwrap();
    kernel.goal_start().unwrap();

    let two_steps = || {
        UsageModel::new(
            vec![
                CompletionResponse {
                    text: None,
                    tool_calls: vec![goal_tool("status", json!({}))],
                    reasoning_content: None,
                },
                answer("done"),
            ],
            1,
        )
    };
    let mut first = two_steps();
    kernel.turn(&mut first, "first stretch").unwrap();
    assert_eq!(kernel.goal().unwrap().unwrap().tokens_used, 2);

    kernel.goal_pause().unwrap();
    let paused_seconds = kernel.goal().unwrap().unwrap().active_seconds;

    // While paused, tokens spent by the user's own turns are not attributed.
    let mut paused_run = two_steps();
    kernel.turn(&mut paused_run, "side conversation").unwrap();
    let after_paused_turn = kernel.goal().unwrap().unwrap();
    assert_eq!(
        after_paused_turn.tokens_used, 2,
        "paused goals do not accumulate"
    );
    assert_eq!(
        after_paused_turn.active_seconds, paused_seconds,
        "paused clock is frozen"
    );

    kernel.goal_resume().unwrap();
    let mut resumed = two_steps();
    kernel.turn(&mut resumed, "resumed stretch").unwrap();
    let goal = kernel.goal().unwrap().unwrap();
    assert_eq!(
        goal.tokens_used, 4,
        "resume continues from the frozen value"
    );
    assert!(goal.active_seconds >= paused_seconds);
}

#[test]
fn a5_closed_status_machine_and_absorbing_terminals() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();

    // No goal yet: transitions fail with a clear message.
    assert!(kernel.goal_start().is_err());
    assert!(kernel.goal_pause().is_err());
    assert!(kernel.goal_complete().is_err());

    kernel.goal_set("machine".into(), None, None).unwrap();
    assert_eq!(kernel.goal().unwrap().unwrap().status, GoalStatus::Idle);
    // set on idle rewrites (same record).
    kernel
        .goal_set("machine v2".into(), Some(10), None)
        .unwrap();
    let goal = kernel.goal().unwrap().unwrap();
    assert_eq!(goal.status, GoalStatus::Idle);
    assert_eq!(goal.objective, "machine v2");

    kernel.goal_start().unwrap();
    assert_eq!(kernel.goal().unwrap().unwrap().status, GoalStatus::Active);
    // set on active is rejected and names the status.
    let err = kernel.goal_set("sneaky".into(), None, None).unwrap_err();
    assert!(
        err.to_string().contains("active"),
        "error names status: {err}"
    );
    // pause on active works; complete from paused works.
    kernel.goal_pause().unwrap();
    assert_eq!(kernel.goal().unwrap().unwrap().status, GoalStatus::Paused);
    kernel.goal_complete().unwrap();
    assert_eq!(kernel.goal().unwrap().unwrap().status, GoalStatus::Complete);
    // complete is absorbing.
    assert!(kernel.goal_pause().is_err());
    assert!(kernel.goal_resume().is_err());
    assert!(kernel.goal_start().is_err());
    // set on a terminal goal rewrites back to idle.
    kernel.goal_set("round two".into(), None, None).unwrap();
    assert_eq!(kernel.goal().unwrap().unwrap().status, GoalStatus::Idle);
}

#[test]
fn a6_validation_rejects_empty_objective_and_zero_budgets() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    assert!(kernel.goal_set("   ".into(), None, None).is_err());
    assert!(kernel.goal_set("ok".into(), Some(0), None).is_err());
    assert!(kernel.goal_set("ok".into(), None, Some(0)).is_err());
    assert!(
        kernel.goal().unwrap().is_none(),
        "failed sets must not create a goal"
    );
}

#[test]
fn a7_goal_tool_dispatches_and_records_no_effect_link() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(dir.path(), KernelConfig::default()).unwrap();
    let mut model = ScriptedModel::new(vec![
        CompletionResponse {
            text: None,
            tool_calls: vec![goal_tool(
                "set",
                json!({"objective": "tool goal", "token_budget": 100}),
            )],
            reasoning_content: None,
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![goal_tool("start", json!({}))],
            reasoning_content: None,
        },
        CompletionResponse {
            text: None,
            tool_calls: vec![goal_tool("status", json!({}))],
            reasoning_content: None,
        },
        answer("goal reached"),
    ]);
    let mut events = Vec::new();
    kernel
        .turn_with_sink(&mut model, "work", &mut |event| events.push(event))
        .unwrap();

    let goal = kernel.goal().unwrap().unwrap();
    assert_eq!(goal.objective, "tool goal");
    assert_eq!(goal.token_budget, Some(100));
    assert_eq!(goal.status, GoalStatus::Active);

    // The goal tool is advertised in the always-on waist (builtin catalog).
    let catalog = optimus_packs::builtin_catalog();
    let core = catalog
        .get(&optimus_packs::PackId::Core)
        .expect("core pack");
    let advertised = core
        .tools
        .iter()
        .any(|tool| tool.invocation == optimus_packs::ToolInvocation::Goal);
    assert!(advertised, "goal tool must be advertised in the core pack");

    // No effect links for goal calls (session state, not external effects).
    let store = optimus_kernel::SessionStore::open(dir.path().join("sessions.db")).unwrap();
    let links = store.effect_links(goal.session_id).unwrap();
    assert!(links.is_empty(), "goal calls must not produce effect links");
}

#[test]
fn a9_restart_mid_goal_gates_the_first_step() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let mut kernel = Kernel::open(home, KernelConfig::default()).unwrap();
    kernel
        .goal_set("restart gate".into(), None, Some(1))
        .unwrap();
    kernel.goal_start().unwrap();
    let session_id = list_sessions(home).unwrap()[0].id;

    // Host dies with the goal active; wall clock runs past the time budget.
    std::thread::sleep(Duration::from_millis(1300));

    let mut reopened =
        Kernel::open_session(home, KernelConfig::default(), Some(session_id)).unwrap();
    let mut model = ScriptedModel::new(vec![answer("never")]);
    let error = reopened.turn(&mut model, "continue").unwrap_err();
    let (_, reason) = assert_budget_limited(&error);
    assert_eq!(reason, GoalBudgetReason::TimeBudget);
    assert_eq!(
        model.seen.len(),
        0,
        "no model call may start after the gate"
    );
    assert_eq!(
        reopened.goal().unwrap().unwrap().status,
        GoalStatus::BudgetLimited
    );
}
