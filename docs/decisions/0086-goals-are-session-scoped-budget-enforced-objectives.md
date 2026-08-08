---
doc_id: decisions-0086-goals-are-session-scoped-budget-enforced-objectives
doc_type: decision
plane: decision
status: current
authority: record
summary: Goals are durable, session-scoped records (objective, optional token and time budgets, closed status machine) persisted in the session store (sessions.db) — not the effect ledger (optimus.db) the roadmap named — and enforced by the turn loop before each model step, ending the turn with a distinct goal_budget_limited terminal outcome; kernel tool + CLI surface with deterministic, approval-free, replayable calls.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: decision
covers:
  - specs/026-goals/spec.md
  - crates/optimus-kernel/src/session.rs
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-kernel/src/tool_dispatch.rs
  - crates/optimus-packs/src/invocation.rs
  - crates/optimus-packs/src/catalog.rs
  - apps/optimus-cli/src/goal.rs
depends_on:
  - docs/decisions/0047-turn-step-budget.md
  - specs/003-kernel-turns/spec.md
validated_by:
  - crates/optimus-kernel/tests/goals.rs
---

# ADR-0086: Goals are session-scoped, budget-enforced objectives

- **Status:** Accepted
- **Date:** 2026-08-08

## Context

The best-agent roadmap v2 (Phase 1a, Spec-026) adds a goal surface: "a
model-held objective with budgets" that survives restart, stops at its
budget, and is exposed as kernel tools and CLI commands. The roadmap says
"persist goal state in optimus.db with the session", naming `optimus.db`
(the effect ledger owned by `optimus-store`/`optimus-runtime`) as the
home, and lists crate ownership as `optimus-kernel`, `optimus-store`.

Two facts constrain the placement. First, `optimus.db` is the runtime
effect ledger: jobs, nodes, effect attempts, receipts, and the Work Graph.
It is keyed by effect, not by session, and its rows are the durable
evidence of external effects. Second, the kernel already owns a
session-scoped store — `sessions.db` (`SessionStore`, `session.rs`) —
which persists sessions, turns, turn events, and effect links with
additive `CREATE TABLE IF NOT EXISTS` migrations, `ON DELETE CASCADE`
from `sessions(id)`, and repair logic in `session/repair.rs`.

A goal is session state: it dies with the session, is loaded with the
session, and is meaningless without one. Placing it in the effect ledger
would couple session lifecycle to effect evidence and force effect-ledger
consumers (doctor, backup, parity) to reason about a non-effect row.

## Decision

1. **Goals live in the session store.** One additive `session_goals`
   table in `sessions.db`, `session_id TEXT NOT NULL REFERENCES
   sessions(id) ON DELETE CASCADE`, `UNIQUE(session_id)` (at most one
   goal per session). This is a documented deviation from the roadmap's
   "optimus.db": the roadmap's intent — durable goal state "with the
   session" — is satisfied by the store that already persists the
   session. `optimus doctor` and the durability inventory pick the table
   up through the existing sessions.db inventory path.
2. **One goal per session.** The model sequences objectives; recursion
   (spec-005) carries sub-goal work in child sessions.
3. **Closed status machine.** `idle`, `active`, `paused`,
   `budget_limited`, `complete`, `error`; absorbing terminal states;
   `set` rewrites only non-active, non-paused goals (spec-026 R2).
4. **Budget enforcement is a turn-loop gate.** Before each model step the
   loop checks the active goal's budgets; after each model step it adds
   provider-reported usage. Over budget: persist `budget_limited` and end
   the turn with `KernelError::GoalBudgetLimited` → turn record
   `error_code = goal_budget_limited`, mirroring the `MaxSteps` terminal
   outcome (ADR-0047). Enforcement runs only while the goal is `active`;
   a paused goal does not gate turns.
5. **Accounting freezes on pause.** Token usage accumulates only while
   `active`; active time accumulates wall-clock from `last_resumed_at`,
   frozen on pause/resume. Restart-safe: both are stored values, never
   derived from execution history.
6. **One always-on `goal` tool.** Actions `set|start|status|pause|
   resume|complete`; `ToolPolicy::Capability` (session-local, no external
   effect, no approval), `ReplayClass::Deterministic`, keyed idempotency.
   CLI `optimus goal ...` mirrors the actions with `--session` and `--json`.
7. **No config surface in v1.** Budgets come from `set` arguments; there
   is no default budget. A goal without budgets is a pure objective
   tracker with manual `complete`.

## Alternatives considered

