---
doc_id: decisions-0030-codex-measured-shell-and-multi-folder-projects
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0030: Codex-measured shell and multi-folder projects, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - apps/optimus-ui/**
  - apps/optimus-electron/**
  - docs/design/codex-convergence-*.md
  - docs/specifications/react-workbench-electron-preview-cutover.md
depends_on:
  - docs/decisions/0027-settings-driven-work-isolation.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
validated_by:
  - apps/optimus-ui/src/**/*.test.ts
  - apps/optimus-ui/src/**/*.test.tsx
  - apps/optimus-electron/e2e/react-browser-contract.spec.cjs
  - apps/optimus-electron/e2e/compiled-shell.spec.cjs
---

# ADR-0030: Codex-measured shell and multi-folder projects

- **Status:** Accepted
- **Date:** 2026-07-24

## Context

ADR-0029 established the secure React/Electron workbench but its first visual
language was an Optimus-specific dark technical shell. The captured Codex App
26.715.72359 reference provides much stronger comparative evidence for a
desktop agent workspace: a 36 px integrated title row, 240 px project rail,
system typography, quiet neutral surfaces, a bounded central composer,
resizable evidence and terminal panels, local scroll ownership, and extensive
`min-width: 0`/ellipsis/containment use.

The same reference also establishes a current project model with
`rootPaths[]`, thread-to-project assignment, an active root set, and an
“Add source” action. Optimus had only one presentation-only `path` per project.
Changing the visual shell without changing that model would make the new
multi-folder affordance cosmetic and misleading.

The reference corpus is read-only comparative evidence. Optimus must not copy
or execute proprietary application code, ship captured assets, or imply that a
presentation project grants runtime filesystem authority.

## Decision

1. **Use measured geometry, not copied implementation.** Optimus owns a
   `codex-shell.css` convergence layer with original tokens and locally authored
   line SVGs. It uses the platform UI font stack and neutral light/dark colors;
   it does not bundle Codex fonts, icons, source, or binary assets.
2. **Adopt the measured wide-shell anchors.** The top row is 36 px, the default
   project rail is 240 px, the evidence workspace defaults to 720 px, the
   composer caps at 736 px, and the execution dock defaults to 190 px. Direct
   pointer resizing remains latest-state and persists only final geometry.
3. **Improve the narrow boundary rather than copy its weakness.** The captured
   Codex app preserves a 240 px rail at its 480 px native minimum. Optimus
   collapses the rail at 1099 px and changes to one selected primary surface at
   899 px. At 520 px the composer controls reflow into two columns. The 480 px
   native-minimum and 320 CSS-pixel/400%-equivalent contracts have no root
   horizontal overflow.
4. **Make light neutral the initial presentation.** Light and dark remain
   user-selectable; density is local presentation state. Runtime status and
   semantic danger/warning/success colors remain Optimus-owned.
5. **Use a versioned multi-folder project catalog.** A local project has
   `id`, `name`, `rootPaths[]`, optional `primaryRoot`, timestamps, and pin
   state. Legacy `{path}` records migrate in place. Session-to-project
   assignments remain separate so a session has one presentation project
   identity while the project can expose several sources.
6. **Do not equate sources with permission.** Adding or removing a source
   changes only the local project catalog. Rust filesystem allowlists,
   SmartDeny, work-isolation enforcement, and explicit approvals remain
   authoritative and are described separately in the project dialog.
7. **Use a Codex-style settings architecture.** Settings has a stable left
   category rail and a scroll-owned content pane. General, Appearance, Agent &
   models, Work isolation, Projects, Browser, Terminal & execution, Tools &
   approvals, Memory, Automations, Authentication, Accessibility, and Advanced
   are present. Controls without a real backend are disabled and labelled
   unavailable or runtime-owned.
8. **Annotations are explicit and bounded.** In the native preview, annotation
   mode installs a one-shot capture in the sandboxed page. It highlights the
   hovered element and returns only bounded URL, page title, tag/role,
   accessible label or short text, and rounded rectangle. Click is consumed,
   Escape and surface changes cancel, capture expires after two minutes, and
   no HTML or raw selector enters the composer.
9. **Suspend native pixels under renderer overlays.** Electron child views sit
   above renderer DOM. Settings, project-source management, and the task panel
   therefore hide the native preview before displaying their overlays and
   restore it after close. The compiled Electron test proves this lifecycle.
