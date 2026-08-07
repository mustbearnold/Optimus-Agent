---
doc_id: spec-014-self-build-reliability
doc_type: reference
plane: work
status: current
authority: canonical
summary: Self-build reliability and responsiveness — the toolchain-aware command envelope for Developer Full Access, pre-card feasibility probes, visible approval-resolution outcomes (resume_error surfacing, multi-node re-park with still_pending), durable session-scoped capability consent, and latency shaping (per-provider reasoning-effort caps, per-step persistence, tool-loop guard scoping, tool-to-tool gap observability).
reviewed_on: 2026-08-05
review_by: 2026-11-05
knowledge_type: specification
covers:
  - crates/optimus-runtime/src/command_envelope.rs
  - crates/optimus-runtime/src/process_ownership.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/policy_bridge.rs
  - crates/optimus-kernel/src/developer_runtime.rs
  - crates/optimus-kernel/src/turn_loop.rs
  - crates/optimus-kernel/src/chat_approval.rs
  - crates/optimus-kernel/src/tool_pairing.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/execution_schema.rs
  - crates/optimus-kernel/src/session.rs
  - crates/optimus-kernel/src/causal.rs
  - crates/optimus-kernel/src/turn_recovery.rs
  - crates/optimus-kernel/src/model_call.rs
  - crates/optimus-kernel/src/config.rs
  - crates/optimus-kernel/src/openai_compat.rs
  - crates/optimus-kernel/src/system_prompt.rs
  - crates/optimus-policy/src/command_class.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-host/src/chat.rs
  - crates/optimus-host/src/router.rs
  - crates/optimus-host/src/sessions.rs
  - apps/optimus-ui/src/state/conversationStore.ts
  - apps/optimus-ui/src/ipc/tauriTransport.ts
  - apps/optimus-ui/src/ipc/wsTransport.ts
  - apps/optimus-ui/src/app/OptimusApp.tsx
  - apps/optimus-ui/src/state/composerStore.ts
  - apps/optimus-ui/src/components/workbench/ActivityTimeline.tsx
  - apps/optimus-ui/src/components/workbench/Transcript.tsx
depends_on:
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0046-approval-resumes-the-turn.md
  - docs/decisions/0060-owned-localhost-is-a-process-bound-lease.md
  - docs/decisions/0076-developer-full-access-is-a-scoped-grant-with-a-stable-supervisor.md
  - docs/decisions/0078-a-transcript-is-a-provider-contract-and-an-unreachable-toggle-asks.md
  - specs/001-desktop-shell/spec.md
  - specs/002-host-ipc/spec.md
  - specs/003-kernel-turns/spec.md
  - specs/004-runtime-effects/spec.md
  - specs/013-self-development/spec.md
validated_by:
  - crates/optimus-runtime/tests/command_envelope.rs
  - crates/optimus-runtime/tests/toolchain.rs
  - crates/optimus-runtime/tests/toolchain_spawn.rs
  - crates/optimus-eval/src/eval.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - apps/optimus-ui/src/state/conversationStore.test.ts
  - apps/optimus-ui/src/ipc/tauriTransport.test.ts
---

# Self-build reliability and responsiveness

Status: current
Owner: optimus-agent-development (prompt-only owner)

## Purpose

The desktop app's self-build loop — chatting with Optimus while it builds
itself — fails in three observable ways: approved commands die instantly
inside the Confined bwrap envelope because the dev toolchain lives in `$HOME`
and is never bound; routine commands ask for approval every time and the
resolution path can hard-fail or hide continuation errors; and tool calls
execute in milliseconds but the gaps between them feel like a second because
of high reasoning effort on every model step, per-call persistence writes,
and a non-streaming transport. This spec makes approved dev work actually
run (toolchain-aware envelope + pre-card feasibility probes), makes approval
resolution truthful and resumable (visible `resume_error`, multi-node
re-park with `still_pending`, durable session consent), and shapes latency
without degrading the final answer (per-provider effort caps, per-step
persistence, step-scoped tool-loop guard, gap observability). The detailed
engineering draft (bind tables, exit enumerations, test lists, round-by-round
review record) is `Development/tmp/spec-self-build-reliability-draft.md`
(git-excluded); this spec is the governing contract.

## Requirements

