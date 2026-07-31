---
doc_id: decisions-0042-signed-pack-manifests
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0042: Signed pack manifests and permission ceilings, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-packs/src/signed.rs
  - crates/optimus-host/src/extensibility.rs
depends_on:
  - docs/decisions/0016-canonical-tool-contract.md
  - docs/decisions/0041-mcp-ingress-pack-gated.md
validated_by:
  - crates/optimus-packs/src/signed.rs
---

# ADR-0042: Signed pack manifests and permission ceilings

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

Third-party packs must not escalate past SmartDeny. Unsigned loads are unsafe
by default for product-complete extensibility.

## Decision

1. Pack manifests are HMAC-SHA256 signed with a named trust root key id + secret
   stored under `{home}/packs/trust_root.json` (operator-managed).
2. **Unsigned packs are rejected by default** (missing file, empty signature,
   key mismatch, or bad MAC).
3. Each pack declares `max_policies: Vec<ToolPolicy>`. Tools outside the ceiling
   fail closed. Default third-party ceiling excludes Process, NetworkWrite, and
   Desktop.
4. Desktop `packs_verify_signed` only accepts paths under `{home}/packs/`.
5. Key rotation: replace trust root key id/secret and re-sign manifests; old
   signatures fail closed.

## Consequences

- Product exit proves load + crypto unit tests; production key management is
  operator process, not auto-enrolled.
- Permission ceiling cannot expand SmartDeny classes.

## Documentation completion addendum (2026-07-31)

## Alternatives considered

The pre-decision behaviour and an ad hoc implementation were considered. Both were rejected because they leave the boundary described by this decision implicit, inconsistently enforced, or unobservable.

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Risks

Implementation can drift from the accepted boundary while the prose remains unchanged. Source-bound documentation checks, the relevant tests below, and the full repository gate are the mitigation.

## Evaluation evidence

- `crates/optimus-packs/src/signed.rs`

## Conditions for reconsideration

Reconsider when the named boundary or threat model changes and a replacement preserves typed enforcement, observability, deterministic failure, and regression coverage.

## Relevant code

- `crates/optimus-packs/src/signed.rs`
- `crates/optimus-host/src/extensibility.rs`

## Relevant tests

- `crates/optimus-packs/src/signed.rs`
