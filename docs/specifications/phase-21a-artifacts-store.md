---
knowledge_type: specification
status: active
covers:
  - crates/optimus-kernel/src/artifacts.rs
  - crates/optimus-host/src/files.rs
  - apps/optimus-desktop/ui/app.js
  - apps/optimus-desktop/ui/index.html
depends_on:
  - docs/decisions/0002-memory-invariants.md
  - docs/architecture/parity-capability-ledger.json
validated_by:
  - cargo test -p optimus-kernel --lib artifacts
  - cargo test -p optimus-desktop --lib method_registry
  - cargo test --workspace
---

# P21A — Content-addressed Artifacts store (MVP)

**Date:** 2026-07-23  
**Goal:** Close SOTA loss #2 with a real store + list surface, not a stub panel.

## Acceptance

| # | What | Done when |
|---|---|---|
| 1 | Content-addressed blob store under `{home}/artifacts` | put → sha256 path; re-put is idempotent |
| 2 | JSONL index of provenance metadata | list returns newest-first records |
| 3 | Desktop IPC `artifacts_list` | returns `{ artifacts: [...] }` |
| 4 | Browser screenshots auto-publish | navigate/click/reload with `screenshot_b64` writes an artifact |
| 5 | Artifacts panel lists real rows | UI calls IPC and renders label/source/size |

## Storage layout

```text
{home}/artifacts/
  blobs/<sha256[0..2]>/<sha256>
  index.jsonl
```

## Out of scope

- Full gallery image viewer
- Cross-session FTS search
- Artifact deletion UI
- Signed external publish
