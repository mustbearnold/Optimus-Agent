---
doc_id: architecture-product-complete-p28-verification
doc_type: history
plane: history
status: historical
authority: historical
summary: Planes: program P28 · delivery PR #37 · architecture hold (Durability local leases/receipts · Security · Observability) · ledger gateway.queue, gateway.telegram, gateway.ui → parity
reviewed_on: 2026-07-31
review_by: never
knowledge_type: verification
owns:
  - docs/architecture/product-complete-p28-verification.md
covers:
  - docs/plans/product-complete-program.md
depends_on:
  - docs/plans/product-complete-program.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-ops/src/gateway.rs
  - crates/optimus-ops/src/telegram.rs
  - crates/optimus-host/src/messaging.rs
  - apps/optimus-ui/src/components/mail/MailPage.tsx
  - scripts/check-desktop-ipc-matrix.py
---

# Product-complete program P28 verification

Planes: **program P28** · delivery **PR #37** · architecture hold (Durability
local leases/receipts · Security · Observability) · ledger `gateway.queue`,
`gateway.telegram`, `gateway.ui` → **parity**

Date: 2026-07-25

## Goal

Outbox delivery receipts + attempt leases; ambiguous-send recovery; Telegram
adapter mock claim→turn→receipt; messaging UI bound to real gateway inbox/outbox
with honesty about external exactly-once.

## What landed

| Item | Result | Evidence |
|---|:---:|---|
| Outbox receipts + leases | **PASS** | `list_outbox_receipts`, claim leases (existing + extended) |
| Ambiguous-send list/ack | **PASS** | `list_ambiguous_sends`, CLI `gateway ambiguous/ack`, doctor counts |
| Telegram mock adapter | **PASS** | `telegram.rs` poll/process + mock transport tests |
| Messaging UI | **PASS** | `MailPage` + `gateway_*` IPC |
| No false external EO | **PASS** | UI/doctor/CLI notes; residual named |
| IPC matrix | **PASS** | 7 messaging methods registered |

## Residuals

| Residual | Owner |
|---|---|
| External messaging exactly-once across remote process death | architecture residual table / never ledger parity |
| Live Telegram long-poll Bot API (config-gated) | ops hardening; mock is product exit |
| Discord/Slack adapters | S5.5–S5.6 parked |
| Gateway HTTP webhook already loopback-only | ADR-0020/0021 hold |

## Hold suite

```bash
cargo test -p optimus-ops --lib
cargo test -p optimus-desktop -- --test-threads=1 messaging
cd apps/optimus-ui && npm test -- MailPage
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
```

## Non-claims

- External exactly-once delivery
- Public listen port for Telegram
- Discord/Slack
- Hermes gate PASS
- Auto SmartDeny grant or project root mint from adapter

## Board

See `docs/evidence/product-complete-p28-hold-2026-07-25.md`.

## Verdict

**program P28 exit: PASS** after review-board MUST-FIX (PR #37).
Next: program P27 extensibility and/or P29 ship.
