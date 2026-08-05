---
doc_id: decisions-0081-truthful-approval-resolution-and-session-consent
doc_type: decision
plane: decision
status: current
authority: record
summary: Approval resolution becomes truthful and resumable: failed continuations surface as visible resume_error (done-payload forwarding, done-handler branching before terminalization, reload preservation, no error swallowing), multi-node re-parks settle the approved node and synthesize a fresh claimable binding ({base}:node{n}) with still_pending instead of erroring, synthetic per-node results are claimed on the provider wire via outgoing-copy claim synthesis, and durable session consent (capability_grants keyed on the transcript session id) removes routine approval friction while keeping per-exact-effect audit rows.
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: decision
covers:
  - specs/014-self-build-reliability/spec.md
  - crates/optimus-kernel/src/chat_approval.rs
  - crates/optimus-kernel/src/tool_pairing.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/config.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-host/src/chat.rs
  - crates/optimus-host/src/router.rs
  - apps/optimus-ui/src/state/conversationStore.ts
  - apps/optimus-ui/src/ipc/tauriTransport.ts
  - apps/optimus-ui/src/app/OptimusApp.tsx
validated_by:
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-ui/src/state/conversationStore.test.ts
  - apps/optimus-ui/src/ipc/tauriTransport.test.ts
---

# ADR-0081: Truthful approval resolution and durable session consent

- **Status:** Accepted
- **Date:** 2026-08-05

## Context

Three approval-path defects break the self-build loop:

1. **Invisible continuation failures.** `chat_approval_resolve_cancellable`
   carries a failed continuation in the OK body as `resume_error`; the UI
   drops the done payload, swallows error events while a card is pending,
   and terminalizes "Completed" — the user sees "Approving…" then nothing,
   or a fake success.
2. **Multi-node re-park hard-fails.** `resolve_chat_approval_exact`
   requires a terminal `resume()`; a second pending node errors the whole
   resolution. Chat creators are single-node today (latent), but the
   runtime supports multi-node jobs and the workflow direction needs it.
3. **Approval friction.** The OpaqueShell class (`bash -c` and friends)
   maps to SystemModify, which no profile can grant, so the model's most
   common command shape asks every time, on 5-minute exact-effect grants.

Design constraints: ADR-0046's "the approved call is never re-derived"
invariant; law 6 (approval records per exact effect); the broker stays
stateless; `Runtime::session_id` is minted per turn and is unusable as a
consent key — the durable transcript session id is the correct key; the
host call-id validator allows `- _ . :` but not `#`.

## Decision

1. **Visible outcomes.** `ChatHandle.done` resolves with the done payload;
   the done handler branches before terminalization (`resume_error` →
   fail with text, preserved across the post-resolve reload;
   `still_pending` → explicit awaiting state); the error-swallow is
   removed (any error during awaiting fails the session — truthful;
   stale-card clicks fail at "missing or already resolved").
2. **Multi-node re-park.** On `AwaitingApproval` from `resume()`: record
   the settled node's outcome as the bound call's result (success from
   `effect.status`), finish that call's approval, synthesize a new binding
   `{base}:node{n}` (cloned ToolCall, derived from the original base id,
   never nested) with its own approval row + lifecycle event, in one
   transaction; return `still_pending`; the host skips resuming; the
   second card renders via record → `get_session` projection → reload.
3. **Wire-claimable synthetic results.** The pairing exemption lives in
   `drop_orphan_results` only (history-wide base-claim lookup; unclaimed
   bases still drop); `is_well_paired` stays strict so the request gate
   fires `repair_tool_pairing`, which synthesizes the parent `tool_calls`
   claim on the outgoing copy. Stored history stays honest.
4. **Session consent.** `capability_grants` keyed
   (durable transcript session id via `KernelConfig.consent_session_id` at
   BOTH construction sites, capability, CommandClass discriminator,
   scope_sha256, expiry); class re-derived via `classify_command` at
   settlement; OpaqueShell consent = the (SystemModify, OpaqueShell) pair —
   no new CapabilityId; class grants exclude OpaqueShell unless explicitly
   consented; auto-grants write the exact-effect audit row; 8 h TTL (cap
   24 h); revalidation of scope + DFA liveness at use time;
   `revoke_capability_grant` behind `session_consent_grant` /
   `session_consent_revoke` host routes + settings affordance. Honest
   framing: DFA already auto-grants `python -c`/`node -e` (ProjectExecute);
   this removes ask friction for the one shell class that still asks.

