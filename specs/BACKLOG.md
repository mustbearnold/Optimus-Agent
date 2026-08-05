---
doc_id: specs-backlog
doc_type: reference
plane: work
status: current
authority: canonical
summary: One line per known-but-unspecced capability or gap; items graduate to a capability spec when work starts.
reviewed_on: 2026-08-05
review_by: 2026-10-31
knowledge_type: specification
covers:
  - specs/**
---

# BACKLOG

One line per known-but-unspecced capability or gap. Items move into a
`specs/NNN-<slug>/spec.md` when work starts (SDD loop: no code without a spec).

- ~~Windows Tauri packaging~~ → `specs/012-windows-tauri-packaging/spec.md`
  (implemented 2026-08-05; `desktop-wry-fallback` ontology row removed,
  both installers stage Tauri exclusively).
- Renderer-pixel evidence under Tauri: the evidence ceiling (launch gate +
  transport unit tests + desktop e2e + webkit layout audit) is recorded in
  spec-001; a WebKitGTK playwright-class driver would raise it (open).
- Historical docs fate: `_attic/` holds plans, evidence, lessons, history,
  verification records, marks, and historical specifications — decide per
  item (keep as docs/decisions-style records, or delete; git preserves all).

## From the retired roadmap (2026-08-05)

- (Current Optimus Agent roadmap) — see roadmap history in _attic/current
- (North star) — see roadmap history in _attic/current
- (How work is prioritized) — see roadmap history in _attic/current
- (Current priority evidence) — see roadmap history in _attic/current
- reduce unnecessary approvals for harmless, explicitly requested confined
- strengthen multi-turn and longitudinal continuity;
- expand adaptive neutral-human testing without letting its scenarios steer the
- mature specialist routing and bounded collaboration from registered verticals
- keep the governed component database, documentation, and Engineering Memory
- retire disposable generated output and machine-local data continuously so
- (Exit measure) — see roadmap history in _attic/current