- R1. The Developer Full Access command envelope MUST gain a classed
  toolchain bind tier: rw binds limited to non-secret caches
  (`~/.cargo/registry`, `~/.cargo/git`, `~/.bun`, `~/.cache/cargo`,
  `~/.cache/bun`), ro binds for non-secret functional paths (`~/.cargo/bin`,
  `~/.rustup`, `~/.cache/ms-playwright`), and credential/identity paths
  (`~/.cargo/credentials.toml`, `~/.cargo/config.toml`, `~/.gitconfig`,
  `~/.config/git`, `~/.config/gh`, `~/.ssh`) NEVER bound under the
  shared-network Confined envelope (ro-bind is readable; readable is
  exfiltratable). Every entry is skip-if-absent; rw sources get host-side
  creation; ordinary profiles keep the strict Confined envelope unchanged.
- R2. Command execution under the toolchain tier MUST resolve bare program
  names deterministically: first PATH entry that is both present on the host
  and visible in the bind set, normalized to an absolute path, with a
  bind-derived `systemd-run Environment=PATH` for child resolution, and the
  same resolution re-executed at spawn (binds are re-derived per turn from
  the live grant snapshot). [ADR-0080]
- R3. Before an approval card is shown for a high-risk effect, the runtime
  MUST probe the exact spawn chain (host PATH resolution → sandbox
  visibility → shim dependencies → bind-mode-aware write targets for
  HostInstall-class effects) and deny with actionable recovery text instead
  of carding a doomed effect. The probe result is a feasibility predictor,
  not authority. [ADR-0080]
- R4. A failed approval continuation MUST be visible: the resolve stream's
  done payload carries `resume_error`; the workbench done handler marks the
  session failed with the error text before any terminalization, preserves
  it across the post-resolve reload, and does not swallow `{type:'error'}`
  events while an approval card is pending (general rule: any error during
  awaiting fails the session; stale-card clicks fail truthfully at
  "missing or already resolved").
- R5. A multi-node job that re-parks during approval resolution MUST NOT
  error the whole resolution: the settled node's outcome is recorded as the
  bound call's result (success derived from `effect.status`), that call's
  approval is finished, a new binding is synthesized
  (`{base}:node{n}`, cloned ToolCall, never nested, one transaction) with
  its own approval row and lifecycle event, `still_pending: true` is
  returned, the host skips resuming the turn, and the second card renders
  via record → `get_session` projection → reload. The approved call is
  never re-derived (ADR-0046). [ADR-0081]
- R6. Synthetic per-node results MUST reach the provider: the pairing
  exemption for `^<base>:node\d+$` results lives in `drop_orphan_results`
  only (history-wide base-claim lookup; unclaimed bases still drop),
  `is_well_paired` stays strict so the request gate fires the repair, and
  `repair_tool_pairing` synthesizes the parent `tool_calls` claim on the
  outgoing copy (stored history stays honest). [ADR-0081]
- R7. Session-scoped capability consent MUST be keyed on the durable
  transcript session id (plumbed as `KernelConfig.consent_session_id` at
  BOTH the turn and resolve construction sites), keyed
  (session, capability, CommandClass discriminator, scope_sha256, expiry),
  re-derived from `classify_command` at settlement, exclude OpaqueShell
  from class grants except as the explicit (SystemModify, OpaqueShell)
  shell consent, revalidate scope and DFA liveness at use time, write the
  exact-effect audit row on auto-grant, expire at 8 h (cap 24 h), and be
  revocable through `revoke_capability_grant` behind the
  `session_consent_grant` / `session_consent_revoke` host routes with a
  settings-panel affordance. [ADR-0081]
- R8. Reasoning effort MUST be shaped per step and per provider: the first
  step of a fresh turn (manifest-derived discriminator, not `steps == 1`)
  keeps the user's choice; subsequent steps cap at `low` (final answer
  included, tools remain available); user `off` is never upgraded; the
  per-provider mapping (flash/codex honored, pro gated on API acceptance,
  open-ai-compat documented) is pinned by tests. [ADR-0082]
- R9. Session persistence MUST batch to one save per model step, flush the
  accumulated effect-link batch on every mid-step exit (park inline;
  `finish_turn` as the mandatory choke point for all other exits), keep the
  approval-park path durable, and document the crash window (stale-but-paired
  transcript, sticky `effect_transcript_consistent` false, re-execution
  hazard accepted). [ADR-0082]
- R10. The tool-loop guard MUST be step-scoped: `synthesis_guard` (guard
  message, tools stay advertised, per-state message) vs `tool_lockdown`
  (empty tools), escalation on the second suppressed step in the turn with
  a manifest-derived engagement counter (`COUNT(DISTINCT step)` over
  suppressed `execution_timing_events`, surviving approval-resume), and
  duplicate-evidence steps counting per step. [ADR-0082]
