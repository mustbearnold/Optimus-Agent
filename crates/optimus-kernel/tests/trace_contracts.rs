use optimus_kernel::{
    resolve_route_traced, ExecutionStore, RouteRequest, RouteSurface, SpanStatus, TraceContext,
    TraceEventKind, TraceStore,
};
use tempfile::tempdir;

#[test]
fn trace_tree_events_and_terminal_outcomes_are_ordered_and_durable() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("trace.db");
    let store = TraceStore::open(&path).unwrap();
    let root = store.begin_root("kernel", "turn").unwrap();
    let model = store.begin_child(root, "provider", "model-call").unwrap();
    let tool = store.begin_child(root, "kernel", "tool-call").unwrap();
    assert_eq!(model.trace_id, root.trace_id);
    assert_ne!(model.span_id, tool.span_id);

    store
        .append_event(model, TraceEventKind::Started, "model", "a".repeat(64))
        .unwrap();
    store
        .append_event(model, TraceEventKind::Evidence, "response", "b".repeat(64))
        .unwrap();
    store.settle(model, SpanStatus::Succeeded).unwrap();
    assert!(store.settle(model, SpanStatus::Failed).is_err());
    assert_eq!(store.events(model).unwrap().len(), 3);
    drop(store);

    let reopened = TraceStore::open(&path).unwrap();
    assert_eq!(reopened.span(model).unwrap().status, SpanStatus::Succeeded);
    assert_eq!(
        reopened.events(model).unwrap()[2].kind,
        TraceEventKind::Terminal
    );
}

#[test]
fn trace_context_rejects_missing_parent_cross_trace_and_cycles() {
    let directory = tempdir().unwrap();
    let store = TraceStore::open(directory.path().join("trace.db")).unwrap();
    let first = store.begin_root("kernel", "turn-1").unwrap();
    let second = store.begin_root("kernel", "turn-2").unwrap();
    let missing = TraceContext::new(
        first.trace_id,
        optimus_kernel::SpanId::new(),
        Some(optimus_kernel::SpanId::new()),
    );
    assert!(store.register_span(missing, "kernel", "missing").is_err());
    let cross = TraceContext::new(
        first.trace_id,
        optimus_kernel::SpanId::new(),
        Some(second.span_id),
    );
    assert!(store.register_span(cross, "kernel", "cross").is_err());
    assert!(store.register_span(first, "kernel", "duplicate").is_err());
}

#[test]
fn route_decision_retains_exact_trace_and_span_identity() {
    let directory = tempdir().unwrap();
    let traces = TraceStore::open(directory.path().join("trace.db")).unwrap();
    let root = traces.begin_root("kernel", "turn").unwrap();
    let route = traces.begin_child(root, "routing", "decision").unwrap();
    let request = RouteRequest::standard(RouteSurface::Cli, "offline", None);
    let decision = resolve_route_traced(directory.path(), &request, route).unwrap();
    assert_eq!(decision.trace_id, Some(route.trace_id.to_string()));
    assert_eq!(decision.span_id, Some(route.span_id.to_string()));

    let executions = ExecutionStore::open(directory.path().join("execution.db")).unwrap();
    let manifest = executions
        .begin(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            "offline",
            "offline-scripted",
            b"prompt",
            b"tools",
            b"policy",
        )
        .unwrap();
    executions.bind_trace(manifest, root).unwrap();
    assert_eq!(executions.trace_context(manifest).unwrap(), Some(root));
    assert!(executions.bind_trace(manifest, route).is_err());
}

#[test]
fn traced_manifest_creation_rolls_back_when_link_insert_fails() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("execution.db");
    let executions = ExecutionStore::open(&path).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_trace_link
             BEFORE INSERT ON execution_trace_links
             BEGIN
               SELECT RAISE(ABORT, 'forced trace-link failure');
             END;",
        )
        .unwrap();
    let turn_id = uuid::Uuid::new_v4();

    assert!(executions
        .begin_traced(
            uuid::Uuid::new_v4(),
            turn_id,
            "offline",
            "offline-scripted",
            "review_changes",
            b"prompt",
            b"tools",
            b"policy",
        )
        .is_err());
    assert_eq!(executions.find_by_turn(turn_id).unwrap(), None);
}
