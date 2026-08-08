---
doc_id: spec-026-goals
doc_type: reference
plane: work
status: current
authority: canonical
summary: Model-held goals for Optimus — a durable, session-scoped goal record (objective, optional token budget, optional time budget, closed status machine) that the turn loop enforces at every model step, stops the turn honestly at a budget, and exposes through kernel tools and CLI commands; the capability class Prime Agent ships as its goal surface, planned natively for Phase 1a of the best-agent roadmap.
reviewed_on: 2026-08-08
review_by: 2026-11-08
knowledge_type: specification
covers:
  - crates/optimus-kernel/src/session.rs
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-kernel/src/tool_dispatch.rs
  - crates/optimus-packs/src/invocation.rs
  - crates/optimus-packs/src/catalog.rs
  - apps/optimus-cli/src/goal.rs
  - apps/optimus-cli/src/main.rs
depends_on:
  - docs/decisions/0086-goals-are-session-scoped-budget-enforced-objectives.md
  - specs/003-kernel-turns/spec.md
  - specs/016-staleness-prevention/spec.md
validated_by:
  - crates/optimus-kernel/tests/goals.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
---

# Spec-026: Goals — model-held objectives with budgets

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | DRAFT | Initial draft from best-agent roadmap v2 Phase 1a (review 96/100, held points unrelated). Roadmap text: "Add a goal state to the kernel session. The goal state has an objective, a token budget, a time budget, and a status. Statuses are idle, active, paused, budget_limited, complete, and error. Persist goal state in optimus.db with the session. Expose goal operations as kernel tools and CLI commands. The turn loop checks the budgets at each model step." | Roadmap "optimus.db" re-homed to `sessions.db` (the session store) by ADR-0086: goals are session state, and the effect ledger must not carry session-scoped records. Every MUST below carries an acceptance criterion. |

## Purpose

A campaign is an ordered list of deterministic steps. It is not a
model-held objective with budgets. This spec defines the missing goal
surface: one durable objective per session, with optional token and time
budgets, enforced by the turn loop, observable by the user, and stopped
honestly when a budget is exhausted.

## Current state (Confirmed behaviour)

- Campaigns exist in `optimus-runtime/src/campaign.rs`: ordered, durable
  agent steps mapped to Work Graph jobs (spec-005).
- The turn loop (`turn_loop.rs`) enforces `max_steps` and cancellation,
  and records provider-reported usage per model call in `execution.db`.
- The session store (`session.rs`, `sessions.db`) persists sessions,
  turns, turn events, and effect links with additive `CREATE TABLE IF
  NOT EXISTS` migrations.
- No goal state exists in the kernel today.

## Requirements

### R1. Goal record

A goal is a durable record scoped to exactly one session. It MUST carry
an objective (non-empty text), a status, an optional token budget, an
optional time budget, accumulated token usage, accumulated active time,
and timestamps. A session MUST have at most one goal at any time.

### R2. Status machine

The status MUST be one of `idle`, `active`, `paused`, `budget_limited`,
`complete`, or `error`. Transitions MUST be closed:

- `set` creates or rewrites a goal in `idle` (a new goal, or an existing
  goal whose status is `idle`, `complete`, `budget_limited`, or `error`).
- `start` moves `idle` to `active`.
- `pause` moves `active` to `paused`.
- `resume` moves `paused` to `active`.
- `complete` moves `active` or `paused` to `complete`.
- The turn loop moves `active` to `budget_limited` when a budget is
  exhausted, and to `error` when a goal invariant fails.
- `complete`, `budget_limited`, and `error` are absorbing: no action
  rewrites a goal in an absorbing status except `set`.

### R3. Budget enforcement

The turn loop MUST check an `active` goal's budgets before each model
step and MUST NOT start a step when the goal is already over a budget.
The loop MUST mark the goal `budget_limited` and end the turn with a
distinct terminal outcome (`goal_budget_limited`) instead of continuing.
After each model step the loop MUST add the provider-reported usage to
the goal's accumulated tokens while the goal is `active`; the goal MUST
transition to `budget_limited` and the turn MUST stop when the
accumulated tokens reach the token budget.

### R4. Time accounting

