---
knowledge_type: verification
status: current
owns:
  - docs/architecture/product-complete-p27-verification.md
covers:
  - docs/plans/product-complete-program.md
depends_on:
  - docs/plans/product-complete-program.md
  - docs/decisions/0041-mcp-ingress-pack-gated.md
  - docs/decisions/0042-signed-pack-manifests.md
validated_by:
  - crates/optimus-kernel/src/routing.rs
  - crates/optimus-ops/src/mcp.rs
  - crates/optimus-packs/src/signed.rs
  - apps/optimus-desktop/src/ipc/extensibility.rs
  - apps/optimus-ui/src/components/capabilities/CapabilitiesPage.tsx
last_verified_commit: null
---

# Product-complete program P27 verification

Planes: **program P27** · delivery **PR #38** · architecture hold (Domain /
Security / Control-plane) · ledger `provider.catalog`, `provider.failover`,
`mcp.client`, `plugins.signed` → **parity**

Date: 2026-07-25

## Goal

Provider catalog + connect/capability flags → UI; capability-aware ordered
failover; pack-gated MCP stdio+HTTP (mock); signed pack manifests with
permission ceilings. MCP never installs a second tool catalog.

## What landed

| Sub-exit | Result | Evidence |
|---|:---:|---|
| P27.a catalog + flags | **PASS** | `provider_catalog_status`, CapabilitiesPage |
| P27.b ordered failover | **PASS** | `fallback_order` routing tests |
| P27.c stdio MCP mock | **PASS** | `stdio_mock_bind` |
| P27.d HTTP MCP mock | **PASS** | public URL gate + `http_mock_bind` |
| P27.e signed packs + ceiling | **PASS** | HMAC verify; Process/NetworkWrite denied |

## Residuals

| Residual | Owner |
|---|---|
| Live stdio MCP child spawn bounds | ops hardening after mock exit |
| Production trust-root key ceremony | operator process (ADR-0042) |
| MCP ToolDesc host effectors | SmartDeny registration beyond Unavailable |

## Hold suite

```bash
cargo test -p optimus-kernel --lib routing::tests
cargo test -p optimus-ops --lib mcp
cargo test -p optimus-packs --lib signed
cargo test -p optimus-desktop -- --test-threads=1 extensibility
cd apps/optimus-ui && npm test -- CapabilitiesPage
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
python3 scripts/check-crate-layers.py
python3 scripts/check-domain-modularity.py
```

## Non-claims

- Live MCP stdio child spawn / real wire protocol (mock list_tools only)
- MCP host effectors under SmartDeny (mapped tools stay `Unavailable`)
- Production trust-root key ceremony/rotation (random local seed residual)
- Runtime chat-path automatic failover (library + preview only)
- Hermes gate PASS
- External channel EO (P28 residual)

## Board

See `docs/evidence/product-complete-p27-hold-2026-07-25.md`.

## Verdict

**program P27 exit: PASS** after review-board MUST-FIX (PR #38).
Next: program P29 ship.
