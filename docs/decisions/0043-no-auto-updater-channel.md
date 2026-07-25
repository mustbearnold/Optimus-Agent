---
knowledge_type: decision
status: current
covers:
  - scripts/rebuild-install-relaunch.sh
  - apps/optimus-desktop/src/ipc/system.rs
  - docs/architecture/desktop-install-relaunch.md
depends_on:
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0038-ui-ipc-architecture.md
validated_by:
  - apps/optimus-desktop/src/ipc/system.rs
  - docs/architecture/product-complete-p29-verification.md
last_verified_commit: null
---

# ADR-0043: No auto-updater channel at product-complete

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

Program P29 requires either a **signed updater + rollback** or an **explicit
no-updater product decision**. A production signing/distribution chain for
background updates is not implemented. Shipping a partial updater would greenwash
`release.updater` and risk silent binary replacement without provenance.

## Decision

1. **Optimus ships without an in-app auto-updater** at product-complete (program
   P29). There is no background download, signature verification channel, or
   silent binary swap.
2. **Install / upgrade path** remains the operator-run
   `scripts/rebuild-install-relaunch.sh` (Linux XDG user install; Windows PS
   counterpart). Rollback is **reinstall previous build** or use the optional
   Legacy Wry desktop action for shell rollback only (not a full product updater).
3. Doctor reports `updater_channel: "none"` and an operator-facing note pointing
   at the install script (Confirmed current behaviour after P29).
4. Ledger row `release.updater` is **partial** with `trajectory: null` (ledger
   rule for non-parity rows). Residual is named via this ADR, scorecard Material
   partials, and verification evidence — not parity.
5. Hermes-style silent product updates are **out of P29** and Track Z / later.

## Consequences

- PRODUCT-COMPLETE does not claim continuous delivery or signed update feeds.
- Users upgrade by reinstalling from source/scripts; install metadata
  (`install-meta.json`) remains the version authority for the local install.
- A future signed updater requires a new ADR superseding this one, with real
  signing keys, rollback, and fail-closed verification tests.
