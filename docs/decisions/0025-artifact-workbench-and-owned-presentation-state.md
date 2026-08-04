---
doc_id: decisions-0025-artifact-workbench-and-owned-presentation-state
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0025: Artifact workbench and owned presentation state, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - DESIGN.md
  - apps/optimus-desktop/src/ui.rs
  - apps/optimus-desktop/ui/index.html
  - apps/optimus-desktop/ui/app.js
  - apps/optimus-desktop/ui/vantage.css
  - apps/optimus-desktop/e2e/02-shell-and-composer.spec.js
  - apps/optimus-desktop/e2e/07-vantage-design.spec.js
  - specs/001-desktop-shell/assets/**
depends_on:
  - docs/decisions/0014-native-webview-ipc-mode.md
validated_by:
  - apps/optimus-desktop/e2e/06-preview-browser.spec.js
  - apps/optimus-desktop/e2e/07-vantage-design.spec.js
  - apps/optimus-desktop/src/ui.rs
---

# ADR-0025: Artifact workbench and owned presentation state

- **Status:** Accepted
- **Date:** 2026-07-23

## Context

The desktop shell already exposed the correct operational surfaces for coding-agent
work: session scope, conversation, an evidence inspector, a native Preview
Browser, terminal output, approvals, and task state. Their presentation semantics
were not owned at one boundary. Pane visibility mixed the HTML `hidden` attribute
with local `display` rules, two generations of CSS overrode each other, and live
assistant output replaced the complete bubble subtree on every display-frame
commit.

Those local choices produced three related risks: pane transitions could not be
coordinated with native Browser geometry, compact responsive states could reveal
clipped controls, and streamed text could recreate readable content while tokens
arrived. A broad editor rewrite would jeopardize stable DOM IDs, IPC methods,
persisted widths, approvals, cancellation, and the persistent Wry/WebKitGTK
Browser child.

Current primary-source product evidence also points away from chat-only coding
agents. Cursor describes an interface centered on agents rather than files;
OpenAI's Codex app emphasizes parallel worktrees and reviewable diffs; GitHub
exposes isolated parallel agent sessions; and Linear describes purpose-built AI
workbenches. This supports an artifact-centered workbench as a bounded forecast,
not as a proven 2027 consensus.

## Decision

1. Adopt **Optimus Vantage**, the compact artifact-centered workbench specified in
   `DESIGN.md`, as the desktop presentation system.
2. The workspace shell owns one persisted presentation state for the left scope
   rail, right evidence inspector, execution dock, and logs drawer.
3. Right, terminal, and log surfaces remain mounted after boot. Visibility is
   represented by `open`, `pane-hidden`, `aria-hidden`, and `inert`, permitting
   reversible transitions without changing stable element IDs or IPC contracts.
4. The left rail collapses to a 48 px command rail rather than disappearing at
   desktop/tablet widths. At narrow focus widths the evidence inspector becomes a
   bounded overlay only when explicitly open, while the work canvas remains the
   underlying primary surface.
5. `vantage.css` is the sole new presentation-policy seam. It is embedded after
   the legacy stylesheet by `ui.rs`; removing that include restores the prior
   visual layer without a DOM or runtime migration.
6. Pane motion is time-based and display-clocked: state changes use bounded
   70–150 ms transitions with monotonic easing and no spring, overshoot, or
   inertial tail. Divider drag has zero transition and updates geometry directly.
7. Browser-affecting shell transitions reuse the existing Browser resize pulse;
   the native child still owns one in-flight request and one latest pending bounds
   snapshot.
8. Stream chunks are accumulated and committed at most once per
   `requestAnimationFrame`. The live `.bubble-body` node remains stable and uses
   plain readable text while streaming; rich Markdown is applied only at the
   final state boundary.
9. Text, code, Browser pixels, and resize surfaces never use blur filters as
   simulated motion blur. Direction is conveyed by short translation, opacity,
   and a static edge shadow so readability survives movement.
10. `prefers-reduced-motion` removes transitions and transforms without altering
    final layout, focus order, state, or information.
11. Compact controls retain a minimum 24 px target, visible keyboard focus, short
    icon-plus-label naming where icons alone are ambiguous, and text labels for
    destructive or approval-sensitive actions.
12. A 240 Hz display is a design budget, not a certification claim. The 4.17 ms
    budget and frame-safe ownership are normative; physical 240 Hz evidence is
    required before claiming measured 240 fps.

## Alternatives considered

### Restyle the existing selectors in place

Rejected. The prior stylesheet already contained successive override layers.
Adding another unbounded patch would preserve ambiguous ownership and make
rollback depend on reconstructing selector precedence.

### Replace the desktop with a new editor framework

Rejected. A new React/Electron/editor shell would expand the migration across
IPC, approvals, Browser embedding, persistence, packaging, and accessibility
without evidence that those contracts are wrong.

### Animate every streamed token or apply CSS blur

Rejected. Per-token animation changes reading geometry continuously; blur/filter
work adds paint cost and reduces code legibility. One stable live text node is
both faster and calmer.

### Use spring physics for panes

Rejected. Overshoot and inertial continuation are disorienting in a dense tool
where users aim at controls immediately after a state change. Bounded monotonic
motion makes the endpoint predictable.

### Claim literal 240 fps from headless browser tests

Rejected. Headless Playwright proves state, layout, scheduling, and regressions,
not the compositor cadence of a physical 240 Hz panel.

## Reasons

One presentation owner compresses several visible problems without moving domain
or security boundaries. The new stylesheet can be removed independently; the
state seam preserves mounted DOM and accessibility semantics; stable stream nodes
eliminate subtree churn; and Browser geometry continues through the already
verified native scheduler. The system becomes easier to reason about because
visual state, runtime state, and native surface state no longer use unrelated
visibility representations.

## Consequences

- The shell is denser: 40 px titlebar, 30 px rows, 32 px send control, 48 px
  collapsed rail, 22 px status strip, and compact inspector/terminal headers.
- Empty state offers real supported starting actions rather than decorative
  whitespace or fictitious recent work.
- Completed task panels close automatically; details remain available through the
  task control instead of covering the transcript.
- The right inspector and execution dock animate while mounted, so transition
  endpoints remain measurable and focus is removed from hidden surfaces.
- Live Markdown formatting appears at completion rather than being reparsed for
  every chunk.
- The legacy stylesheet remains beneath the reversible Vantage seam. This is an
  intentional rollback boundary, not a second source of current design truth.

## Risks and unresolved boundaries

- CSS grid width and terminal height transitions perform bounded layout work. A
  physical 240 Hz capture is still required before any measured high-refresh
  performance claim.
- Native Browser pixels cannot be transformed by CSS; shell transitions therefore
  pulse exact geometry and reveal the Browser only at valid bounds.
- Some long localized labels may require responsive text reduction even though
  current English labels fit verified widths.
- The 2027 market forecast may be disproved if coding-agent products converge on
  file-first IDEs or chat-only shells instead of reviewable work objects.
- The legacy stylesheet can still affect selectors not explicitly governed by
  Vantage. New presentation work must extend `vantage.css`, not append a third
  policy layer.

## Evaluation evidence

- `npx playwright test --reporter=line`: 60 desktop tests pass, including the new
  presentation-state, stable-stream-node, direct-resize, reduced-motion, and all
  existing Preview Browser geometry contracts.
- `cargo test -p optimus-desktop`: 34 native/desktop tests pass.
- Real HTTP-harness screenshots verify 1600 × 1000 workbench and empty states,
  960 × 760 split view, and 640 × 800 focus view.
- `npx -y @google/design.md lint DESIGN.md`: zero schema errors.
- `cargo build --release -p optimus-desktop`: optimized build succeeds.

## Relevant code

- `DESIGN.md`
- `apps/optimus-desktop/ui/vantage.css`
- `apps/optimus-desktop/ui/app.js`
- `apps/optimus-desktop/ui/index.html`
- `apps/optimus-desktop/src/ui.rs`

## Relevant tests

- `apps/optimus-desktop/e2e/07-vantage-design.spec.js`
- `apps/optimus-desktop/e2e/06-preview-browser.spec.js`
- `apps/optimus-desktop/e2e/02-shell-and-composer.spec.js`

## Conditions for reconsideration

Reconsider the workbench topology if task evidence shows that another primary
work object produces faster verified review with lower navigation cost. Reconsider
durations only from physical-display motion evidence and accessibility testing,
not aesthetic preference alone. Replace the reversible stylesheet seam only after
equivalent DOM, IPC, Browser geometry, reduced-motion, and rollback proofs exist.
