---
doc_id: decisions-0075-a-block-owns-its-lifecycle-rows-only-paint-it
doc_type: decision
plane: decision
status: current
authority: record
summary: The terminal becomes a structured workbench of typed blocks driven by typed domain events — a block carries identity and lifecycle, rendered rows are only its projection, one animation clock owns all motion, and every block kind that lacks a real event source is labelled planned rather than scaffolded as working.
reviewed_on: 2026-08-02
review_by: 2026-11-02
knowledge_type: decision
covers:
  - apps/optimus-tui/src/workbench/mod.rs
  - apps/optimus-tui/src/workbench/detail.rs
  - apps/optimus-tui/src/workbench/grouping.rs
  - apps/optimus-tui/src/workbench/effects.rs
  - apps/optimus-tui/src/workbench/selection.rs
  - apps/optimus-tui/src/session.rs
  - apps/optimus-tui/src/session/event_adapter.rs
  - apps/optimus-tui/src/lib.rs
  - apps/optimus-tui/src/view.rs
  - apps/optimus-tui/src/transcript.rs
  - apps/optimus-tui/src/keys.rs
  - apps/optimus-tui/src/mouse.rs
  - crates/optimus-kernel/src/lib.rs
depends_on:
  - docs/decisions/0045-agent-host-and-surface-transports.md
  - docs/decisions/0046-approval-resumes-the-turn.md
  - docs/decisions/0049-module-size-is-measured-honestly.md
  - docs/decisions/0074-a-surface-owns-the-catalog-it-answers-from.md
validated_by:
  - apps/optimus-tui/tests/pty.rs
  - apps/optimus-tui/src/workbench/mod.rs
  - apps/optimus-tui/src/workbench/detail.rs
  - apps/optimus-tui/src/workbench/grouping.rs
  - apps/optimus-tui/src/workbench/effects.rs
  - apps/optimus-tui/src/workbench/selection.rs
  - apps/optimus-tui/src/session.rs
  - apps/optimus-tui/src/transcript.rs
  - apps/optimus-tui/src/view.rs
  - apps/optimus-tui/src/keys.rs
  - apps/optimus-tui/src/mouse.rs
---

# ADR-0075: A block owns its lifecycle; rows only paint it

- **Status:** Accepted
- **Date:** 2026-08-02
- **Phase 1 delivered:** 2026-08-02 — `workbench/mod.rs` lands the §1
  contract as a 1:1 mirror of the `Message` vector (`blocks()[i]` describes
  `messages[i]`), fed only by typed events at four mutation sites. One
  deviation from the §1 sketch, forced by the kernel's actual types:
  `ToolLifecycleEvent::event_id` and `run_id` are `String`, so provenance
  carries the event ids verbatim and `turn_id` is parsed to a `Uuid` only
  when the run id really is one — a fixture id like `run-1` stays `None`
  rather than being invented.
- **Phase 2 delivered:** 2026-08-02 — the row projection becomes total. Rows
  are painted from the workbench's *items* rather than from the `Message`
  vector, every row names the block it paints, folding and semantic selection
  land, and Tab moves the keyboard between the composer and the transcript
  (§4). Four deviations from the phase sketch, each recorded below in
  Consequences: grouping is derived rather than parented, what may be folded
  is read from `optimus-packs::ToolPolicy` rather than from a name list, a run
  is three calls before it folds, and the fullscreen viewer and copy action
  the program lists under this phase move to the phases that give them
  something worth viewing and copying.
- **Phase 3 delivered (command blocks):** 2026-08-02 — a command's real output
  reaches the screen. `ToolLifecycleEvent::outcome` already carries the tool's
  own structured result parsed back into JSON (`turn_loop.rs` builds
  `ToolOutcome::succeeded` from it), so `terminal`'s `stdout`, `stderr`,
  `exit_code`, `truncated_stdout`, `truncated_stderr` and `timed_out` are
  typed facts the surface reads rather than prose it parses. A command becomes
  one evolving block whose summary line folds open onto its bounded output;
  every place something was left out says so. What this slice does **not**
  deliver, and why, is recorded under Consequences: file-edit detail and
  syntax-highlighted diffs have no event source yet, and `Timing` stays
  unconsumed because per-call durations already arrive on the lifecycle event.

