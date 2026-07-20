use optimus_kernel::{
    record_route_telemetry, resolve_route_traced, route_telemetry_aggregate, ModelCapability,
    ProviderId, RouteRequest, RouteSurface, RouteTelemetryObservation, RouteTelemetryOutcome,
    RouteTelemetryPolicy, TraceStore,
};
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn route_telemetry_is_provenance_bound_deduplicated_and_aggregated_with_integers() {
    let directory = tempdir().unwrap();
    let traces = TraceStore::open(directory.path().join("trace.db")).unwrap();
    let root = traces.begin_root("kernel", "turn").unwrap();
    let route_span = traces.begin_child(root, "routing", "decision").unwrap();
    let request = RouteRequest::standard(RouteSurface::Cli, "codex", None);
    let route = resolve_route_traced(directory.path(), &request, route_span).unwrap();

    for (index, (outcome, latency, cost)) in [
        (RouteTelemetryOutcome::Succeeded, 10, 5),
        (RouteTelemetryOutcome::Succeeded, 20, 7),
        (RouteTelemetryOutcome::Failed, 30, 11),
        (RouteTelemetryOutcome::Succeeded, 40, 13),
    ]
    .into_iter()
    .enumerate()
    {
        record_route_telemetry(
            directory.path(),
            &RouteTelemetryObservation {
                attempt_id: Uuid::new_v4(),
                route_id: route.id,
                trace_id: Some(route_span.trace_id),
                provider: route.provider,
                model: route.model.clone(),
                outcome,
                latency_millis: latency,
                cost_microunits: cost,
                observed_unix: 1_000 + index as u64,
            },
        )
        .unwrap();
    }

    let aggregate = route_telemetry_aggregate(
        directory.path(),
        ProviderId::Codex,
        route.model.as_str(),
        900,
        100,
    )
    .unwrap()
    .unwrap();
    assert_eq!(aggregate.samples, 4);
    assert_eq!(aggregate.successes, 3);
    assert_eq!(aggregate.success_basis_points, 7_500);
    assert_eq!(aggregate.median_latency_millis, 25);
    assert_eq!(aggregate.p95_latency_millis, 40);
    assert_eq!(aggregate.total_cost_microunits, 36);

    let mut wrong = RouteTelemetryObservation {
        attempt_id: Uuid::new_v4(),
        route_id: route.id,
        trace_id: Some(route_span.trace_id),
        provider: ProviderId::Offline,
        model: route.model,
        outcome: RouteTelemetryOutcome::Succeeded,
        latency_millis: 1,
        cost_microunits: 0,
        observed_unix: 1_005,
    };
    assert!(record_route_telemetry(directory.path(), &wrong).is_err());
    wrong.provider = ProviderId::Codex;
    wrong.latency_millis = 0;
    assert!(record_route_telemetry(directory.path(), &wrong).is_err());
}

#[test]
fn routing_uses_only_fresh_policy_approved_telemetry_for_explicit_fallback() {
    let directory = tempdir().unwrap();
    let traces = TraceStore::open(directory.path().join("trace.db")).unwrap();
    let root = traces.begin_root("kernel", "turn").unwrap();
    let codex_span = traces.begin_child(root, "routing", "codex").unwrap();
    let codex = resolve_route_traced(
        directory.path(),
        &RouteRequest::standard(RouteSurface::Cli, "codex", None),
        codex_span,
    )
    .unwrap();
    let openai_span = traces.begin_child(root, "routing", "openai").unwrap();
    let openai = resolve_route_traced(
        directory.path(),
        &RouteRequest::standard(RouteSurface::Cli, "openai-compat", None),
        openai_span,
    )
    .unwrap();
    for (route, span, outcome, latency) in [
        (&codex, codex_span, RouteTelemetryOutcome::Failed, 500),
        (&codex, codex_span, RouteTelemetryOutcome::Failed, 600),
        (&openai, openai_span, RouteTelemetryOutcome::Succeeded, 30),
        (&openai, openai_span, RouteTelemetryOutcome::Succeeded, 40),
    ] {
        record_route_telemetry(
            directory.path(),
            &RouteTelemetryObservation {
                attempt_id: Uuid::new_v4(),
                route_id: route.id,
                trace_id: Some(span.trace_id),
                provider: route.provider,
                model: route.model.clone(),
                outcome,
                latency_millis: latency,
                cost_microunits: 5,
                observed_unix: 1_000,
            },
        )
        .unwrap();
    }

    let mut request = RouteRequest::standard(RouteSurface::Gateway, "codex", None);
    request.allow_fallback = true;
    request
        .required_capabilities
        .insert(ModelCapability::Streaming);
    request.telemetry_policy = Some(RouteTelemetryPolicy {
        evaluated_unix: 1_100,
        max_age_seconds: 500,
        min_samples: 2,
        min_success_basis_points: 8_000,
        max_p95_latency_millis: 100,
        allow_missing: false,
    });
    let selected = optimus_kernel::resolve_route(directory.path(), &request).unwrap();
    assert_eq!(selected.provider, ProviderId::OpenAiCompat);
    assert_eq!(selected.fallback_from, Some(ProviderId::Codex));
    assert!(selected
        .reasons
        .iter()
        .any(|reason| reason.starts_with("telemetry_snapshot_sha256=")));
}
