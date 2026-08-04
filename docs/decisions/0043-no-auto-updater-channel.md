---
doc_id: decisions-0043-no-auto-updater-channel
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0043: No auto-updater channel at product-complete, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - scripts/rebuild-install-relaunch.sh
  - crates/optimus-host/src/system.rs
  - docs/architecture/desktop-install-relaunch.md
depends_on:
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0038-ui-ipc-architecture.md
validated_by:
  - crates/optimus-host/src/system.rs
  - _attic/architecture-records/product-complete-p29-verification.md
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

## Documentation completion addendum (2026-07-31)

## Alternatives considered

The pre-decision behaviour and an ad hoc implementation were considered. Both were rejected because they leave the boundary described by this decision implicit, inconsistently enforced, or unobservable.

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Risks

Implementation can drift from the accepted boundary while the prose remains unchanged. Source-bound documentation checks, the relevant tests below, and the full repository gate are the mitigation.

## Evaluation evidence

- `crates/optimus-host/src/system.rs`
- `_attic/architecture-records/product-complete-p29-verification.md`

## Conditions for reconsideration

Reconsider when the named boundary or threat model changes and a replacement preserves typed enforcement, observability, deterministic failure, and regression coverage.

## Relevant code

- `scripts/rebuild-install-relaunch.sh`
- `crates/optimus-host/src/system.rs`
- `docs/architecture/desktop-install-relaunch.md`

## Relevant tests

- `crates/optimus-host/src/system.rs`
- `_attic/architecture-records/product-complete-p29-verification.md`