10. **Prefer stable content over decorative entry motion.** Messages do not
    fade in. Finite panel motion remains bounded and reduced-motion compatible.
    Neither stylesheet may contain `transition: all`, blur/backdrop filtering,
    or persistent `will-change`.

## Alternatives considered

### Copy the captured Codex styles and assets

Rejected. It would create provenance, maintainability, and licensing risk and
would couple Optimus to minified implementation details rather than measured
behavior.

### Keep one path per project and show a synthetic source count

Rejected. The visual affordance would not survive persistence and could not
represent primary-source selection or migration.

### Treat every project source as an automatic runtime allowlist

Rejected. A local presentation action must not silently broaden filesystem
permissions.

### Render the preview in an iframe so modals naturally cover it

Rejected. ADR-0029 requires main-owned native pixels and a stronger process
boundary. Explicit child-view suspension preserves that boundary.

### Copy Codex’s 480 px two-column squeeze exactly

Rejected. The measured root does not overflow, but retaining half the window
for the rail leaves too little primary work area. An earlier state-preserving
surface switch is more accessible.

## Reasons

Measured geometry gives the team a repeatable baseline instead of aesthetic
guesswork. A real `rootPaths[]` schema makes the multi-folder affordance
durable, while the explicit presentation/runtime boundary prevents it from
becoming an accidental permission grant. Bounded annotations and preview
suspension preserve the stronger native isolation chosen by ADR-0029.

## Consequences

- The workbench is visibly closer to the measured Codex shell at maximum,
  medium, compact, native-minimum, and zoom-equivalent widths.
- Project-source management is durable across renderer sessions but remains
  presentation state until project authority exists in Rust.
- Browser annotations now work in the real `WebContentsView`, not only the
  deterministic fixture.
- The settings information architecture can absorb Optimus-exclusive controls
  without becoming a card grid or hiding unavailable behavior.
- The local theme layer is larger because it intentionally overrides the
  pre-convergence stylesheet while preserving unrelated in-progress work.

## Risks and unresolved boundaries

- **Planned behaviour:** move the project catalog and assignment authority into
  Rust before claiming enforced multi-folder project isolation.
- **Planned behaviour:** expand theme choices beyond light, dark, and density
  after the Optimus visual identity is defined.
- **Unknown or unresolved behaviour:** installed-app verification on the
  user’s physical desktop was not performed; compiled Electron verification
  ran in an isolated virtual X display because the Codex desktop session was
  closed.
- **Unknown or unresolved behaviour:** physical display cadence and native
  operating-system zoom behavior still require installed hardware evidence.

## Evaluation evidence

- Vitest covers legacy project migration, source de-duplication, primary-root
  changes, versioned persistence, bounded annotation projection, motion rules,
  and existing session/composer/artifact contracts.
- React Playwright covers 1919×1079, 1600×1000, 960×760, 640×800, 480×600, and
  320×800, root overflow, visible-element containment, settings navigation,
  multi-folder management, reduced motion, and computed work-surface contrast.
- Compiled Electron Playwright covers native preview paint, alignment,
  clickability, resizing, one-shot annotation capture, consumed annotated
  clicks, and preview suspension/restoration under Settings.

## Relevant code

- `apps/optimus-ui/src/codex-shell.css`
- `apps/optimus-ui/src/state/projectStore.ts`
- `apps/optimus-ui/src/components/projects/ProjectSourcesDialog.tsx`
- `apps/optimus-ui/src/components/settings/SettingsDialog.tsx`
- `apps/optimus-ui/src/components/workspace/BrowserSurface.tsx`
- `apps/optimus-electron/main.cjs`
- `apps/optimus-electron/preload.cjs`

## Relevant tests

- `apps/optimus-ui/src/state/projectStore.test.ts`
- `apps/optimus-ui/src/components/workspace/BrowserSurface.test.tsx`
- `apps/optimus-ui/src/styles.test.ts`
- `apps/optimus-electron/e2e/react-browser-contract.spec.cjs`
- `apps/optimus-electron/e2e/compiled-shell.spec.cjs`

## Conditions for reconsideration

Reconsider the project schema when Rust gains a typed project authority.
Reconsider annotation capture if Chromium exposes a safer equivalent that
preserves the same bounded result. Rebaseline geometry when a newer captured
Codex corpus includes a complete measurement report rather than changing
tokens from visual memory.
