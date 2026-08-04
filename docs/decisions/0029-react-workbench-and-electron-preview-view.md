---
doc_id: decisions-0029-react-workbench-and-electron-preview-view
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0029: React workbench and Electron preview view, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - apps/optimus-ui/**
  - apps/optimus-desktop/src/server.rs
  - crates/optimus-host/src/**
  - docs/contracts/desktop-ipc-methods.md
depends_on:
  - docs/decisions/0025-artifact-workbench-and-owned-presentation-state.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
validated_by:
  - apps/optimus-ui/src/**/*.test.ts
  - apps/optimus-ui/src/**/*.test.tsx
---

# ADR-0029: React workbench and Electron preview view

- **Status:** Accepted
- **Date:** 2026-07-23

## Context

The initial React application proved the Vite and Rust-host seams but exposed
only sessions, an undifferentiated stream log, and a composer. The legacy Wry
Vantage UI still owned the complete product surfaces. Electron loaded the
legacy host document by default, and its production renderer depended directly
on the Rust bearer token.

The required cutover is larger than a restyle. Session-owned streaming, durable
approvals, native Browser pixels, responsive surface changes, and direct
divider geometry each need an explicit owner. The work must preserve every
Rust IPC method, database, safety decision, and terminal outcome while avoiding
a claim that the user-facing Electron preview is the Rust agent Browser.

## Decision

1. **React is the default Electron product renderer.** The production SPA is
   built under `apps/optimus-ui/dist` and served through
   `optimus-app://ui/`. `OPTIMUS_ELECTRON_UI=legacy` retains the assembled Wry
   document as an explicit rollback path.
2. **Rust remains authoritative.** Sessions, transcript history, settings,
   tools, approvals, jobs, campaigns, cron, artifacts, files, cancellation, and
   terminal outcomes continue through the frozen desktop methods and
   authenticated loopback host. No database migration is introduced.
3. **The production renderer does not receive the bearer token.** A
   context-isolated preload exposes bounded command, chat, Browser, window,
   folder, and open-path capabilities. Electron main authenticates host
   requests, validates sender ownership, caps payload size, and permits one
   foreground stream.
4. **Chat ownership is explicit.** Main owns one `AbortController` per stream.
   Events include both stream and session identity. The renderer buffers events
   that can arrive before `chat.start` resolves and projects them only into the
   owning conversation. Cancellation is idempotent; a local request never
   overwrites a later authoritative terminal event.
5. **The workbench is a dense three-surface composition.** The project/session
   rail, central work surface, and Browser/Files/Artifacts workspace are
   co-equal at wide widths. Compact widths select one primary surface rather
   than squeezing three columns. Terminal, jobs, and exact-effect approvals
   occupy a bounded execution dock.
6. **Presentation state is versioned and local.** Pane widths, collapse state,
   selected workspace surface, execution height, project grouping, pins, and
   theme are presentation state. Final divider values persist on pointer
   release. These values do not imply project isolation.
7. **Streaming and geometry share one display-clock coordinator.** Dirty lanes
   retain only their latest value. Stream chunks publish at most once per
   animation frame. Browser bounds are read and written once per changed frame,
   deduplicated, rounded to CSS pixels, and settled before the native view is
   revealed. Divider drag has no spring, transition, or inertial continuation.
8. **Electron owns remote Browser pixels with `WebContentsView`.** React owns
   Browser chrome and the measured content hole. Remote content has no Node
   preload, uses sandboxing and normal web security, and may navigate only to
   HTTPS or loopback HTTP. Permissions, downloads, and new windows are denied.
9. **The two Browser paths remain distinct (SharedBrowserContract).** The
   Electron preview is a user-facing surface. Rust `browser_*` methods remain
   the agent-effect path. Cookies, history, process identity, and a shared
   target are not promised. Product law is expanded and restated in
   [ADR-0040](./0040-shared-browser-contract.md): coordination is host-owned
   URL/state events only; never merge storage partitions or attach agent CDP to
   the preview `WebContentsView` without a break-glass ADR. An annotation enters
   a notes gallery first; the composer receives it only after an explicit user
   **Add to prompt** action and is treated as untrusted context.
10. **Motion is bounded and non-overshooting.** Interaction acknowledgment is
    immediate or 70–160 ms, uses primarily transform/opacity/color/border, and
    contains no animated blur, backdrop filter, `transition: all`, persistent
    `will-change`, spring, or per-token animation. Reduced motion preserves
    state order and final geometry without displacement.
11. **Unavailable capability remains visible as unavailable.** Messaging,
    specialist agents, child-agent orchestration, stream replay/resume, durable
    denial, and project-isolation enforcement are not implied by navigation or
    settings labels.
12. **Evidence is separated by proof strength.** Fixture browser-contract
    screenshots prove React layout only. The Electron Playwright suite proves
    built-protocol loading, token absence, real `WebContentsView` paint,
    clickability, alignment, divider settlement, and lifecycle in a compiled
    Electron shell. It is not installed-application evidence.

## Alternatives considered

### Keep the initial React scaffold beside the legacy UI

Rejected. Two production presentation owners would make parity and safety
labels drift while the default product experience remained unchanged.

### Expose the host token and call HTTP directly from React

Rejected. The renderer would hold a reusable authority secret and every
frontend request site would become part of the host authentication boundary.

### Use `<webview>`, an iframe, or BrowserView

Rejected. Remote content must stay outside the renderer DOM and must not gain a
Node-capable preload. `WebContentsView` provides a main-owned native surface;
BrowserView is deprecated and an iframe cannot provide the same isolation or
native ownership.