- R11. The execution dock MUST show a tool-to-tool gap breakdown computed
  from existing timing events; live thinking indicators are explicitly
  deferred to a streaming-transport follow-up.
- R12. The desktop UI MUST offer "Always allow <class> in this project
  (this session)" on the approval card (wired to `session_consent_grant`),
  a "Revoke session grants" affordance in Developer Full Access settings,
  and a non-blocking once-per-session profile-suggestion banner after ≥3
  consecutive approvals (counted from `approval_required` lifecycle
  events).
- R13. The installed-app evaluation loop (`desktop_task_suite.py` +
  `desktop_task_harness.py`, spec-015 surface) MUST launch the packaged
  desktop app with the WebKit remote inspector on an isolated `--home`,
  submit composer prompts over the deterministic offline echo provider,
  bind durable traces from `sessions.db` / `execution.db`, capture the
  inspector console stream, and record a per-prompt input channel:
  `atspi` when the host AT stack (pyatspi + a11y bus) is available —
  accessibility-level input per the native-UI evidence ladder — and `dom`
  otherwise, with the DOM channel always the fallback and the contracts
  identical on both channels. The loop MUST self-skip on missing binary,
  missing `websockets`, or missing display hardware (the optional-device
  pattern), and a host without the AT stack MUST keep passing.

## Acceptance criteria

- [ ] A1. Given a Developer Full Access grant with terminal execution and a
      rustup toolchain in `$HOME`, when a `terminal` effect runs `cargo
      build` on a fixture crate through the exact product chain
      (`systemd-run --user --wait --pipe` → bwrap → bare `cargo`), then it
      succeeds; and when the grant is absent, then the strict envelope
      binds zero `$HOME` paths and credential files are invisible
      in-sandbox.
- [ ] A2. Given a high-risk effect whose program is invisible in the active
      envelope, when the runtime settles authority, then no approval card is
      produced and the denial names the missing path and the remedy; and
      when the program is a HostInstall-class write into a ro-bound toolchain
      dir (including shell-wrapped), then the denial names the read-only
      target.
- [ ] A3. Given an approval whose continuation fails, when the resolve
      stream terminates, then the session shows the real error text as its
      status and it survives the post-resolve reload; and no error event
      while a card is pending is swallowed.
- [ ] A4. Given a multi-node job whose second node parks, when the first
      card is approved, then the resolution returns `still_pending`, the
      turn is not resumed, the second card renders, and approving it
      produces a provider-valid transcript (synthetic result claimed on the
      wire) and a terminal turn.
- [ ] A5. Given session consent for (SystemModify, OpaqueShell) under a live
      DFA grant, when a `bash -lc` effect runs in the same durable session
      (including immediately after an approval resolution), then it
      auto-grants with an exact-effect audit row; and after scope widening,
      DFA disable, expiry, or revocation, then it asks again.
- [ ] A6. Given a tool-using turn, when model steps 2..n run, then effort is
      capped per the provider mapping (first step uncapped, terminal step
      capped, `off` never upgraded); and when a step exceeds the tool-call
      budget, then tools return on the next step, the second suppressed
      step in the turn (counting across an approval pause) locks tools
      down, and the guard events are visible.
- [ ] A7. Given a step with two durable siblings and an approval park, when
      the turn parks, then both siblings' effect links survive reload; and
      when any mid-step exit (cancel, control-plane error, defensive
      error) occurs, then the accumulated batch is flushed through
      `finish_turn`.
- [ ] A8. Given a completed tool turn, when the execution dock renders,
      then the tool-to-tool gap breakdown is shown from timing events.
- [ ] A9. Given the full implementation, when `bash scripts/verify.sh all`
      runs, then all gates are green, including the new envelope live
      probe, the approval vertical, the session-consent store tests, and
      the desktop e2e `09-self-build-reliability.spec.js`.
- [ ] A10. Given the installed desktop app on a host with or without the AT
      stack, when the desktop task suite runs the easy..ultra-hard
      contracts, then every prompt is submitted (AT-SPI channel when
      available, DOM fallback otherwise — both recorded in the evidence),
      every turn settles succeeded in `execution.db` bound to
      offline/offline-scripted with zero tool calls and zero approvals,
      and the console stream carries no uncaught exceptions. (proven
      2026-08-07: suite hermetic on this host — `easy-echo: ok`, four
      tiers green, per-prompt channel evidence present; AT-SPI live path
      is exercised on hosts with the stack and self-documents otherwise)