While `active`, the goal MUST accumulate active time. `pause` MUST freeze
both time and token accounting. `resume` MUST continue both from the
frozen values. A goal with a time budget MUST transition to
`budget_limited` when accumulated active time reaches the budget.

### R5. Persistence

The goal MUST survive host restart: opening the same session id after a
restart MUST restore the goal, its status, its budgets, and its
accumulated usage. The goals table MUST be an additive, versioned
migration in `sessions.db` (ADR-0086), and `optimus doctor` MUST report
the new table like any other store object.

### R6. Tool surface

The kernel MUST expose the goal operations as one always-on tool (`goal`)
with actions `set`, `start`, `status`, `pause`, `resume`, and `complete`.
`set` MUST reject a non-empty objective and MUST reject a rewrite while
the goal is `active` or `paused` with an error naming the current status.
`status` MUST report objective, status, budgets, accumulated tokens, and
accumulated active time. Tool calls MUST be replayable (deterministic,
keyed idempotency) and MUST NOT require approval: they mutate session
state, not external effects.

### R7. CLI surface

The CLI MUST mirror the tool actions as `optimus goal set|start|status|
pause|resume|complete` against a session, with `--session` to select a
session and the current session as the default. CLI output MUST be
human-readable by default and JSON with `--json`.

### R8. Observability

The goal MUST be visible in the session context: `goal status` MUST work
without a model, and the `budget_limited` terminal outcome MUST be
distinguishable from other turn failures in the turn record
(`error_code = goal_budget_limited`).

## Acceptance criteria

| # | Criterion |
|---|---|
| A1 | Create a goal via `set`; reopen the kernel with the same session id; `status` reports the same objective, status, and budgets (R1, R5). |
| A2 | A goal with token budget 2 and a scripted model that reports 1 token per call: the turn ends after the second call with the goal `budget_limited` and turn `error_code = goal_budget_limited`; no third model call is recorded (R3, R8). |
| A3 | A goal with time budget 1 s and a scripted model that sleeps 1.1 s per step: the second step does not start; the goal is `budget_limited` (R3, R4). |
| A4 | `pause` freezes accounting: pause after 2 tokens, resume, run 2 more tokens; `status` reports 4 total, and the elapsed active time excludes the paused interval (R4). |
| A5 | `complete` from `active` and from `paused` lands on `complete`; `set` on an `active` goal fails naming `active`; `pause` on `idle` fails (R2, R6). |
| A6 | `set` with an empty objective fails; `set` with token budget 0 fails (R1, R6). |
| A7 | The `goal` tool is advertised in the always-on pack, dispatches each action, and records no effect link (R6). |
| A8 | `optimus goal status --json` on a session with a goal prints the goal record; on a session without one prints an explicit empty result (R7, R8). |
| A9 | A restart in the middle of an over-budget goal: the next turn's first step check marks `budget_limited` and stops before any model call (R3, R5). |

## Out of scope

- Multiple concurrent goals per session (one goal; the model sequences
  objectives).
- Goal hierarchies or sub-goals (recursive children carry that, spec-005).
- Automatic goal completion from outcome evidence (the model or the user
  calls `complete`; the roadmap's autonomous-mode quality gates are a
  separate capability, spec-027).
- Cost routing or cheap-model selection for goal work (Section 8 of the
  best-agent roadmap governs auxiliary-model cost; the goal itself spends
  on the session's normal provider).

## Open questions

- Should a `complete` goal archive into the session transcript as a
  message? Default for v1: no — the goal record is the ledger, and the
  model says so in its own words.

## Links

- `crates/optimus-runtime/src/campaign.rs` — the ordered deterministic
  steps this generalizes.
- `crates/optimus-kernel/src/session.rs` — the durable session store the
  goals table attaches to.
- `crates/optimus-kernel/src/turn_loop.rs` — the enforcement point
  (mirrors `MaxSteps`).
- `crates/optimus-kernel/src/execution_schema.rs` — provider-reported
  usage recording the goal consumes.
- Prime Agent goal surface (goal statuses, budgets, start/stop): the
  capability class this spec plans natively.
- `docs/decisions/0086-goals-are-session-scoped-budget-enforced-objectives.md`
  — the placement and enforcement decision.
