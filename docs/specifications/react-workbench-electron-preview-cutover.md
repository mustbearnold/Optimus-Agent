---
knowledge_type: specification
status: active
covers:
  - apps/optimus-ui/**
  - apps/optimus-electron/**
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/ipc/**
  - docs/contracts/desktop-ipc-methods.md
depends_on:
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
validated_by:
  - apps/optimus-ui/src/**/*.test.ts
  - apps/optimus-ui/src/**/*.test.tsx
  - apps/optimus-electron/test/*.test.cjs
  - apps/optimus-electron/e2e/*.spec.cjs
  - apps/optimus-desktop/e2e/**
last_verified_commit: null
---

# React workbench and Electron preview cutover

## Objective and release boundary

Replace the minimal React renderer with the Optimus Vantage workbench and make
it the default Electron renderer without changing Rust method names, durable
state, approval semantics, or databases.

This implementation stops at repository and compiled-shell evidence. It does
not install or relaunch the installed application, use live credentials or a
paid model, commit, push, publish, or deploy.

## Behaviour classification

### Confirmed current behaviour

- React renders the title/truth strips, project/session rail, transcript,
  composer, Settings, Capabilities, Browser/Files/Artifacts workspace,
  execution dock, terminal, approvals, jobs, and task popover.
- Session creation/open/rename/delete, UI pinning/grouping, durable history,
  provider/model/thinking/fast/access settings, streaming, Stop, files,
  artifacts, settings, auth import controls, cron state, campaigns, jobs, and
  `term_run` call their existing Rust-owned methods.
- Electron defaults to React and retains
  `OPTIMUS_ELECTRON_UI=legacy`.
- Production assets load from `optimus-app://ui/`; the production renderer
  receives no bearer token.
- Electron main validates the owning renderer, method allowlist, serialized
  payload size, active stream count, Browser URL, and Browser bounds.
- A main-owned `WebContentsView` displays the user preview with remote Node
  integration disabled, context isolation and sandbox enabled, and permission,
  download, and new-window requests denied.
- One active foreground run is allowed per window. Its events retain stream and
  session ownership while another session is inspected.
- Live stream text and native bounds converge through latest-value
  display-frame lanes.
- Responsive modes are wide three-surface, medium split, compact selected
  surface, and 320 px single-surface reflow.
- The Codex-measured convergence layer uses a 36 px title row, 240 px project
  rail, 720 px evidence workspace, 736 px composer cap, system UI typography,
  neutral light/dark tokens, and Reicon Outline icons that inherit those tokens
  through `currentColor`.
- Local project catalog version 2 stores `rootPaths[]` and a `primaryRoot`,
  migrates the prior single `path`, and keeps session-to-project assignment
  separate. Adding a source does not broaden runtime filesystem permission.
- The native preview annotation mode returns only bounded page/element
  metadata, consumes the selected click, supports cancellation/expiry, and
  never adds page HTML or a selector to the composer.
- Settings, project-source management, and the task panel suspend the native
  preview so the Electron child view cannot cover renderer overlays.
- Empty-session starters are compact task rows, not equal-weight cards.
  Capabilities presents Rust packs and tools as an inspectable registry with a
  separate unavailable boundary. The composer keeps Access as its first
  control and collapses Provider, Model, effort, and Fast into one summary
  button with a wider settings popover. At 320 CSS px both compact controls
  remain reachable without horizontal clipping.
- Completed assistant replies present their content directly rather than
  repeating the Optimus sender name; active replies retain their terminal or
  progress status where applicable.
- Inactive primary-rail destinations use dim neutral text and icons; hover and
  the active destination retain the brighter readable treatment.
- Secondary rail section labels, including Projects, use the darker neutral
  treatment without changing their adjacent action controls.
- Project rows align their folder icon with the project-name line rather than
  centering it across the project name and source-count stack.
- User transcript cards use the one-step-lighter neutral surface while
  assistant replies remain directly on the work surface.
- User and assistant transcript body text uses a compact 13 px reading scale.
- Session rows use an 8 px corner radius for a subtly softer active-state
  outline.
- Child session cards stay within their parent project title column and do not
  extend into the project's reserved action area.
- The top-bar separator is intentionally omitted only above the desktop project
  rail; the work-surface separator remains visible.
- The session-bar project disclosure is a folder-only accessible button; the
  adjacent session title is context text and does not trigger the menu.
- Browser chrome contains only navigation and the address field; the inactive
  single-tab strip and redundant preview-status strip are omitted.
- The session-bar folder control and session title sit on an 8 px left inset,
  keeping the mini header aligned closely with its work-surface edge.
- Completed assistant replies with a runtime-provided elapsed duration display a
  compact “Worked for Xm XXs” header; historical replies without timing do not
  invent one.
- The composer omits its redundant local-checkout and ready-status footer.
- Back/Forward, project-rail/workspace toggles, and Terminal share the slightly
  lighter neutral chrome-icon treatment while hover and active states remain
  high contrast.
- Access uses an icon-first listbox without a trailing disclosure arrow; every
  option has the same neutral icon treatment and remains keyboard selectable.
- Codex and OpenAI-compatible model summaries use the concise visible order
  “model number, model name, thinking level” (for example, “5.6 Terra High
  effort”) rather than their internal model identifier.
- Composer focus uses a neutral border treatment instead of a blue selection
  outline while preserving visible keyboard focus.
- The top bar keeps process-local browser-style history for visited app routes.
  Back and Forward use full arrow glyphs, enable only when a matching history
  entry exists, and a new route choice truncates the forward branch.

### Planned behaviour

- Installed Electron packaging and default desktop entry cutover.
- Installed native UI and physical-display high-refresh verification.
- Project-bound and isolated-profile enforcement.
- Rich Diff/Changes review and broader campaign/cron authoring workflows.

### Inferred behaviour

- The React workbench is safe to make the repository-level Electron default
  because it uses the same frozen Rust calls and compiled-shell tests exercise
  the production protocol. This is not installed-product proof.

### Unknown or unresolved behaviour

- Renderer refresh cannot replay or resume an active stream.
- The Electron preview and Rust Browser effector do not share browser state.
- Physical p95/p99 frame budgets vary by host hardware and display.

## Surface contract

| Width | Project scope | Work/evidence relationship |
|---|---|---|
| `>=1280` | 240 px default; 200–400 px resize | Central work surface at least 420 px; evidence workspace defaults to 720 px and remains at least 360 px |
| `1100–1279` | 240 px | Evidence workspace caps near 44%; low-priority top-bar labels collapse |
| `900–1099` | 52 px command rail | Work and evidence remain a resizable split without a full text rail |
| `<900`, native 480 px, and 320 px proof | Surface switcher, rail removed | Exactly one of Work, Browser, Files, Artifacts, or Execution is primary; composer controls reflow by 520 px |

The composer retains model, effort, access, Send/Stop, IME behavior, and a
usable full-width text area in every mode.

## Ownership and transport

```text
React SPA
  |-- preload invoke/chat/browser/window/folder/path
  v
Electron main -- bearer + CSRF --> Rust loopback host
  |
  `-- WebContentsView --> allowed remote preview page
```

- Rust owns data, policy, effects, approvals, cancellation outcomes, and
  terminal truth.
- Electron owns bearer-token use, request bounds, stream controllers, native
  preview navigation, permissions, and lifecycle.
- React owns presentation state, Browser chrome, native content-hole geometry,
  route selection, input intent, and frame-bounded projection.
- Fixture transport owns deterministic browser-contract state only.

The production preload contract is intentionally narrower than the complete
Rust registry. `hostInfo` remains for legacy compatibility but omits the token
in React mode.

## Chat state machine

1. `send` creates or reuses the selected Rust session.
2. The conversation store appends stable user and active-assistant identities.
3. Electron main starts one bounded SSE request and returns a `streamId`.
4. Events are enveloped with `streamId` and `sessionId`. Events received before
   the `start` reply are buffered.
5. Text chunks append outside React and publish at most once per display frame.
6. Tool, timing, status, failure, cancelled, and done events update only the
   owning session.
7. Stop marks the owning conversation as cancellation-requested and aborts
   exactly the associated fetch. The terminal event remains authoritative.
8. Partial text is retained on failure, cancellation, or connection loss.

Another send is disabled until the foreground run settles. Session inspection
does not redirect the stream or remove the owning session’s Working marker.

## Native Browser contract

Accepted top-level URLs are HTTPS and HTTP on `localhost`, loopback IPv4, or
loopback IPv6. `file:`, `javascript:`, `data:`, Electron-privileged URLs,
malformed targets, and remote HTTP are rejected.

One `ResizeObserver` marks Browser geometry dirty. The frame coordinator reads
the content hole, rounds once, suppresses stationary rectangles, writes at most
the latest rectangle for that frame, and reveals the view after final bounds.
Switching away hides the native view and stops hidden-hole measurement.

Browser errors remain Browser errors. React fixture screenshots are not
evidence of native paint. Native evidence requires Electron launch,
`WebContentsView.capturePage`, an injected native click, and equality between
the settled native bounds and the DOM content-hole rectangle.

When renderer UI must overlay the preview, React suspends the child view first.
The compiled test proves Settings is unobstructed while the native view reports
hidden, then proves the view is restored after close.

## Motion and frame contract

One `requestAnimationFrame` coordinator owns dirty lanes for stream text,
scroll anchoring, and Browser geometry. A burst schedules one frame; later
values replace earlier values in the same lane. Visibility return performs one
latest-state reconciliation.

Interaction motion is 70–160 ms with monotonic curves and at most 4 px
displacement. Divider drag is direct and transition-free. CSS rejects
`transition: all`, backdrop filters, animated blur, persistent `will-change`,
large animated shadows, and token animation. Reduced motion makes transitions
effectively immediate and removes displacement without changing focus or final
state.

Acceptance is expressed relative to the observed display period `P`: foreground
application scripting plus style/layout targets p95 no more than `0.50P` and
p99 no more than `0.75P`. Deterministic tests prove convergence and bounded
work; they do not certify physical-monitor FPS.

## Accessibility contract

- Named landmarks identify projects, work, evidence, execution, and status.
- Icon controls and window actions have accessible names.
- Tabs, buttons, inputs, trees, separators, and dialogs use native semantics.
- Enter sends only outside IME composition; Shift+Enter inserts a newline.
- The transcript is a throttled log and new messages never steal focus.
- Detached scrolling exposes Jump to latest and disables auto-follow.
- Compact targets are at least 24 px and focus remains visible in dark, light,
  reduced-motion, and forced-color modes.
- Compact navigation exposes a visible surface switcher and restores the work
  surface without relying on gesture-only behavior.
- The Composer settings popover moves focus into its first field, closes on
  Escape or outside interaction, and restores focus to its trigger on keyboard
  dismissal.

## Safety and honesty

- Approval cards show exact effect JSON and use “Approve command.” Closing a
  surface is not labelled Deny because no durable denial method exists.
- `AwaitingApproval`, Working, cancelling, Cancelled, Failed, Completed, and
  connection loss remain distinct.
- Settings describe non-shared work isolation as configured intent.
- Artifact deletion opens an accessible confirmation boundary, focuses the
  non-destructive Cancel action first, and restores the initiating control
  after cancellation.
- Mail provides a local, read-only Optimus update preview with list, reader,
  and unread projection. External message delivery, account sync, notification
  schedules, and customization remain visibly unavailable.
- Specialist-agent orchestration remains visibly unavailable.
- Browser annotations enter the composer only after an explicit one-shot page
  selection and remain untrusted text.
- Tool activity is summary-first and expands with a native disclosure. Each
  live tool projection belongs to its assistant turn; current transport data
  does not claim persisted tool-call replay after renderer reload.

## Verification matrix

| Gate | Proof |
|---|---|
| React unit/component | Typed transport races, layout migration, app-route back/forward history, local Mail list/reader/unread projection, Composer settings disclosure and keyboard dismissal, frame convergence, terminal de-duplication, IME and Send/Stop, confirmed/cancelled artifact deletion, static motion audit |
| React build | TypeScript project build plus relative Vite production assets |
| Browser contract | 1919, 1600, 960, 640, 480, 320, dark/light/reduced-motion, contrast, multi-folder sources, Settings navigation, and zero root overflow |
| Electron policy | URL allow/deny and preload/main syntax checks |
| Compiled Electron shell | Production protocol, no renderer token, offline Rust session/chat/cancel, files/artifacts/settings, native preview paint/click/alignment/resize, bounded annotation capture, and modal suspension/restoration |
| Rust desktop | `cargo test -p optimus-desktop` |
| Legacy Wry | `npm --prefix apps/optimus-desktop run test:e2e` |
| Engineering Memory | check, generate, validate, strict validate; generated JSON only |

## Engineering Memory baseline

**Confirmed current behaviour at this implementation entry on 2026-07-24:**
`python3 scripts/engineering_memory.py check` reported
`ENGINEERING_MEMORY_STALE` because broader in-progress shell work had already
changed mapped source and documentation. The final handoff reports the
post-generation validation state and any remaining gap separately; it does not
invent a Git commit SHA.

## Completion and rollback

Repository completion requires every applicable row in the verification matrix
to pass or be reported explicitly. Installed completion is out of scope.

Rollback sets `OPTIMUS_ELECTRON_UI=legacy`. It does not migrate or rewrite Rust
data. Removing React mode must also destroy the preview `webContents`, abort
active streams, and leave the legacy method contracts unchanged.
