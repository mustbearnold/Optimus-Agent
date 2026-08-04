---
doc_id: design-codex-convergence-experience-contract
doc_type: explanation
plane: current
status: current
authority: supporting
summary: - Product / feature: Optimus Agent desktop workbench, measured Codex-shell convergence and multi-folder projects. - Primary user: a developer supervising coding-agent work across local repositories and evidence surfaces. - Job: state an...
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: design-contract
covers:
  - apps/optimus-ui/**
  - apps/optimus-tauri/src/**
depends_on:
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
  - docs/decisions/0030-codex-measured-shell-and-multi-folder-projects.md
validated_by:
  - apps/optimus-ui/src/**/*.test.ts
  - apps/optimus-ui/src/**/*.test.tsx
  - apps/optimus-desktop/e2e/**
---

# Codex-converged Optimus experience contract

## 1. Product and user

- **Product / feature:** Optimus Agent desktop workbench, measured Codex-shell
  convergence and multi-folder projects.
- **Primary user:** a developer supervising coding-agent work across local
  repositories and evidence surfaces.
- **Job:** state an outcome, inspect work and evidence, control consequential
  actions, and retain the resulting project/session state.
- **Why an agent:** repository work requires uncertain investigation, tool use,
  and adaptation. Deterministic tools remain authoritative for repeatable
  effects.
- **Accessibility context:** keyboard and pointer users, compact desktop
  windows, 200–400% zoom-equivalent reflow, reduced motion, and forced colors.

## 2. Outcome

- **Durable state:** Rust-owned session/messages/jobs/artifacts plus a local
  versioned project catalog and layout preferences.
- **Done:** exactly one terminal run state; evidence and approvals remain
  inspectable; presentation state survives renderer restart.
- **Quality method:** focused component tests, responsive browser contracts,
  desktop e2e suites, and Engineering Memory validation.
- **Partial usefulness:** partial assistant text, tool activity, artifacts, and
  terminal output survive cancellation/failure when the owning backend does.

## 3. Stakes and failure

- **False positive cost:** implying a tool, isolation mode, source permission,
  or completed run that does not exist.
- **False negative cost:** hiding recoverable work or a valid runtime feature.
- **Most harmful mistake:** letting a local project/source gesture silently
  broaden runtime permissions.
- **Fallback:** use the exact runtime surface/CLI and retained session state.
- **Required review:** explicit approval for high-risk effects; user review
  before publish/commit/install.

## 4. Actual capabilities

| Capability | Owner/tool | Input | Output | Known limit |
|---|---|---|---|---|
| Chat/run | Rust desktop via the Tauri bridge | Session, message, provider settings | Ordered stream and one terminal outcome | One foreground run; no refresh resume |
| Local project catalog | React local storage v2 | Name, `rootPaths[]`, primary root | Presentation grouping and session assignment | Not runtime permission authority |
| Preview browser | Kernel `browser_*` effector (CDP when available) | HTTPS or loopback URL | Bounded navigation/click/reload state | Separate from agent tool cookies/history |
| Annotation | Explicit one-shot native capture | User-selected page element | Bounded role/label/URL/rect note | No HTML, selector, or cross-page automation |
| Terminal/effects | Rust `term_run` and approvals | Exact command/effect | Durable job/status/output | High-risk effects can wait for approval |
| Files/artifacts | Rust desktop methods | Allowlisted paths/artifact IDs | Read/evidence surfaces | Existing runtime policy remains authoritative |

## 5. Autonomy and consequence map

| Action | Tier | Boundary | Reversible | Approval/receipt |
|---|---:|---|---|---|
| Resize/select panels or theme | A0 | Local presentation | Yes | Immediate local persistence |
| Add/remove project source | A0 | Local catalog only | Yes before/after edit | Project dialog states no permission change |
| Navigate preview | A1 | HTTPS/loopback child view | Yes | URL/status in browser chrome |
| Attach preview annotation | A1 | One selected element | Yes in composer | Explicit mode; click consumed |
| Run terminal command | A2/A3 by policy | Rust runtime | Varies | SmartDeny/approval and terminal receipt |
| Delete artifact | A2 | Artifact store | No in current UI | Accessible confirmation required |

- **Stop:** foreground Stop targets only the owning stream; annotation Escape or
  surface change cancels capture.
- **Invalid approval:** changed effect arguments require a new durable approval.

## 6. Lifecycle

- Session and stream identities are distinct and persisted by Rust.
- `submitting`, `working`, `awaiting_approval`, `cancelling`, `completed`,
  `cancelled`, `failed`, and `disconnected` remain distinct.
- Active work can be inspected from another session but not redirected.
- Refresh/reconnect replay is **unknown/unresolved** and never implied.
- Retry creates a new explicit operation unless the backend proves idempotency.

## 7. Surface architecture

- **Primary:** artifact/work workspace.
- **Dominant object:** current session outcome and composer.
- **Secondary:** Browser/Files/Artifacts evidence workspace.
- **Operations:** resizable bottom Terminal/Approvals/Jobs dock.
- **Desktop:** 240 px rail, central work, 720 px evidence, 36 px native header.
- **Compact:** one state-preserving primary surface selected by a visible tab
  strip; no compressed three-column layout.
- Activity may collapse; consequence, run state, Stop, approval, and composer
  controls may not.

## 8. Artifact and collaboration

- Rust artifact hashes are authoritative; UI filters/selection are local.
- Deletion requires confirmation and does not promise recovery.
- Diff/Changes review and concurrent artifact editing are **planned**.
- Publish/share/commit remain outside this UI slice and require explicit user
  action.

## 9. Evidence and trust

- Fixture Browser pixels are labelled contract preview; native preview is
  labelled Live.
- Capabilities explicitly separate available and unavailable behavior.
- Project dialogs distinguish presentation catalog from runtime enforcement.
- Technical detail belongs in approval/evidence/terminal surfaces, not generic
  badges or confidence decoration.

## 10. Memory and privacy

- Runtime memory, session state, project catalog, skills, project knowledge,
  browser profile, and Engineering Memory are separate systems.
- The project catalog stores folder paths locally; it has edit/remove controls.
- Annotation sends only bounded selected context to the composer.
- Remote pages have no Node preload; permissions, downloads, and popups are
  denied.

## 11. Generative UI boundary

- Rendering is host-native React plus one sandboxed native browser resource.
- The model cannot generate arbitrary renderer components or bridge methods.
- Desktop bridge payloads are JSON-bounded and sender-validated.
- Malformed or unavailable behavior fails visibly rather than generating a
  synthetic control.

## 12. Content and progress

- First status is submitting/working; real tool and timing events follow.
- Percentages/ETAs are absent unless the runtime exposes measurable units.
- Approval wording names the exact effect and uses “Approve command.”
- Technical terms remain available in Terminal, Capabilities, and status
  surfaces; the primary work area uses task language.

## 13. Visual direction

- **Adjectives:** quiet, exact, capable.
- **Composition:** Codex-measured desktop workbench, not a dashboard.
- **Type:** platform UI stack, 13–14 px working text, 12 px navigation.
- **Density:** compact but legible; no decorative card grid.
- **Color:** neutral light default, neutral dark option, semantic colors only
  where state changes meaning.
- **Depth:** borders and restrained shadows on composer/popovers/modals.
- **Motion:** monotonic 70–160 ms where useful; messages do not fade.
- **Avoid:** gradients, glass, blur, giant radii, emoji icons, fake metrics,
  card soup, and hidden stateful controls.

## 14. Accessibility

- Named landmarks cover projects, work, evidence, execution, dialogs, and
  status.
- Roving workspace tabs, native controls, modal focus containment, Escape,
  visible focus, and focus restoration are required.
- Transcript announcements are throttled and never steal focus.
- Compact targets are at least 24 px; reflow is proven at 480 and 320 CSS px.
- Reduced motion preserves final state; forced colors retain selected/focus
  boundaries.

## 15. Performance and reliability budgets

- Interaction acknowledgment: same frame where possible, otherwise under
  100 ms before slow backend work.
- Stream and browser geometry project at most once per animation frame and keep
  only the latest dirty value.
- Transcript initially mounts a bounded message window.
- Remote preview URL and annotation strings are length-bounded.
- Offline/stale transport is explicit; it does not fabricate completion.

## 16. Measurement

- Task success and correct terminal outcome outrank engagement.
- Track cancellation success, approval comprehension, unsupported-capability
  rate, root-overflow defects, focus defects, and native-view overlay defects.
- Geometry baselines: 1919 maximum proof, 480 native-minimum proof, 320
  zoom-equivalent proof.

## 17. Verification matrix

| Scenario | User-visible state | Backend/native state | Evidence |
|---|---|---|---|
| Short success | Stable transcript and Completed/idle | One terminal event | conversation/component tests |
| Awaiting approval | Exact effect in bottom dock | Durable pending approval | execution UI + Rust contract |
| Cancel | Cancelling then Cancelled/terminal state | Owning stream abort only | transport/component/compiled tests |
| Native annotation | Crosshair mode then bounded composer note | Selected click consumed, capture removed | BrowserSurface + compiled Electron |
| Modal over preview | Opaque, keyboard-contained dialog | Child view hidden then restored | compiled Electron screenshot/test |
| Multi-folder edit | Roots and primary source in one dialog | Local v2 catalog only | projectStore + browser contract |
| Narrow/zoomed | One selected primary surface | State retained | 480/320 Playwright |
| Hostile preview URL | Visible navigation error | Main rejects URL | browser-policy Node test |