- **`optimus.db` (roadmap literal).** Rejected: the effect ledger must
  not carry session-scoped records; `sessions.db` already provides
  cascade, repair, backup, and inventory for session state (Context).
- **Execution-history derivation of token usage.** Rejected: usage would
  need querying `execution.db` across restarts and would count tokens
  spent while paused; stored accumulation is simpler and pause-correct.
- **Separate `goals.db`.** Rejected: no benefit over a table in the
  store that already owns the session lifecycle; a fifth home database
  would add doctor/backup surface for one row type.
- **Separate tools per action (goal_set, goal_status, ...).** Rejected:
  one tool with an action enum keeps the always-on waist small and the
  schema stable; the compiler still names every dispatch arm.
- **Goal budget check only pre-step.** Rejected: a step can exhaust the
  budget mid-step; the post-step accumulation transitions immediately so
  the next model call never starts (spec-026 R3, A2).

## Reasons

- Session state belongs in the session store (Architectural law 11:
  runtime events observable and ordered; law 21: module size — a new
  table in the existing store beats a new crate).
- The roadmap's acceptance — "a goal survives host restart. It stops at
  its budget. The user can pause, resume, and complete it." — is
  satisfied by stored accumulation plus a loop gate, with no new
  background machinery.
- `MaxSteps` (ADR-0047) already establishes the honest terminal-outcome
  pattern for a limit stop; `goal_budget_limited` extends it without new
  settlement logic.
- Tool determinism keeps fixture-replay evaluation meaningful: goal calls
  are keyed idempotent and effect-free.

## Consequences

- `sessions.db` gains one additive table; existing databases migrate by
  `CREATE TABLE IF NOT EXISTS` on open, with a schema check in the
  session-store repair path.
- The turn loop gains one pre-step check and one post-step accounting
  block; both are no-ops when no goal is `active` (zero cost for the
  common path).
- The tool waist grows by one always-on tool (~150 schema tokens).
- `optimus goal ...` becomes the user surface; the desktop panels are a
  Phase 4 follow-up (spec-001) and are not required for acceptance.

## Risks

- **Budget overshoot by one step.** A step may push accumulated tokens
  over the budget; the stop happens at the step boundary, so the final
  turn is `goal_budget_limited` and its last assistant text is kept in
  the transcript (session save precedes the error, as with `MaxSteps`).
  Mitigation: post-step transition guarantees no further model call.
- **Model never completes the goal.** The goal stays `active` and gates
  every turn. Mitigation: the user can `pause` or `complete` from CLI;
  budgets bound the damage by default when set.
- **Provider usage gaps.** A provider may omit usage; accumulation then
  adds zero and a token budget never trips on missing data. Mitigation:
  honest per spec-026 R3 (usage is provider-reported; Optimus never
  estimates, per `model_usage.rs`); the time budget still bounds the run.

## Evaluation evidence

- `crates/optimus-kernel/tests/goals.rs` — A1–A9: restart survival,
  token-budget stop with no further model call, time-budget stop,
  pause/resume freezing, closed status machine, validation failures,
  tool dispatch without effect links, CLI JSON surface, restart-mid-goal
  gate.
- Existing suites: `kernel_turn.rs` (turn settlement unchanged when no
  goal is active), tool-coverage gate (advertised ≡ handlers).

## Conditions for reconsideration

- If a session must hold concurrent goals, drop `UNIQUE(session_id)` and
  make the active goal an explicit field; the enforcement gate then
  needs a selected-goal rule.
- If provider usage reporting improves to per-step completeness, the
  post-step accumulation may move to the execution store derivation
  without changing the spec surface.
- If the desktop goals panel (Phase 4) needs goal history, add a
  `goal_events` table mirroring `session_turn_events`; the record itself
  stays as specified.

## Relevant code

- `crates/optimus-kernel/src/session.rs` — `session_goals` table and
  goal methods on `SessionStore`.
- `crates/optimus-kernel/src/turn_loop.rs` — pre-step gate and post-step
  accounting.
- `crates/optimus-kernel/src/tool_dispatch.rs` — `ToolInvocation::Goal`
  dispatch arm.
- `crates/optimus-packs/src/invocation.rs`, `crates/optimus-packs/src/catalog.rs`
  — variant, policy, replay class, catalog entry.
- `apps/optimus-cli/src/goal.rs` — CLI commands.

## Relevant tests

- `crates/optimus-kernel/tests/goals.rs` (new; A1–A9).
- `crates/optimus-kernel/tests/kernel_turn.rs` (regression: no-goal
  turns unchanged).
- `crates/optimus-packs` tool-coverage gate (advertised ≡ handlers).
