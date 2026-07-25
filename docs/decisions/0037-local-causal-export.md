---
knowledge_type: decision
status: current
covers:
  - crates/optimus-kernel/src/causal.rs
  - apps/optimus-cli/src/main.rs
  - scripts/check-observability-gate.py
depends_on:
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/decisions/0023-fixture-replay-trace-telemetry-evaluation.md
  - docs/plans/s-plus-plus-plus-program.md
validated_by:
  - crates/optimus-kernel/tests/causal_trace.rs
  - scripts/check-observability-gate.py
last_verified_commit: null
---

# ADR-0037: Local causal export (not OTLP) — P14

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

Observability was **A-**: offline integrity and `optimus trace show` reconstruct
turns from durable stores, but there was no versioned machine-readable **export**
of the causal graph, and no gate that locked export invariants. Full OTLP/OpenTelemetry
export would imply a distributed telemetry product Optimus does not ship.

## Decision

1. **Local-only S+++ export format** `optimus.causal.v1` (JSON), versioned by
   `CAUSAL_EXPORT_VERSION` / field `export_version`.
2. **Source of truth remains stores** (`execution.db` + session effect links).
   Export is `load_causal_turn` + redaction — not log scraping.
3. **Redaction:** absolute Optimus home paths become `$OPTIMUS_HOME`.
4. **Honesty fields:** `store_backed: true`, `live_provider_replay: false`
   (fixture replay does not re-run live providers; export never claims otherwise).
5. **CLI:** `optimus trace export <id> --out path.json`.
6. **Not chosen:** OTLP wire export in P14 (may revisit when product needs
   external collectors; would be a new ADR).

## Consequences

- Positive: operators/agents can archive deterministic causal graphs without OTel.
- Positive: observability gate fails if export API or CLI disappears.
- Residual: multi-DB identity remains reconciled not single-transaction;
  `security_denials` on export is best-effort from lifecycle **phase names**
  only (often empty for FS/SSRF fences until denials are durably coded).

## Alternatives

- **OTLP first.** Rejected for P14 scope/honesty.
- **Only human `trace show`.** Rejected — S+++ needs machine-readable export.

## Risks

- Consumers parse JSON without version check. Mitigate: require `export_version`
  and `format == optimus.causal.v1`.

## Reconsideration

- Add OTLP when an external collector is a product requirement with redaction
  parity tests.