## Consequences

- The approve-then-fail UX is eliminated: approvals either run and report
  truthfully, or re-park with a second card; continuation failures are
  visible.
- Session consent reduces card volume dramatically for self-build sessions
  while every auto-grant still audits the exact effect; grants die with the
  session (bounded by TTL) and are revocable.
- The pairing exemption adds a small, pinned surface to `tool_pairing.rs`;
  claim synthesis touches only the outgoing request copy.

## Alternatives considered

- Keep the resolve hard-error on re-park and make all chat jobs single-node
  forever: rejected — the runtime supports multi-node jobs, the workflow
  direction needs them, and the failure mode is a hard error on a legitimate
  first click.
- Key session consent on `Runtime::session_id`: rejected — it is minted per
  turn (`workspace_identity.rs:90`), so TTL, revocation, and "this session"
  semantics would all be fiction; the durable transcript session id is the only
  key that survives across turns and restarts.
- Add a `SessionShellCommand` CapabilityId: rejected — a new variant must flow
  through `allows()`/`is_user_enablable`/`governing_toggle`/`decide()` and risks
  becoming toggle-grantable; the (capability, CommandClass) grant-key pair
  leaves the broker untouched.
- Synthesize a durable assistant message claiming synthetic call ids: rejected —
  a fabricated model turn in stored history is a transcript lie the model could
  echo back; outgoing-copy claim synthesis keeps history honest.
- Pairing exemption in `is_well_paired`: rejected — an exemption there skips the
  request-gate repair entirely, so the unclaimed synthetic result reaches the
  provider and is rejected with a 400.

## Evaluation evidence

- Live reproduction: adapter serializes `tool_call_id` verbatim
  (`openai_compat.rs:327-328`); the provider rejects unclaimed tool results
  ("Messages with role 'tool' must be a response to a preceding message with
  'tool_calls'", parsed at `openai_compat.rs:498`; failure class documented at
  `tool_pairing.rs:1-22`).
- Verified chains: `record_chat_approval_required` requires
  `call.id == binding.call_id == event.call_id` (`execution.rs:580-584`);
  second-card render chain (record → `tool_lifecycle_for_session` →
  `project_turn` → `load()` keyed by `event.call_id`) holds end-to-end;
  `has_pending_chat_approval` blocks a resumed turn while any card is pending
  (`kernel/lib.rs:769-776`), which the host still_pending skip relies on.

## Conditions for reconsideration

- If a provider gains native multi-call-claim support, the outgoing claim
  synthesis could be replaced by a provider-side split.
- If sessions gain a first-class durable consent concept (e.g. project trust
  grants), `capability_grants` can fold into it.

## Reasons

Truthful outcomes and resumable re-parks are prerequisites for the self-build
loop: hiding continuation failures made "Approving… then nothing" the standard
experience, and erroring a legitimate first click made multi-node work
impossible. Session consent removes the OpaqueShell friction that no profile
can grant while preserving per-exact-effect audit (law 6).

## Risks

- Stale-card clicks now fail the session truthfully (documented, not exempted);
  users clicking resolved cards see a failure state.
- Session grants die on app restart only when the session is not reopened; the
  8 h TTL bounds any residual window. Revocation is explicit.
- Claim synthesis must not increment `PairingRepair.changed()` or the repair
  status message fires on every step (pinned in spec §8).

## Relevant code

- `crates/optimus-kernel/src/chat_approval.rs`, `tool_pairing.rs`, `execution.rs`
- `crates/optimus-host/src/chat.rs`, `router.rs`
- `crates/optimus-store/src/lib.rs`
- `apps/optimus-ui/src/state/conversationStore.ts`, `ipc/tauriTransport.ts`,
  `app/OptimusApp.tsx`

## Relevant tests

- `crates/optimus-kernel/tests/kernel_turn.rs`, `approval_vertical.rs`,
  `tool_pairing_vertical.rs`
- `crates/optimus-store/tests/session_consent.rs`
- `apps/optimus-ui/src/state/conversationStore.test.ts`,
  `apps/optimus-ui/src/ipc/tauriTransport.test.ts`
- `apps/optimus-desktop/e2e/09-self-build-reliability.spec.js`