## Context

The owner has commissioned a structured-workbench program for the terminal:
replace the linear stream of text rows with durable, foldable, interactive
blocks driven by real domain events, animated smoothly, measurable, and
reversible — explicitly not a visual reskin, and explicitly not a rewrite in
another language or framework. This ADR is the program's phase 0 deliverable:
the audit of what exists, the smallest stable contracts for blocks, events,
and animation, the honest classification of what each future block kind can be
driven by today, and the phase map that lands the work without ever leaving
`main` red. Phase 0 changes no user-facing behaviour.

### What the terminal is today (Confirmed current behaviour)

**Modules.** `apps/optimus-tui/src/` holds session.rs (1592 raw lines, 796
production — four under the module-size ratchet's hard 800; no
`apps/optimus-tui` file is grandfathered in the baseline), commands.rs (870
raw, under the limit once `#[cfg(test)]` is excluded per ADR-0049), view.rs
(612), transcript.rs (600), composer.rs (485), keys.rs (439), lib.rs (387),
mouse.rs (325), tool_line.rs (266), completion.rs (234), view/composer.rs
(218), preferences.rs (212), session/approval.rs (197), history.rs (187),
logging.rs (181), picker.rs (165), activity.rs (90), session/reservation.rs
(44), plus tests/pty.rs (567) driving the shipped binary through a real PTY.

**Event flow.** The kernel already speaks typed lifecycle events, not logs.
`StreamEvent { TextDelta, ThinkingDelta, Tool(Box<ToolLifecycleEvent>),
Status, Timing }` (crates/optimus-kernel/src/lib.rs:295) carries
`ToolLifecycleEvent { schema_version, event_id, run_id, call_id, tool_id,
phase, summary, duration_ms, outcome, approval }` with
`TOOL_LIFECYCLE_SCHEMA_VERSION = 1` and `ToolLifecyclePhase { Started,
ApprovalRequired, Succeeded, Failed, Cancelled, Suppressed, Ambiguous }`
(lib.rs:312). The sink returns `StreamControl { Continue, Cancel }`
(lib.rs:367), so cancellation is in-band. The terminal's `stream_sink`
(apps/optimus-tui/src/session.rs:56) maps this onto `TurnUpdate {
SessionReserved, Text, Status, Tool(ToolStep), Approval(Box<
ToolApprovalBinding>), ApprovalSettled, Done, Failed }` over an `mpsc`
channel; `pump()` drains it in per-frame batches into `Vec<Message>` where
`Message { role, text, call_id }` and `apply_tool_step` rev-finds the row by
`call_id` and rewrites it in place. Two events are dropped on the floor today:
`ThinkingDelta` and `Timing` (session.rs's match arm discards them with a
comment that they "belong in a dock"). A vanished worker channel without a
terminal update is detected and reported as a worker panic with the log path.

**Render loop.** `event_loop` (apps/optimus-tui/src/lib.rs) runs a fixed
40 ms cadence: every iteration pumps, ticks, syncs mouse capture, then calls
`terminal.draw` unconditionally before polling input with a 40 ms timeout —
roughly 25 full draws per second while completely idle. There is no dirty
tracking and no notion of animation modes; the single braille spinner advances
every second frame (`SPINNER_EVERY = 2`, ~12 steps/s). Terminal restoration is
already complete and correct: raw mode, bracketed paste, mouse capture,
alternate screen, and cursor are restored on exit and from a panic hook that
checks the owning thread so a worker panic cannot tear down a live screen.

**Input.** Key handling is pure intent functions: `keys::Mode { picker, busy,
drafting, suggesting }` maps events to a typed `Intent`. The picker owns the
keyboard wholly (early return); the suggestion overlay borrows exactly
Tab/Up/Down when no modifiers are held and lets everything else fall through
(ADR-0074). Mouse handling computes `Regions { transcript, track }` from the
area plus `composer_height` in one place shared by painting and hit-testing;
wheel scrolls anywhere; right-click opens the command menu; there is no hover
concept and no general hit map.

