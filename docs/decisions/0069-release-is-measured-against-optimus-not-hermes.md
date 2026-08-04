---
doc_id: decisions-0069-release-is-measured-against-optimus-not-hermes
doc_type: decision
plane: decision
status: current
authority: record
summary: Proposed resolution of the yardstick contradiction — the north-star retired Hermes as a criterion while the release ratchet stays wired to the fail-closed 2,063-contract Hermes gate; this ADR re-scopes release to Optimus-native bars and demotes the Hermes gate to an informational scorecard.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - scripts/tools/optimus_version.py
  - docs/architecture/optimus-version.json
depends_on:
  - docs/decisions/0024-hermes-parity-version-gate.md
validated_by:
  - scripts/tests/test_optimus_version.py
  - scripts/verify.sh
---

# ADR-0069: Release is measured against Optimus, not Hermes

- **Status:** Accepted
- **Date:** 2026-08-01
- **Accepted:** 2026-08-01 by owner instruction, recorded at the acceptance
  land.

## Context

Two accepted authorities contradict each other. The north-star
(2026-07) rules that "Hermes is not the yardstick — no criterion's pass/fail
may depend on observing Hermes," and the comparative evaluation machinery was
deleted accordingly. Yet the release ratchet (ADR-0024) still binds the
product version to a fail-closed Hermes parity gate: 2,063 per-feature
contracts and 8 paired performance scenarios, all at zero evidence, no
waivers permitted, evidence expiring in 30 days. The result is that
"release" is undefined: the gate is BLOCKED by design against a benchmark
the north-star discarded, so no amount of product work can ever move the
version, and the README's headline status is permanently "parity
unverified" (B-STRAT-01 in the 2026-08-01 competitive audit).

## Options

**A. Re-adopt the yardstick and fund it.** Keep ADR-0024's gate as the
release bar and staff the 2,063-contract inventory audit plus the paired
benchmark protocol against a competitor that ships weekly. Cost: enormous,
perpetual, and already rejected in principle by the north-star's accepted
criteria.

**B. Re-scope release to Optimus-native bars (recommended).** Release means:
every parity-ledger row on the two thesis axes is green or explicitly
accepted as partial/missing by decision; the four structural wins keep their
runnable trajectories; and the performance harness (B-STRAT-02) shows no
regression against the previous Optimus release on the same eight scenario
shapes. The Hermes gate machinery is kept but demoted to an informational
scorecard command; the Hermes parity version stays `null` and is described
as informational, not as the release bar. Option A can still be funded
later, deliberately.

**C. Delete the Hermes gate entirely.** Simplest, but destroys 2,063
contracts of structured reference data and forecloses option A cheaply.
Rejected: the data is an asset; only its gating role is the defect.

## Decision (proposed)

Adopt B. Concretely: `scripts/tools/optimus_version.py gate` re-keys to the
Optimus-native bars above; the Hermes evaluation moves to an explicit
`scripts/tools/optimus_version.py hermes-scorecard` (informational, never wired
into verify or land); `docs/architecture/optimus-version.json` release_rules
re-key accordingly; the README status line names the native bar and its live
state instead of "parity unverified."

## Consequences

- "Release" becomes achievable, honest, and self-ratcheting: each release's
  measured performance becomes the floor for the next.
- Competitor comparison survives as information (scorecard + audit doc),
  not as semantics.
- ADR-0024 is superseded in its gating role and preserved unrewritten as
  history, per the documentary-debt rules.
- The eight performance scenario shapes are retained verbatim so any future
  option-A comparison stays protocol-compatible.

## Reconsider when

The owner decides to fund a formal competitive certification (option A), or
an external requirement (marketplace listing, procurement) demands a
third-party-verifiable comparison bar.