### Move Browser navigation or durable state into Electron

Rejected. Electron owns only the user preview and transport mediation. Moving
agent effects, approvals, sessions, or durable outcomes would split Rust
authority and violate the frozen contract.

### Animate every stream delta or resize sample

Rejected. Historical animation queues lag behind the pointer and cause
completed text to be repeatedly reconciled. Latest-value frame projection is
both more responsive and easier to verify.

## Reasons

The decision cuts over the product shell without changing the agent kernel. It
gives each timing-sensitive surface one owner, removes credentials from the
production renderer, and keeps rollback independent of durable data. Explicit
proof labels prevent deterministic fixtures from being reported as native
Browser or installed-product evidence.

## Consequences

- `apps/optimus-ui` is now a componentized React workbench with typed fixture,
  HTTP-development, and Electron transports.
- `apps/optimus-electron` is a security and native-view boundary rather than a
  transparent browser wrapper.
- The Wry UI and its regression suite remain supported as a rollback shell.
- Production starts require a built UI directory; development uses an explicit
  Vite URL.
- Completed transcript rows are stable; only the active assistant message
  receives display-frame projections.
- Layout tests cover 1600, 960, 640, and 320 CSS-pixel modes. Native Browser
  evidence is collected separately from React browser-contract evidence.

## Risks and unresolved boundaries

- **Unknown or unresolved behaviour:** physical high-refresh cadence is not
  certified. Headless and compiled-shell tests prove scheduling contracts and
  geometry, not a literal frame rate on a particular monitor/GPU.
- **Unknown or unresolved behaviour:** stream replay/reconnect after renderer
  refresh is not implemented. Refresh must not claim resumption.
- **Planned behaviour:** installed-app cutover, packaging proof, and installed
  native UI verification require an explicit future instruction.
- **Planned behaviour:** project-bound and isolated-profile enforcement remains
  configured intent under ADR-0027.
- **Unknown or unresolved behaviour:** the Electron preview and Rust Browser
  effector do not share cookies, navigation history, or one automation target.

## Evaluation evidence

- React Vitest and Testing Library cover layout migration, frame convergence,
  conversation terminal de-duplication, preload stream races, composer IME,
  Send/Stop, and forbidden motion properties.
- Node tests cover accepted and denied Browser URLs.
- React browser-contract Playwright covers wide, medium, compact, 320 px,
  reduced-motion, light theme, and secondary surfaces without console errors.
- Electron Playwright launches built assets through the production protocol
  with an isolated Optimus home and offline provider, then verifies the native
  Browser view and Rust bridge.
- Rust desktop and legacy Wry suites remain required regression gates.

## Relevant code

- `apps/optimus-ui/src/app/OptimusApp.tsx`
- `apps/optimus-ui/src/ipc/`
- `apps/optimus-ui/src/state/`
- `apps/optimus-ui/src/performance/frameCoordinator.ts`
- `apps/optimus-electron/main.cjs`
- `apps/optimus-electron/preload.cjs`

## Relevant tests

- `apps/optimus-ui/src/ipc/electronTransport.test.ts`
- `apps/optimus-ui/src/state/conversationStore.test.ts`
- `apps/optimus-ui/src/performance/frameCoordinator.test.ts`
- `apps/optimus-electron/test/browser-policy.test.cjs`
- `apps/optimus-electron/e2e/react-browser-contract.spec.cjs`
- `apps/optimus-electron/e2e/compiled-shell.spec.cjs`

## Conditions for reconsideration

Reconsider the transport if Rust gains an equivalent authenticated native IPC
channel that preserves the same sender, cancellation, and payload boundaries.
Reconsider the native Browser surface if Electron replaces `WebContentsView`.
Retire the legacy rollback only after installed packaging, migration, Browser
geometry, accessibility, and recovery evidence are independently green.

## 2026-07-24 shell-convergence addendum

ADR-0030 refines this accepted shell without changing its Rust authority or
transport boundaries. The current presentation uses the measured 240 px rail,
36 px header, 720 px evidence default, and 736 px composer cap; local projects
store `rootPaths[]`; native annotations return bounded element context; and
renderer overlays suspend the `WebContentsView`. The React contract now covers
1919, 1600, 960, 640, 480, and 320 CSS-pixel states. Runtime project
enforcement and installed-product proof remain planned.

## 2026-07-24 bounded typewriter projection addendum

At the user's direction, the active assistant message may reveal streamed text
one Unicode character per available display frame. This is a presentation-only
projection over the complete authoritative stream buffer, not a queue of model
events: the frame coordinator retains one latest job per message, fast-forwards
when the unrevealed tail exceeds 180 characters, and never replays completed
messages when they mount. Reduced-motion and hidden-document states converge
immediately to the latest complete text. Loaded history, cancellation,
authorization, terminal outcomes, and the Rust stream contract are unchanged.

## 2026-07-24 per-turn tool activity addendum

Tool activity is projected onto the stable assistant message that owns the
current run instead of a transcript-global footer. The collapsed summary is
the default reading surface; a native `details` disclosure expands the bounded
individual calls and their running, completed, or failed presentation. Starting
a later run does not erase tool activity from earlier in-memory turns.

The current Rust stream emits ordered tool name/detail events without a stable
tool-call ID, and persisted session detail does not replay tool events after a
renderer reload. The renderer therefore coalesces only the latest open,
same-name call in the single sequential run and does not claim durable tool
history. Stable call IDs and persisted replay remain required before concurrent
or refresh-resilient tool timelines can be claimed.