**Approval continuity.** `ApprovalRequired` with a binding becomes an approval
card holding the exact `ToolApprovalBinding { run_id, call_id, tool_id,
job_id, node_id, node_index, effect_sha256, summary }`. Resolution calls
`chat_approval_resolve` with the held binding — never re-derived from what the
renderer shows — and `ApprovalSettled(call_id)` clears only the matching card,
because a resumed turn can park again before the resolver returns (ADR-0046).

**What the host can push.** `handle_ipc`
(crates/optimus-host/src/router.rs:141) exposes a frozen, test-pinned method
table. Every method is request/response. The only streaming source in the
entire host surface is the chat turn / approval-resolve `StreamEvent` sink.
Plans, background jobs, campaigns, cron, gateway traffic, and browser actions
have polling endpoints only (`jobs_list`, `campaign_status`, `cron_history`,
`gateway_inbox`, `browser_navigate`, …). Any block kind for those domains
therefore has no push event to subscribe to today.

**Prior audit lineage.** Of the July terminal audit's findings
(docs/architecture/tui-quality-audit-2026-07.md): U1 composer defects are
fixed; U2 (durable sessions unreachable from the terminal — no /sessions or
/resume, `get_session` never called) and U3 (no project concept on terminal
turns) remain open and map to phase 13 below; U4 is fixed — verified
2026-08-02, `/access` now narrows out of break-glass and the unrestricted
word is refused everywhere except the literal `/yolo`
(apps/optimus-tui/src/commands.rs:369-413, with tests); U5 remains partially
open — markdown renders bold only, fenced code paints as body text.

**Already-landed overlap.** Predictive slash completion exists and is
non-modal (ADR-0074: the surface owns its catalog; suggestions overlay the
transcript and borrow three keys). The performance harness
(scripts/perf_harness.py, baseline docs/architecture/perf-baseline.json,
schema `optimus-perf/1`) measures the eight CLI turn scenarios offline; it
does not measure terminal frames, and no input-to-render latency baseline
exists anywhere.

## Decision

The terminal becomes a workbench of typed blocks. Four contracts and one
honesty rule govern every phase; they are deliberately the smallest shapes
that satisfy the program, and they extend what exists rather than replacing
it.

### 1. The block contract

A block is semantic state keyed by domain identity, never by row position.

```rust
struct BlockId(Uuid);

enum BlockLifecycle {
    Queued, Running, Waiting, Blocked,
    Succeeded, Failed, Cancelled, PossiblyStalled,
}

struct WorkbenchBlock {
    id: BlockId,
    kind: WorkbenchBlockKind,       // grows phase by phase; see §5
    lifecycle: BlockLifecycle,
    turn_id: Option<Uuid>,          // run_id of the owning turn
    parent_id: Option<BlockId>,     // grouping / nesting
    presentation: BlockPresentation, // expanded, user_changed_expansion,
                                     // selected_tab, pinned
    started_at: Option<Instant>,
    settled_at: Option<Instant>,
    provenance: Vec<Uuid>,          // kernel event_ids that drove this block
}
```

Rules:

- Tool blocks key on the kernel's `call_id`; turn-level blocks key on
  `run_id`. `BlockId` is minted at adaptation time and stable for the block's
  life across streaming, folding, resize, and redraw.
- Selection is semantic — `selected_block: Option<BlockId>` — and survives
  reflow. Rendered rows are a projection computed at draw time; nothing reads
  row indices back into state.
- `PossiblyStalled` is a warning derived from real absence of events, never a
  declared fact; `lifecycle` moves only on typed events.
- `user_changed_expansion` records that a human touched the fold; streaming
  output must never reopen or close such a block.
- The existing `Message` vector remains, during migration, as the
  compatibility projection: phase 1 wraps today's user/assistant/tool/status
  rows as blocks without changing what paints, and the PTY suite proves it.

### 2. The event contract

Typed domain events are the only source of lifecycle truth.

- The spine is what already exists: kernel `StreamEvent` →
  `stream_sink` → `TurnUpdate` → `pump`. `TurnUpdate` is the adapter seam —
  ADR-0045 already names it the shape a future stdio/WebSocket transport
  publishes — and the session split (phase 1 prerequisite) formalizes it as
  an `event_adapter` module.
