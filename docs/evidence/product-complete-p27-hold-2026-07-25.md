# Product-complete program P27 hold — 2026-07-25

Planes: **program P27** · delivery **PR #38** · architecture hold (Domain /
Security / Control-plane)

## Board

Three-expert review (security / product-ledger / correctness) →
**APPROVE-WITH-FIXES**.

### MUST-FIX applied

1. Signed pack paths: relative under `packs/` only; reject `..` and absolute
2. HTTP MCP URL gate: IPv6 loopback, decimal IP, `.localhost` blocked
3. Trust root: random secret on first init (not fixed public key)
4. Host clamps MCP session `max_policies` to third-party ceiling
5. Pack verify rejects over-ceiling `max_policies`
6. `ModelId` serde transparent for UI string shape
7. Verification non-claims + delivery **PR #38**

## Commands (green after fixes)

```text
cargo test -p optimus-kernel --lib routing::tests
cargo test -p optimus-ops --lib mcp
cargo test -p optimus-packs --lib signed
cargo test -p optimus-desktop -- extensibility
npm test CapabilitiesPage
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
python3 scripts/check-crate-layers.py
python3 scripts/check-domain-modularity.py
```

## Ledger

- `provider.catalog` → parity
- `provider.failover` → parity
- `mcp.client` → parity (mock)
- `plugins.signed` → parity

## Non-claims

- Live MCP child spawn / wire protocol
- MCP available host effectors
- Production key ceremony
- Chat-path automatic failover
- Hermes gate PASS

## Verdict

**program P27 closed after review board fixes.** Next: **program P29**.
