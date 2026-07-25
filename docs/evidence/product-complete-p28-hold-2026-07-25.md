# Product-complete program P28 hold — 2026-07-25

Planes: **program P28** · delivery **PR #37** · architecture hold (Durability
local · Security · Observability)

## Board

Three-expert review (security / product-ledger / correctness) →
**APPROVE-WITH-FIXES** (correctness initially **BLOCK** on ambiguous list +
Telegram send semantics).

### MUST-FIX applied

1. `list_ambiguous_sends` SQL-filters before LIMIT (not limit-then-filter)
2. Telegram external-send only when drain status is `ok`
3. `SendOutcome::Failed` → `mark_external_send_failed` (not ambiguous)
4. `poll_once` processes only just-enqueued telegram ids; advances offset for ignored chats
5. Live telegram requires non-empty `allowed_chat_ids`
6. Desktop `gateway_enqueue` provider allowlist = `offline` only
7. Scorecard losses/partials no longer claim missing gateway/Telegram
8. Security map + high-risk C-17 updated for P28 messaging semantics

## Commands (green after fixes)

```text
cargo test -p optimus-ops --lib
cargo test -p optimus-desktop -- --test-threads=1
npm test MailPage
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
```

## Ledger

- `gateway.queue` → parity
- `gateway.telegram` → parity (mock; live residual)
- `gateway.ui` → parity

## Non-claims

- External exactly-once delivery
- Public Telegram listen port
- Discord/Slack
- Hermes gate PASS

## Verdict

**program P28 closed after review board fixes.** Next: **program P27** and/or **P29**.
