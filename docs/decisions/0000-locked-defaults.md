---
doc_id: decisions-0000-locked-defaults
doc_type: decision
plane: decision
status: current
authority: record
summary: User approval: "Go ahead" after recommendation to lock §11 and begin Phase 0. Policy: blanket approval → choose recommended defaults and execute.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# Locked defaults (2026-07-18)

User approval: "Go ahead" after recommendation to lock §11 and begin Phase 0.
Policy: blanket approval → choose recommended defaults and execute.

| # | Decision | Choice | Rationale |
|---|---|---|---|
| 1 | Kernel language | **Rust** | Durability, Windows tier-0, supervisor quality; Python is tool sandbox only |
| 2 | Desktop | **CLI-first MVP**, Tauri after Phase 1 Work Graph is green | Prove resume before UI surface area |
| 3 | Memory process | **In-process library** (`optimus-memory`) for v1; daemon optional later | Fewer moving parts for Phase 0–2 |
| 4 | Hermes import week 1 | **No** hard requirement; design-compatible paths only | Phase 0 is spine, not migration |
| 5 | Default policy | **smart-deny** | Exceed Hermes security posture from day one |
| 6 | Model default | **Provider-agnostic empty** until configured | No fake batteries; CLI fails closed without model for chat (tools/jobs still testable offline) |
| 7 | Gateway v1 | **None in Phase 0** | Work Graph resume before messaging adapters |

Phase 0 exit criterion (unchanged):

> Crash the process mid multi-node job; restart; resume from last committed node; finish task.
