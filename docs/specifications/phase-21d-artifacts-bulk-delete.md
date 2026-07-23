---
knowledge_type: specification
status: active
covers:
  - crates/optimus-kernel/src/artifacts.rs
  - apps/optimus-desktop/src/ipc/files.rs
  - apps/optimus-desktop/src/ipc/router.rs
  - apps/optimus-desktop/ui/app.js
  - apps/optimus-desktop/ui/index.html
depends_on:
  - docs/specifications/phase-21a-artifacts-store.md
validated_by:
  - cargo test -p optimus-kernel --lib artifacts
  - cargo test -p optimus-desktop --bin optimus-desktop -- artifacts_
---

# P21D — Artifacts bulk delete

**Date:** 2026-07-23  
**Goal:** Close the bulk-ops gap on the artifacts store with multi-select delete.

## Acceptance

| # | What | Done when |
|---|---|---|
| 1 | `ArtifactStore::delete_many` | One exclusive lock; max 50; per-item best effort |
| 2 | IPC `artifacts_delete_many` | `{ sha256s: string[] }` → `{ deleted, failed, ok }` |
| 3 | Desktop multi-select | Checkboxes + Delete selected (sidebar + page) |
| 4 | Tests | Kernel + desktop unit coverage |

## Out of scope

- Bulk export / zip download
- Cross-session retention policies