- Extending the workbench means extending the typed spine: a new block
  lifecycle needs either a new typed kernel/host event or an explicit typed
  poller over an existing request/response method. Parsing log or status text
  to move lifecycle is prohibited. Logs may appear inside a block's body;
  they never control its state.
- `ThinkingDelta` and `Timing` stop being dropped in the phase that ships the
  block consuming them (thinking presence in phase 4's activity treatment,
  durations on tool blocks in phase 3). Until then dropping them remains
  correct.
- Approval blocks carry the exact `ToolApprovalBinding` and settle only by
  `call_id`, exactly as today (ADR-0046). The policy layer stays
  authoritative; the workbench renders it and never widens a grant.

### 3. The animation contract

One clock owns all motion; domain events own all truth.

```rust
enum AnimationMode { Off, Fps30, Fps60, Adaptive }

struct AnimationClock {
    mode: AnimationMode,
    reduced_motion: bool,
    // next_wake() -> Option<Instant>; injectable fake clock for tests
}
```

- The event loop becomes: poll input with timeout = `next_wake`; handle input;
  drain domain events; tick animation if due; draw only if dirty. This
  replaces the unconditional 40 ms draw. Idle means no redraw and no wake.
- Tick rate and render rate are independent; stream deltas coalesce at frame
  boundaries (pump's per-frame batching already does this — it is kept).
- Spinners animate only on visible, running blocks, at spinner-family rates
  (~15 FPS), not the frame ceiling. Nothing animates progress that no event
  reported; complete text is never replayed as fake typing.
- `reduced_motion` and `AnimationMode::Off` degrade to static indicators with
  identical semantics.

### 4. Keyboard reconciliation

The program's key map meets the landed completion behaviour as follows: Tab
completes while the suggestion overlay is open (landed, ADR-0074), and toggles
Composer↔Inspect focus otherwise (new). The picker keeps whole-keyboard
ownership; the suggestion overlay keeps borrowing exactly Tab/Up/Down. Esc
unwinds outermost-first: overlay, then suggestions, then Inspect focus, then
draft. Hover, when it arrives in phase 6, never executes actions.

### 5. What is buildable now, honestly

Block kinds ship only with a real event source, and the classification is
recorded here so no phase scaffolds a lie:

| Block kind | Source today | Standing |
|---|---|---|
| UserPrompt, AssistantAnswer | `TextDelta`, `Done` | Confirmed current behaviour — adapt in phase 1 |
| StatusNote | `Status` | Confirmed — phase 1 |
| ToolCall (incl. grouping) | `ToolLifecycleEvent` (stable `call_id`, `tool_id`, phases, outcome) + `optimus-packs` `ToolPolicy` | Confirmed current behaviour — grouping delivered in phase 2; command/edit/diff detail in phase 3 |
| ApprovalRequest | `ApprovalRequired` + binding, `ApprovalSettled` | Confirmed — phase 9 restyles what exists |
| ThinkingActivity, timing detail | `ThinkingDelta`, `Timing` (currently dropped) | Confirmed events, unconsumed — phases 3–4 |
| Plan, AgentRun, BackgroundJob, BrowserRun, GatewayActivity | request/response IPC only (`jobs_list`, `campaign_status`, `gateway_inbox`, …) | Planned behaviour — needs typed pollers or new kernel/host emission in phases 7, 8, 11, 13 |
| MemoryRecall, ContextComposition, VerificationEvidence | host methods exist (`memory_recall`, …); no turn-scoped events | Planned behaviour — phases 10, 12 |

### 6. Delivery phases and file impact

Each phase lands separately through managed delivery and leaves `main` green
and the terminal usable. Prerequisite before any phase-1 code: split
session.rs (796/800 production lines) along its existing seams — session
state, event adapter, turn workers — since phase 1 must add code where no
headroom exists. Never add a baseline entry instead of splitting.

| Phase | Scope | Files principally touched |
|---|---|---|
| 0 | This ADR; contracts, audit, baselines; no behaviour change | docs/decisions/ |
| 1 | Block foundation: `BlockId`, `WorkbenchBlock`, adapter, existing rows as blocks, semantic selection | session.rs split → session/{mod,event_adapter,workers}.rs, new workbench/ module, view.rs |
| 2 | Folding + grouping of adjacent low-risk tool calls | workbench/, transcript.rs, keys.rs |
| 3 | Command/edit/diff blocks; durations from `Timing`; markdown/code paint (closes U5) | tool_line.rs, transcript.rs, workbench/ |
| 4 | Animation foundation: clock, modes, dirty-frame loop; frame bench layer | lib.rs, new animation/, activity.rs |
| 5 | Composer growth: typed argument completion, history search, prompt queue | completion.rs, composer.rs, commands.rs |
| 6 | Mouse hit map + overlay stack; hover | mouse.rs, view.rs, new overlay/ |
| 7 | Plan blocks (typed poller or new emission) | kernel/host first, then workbench/ |
| 8 | Agent/job dashboards with stall warnings | host jobs surface, workbench/ |
| 9 | Approval cards inline ([Allow once] [Allow for project] [Deny]) | session/approval.rs, view.rs |
| 10 | Memory-recall + context blocks, provenance-labelled | host memory surface, workbench/ |
| 11 | Browser run blocks | host browser surface, workbench/ |
| 12 | Verification/provenance/reversibility (undo order: safe ops → captured edits → workspace restore; never conversation-only undo) | runtime + workbench/ |
| 13 | Sessions/worktrees/projects reachability (closes U2, U3) | commands.rs, session.rs, host sessions surface |
| 14 | Polish, accessibility, optimization | across apps/optimus-tui |

Phases 7–13 that need new event sources begin kernel/host-side, ride the
SmartDeny approval spine for any durable effect, and only then grow their
blocks — the terminal never becomes a separate state island from the desktop
app, because both consume the same host surface.

## Alternatives considered

- **Rewrite the terminal in a richer UI stack** (React/Ink, webview, or a
  desktop-app embed). Rejected: the program mandate keeps Rust/Ratatui, and
  ADR-0045's transport design already gives the terminal the same host
  surface as Electron — the gap is structure, not stack.
- **Derive block lifecycle from rendered text or log lines.** Rejected: the
  kernel already emits versioned typed lifecycle with stable identity
  (`TOOL_LIFECYCLE_SCHEMA_VERSION = 1`); parsing prose would recreate exactly
  the fragility that schema exists to prevent.
- **Keep the linear `Message` vector and bolt folding onto row ranges.**
  Rejected: row-keyed state breaks on every resize, reflow, and mid-stream
  rewrite; `apply_tool_step`'s call_id-keyed rewrite is the existing proof
  that identity-keyed state is the workable shape.
- **Scaffold all block kinds now and attach sources later.** Rejected: a
  plan or agent dashboard with no event source is an unavailable capability
  presented as working, which the evidence rules prohibit.
- **Per-widget animation timers.** Rejected: uncoordinated wakes make idle
  CPU unbounded and untestable; a single clock with `next_wake` keeps idle at
  zero and injects cleanly into tests.
- **Add a module-size baseline entry for session.rs instead of splitting.**
  Prohibited outright by law 21; the split is the prerequisite.

## Reasons

- **The typed spine already exists.** Text, status, tool lifecycle, and
  approval binding all arrive as typed events with stable ids; the workbench
  is a generalization of what `stream_sink` → `TurnUpdate` → `pump` already
  does, not a new invention.
- **Row projection is already half-true.** `transcript::laid_out` computes
  rows at draw time and nothing persists them — except selection and
  scrolling, which is precisely where today's model runs out. Making the
  projection rule total is the smallest fix that covers folding, hover, and
  10k-block sessions.
- **The idle cost is real and sourced.** ~25 unconditional full draws per
  second while nothing happens is arithmetic on `lib.rs`, not an estimate;
  the dirty-frame loop turns that to zero without touching truth.
- **The honesty table prevents the expensive failure.** The costliest
  workbench bug is a dashboard that looks live and is not; recording each
  kind's real source in the decision makes that unshippable by construction.

## Consequences

- Selection, folds, and approvals survive streaming and reflow because they
  key on domain identity; `apply_tool_step`'s call_id rewrite generalizes
  from one special case into the rule.
- **Grouping is derived, not stored** (phase 2). A run of repeated calls is
  recomputed from the block list on every projection, so a run growing as more
  calls arrive re-keys nothing and `parent_id` stays unused by grouping. The
  one thing a human owns — whether the run is open — lives on the run's first
  block, which is the block a growing run cannot move.
- **What may be folded comes from the tool contract, not from a list of
  names** (phase 2). Folding hides work, so the rule for what may be hidden
  reads each tool's declared `ToolPolicy` from `optimus-packs::builtin_catalog`
  and folds only `WorkspaceRead`, `MemoryRead`, `SkillRead`, and `NetworkRead`.
  Anything the catalog does not carry is unknown rather than assumed harmless,
  and `Browser` is excluded because navigate, snapshot, and click share one
  policy — the contract cannot tell the observation from the effect. This
  costs `optimus-tui` a direct `optimus-packs` dependency, which is the
  canonical tool contract this repository already names as authoritative; the
  alternative was a name list in the terminal that drifts silently in the
  direction of hiding an effect.
- **A run is three calls before it folds** (phase 2). Two rows becoming one
  header saves a single row and costs the reader both call summaries.
- **The fullscreen viewer and the copy action move to their own phases.** In
  phase 2 nothing has a body that does not already fit: tool rows are one
  clipped line each, so a viewer would open onto the row it was opened from
  and a copy would yield that same line. Both become worth having in phase 3,
  when command output and diffs arrive, and a viewer is an overlay — building
  one before phase 6's overlay stack is exactly the widget-specific hack that
  phase forbids. Recorded here rather than dropped.
- Reverse video marks the selected item, because it is the one emphasis every
  terminal has: which block the keyboard is pointed at never depends on a
  colour a `NO_COLOR` or monochrome session will not render. Phase 14 may
  refine the treatment; it may not make it colour-only.
- **A command result is recognised by its shape, not its tool id** (phase 3).
  `exit_code` is the field only a command outcome carries, so a pack that runs
  commands through a differently named tool gets the same block, and a tool
  merely named like one does not. An outcome this surface has no reader for
  stays bodyless and its block does not pretend to open.
- **Output is bounded twice and both bounds are stated** (phase 3). The runtime
  cuts a stream at its own capture limit and says so in `truncated_*`; this
  surface keeps at most 200 lines so a hundred-thousand-line build does not
  become a transcript to re-lay-out every frame. The *tail* is kept, because
  that is where a failed command says why, and both cuts print a line. A tail
  shown without a notice reads as the whole output.
- **File-edit detail and diffs are not in this slice, and cannot be yet.**
  `patch_file` takes `old_string`/`new_string` as call *arguments*, and
  `ToolOutcome` carries only the result — no path, no before/after, no diff
  (`tool_dispatch.rs`; `enrich_workspace_tool_data` adds `relative_path` for
  `write_file` alone). A diff block therefore needs a kernel-side change to
  the edit result first, exactly as phases 7–13 need theirs; inventing one
  from the summary preview is the log parsing this decision prohibits. The
  next phase-3 slice makes that kernel change and then grows the block.
- **`Timing` stays unconsumed** (phase 3). §2 anticipated durations arriving
  with it, but `ToolLifecycleEvent::duration_ms` already carries the per-call
  duration the rows show, so consuming `TimingEvent` now would add a second
  path to the same number. It lands with the first block that needs
  turn-relative timing rather than being wired up for appearances.
- The idle terminal stops drawing ~25 frames a second; dirty-tracking bugs
  become the new risk, mitigated as recorded under Risks.
- Every future block kind inherits an honesty bar: no plan/agent/job/browser
  block may appear to work before its event source exists, and stall warnings
  stay warnings.
- Learning data, when it arrives, records interaction shape (folds, retries,
  latencies) and never prompt text, file contents, command output, or private
  gateway content by default; remote telemetry is opt-in.
- The session.rs split spends churn now to keep the module-size ratchet
  honest; the alternative — hand-editing the baseline — is prohibited.

## Risks

- **Under-drawing.** A dirty-frame loop can miss a repaint and show stale
  state. Mitigation: every state mutation sets the dirty bit at one choke
  point (the adapter), a property test asserts any valid event sequence ends
  drawn-when-changed, and the differential rule applies — disable the dirty
  check and prove the test fails.
- **Split regressions.** Splitting session.rs can disturb approval
  continuity, the most safety-critical path in the terminal. Mitigation: the
  PTY suite and the approval unit tests pin behaviour before the split; the
  split is landed alone, before any workbench code.
- **Poller dishonesty.** Phases that poll request/response methods can drift
  into fabricated liveness. Mitigation: `PossiblyStalled` derives only from
  measured event absence against stated thresholds and renders as a warning,
  never a declared fact; no remaining-time estimates without evidence.
- **Scope creep into one giant branch.** Mitigation: each phase lands
  separately through managed delivery with its own acceptance criteria, and
  `main` stays green and usable after every slice.

## Evaluation evidence

- **Confirmed current behaviour.** apps/optimus-tui/tests/pty.rs (567 lines;
  40×20 PTY, cursor row 17) drives the shipped binary and pins composer,
  transcript, picker, and suggestion behaviour — the behavioural baseline
  phase 1 must preserve.
- **Confirmed current behaviour.** The render loop draws unconditionally on
  a fixed 40 ms cadence (~25 draws/s idle; spinner ~12 steps/s;
  `pump()` batches stream updates per frame) — read directly from
  apps/optimus-tui/src/lib.rs. No measured input-to-render latency baseline
  exists; the frame bench layer is deferred to phase 4 and must begin with a
  current best-practice search on the date of that work (AGENTS.md workflow
  step 6).
- **Confirmed current behaviour.** `stream_sink`
  (apps/optimus-tui/src/session.rs:56) discards `ThinkingDelta` and `Timing`
  today; every other kernel event reaches the transcript typed.
- **Confirmed current behaviour.** The host IPC table
  (crates/optimus-host/src/router.rs:141) is request/response only; the chat
  turn / approval-resolve sink is the sole push source, which grounds the
  planned-vs-confirmed table in §5.
- **Confirmed current behaviour.** `/access` narrows out of break-glass and
  unrestricted synonyms are refused (apps/optimus-tui/src/commands.rs:369-413
  with tests) — the July audit's U4 is closed; U2, U3, and U5 remain open as
  mapped.
- **Confirmed current behaviour.** scripts/perf_harness.py and
  docs/architecture/perf-baseline.json (`optimus-perf/1`) baseline the eight
  CLI turn scenarios; they do not measure terminal frames.
- Phase 0 is docs-only: no source file changes, so the existing suites stand
  unchanged as the baseline the next phase diffs against.

## Conditions for reconsideration

- The host grows a real push/subscription transport (the WebSocket lane
  ADR-0045 anticipates): revisit every phase-7-to-13 poller in favour of
  subscriptions.
- Dirty-frame tracking produces a class of missed-draw regressions in
  practice: fall back to drawing on every input and domain event with idle
  suppression only, keeping the clock.
- Adaptive animation cannot hold its budget on target hardware: re-scope the
  default mode rather than faking smoothness.
- A desktop-app workbench emerges with its own block model: the contracts
  merge at the host seam rather than forking per surface.
- The module-size law or its measurement (ADR-0049) changes: re-derive the
  split plan before phase 1.

## Relevant code

- apps/optimus-tui/src/workbench/mod.rs — the §1 contract implemented:
  `BlockId`, `WorkbenchBlock`, `BlockLifecycle`, `BlockPresentation`,
  `WorkbenchState` with semantic selection; lifecycle moves only on typed
  events, and settlement spares a `Blocked` block because its truth is the
  held approval binding, which outlives the worker.
- apps/optimus-tui/src/session/event_adapter.rs — `stream_sink`, `TurnUpdate`,
  `pump`, `apply_tool_step`; the adapter seam, split out of session.rs along
  the seams this ADR prescribed. `ToolStep` now carries the typed
  `run_id`/`event_id`/`phase` facts the block mirror consumes, and `pump`
  drives the mirror's hold/settle transitions beside the row updates.
- apps/optimus-tui/src/workbench/grouping.rs — the derived projection: what a
  run is, what breaks one, and the honesty rule that a call still running,
  waiting on a human, failed, or cancelled is never folded away.
- apps/optimus-tui/src/workbench/detail.rs — the typed body read from
  `ToolOutcome::data`: a command's streams, exit code, timeout, and both
  truncation bounds, with the tail kept and every cut stated.
- apps/optimus-tui/src/workbench/effects.rs — the fold rule read from
  `optimus-packs::builtin_catalog`, with `Browser` excluded and unknown ids
  treated as unfoldable.
- apps/optimus-tui/src/workbench/selection.rs — `FocusRegion`, movement over
  items rather than blocks, and the fold toggle that records a human touched
  it.
- apps/optimus-tui/src/lib.rs — the event loop this decision's animation
  contract replaces; `anchor` keeps the selected block on screen while the
  transcript has the keyboard, and the mouse path reaches the same two moves
  the keyboard does.
- apps/optimus-tui/src/transcript.rs, view.rs — the row projection: rows are
  painted from items, each names its block, and `view::hit` reads a screen row
  back to the block it paints without parsing what the row says.
- apps/optimus-tui/src/keys.rs, mouse.rs — intent functions, region
  hit-testing, the Tab reconciliation, and inspect focus as the second
  whole-keyboard claim after the picker.
- apps/optimus-tui/src/session/approval.rs — exact-binding resolution the
  workbench must not disturb.
- crates/optimus-kernel/src/lib.rs — `StreamEvent`, `ToolLifecycleEvent`,
  `StreamControl`.
- crates/optimus-host/src/router.rs — the frozen method table grounding the
  §5 classification.

## Relevant tests

- apps/optimus-tui/tests/pty.rs — end-to-end behavioural pin on the shipped
  binary.
- apps/optimus-tui/src/keys.rs, mouse.rs, session.rs, commands.rs,
  completion.rs unit suites — intent tables, regions, pump batching, catalog
  and suggestion behaviour.
- apps/optimus-tui/src/workbench/mod.rs unit suite plus the session.rs
  mirror tests — phase mapping honesty, provenance citation, identity across
  streaming, mid-stream selection stability, park survival, and the row↔block
  lockstep across scripted turns (the differential check: disabling
  settlement fails the lockstep test).
- apps/optimus-tui/src/workbench/{grouping,effects,selection}.rs unit suites —
  every block painted exactly once however it is grouped, a write or a failure
  breaking a run, a call still running or waiting on a human never folded, no
  run crossing a turn, the classification checked against the catalog's own
  declared policies, movement over items, and both ends stopping rather than
  wrapping.
- apps/optimus-tui/src/session.rs — a fold a human opened surviving the rest of
  the turn streaming into it; the pointer and the keyboard leaving one screen;
  ten thousand blocks still projecting and navigating; a command arriving as
  one block whose output opens, a failed command staying inspectable, and a
  later event never erasing a body the call already reported.
- apps/optimus-tui/src/workbench/detail.rs — an exit code that never arrived
  not reading as success, both truncation bounds stated, the tail kept, and an
  outcome with no reader opening onto nothing.
- apps/optimus-tui/tests/pty.rs — Tab handing the keyboard to the transcript
  and back through the shipped binary, letters typed while inspecting never
  reaching the draft, and an SGR click selecting a block.
- Planned behaviour: property tests for event-order/resize permutations, the
  dirty-draw differential test, and the phase-4 frame bench do not exist yet
  and land with their phases.

## Explicit non-claims

- No claim that plan, agent, job, browser, memory, context, verification, or
  gateway blocks work today — §5 records them as planned, and their event
  sources do not exist yet.
- No claim of measured terminal frame latency; the only measured performance
  baseline is the CLI harness.
- No claim that thinking content is exposed: `ThinkingDelta` may drive a
  presence indicator, never hidden chain-of-thought display.
- Phases 1–14 are not delivered by this ADR; it delivers the contracts they
  are built against.
