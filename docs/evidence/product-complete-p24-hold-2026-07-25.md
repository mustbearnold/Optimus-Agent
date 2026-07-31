---
doc_id: evidence-product-complete-p24-hold-2026-07-25
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: Planes: program P24 · delivery PR #34 · architecture hold
reviewed_on: 2026-07-31
review_by: never
---

# Product-complete program P24 hold — 2026-07-25

Planes: **program P24** · delivery **PR #34** · architecture hold

## Board

Three-expert review (architecture-UI / product-ledger / correctness) →
**APPROVE-WITH-FIXES** (correctness initially **BLOCK** on FTS path).

### MUST-FIX applied

1. FTS reindex on `begin_turn` / `finish_turn` / `rename` (not only `save`)
2. FTS backfill on open when empty
3. Empty MATCH (punctuation-only) returns empty list
4. Terminal paths flush thinking before settle
5. Client re-sort after pin/archive
6. Drop “jump” from ledger capability under parity
7. Residual ownership + critical IPC allowlist drift fixed

## Commands (green after fixes)

```text
cargo test -p optimus-kernel --lib hygiene_tests
cargo test -p optimus-desktop -- --test-threads=1
npm test conversationStore / Transcript
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-parity-ledger.py
```

## Ledger

- `chat.thinking-tools` → parity
- `session.search-hygiene` → parity

## Non-claims

- Provider-native full CoT token stream
- Durable thinking on session reopen
- Hermes gate PASS

## Verdict

**program P24 closed after review board fixes.** Next: **program P25**.
