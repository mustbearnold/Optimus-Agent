---
knowledge_type: decision
status: current
covers:
  - crates/optimus-packs/src/signed.rs
  - apps/optimus-desktop/src/ipc/extensibility.rs
depends_on:
  - docs/decisions/0016-canonical-tool-contract.md
  - docs/decisions/0041-mcp-ingress-pack-gated.md
validated_by:
  - crates/optimus-packs/src/signed.rs
last_verified_commit: null
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
