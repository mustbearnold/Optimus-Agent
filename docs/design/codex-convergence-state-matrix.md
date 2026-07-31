---
doc_id: design-codex-convergence-state-matrix
doc_type: explanation
plane: current
status: current
authority: supporting
summary: Queued, paused, backgrounded, and durable resume are currently inapplicable to the foreground chat contract. Jobs and cron expose their own runtime states; the UI must not map them onto foreground chat.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: design-state-matrix
covers:
  - apps/optimus-ui/**
  - apps/optimus-electron/**
depends_on:
  - docs/design/codex-convergence-experience-contract.md
validated_by:
  - apps/optimus-ui/src/**/*.test.ts
  - apps/optimus-electron/e2e/*.spec.cjs
---

# Codex-converged Optimus state matrix

## Run lifecycle

| State | Entry | Visible treatment | Controls | Exit/persistence | Proof |
|---|---|---|---|---|---|
| idle | No foreground stream | Ready; Send enabled when input exists | Compose, inspect | Send | Composer/E2E |
| submitting | Send accepted locally | Submitting | Stop for owner | First event/error | conversationStore |
| working | Stream/tool event | Working marker and progressive assistant row | Stop, inspect another session | terminal event | store/frame tests |
| awaiting approval | Runtime reports durable pending effect | Awaiting approval in run plus exact effect in dock | Approve command, inspect | grant/cancel/failure | ExecutionDock |
| cancelling | Stop requested | Cancelling; partial text retained | No duplicate Stop | authoritative terminal event | transport tests |
| completed | Done event | Stable final text; Ready | New send | New run | store tests |
| cancelled | Cancelled event | Cancelled; partial work retained | New send | New run | compiled shell |
| failed | Error event | Failed/disconnected distinction | Retry by explicit new send | New run/refetch | store tests |

Queued, paused, backgrounded, and durable resume are currently inapplicable to
the foreground chat contract. Jobs and cron expose their own runtime states;
the UI must not map them onto foreground chat.

### Invariants

- A final text delta is not completion.
- Cancellation requested is not Cancelled.
- A terminal state cannot regress without a new run.
- One stream remains owned by one session.
- Percentage and ETA are absent without measurable runtime units.

## Connection overlay

| State | Detection | Treatment | Commands | Recovery |
|---|---|---|---|---|
| live | Bridge/fixture calls succeed | Normal status strip | Normal | None |
| disconnected | Stream/bridge failure | Disconnected/boot error | Retry/refetch only | Explicit refresh/retry |
| stale/reconnecting | Not implemented | Must be labelled unavailable, not simulated | None | Planned |
| schema incompatible | Bounded call/shape failure | Visible error boundary | Reload only | Fix version mismatch |

Refresh replay and active-run resume remain **unknown/unresolved**.

## Tool and approval

| State | Summary | Detail | Controls | Receipt/recovery |
|---|---|---|---|---|
| running | Named activity row | Transcript activity/terminal | Stop when owning run supports it | Tool result |
| approval required | Exact effect waiting | Approval card JSON | Approve command | Durable job transition |
| approved/executing | Runtime-owned status | Job/terminal | Refresh/inspect | Terminal output |
| succeeded | Completed tool/job | Activity/job row | Inspect | Persisted runtime result |
| failed | Failed tool/job | Error/output | Explicit retry | New invocation |
| denied | Durable denial method unavailable | Never fake a Deny control | Cancel/close only where truthful | Planned |

## Project catalog

| State | Visible label | Controls | Authority | Proof |
|---|---|---|---|---|
| legacy single path | Migrated as 1 source | Manage | Local presentation | projectStore migration |
| one source | `1 source`; Primary | Add/remove/manage | Local presentation | project dialog |
| multiple sources | `N sources`; one Primary | Add, Make primary, remove | Local presentation | E2E multi-folder |
| zero sources | No source folders | Add source | Local presentation | dialog empty state |
| project assigned | Session appears under project | Drag/new session | Local assignment | ProjectsRail |
| runtime enforced | Enforcement active | Runtime-owned | Rust | ADR-0027; not implied by catalog |

### Invariants

- Root paths are de-duplicated.
- Primary root must be one of `rootPaths[]` or absent.
- Removing the primary deterministically selects the first remaining root.
- Catalog writes never edit a source folder or runtime allowlist.

## Preview and annotations

| State | Visible treatment | Native behavior | Controls | Proof |
|---|---|---|---|---|
| fixture | Contract preview | No child view | Navigate/test annotation | React E2E |
| live | Live + host | Sandboxed visible child view | Back/forward/reload/address | compiled shell |
| loading/error | Spinner or preview error | Load/reject state | Retry/navigation | BrowserSurface |
| annotating | Select element, Esc cancels | One-shot highlight/capture | Cancel/select | compiled shell |
| annotation attached | Bounded note in composer | Capture removed; click consumed | Edit/delete text | component/compiled |
| overlay open | Settings/project/task UI unobstructed | Child view hidden | Close overlay | compiled shell |
| overlay closed | Browser returns at settled bounds | Child view restored | Normal browser controls | compiled shell |

## Artifacts

| State | Treatment | User control | Protection | Proof |
|---|---|---|---|---|
| empty | No matching artifacts | Refresh/filter | No fake output | component/E2E |
| ready | Hash-backed preview/detail | Select/open | Rust artifact ID | ArtifactsSurface |
| delete requested | Accessible confirmation | Cancel first, Delete | Focus trap/restore | component test |
| deleted | List refresh | None | Current action irreversible | runtime method |

Streaming drafts, user edits, compare/merge, publish, and revert are planned and
must not be inferred from the current artifact viewer.

## Critical component states

| Component | Focus/selected | Disabled/pending | Error/recovery | Keyboard |
|---|---|---|---|---|
| Composer | Border/focus ring | Other-session owner or active Send/Stop state | Partial text retained | Enter send, Shift+Enter newline, IME safe |
| Workspace tabs | Roving selected tab | None | Surface error stays local | Arrows/Home/End |
| Project source dialog | Modal focus containment | Save disabled for empty name | Cancel preserves catalog | Tab/Shift+Tab/Escape |
| Settings | Left-nav current category | Unavailable controls labelled/disabled | Close/reopen/refetch | Tab/Shift+Tab/Escape |
| Annotation | Toolbar pressed state | Disabled by hidden/inactive browser | Escape/surface change cancels | Button then page selection |
| Approval | Exact effect card | Busy while granting | Refresh and retained job state | Native buttons |

## Responsive and accessibility

| Scenario | Layout adaptation | Preserved controls/status | Proof |
|---|---|---|---|
| 1919×1079 | 240 rail + work + 720 evidence + optional 190 dock | All | measured maximum E2E |
| 960×760 | 52 rail + split work/evidence | Composer, evidence tabs, status | medium E2E |
| 640×800 | One selected primary surface | Composer controls and switcher | compact E2E |
| 480×600 | One surface; composer 2-column controls | All stateful composer controls | native-minimum E2E |
| 320×800 / 400% equivalent | One surface, no root spill | Focus and all composer controls | reflow E2E |
| reduced motion | Near-zero finite motion | Same state/order/focus | E2E + CSS audit |
| forced colors | Focus and selected borders | Native semantics | CSS contract |
| long paths/titles | Ellipsis/local wrapping | Full value in title/dialog | overflow tests |

## Sign-off

- [x] Each implemented state has a real trigger.
- [x] Visible labels distinguish implemented, runtime-owned, and unavailable.
- [x] Consequential effects retain approval/receipt ownership.
- [x] Run and connection states remain separate.
- [x] Narrow, reduced-motion, and native overlay states map to tests.
- [ ] Refresh replay/resume awaits a backend contract.
- [ ] User-editable artifact version/conflict behavior awaits implementation.
