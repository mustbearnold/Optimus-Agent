---
doc_id: decisions-0082-latency-shaping-for-the-tool-loop
doc_type: decision
plane: decision
status: current
authority: record
summary: The tool-to-tool gap is model round-trip dominated (non-streaming transport, high reasoning effort on every step, per-call persistence). Latency is shaped without degrading the final answer: per-provider reasoning-effort caps (first step keeps the user's choice, tool-loop and terminal steps cap at low, off is never upgraded, per-provider mapping pinned), one session save per model step with a mandatory flush choke point on every mid-step exit, a step-scoped tool-loop guard with a manifest-derived engagement counter surviving approval-resume, and a tool-to-tool gap breakdown in the execution dock.
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: decision
covers:
  - specs/014-self-build-reliability/spec.md
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-kernel/src/session.rs
  - crates/optimus-kernel/src/turn_recovery.rs
  - crates/optimus-kernel/src/model_call.rs
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-kernel/src/execution_schema.rs
  - apps/optimus-ui/src/state/composerStore.ts
  - apps/optimus-ui/src/components/workbench/ActivityTimeline.tsx
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-ui/src/state/conversationStore.test.ts
---

# ADR-0082: Latency shaping for the tool loop

- **Status:** Accepted
- **Date:** 2026-08-05

## Context

Tool calls execute in 5–40 ms; the gap between them is a full model round
trip on a non-streaming transport (TTFT + complete reasoning + complete
generation, one monolithic timeout). The desktop defaults (`thinking: high`,
`fast: off`) put high reasoning effort on every step, including steps whose
only job is to pick the next tool. Per-tool-call persistence adds ~6 fsync'd
SQLite commits plus an O(transcript) FTS rebuild per call. A separate defect:
exceeding the per-step tool-call budget sets `synthesis_only` for the rest
of the turn (zero tools), and the guard's forced step runs with an empty
tool list, so a compliant continuation is impossible and the guard can never
escalate deterministically.

Provider mapping facts: deepseek-v4-flash honors `low`; deepseek-v4-pro
collapses minimal/low to `high` (gate: verify the API accepts low before
landing the mapping, else document Pro as transport/batching-only);
open-ai-compat emits no `reasoning_effort` at all; codex passes effort
through.

## Decision

1. **Effort shaping.** First step of a fresh turn (manifest-derived
   discriminator: `list_model_calls(manifest_id).is_empty()`, not
   `steps == 1` — approval-resume re-enters with steps > 1) keeps the
   user's choice; every subsequent step caps at `low`, including the
   terminal text step (always preceded by a tool-calling step); user `off`
   is never upgraded (min-ordering off < minimal < low); `auto` caps to
   `low`. Chosen semantics (a): the final answer is generated at capped
   effort with tools still available — the model can request more work; the
   +1-round-trip re-synthesis alternative is a deferred opt-in. Fresh-install
   default `thinking` moves `high` → `minimal` (stored prefs preserved).
2. **Per-step persistence.** One `save_with_effect_links` per model step;
   the approval-park exit flushes the accumulated link batch inline; all
   other mid-step exits flush through `finish_turn` (mandatory choke point;
   callers `turn_recovery.rs:20/:49` and test sites pass `&[]`). Crash
   window: stale-but-paired transcript, sticky
   `effect_transcript_consistent` false, accepted re-execution hazard.
3. **Step-scoped guard.** Split `synthesis_guard` (guard message, tools
   advertised, per-state message) from `tool_lockdown` (empty tools);
   escalation on the second suppressed step in the turn via a
   manifest-derived engagement counter — `COUNT(DISTINCT step)` over
   `execution_timing_events` with `kind='tool_finished' AND suppressed=1`
   (survives approval-resume; per-step semantics; duplicate-evidence steps
   count per step). C5 batching guidance lands only after this.
4. **Observability.** Tool-to-tool gap breakdown in the execution dock from
   existing ToolStarted/ToolFinished timing events; live thinking
   indicators explicitly deferred to a streaming-transport follow-up.

## Consequences

- The perceived gap between tool calls shrinks on flash/codex immediately,
  and on Pro/open-ai-compat as the G1 gate resolves; the final answer's
  quality is preserved (capped but tools remain available).
- Persistence amortizes the FTS rebuild to O(1) per step; every exit path
  keeps effect links durable.
- The guard becomes deterministic: one suppressed step restores tools, two
  suppress the turn to synthesis — bounded across approval pauses.

## Alternatives considered

- Cap effort on every step including the first: rejected — the first step
  frames the whole task and needs the user's chosen effort.
- Re-issue the final synthesis at user effort after a capped text response
  (+1 round trip per tool turn): rejected for the first iteration — it adds
  the longest round trip of the turn on a non-streaming transport to every tool
  turn; kept as a deferred opt-in.
- Per-call saves with a WAL-tuned sessions DB: rejected — the dominant cost is
  the O(transcript) FTS rebuild per save; batching to one save per step is the
  O(n)→O(1) win.
- Keep `synthesis_only` turn-wide: rejected — one over-budget step locked the
  whole rest of the turn; step-scoping with a manifest-derived counter bounds
  the alternation cycle across approval pauses.
- Local engagement counter: rejected — it resets on approval-resume (same
  defect class as the fresh-turn flag), and the guard-forced step can park.

## Evaluation evidence

- Provider mapping verified: flash `low` honored (`openai_compat.rs:295-300`);
  pro collapses minimal/low → `high` (G1 gate); open-ai-compat emits no
  `reasoning_effort` (`:252-264`); codex passthrough (`codex_oauth.rs:909-916`).
- Persistence: exactly 3 `save_with_effect_links` callers today, no backfill
  anywhere (`causal.rs:253-277`); executions DB is WAL (`execution_schema.rs:5`);
  `execution_timing_events` is the only table carrying step + suppressed per
  call — the manifest-derived counter source.
- Guard mechanics: text-only responses terminate (`turn_loop.rs:637-652`);
  `synthesis_only` is a per-invocation local (`:149`); the guard-forced step
  with an empty tool list cannot continue, so the split is required.

## Conditions for reconsideration

- If the DeepSeek V4 API accepts low/minimal on Pro, land the Pro mapping
  (G1 gate) and remove the documented exception.
- If a streaming transport lands, live thinking indicators become feasible and
  the static ThinkingDelta can be replaced.

## Reasons

The tool-to-tool gap is model round-trip dominated; effort shaping on
tool-loop steps is the only lever that does not add round trips or degrade the
final answer. Persistence batching and the guard split fix measurable
per-call overhead and a deterministic lockdown defect respectively.

## Risks

- Capped final answers may be terser; mitigated by tools remaining available
  (the model can request more work) and the deferred re-synthesis opt-in.
- A mid-step crash under batching loses the step's completed sibling Tool
  messages; a follow-up turn's model may re-issue executed effects — accepted,
  documented, pinned by the kill-between-calls test.
- Duplicate-evidence suppression now loosens to per-step guard messaging;
  a disobedient model during lockdown still hard-fails via ToolNotAdvertised
  (identical to today).

## Relevant code

- `crates/optimus-kernel/src/turn_loop.rs`, `session.rs`, `turn_recovery.rs`,
  `model_call.rs`, `openai_compat.rs`, `execution_schema.rs`, `config.rs`
- `apps/optimus-ui/src/state/composerStore.ts`,
  `apps/optimus-ui/src/components/workbench/ActivityTimeline.tsx`

## Relevant tests

- `crates/optimus-kernel/tests/kernel_turn.rs`, `latency_shaping.rs`
- `apps/optimus-ui/src/state/conversationStore.test.ts`
