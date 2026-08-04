---
doc_id: architecture-workspace-redesign-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Planes: workspace redesign (docs/plans/workspace-redesign.md, Slice 1) · architecture hold (all marks S+++) · no program P## claim
reviewed_on: 2026-07-31
review_by: never
knowledge_type: verification
owns:
  - docs/architecture/workspace-redesign-verification.md
covers:
  - docs/plans/workspace-redesign.md
depends_on:
  - docs/plans/workspace-redesign.md
  - docs/plans/product-complete-program.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
validated_by:
  - apps/optimus-ui/src/app/OptimusApp.test.tsx
  - scripts/verify.sh
---

# Workspace redesign — shell verification

Planes: **workspace redesign** (`docs/plans/workspace-redesign.md`, Slice 1) ·
architecture hold (all marks S+++) · no program P## claim

Date: 2026-07-26

## Why this record exists

The workspace redesign landed on `main` in `ce7ed90` with component coverage
only. Its own plan is still **"implementation in progress"** with Slice 1 of 5
implemented, and its "Current evidence limits" already said installed native
paint and the platform accessibility tree were **not tested**.

Nothing failed when the redesign changed the shell contract, because the
`apps/optimus-electron` Playwright suite ran in **no gate**. `scripts/verify.sh`
executed the `apps/optimus-desktop` suite only. Eleven Electron e2e tests were
therefore dead code, and nine of them asserted a UI that no longer existed.

## Confirmed current behaviour

| Surface | Result | Evidence |
|---|:---:|---|
| Electron e2e tier runs in `just ui` / `just verify` | **PASS** | `scripts/verify.sh` `tier_ui` |
| Compiled shell: transport token absent, offline chat, cancel, native preview bounds, annotation gallery | **PASS** | `e2e/compiled-shell.spec.cjs` |
| Compiled shell: offline turn, durable reopen, new-thread isolation | **PASS** | `e2e/compiled-workbench.spec.cjs` |
| Every evidence surface reachable on a wide layout | **PASS** | `OptimusApp.test.tsx` wide-desktop regression |
| Responsive/layout contract, 8 tests at 320–1919px | **PASS** | `e2e/react-browser-contract.spec.cjs` |
| Installed shell offline acceptance | **RUNNABLE, not captured** | `e2e/installed-shell.spec.cjs` (self-skips without install paths) |

## Regression found and fixed

**Files and Artifacts were unreachable on any window ≥900px.**

`workspaceTab` had exactly three setters, all `SurfaceButton`s inside
`.compact-switcher`, which is `display: none` above 899px (`styles.css`,
`codex-shell.css`, `workbench-shell.css` all gate it behind `max-width`). The
redesign had also removed the topbar `Browser` / `Files` / `Artifacts` buttons.
With `workspaceTab` defaulting to `browser`, a normal desktop window could only
ever render the Browser panel.

This contradicted the redesign plan's own surface architecture ("browser, files,
and artifacts open as a peer pane on wide screens") and the `artifacts.store-ui`
and `files.read` ledger rows.

Fix: `WorkspacePane` now renders the `role="tablist"` header the markup already
implied — the `workspace-panel-*` ids existed with nothing pointing at them, and
`.workspace-tabs` CSS (including `button.is-active`) was already present and
orphaned in two stylesheets. No stylesheet changes were needed.

Regression tests: `OptimusApp.test.tsx` (wide layout, aria-selected + panel
`hidden`) and `compiled-shell.spec.cjs` (Files tree and Artifacts empty state
through the real compiled shell).

## ADR-0030 measured anchors restored

ADR-0030 (**Accepted**) fixes the measured wide-shell anchors: "The top row is
36 px, the default [rail is 240 px], the composer caps at 736 px, and the
execution dock defaults to 190 px", and says to rebaseline only "when a newer
captured baseline supersedes this one". `styles.css` and `codex-shell.css` both
still hold those values; the redesign's `workbench-shell.css` — imported last, so
it wins — had drifted three of them with no superseding ADR and no captured
baseline:

| Anchor | ADR-0030 | Redesign drift | Now |
|---|---:|---:|---:|
| Top row height | 36px | 52px | **36px** |
| Composer cap | 736px | 780px | **736px** |
| Narrow switcher row | 34px | 40px | **34px** |
| Project rail | 240px | 240px | 240px (never drifted) |
| Execution dock | 190px | 190px | 190px (never drifted) |

Restored rather than re-baselined, because an accepted ADR outranks an
undocumented token edit. **If 52/780/40 were intended, that needs an ADR
superseding 0030 plus a captured baseline — then these three values and the
matching assertions move together.**

## Root-cause fix

`installed-shell.spec.cjs` and `compiled-workbench.spec.cjs` now share one flow
(`e2e/support/workbench-flow.cjs`). The compiled twin runs on every `just ui`, so
a renamed control fails a gate immediately instead of surfacing only when someone
attempts an installed capture.

Stale assertions corrected against the current contract: `New session` →
`New thread`; composer `Provider` / `Model` / `Thinking level` now inside the
`Model and run settings` popover; `Access` via its listbox; empty state
`Start with an outcome` (a string that never existed in shipped source) →
`What should Optimus do?`; removed `Tasks` topbar button and `N messages` row
text dropped in favour of durable IPC assertions; `sessions` IPC reads
`{ sessions: [...] }` per `crates/optimus-host/src/sessions.rs`; annotation
asserts gallery-then-explicit-`Add to prompt` rather than direct composer
injection.

## Residuals (explicit)

| Residual | Owner |
|---|---|
| **Light theme is a no-op.** `workbench-shell.css` groups `:root, :root[data-theme="light"]` and gives both dark values (`--text: #f1f1f1`, `--surface: rgba(10,10,10,0.84)`). Light is the *default*, so selecting it in Settings changes nothing visually, while the redesign plan requires verifying "both themes". Needs a real light palette or an explicit dark-only decision that also removes the Settings control. | apps/optimus-ui |
| **Settings controls have no accessible name.** `SettingRow` renders its title in a `<strong>`, not a `<label>`, so every `<select>` in the dialog (Color theme included) is unlabelled — WCAG 1.3.1 / 4.1.2. `react-browser-contract.spec.cjs` uses a structural locator as a result. | apps/optimus-ui |
| PF-00 installed baseline still uncaptured; `desktop.native-cua` stays **partial** | operator: install from a tracked `main` commit, then run the installed spec |
| Installed candidate on this host was built from `local/worktrees/installed-ui-quality` (detached `9343ae5`, a t3 checkpoint), not from `main`, and its composer access model is **ahead** of `main` (`full` / `full_project` / `unrestricted` vs `full`) | decide: land or discard that worktree before any PF-00 claim |
| Redesign Slices 2–5 unimplemented while the redesign is the default shipped shell | apps/optimus-ui |

## Non-claims

- No PF-00 / `desktop.native-cua` parity claim. No installed baseline is committed.
- No new geometry contract. ADR-0030's anchors were restored, not replaced.
- No claim that the light theme works; it is a named residual.
- No claim that redesign Slices 2–5 are done.
- Architecture marks unchanged; this is product/gate hygiene, not a mark movement.

## Hold suite

```bash
bash scripts/verify.sh ui
npm --prefix apps/optimus-ui test
python3 scripts/check-module-size.py
```

## Verdict

Electron shell contract is **gated and green**. One real wide-layout regression
found and fixed with regression tests. Installed-app baseline remains an open
residual blocked on install provenance, not on tooling.
