---
knowledge_type: decision
status: current
covers:
  - crates/optimus-kernel/src/browser_coord.rs
  - crates/optimus-kernel/src/browser.rs
  - apps/optimus-ui/src/components/workspace/BrowserSurface.tsx
  - apps/optimus-electron/**
  - docs/maps/security-and-approvals.md
depends_on:
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
  - docs/decisions/0015-preview-browser-cdp.md
  - docs/decisions/0035-command-capability-envelope.md
validated_by:
  - crates/optimus-kernel/src/browser_coord.rs
  - crates/optimus-kernel/src/browser.rs
  - apps/optimus-ui/src/components/workspace/BrowserSurface.test.tsx
  - apps/optimus-electron/test/preview-security.test.cjs
last_verified_commit: null
---

# ADR-0040: SharedBrowserContract (coordinated, not merged trust)

- **Status:** Accepted
- **Date:** 2026-07-25
- **Program:** product-complete **program P23**

## Context

Historical ADR-0015 said agent `browser_*` tools and the right-rail Preview
Browser share one browser session. ADR-0029 §9 already split the Electron user
preview (`WebContentsView`) from the Rust agent effector, forbidding shared
cookies/history/target claims. Product-complete program P23 needs an explicit
**SharedBrowserContract** so ledger and UI claims cannot greenwash “one CDP
session” while Security / UI marks stay S+++.

## Decision

1. **Two trust domains remain distinct by default.**
   - **UserPreview** — sandboxed Electron `WebContentsView` (or fixture); user
     navigation only; no Node preload; permissions/downloads/popups denied.
   - **AgentEffector** — kernel `BrowserEffector` (`HttpBrowserEffector` and
     optional CDP backend). Work Graph tool path; SSRF via
     `network_policy::assert_public_http_url` pre-DNS and post-redirect.

2. **Coordination is host-owned protocol, not shared Chromium state.**
   Domains may publish versioned navigation/annotation events (URL, title,
   timestamps, domain id) through a host coordination bus. That is the only
   allowed “shared session” product language: **coordinated preview + agent
   browser**.

3. **Forbidden without a separate break-glass ADR + tests:**
   - Agent CDP attached to the Electron preview `WebContentsView` partition
   - Shared cookies / localStorage / IndexedDB between preview and agent
   - Elevating preview permissions, downloads, or popups “because the agent needs it”
   - Treating free-form planner strings as authority to drive the user preview

4. **Annotations are untrusted context.** Element picks enter a notes gallery;
   composer injection requires an explicit user **Add to prompt** action
   (ADR-0029 §9 restated). Notes are bounded plain text — no HTML, no selectors
   as execution authority.

5. **Rust remains effect authority.** Electron main never grows a second
   durable effect ledger for agent browse (ADR-0029).

6. **Supersession.** ADR-0015 decision point 3 (“share one browser session”) is
   **superseded** by this contract. ADR-0015 remains historical for CDP process
   lifecycle intent; product law for trust domains is this ADR + ADR-0029.

## Alternatives considered

### Merge preview WebContentsView with agent CDP for paint parity

Rejected. Merges trust domains, breaks sandbox tests, and demotes Security/UI
S+++ hold for convenience.

### Coordination only in UI local state

Rejected as sole authority. Agent tool path must record effector events in a
host-visible bus for observability and honest dual-URL status.

## Consequences

- Ledger `browser.cdp` means **coordinated dual-domain browser**, not shared CDP.
- Preview security tests remain merge-blocking.
- Future “open this agent URL in preview” is a **host event**, never cookie jar merge.

## Evidence

- `crates/optimus-kernel/src/browser_coord.rs` (+ unit tests)
- HTTP agent browser SSRF tests in `browser.rs` / `network_policy.rs`
- React annotation gallery + Add to prompt tests
- `apps/optimus-electron/test/preview-security.test.cjs`
