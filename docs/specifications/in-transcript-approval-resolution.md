---
knowledge_type: specification
status: current
covers:
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-host/src/**
  - apps/optimus-electron/**
  - apps/optimus-ui/src/**
depends_on:
  - docs/decisions/0031-safe-project-work-loop.md
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-host/src/**
  - apps/optimus-ui/src/components/workbench/Transcript.test.tsx
  - apps/optimus-ui/src/state/conversationStore.test.ts
last_verified_commit: null
---

# In-transcript exact approval resolution

## Experience contract

- **User and job:** a local developer reviews one exact project mutation at the
  point where Optimus paused, decides whether it may run, and continues without
  leaving the owning session.
- **Durable outcome:** the exact decision, bound effect, runtime receipt, tool
  lifecycle, and settled conversation turn survive reload. Success is not the
  button click; it is a runtime-confirmed terminal tool state plus a durable
  receipt or denial record.
- **Stakes:** a false approval can mutate the wrong project; a false completion
  can hide an unexecuted or ambiguous effect. The manual fallback is the
  Execution dock, which remains a projection of the same runtime decision.
- **Primary surface:** the owning transcript tool card. The Execution dock is a
  secondary inspector, not a separate source of authority.
- **Actual capability:** project writes and commands already pause as exact
  SmartDeny jobs. This slice adds exact approve/deny resolution and durable UI
  projection; it does not add broad project permission or renderer path trust.

## Authority and lifecycle

| Action | Tier | Exact boundary | Approval/receipt | Recovery |
|---|---:|---|---|---|
| Inspect pending action | A0 | session + run + call + job + node + effect digest | none | reload canonical projection |
| Deny action | A0 | same immutable binding | durable denial; no effect receipt | continue with retained transcript |
| Approve action | A2 | same immutable binding and authorized project root | single-use grant plus runtime receipt | reconcile canonical job/tool state |

The owning request contains `session_id`; its approval binding contains
`run_id`, `call_id`, `tool_id`, `job_id`, `node_id`, `node_index`, and the
canonical effect SHA-256. The desktop host and kernel recheck every executable
identity field. A missing, changed, foreign, already-terminal, or stale binding
fails closed. The renderer never supplies a filesystem path as authority.

The turn remains durably resumable while the tool is awaiting approval. One
resolution transitions the bound tool to a terminal state and settles or
continues the same accepted turn without duplicating the user message or
replaying the effect. A second resolution cannot execute again.

## Outcome, control, activity, evidence

1. **Outcome:** exact action summary and its eventual receipt/denial.
2. **Control:** `Approve and continue` and `Deny` on the bound tool card.
3. **Activity:** pending, resolving, completed, denied/cancelled, failed, or
   ambiguous lifecycle copy driven only by typed events.
4. **Evidence:** expandable technical details retain job, call, effect digest,
   duration, and validated terminal outcome. Raw provider protocol JSON stays
   hidden.

## Critical state matrix

| State | User-visible treatment | Available controls | Backend truth | Accessibility/test |
|---|---|---|---|---|
| awaiting approval | exact verb, target, consequence, and “Approval required” | approve; deny | active turn + running manifest + pending exact job | polite status; native buttons; `approval-card-awaiting` |
| resolving approval | chosen action is named; card remains in place | both disabled | decision command in flight; no optimistic effect success | `aria-busy`; `approval-card-resolving` |
| approved and receipted | terminal tool outcome and concise continuation receipt | expand evidence | grant consumed once; effect terminal receipt persisted | `approval-approved-replay` |
| denied | “Denied — not run” | expand evidence | exact denial persisted; job/effect cannot run | `approval-denied-no-effect` |
| resolution failed | bounded error beside unchanged pending card | retry same semantic decision | canonical pending state retained or refetched | focus remains on invoking button; `approval-resolution-failure` |
| reload/reconnect | same card and decision state, no duplicate control action | based on canonical state | ordered durable lifecycle replay, deduped by event ID | `approval-reload-equivalence` |
| changed/foreign binding | no success claim; blocked error | refetch only | server rejects identity/digest mismatch | `approval-binding-rejected` |
| duplicate decision | original terminal state retained | none | no second effect; idempotent lookup or explicit terminal rejection | `approval-single-use` |

Connection state remains separate from run state. Losing the renderer does not
deny, approve, cancel, or complete the action. Color may support the state but
text and control labels carry the meaning.

## Verification contract

- Kernel: pause leaves one active turn and running execution; exact approval
  executes once; denial executes zero times; changed identities and duplicate
  decisions fail closed; replay reconstructs the same terminal tool state.
- Desktop/Electron: only the allowlisted bounded resolver crosses the bridge;
  project authority is reopened from Rust-owned scope; presentation responses
  omit provider protocol messages.
- React: controls appear only for typed bound approval events; pending/error
  states are keyboard usable; reload and duplicate event delivery preserve one
  card and one decision.
- Browser proof: fixture state shows the inline card at normal and compact
  widths. This is browser-contract evidence, not installed-native proof.

## Explicit boundaries

- Approval remains exact and single-use; this slice does not introduce
  “approve all” or remembered broad authority.
- If provider synthesis after the effect cannot be resumed without weakening
  the durable contract, deterministic receipt settlement is preferred and the
  missing synthesis is reported as a later boundary.
- Arbitrary approved child processes remain outside built-in file-effect
  confinement, as recorded by ADR-0031.
