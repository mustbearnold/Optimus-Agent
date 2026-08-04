---
doc_id: decisions-0077-verified-progress-per-token-is-the-development-objective
doc_type: decision
plane: decision
status: current
authority: record
summary: Optimus optimizes verified progress per model token by pairing exact wall-clock action timing with provider-reported usage, compact structured context, targeted verification, and explicit unknown accounting.
reviewed_on: 2026-08-03
review_by: 2026-11-03
knowledge_type: decision
covers:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/model_contract.rs
  - crates/optimus-kernel/src/model_usage.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/execution_schema.rs
  - crates/optimus-kernel/src/execution_timing.rs
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-kernel/src/codex_oauth.rs
  - crates/optimus-kernel/src/codex_responses.rs
  - crates/optimus-host/src/developer.rs
  - scripts/tools/development_efficiency.py
depends_on:
  - docs/decisions/0032-engineering-memory-compact-lenses.md
  - docs/decisions/0049-module-size-is-measured-honestly.md
  - docs/decisions/0076-developer-full-access-is-a-scoped-grant-with-a-stable-supervisor.md
validated_by:
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/tests/openai_http.rs
  - crates/optimus-kernel/tests/codex_oauth.rs
  - crates/optimus-host/src/developer.rs
  - scripts/tests/test_development_efficiency.py
---

# ADR-0077: Verified progress per token is the development objective

- **Status:** Accepted
- **Date:** 2026-08-03

## Context

Raw token minimization is the wrong objective for self-development. A shorter
turn that misses the root cause, repeats a failed edit, or skips verification
costs more than a deliberate turn that produces a tested change. Optimus also
had timing evidence for turns and tool calls, but its Developer Full Access
action log did not include duration and provider usage was not retained beside
the model call that consumed it.

## Decision

1. Optimus treats **verified progress per model token** as the optimization
   target. Verification means a relevant test, build, health check, or other
   explicit acceptance result—not a model assertion.
2. Every Developer Full Access host action is recorded in bounded JSONL with a
   stable action id, start/end Unix milliseconds, monotonic `duration_ms`,
   method, and outcome. Read/status actions are included so the log is a
   complete action timeline rather than a mutation-only sample.
3. Provider adapters retain optional input, output, total, reasoning, cached
   input, and cache-write token fields on the execution model-call row. Missing
   provider fields remain `NULL`; Optimus never estimates tokens from
   characters. Reports expose accounted and unaccounted model calls separately.
4. The inner development loop is bounded and staged: maintain a compact
   structured notebook, select only relevant files/symbols, run targeted tests
   after each coherent edit, then run the full delivery gate. The stable
   context prefix contains invariants and durable instructions; changing task
   detail stays in the delta.
5. Model routing is empirical: use the strongest model/effort for architecture,
   ambiguity, difficult debugging, and final review; use lower-cost models or
   effort for mechanical transformations only when the same acceptance tests
   show equal verified progress. Deterministic local tools own formatting,
   filtering, batching, and test execution.
6. `scripts/tools/development_efficiency.py` is the compact local readout for wall
   time, action percentiles, model/tool counts, provider token totals, and
   unknown accounting. It is diagnostic evidence, not a new authority plane.

## Consequences

- A slower turn can be the better turn when it reduces rework or raises the
  verified success rate; the report makes that trade visible.
- Provider differences and gateway omissions no longer create fake precision.
- Stable context and targeted verification reduce repeated prompt/tool work,
  while full verification remains the delivery boundary.
- The current metric is a foundation, not a semantic grader: “verified
  progress” is represented by acceptance outcomes and still needs task-level
  benchmark aggregation before it can be a single universal score.

## Alternatives considered

- **Minimize tokens unconditionally.** Rejected because premature compression,
  omitted context, and underpowered debugging can increase total work.
- **Estimate tokens from characters.** Rejected because tokenizer and provider
  behavior differ, and an estimate would look like measured usage in reports.
- **Log only mutating actions.** Rejected because status, health, and read
  actions consume development time and are part of the causal timeline.
- **Send raw histories to a model for optimization.** Rejected because the
  report should be cheap, deterministic, and inspectable without adding a
  second context tax.

## Evaluation evidence

- OpenAI-compatible usage mapping: `openai_http` test passes.
- Codex Responses JSON and SSE usage mapping: `codex_oauth` tests pass.
- Execution usage persistence and accounted/unaccounted aggregation: kernel
  execution test passes.
- Developer action timing: host unit test passes.
- Compact report and unknown accounting: `scripts/tests/test_development_efficiency.py`
  passes.

## Conditions for reconsideration

Add task-level verified-progress scoring only when the score can be grounded in
independent acceptance evidence and benchmark results. Do not collapse unknown
provider usage into zero or promote a model-routing policy based on one task.

## Reasons

The execution manifest already owns causal timing and replay provenance, so
adding usage to that record keeps performance and cost evidence aligned with the
model call that produced it. A local report avoids a second model context tax.

## Risks

Provider schemas can change or omit cache fields. Action logs can be truncated
by bounded rotation. Both cases are surfaced as missing or partial evidence;
neither is silently converted into a zero-cost claim.

## Relevant code

- `crates/optimus-kernel/src/model_usage.rs`
- `crates/optimus-kernel/src/execution_timing.rs`
- `crates/optimus-host/src/developer.rs`
- `scripts/tools/development_efficiency.py`

## Relevant tests

- `crates/optimus-kernel/tests/openai_http.rs`
- `crates/optimus-kernel/tests/codex_oauth.rs`
- `crates/optimus-host/src/developer.rs`
- `scripts/tests/test_development_efficiency.py`
