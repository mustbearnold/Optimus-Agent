---
doc_id: architecture-sota-scorecard
doc_type: explanation
plane: current
status: current
authority: supporting
summary: Updated: 2026-07-28 · thesis-axis re-key (north-star C-criteria); 13/50 runnable trajectories, unclassified pinned at 37 shrink-only; projects.scope+updater+pty+native-cua partial
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# Optimus vs Hermes — evidence-backed SOTA scorecard

Updated: 2026-07-28 · thesis-axis re-key (north-star C-criteria); 13/50 runnable trajectories, unclassified pinned at 37 shrink-only; projects.scope+updater+pty+native-cua partial

**Status banner:** This scorecard is a **parity/planning rollup**, not the
architecture quality grade sheet. For modular architecture grades (S+++ climb)
see [architecture-marks.md](../runbooks/architecture-marks.md). For current topology and
Confirmed behaviour see [system-overview.md](../architecture.md).

**Default product shell (Confirmed):** Tauri + React over Rust host
(exclusively — no Electron, no Wry rollback since 2026-08-05, spec-012).
The block below is the dated 2026-07-28 pre-cutover snapshot: treat its
Electron/Wry default-shell claims as historical, not current — do not
read them as the default install path.

**Source of truth:** `docs/architecture/parity-capability-ledger.json`  
**Validator:** `python scripts/gates/check-parity-ledger.py`  
**Rule:** executable current-repository evidence outranks architecture blueprints and historical phase prose. A `parity` or `win` row requires an existing evidence path; every row's trajectory is either runnable (`cargo:`/`playwright:`, resolved to a real target by the validator) or pinned on the validator's shrink-only unclassified list.
**Release-version gate:** `docs/architecture/optimus-version.json` plus `python scripts/tools/optimus_version.py gate`. The 50 rows below are a planning rollup, not sufficient for a product-level Hermes parity claim. The strict v0.19.0 baseline contains 2,063 per-feature contracts.

## Current ledger summary

| State | Count | Meaning |
|---|---:|---|
| **win** | 4 | Current executable evidence demonstrates a structural advantage over Hermes |
| **parity** | 41 | A bounded Hermes-equivalent capability has current executable evidence |
| **partial** | 4 | Useful implementation exists, but the Hermes behavior/surface is incomplete |
| **missing** | 1 | No complete executable path exists yet |
| **total** | 50 | Capability rows tracked by the executable ledger |

## Defensible wins

- Crash-resumable Work Graph effects
- Evidence-fenced bitemporal MetaMemory
- Outcome-gated, permission-closed Skills lifecycle
- Durable SmartDeny approval model

These are narrow evidence-backed wins, not a claim that the complete product is already superior.

## Implemented parity slices

- OpenAI-compatible provider client
- Codex OAuth Responses provider
- Streaming desktop chat
- Durable session reopen
- Tauri + React exclusive desktop shell (Wry legacy retired)
- Sandboxed Files list/read
- Bounded terminal job stream
- Sequential durable write/command campaigns
- Deterministic offline eval suite
- Store-backed causal reconstruction + local export (`optimus.causal.v1`)
- Fail-closed tool ads↔handler registry + progressive pack schema budget (program P21)
- Files mutate under SmartDeny (mkdir/rename/delete/patch + write) (program P22)
- Coordinated preview + agent browser under ADR-0040 (not shared CDP session) (program P23)
- Web search versioned extract + provenance URL (program P23)
- Annotation gallery + explicit Add to prompt (program P23)
- HTTP browser SSRF without CDP (program P23)
- Thinking blocks separate from assistant text + timed tool lifecycle cards (program P24)
- Session FTS, archive/unarchive, durable pins + sort (program P24)
- Memory FTS: free-text claim recall (`memory_search`) with per-hit standing/provenance, no new dependency (ADR-0072)
- Artifacts gallery, filters, export + bulk zip (program P25)
- Cron create/pause/resume/remove/history workbench (program P25)
- Skills/memory/packs consoles + redacted logs + command palette (program P26)
- Gateway outbox receipts, ambiguous-send recovery, mock Telegram adapter, messaging UI (program P28; external EO residual)
- Provider catalog + ordered failover, pack-gated MCP mock, signed packs (program P27)
- Product ship path: Tauri install default, doctor shell/isolation/gateway/packs, ADR-0043 no auto-updater (program P29)
- S7: profile homes, leased child agents, CUA pack scaffold, Hermes importers
- Track Z: offline comparative runner, surface/media/breadth scaffolds, Discord/Slack mock adapters

## Material partials

- Installed native paint/accessibility (`desktop.native-cua`): the PF-00 installed-app CUA baseline is not committed, so the row carries no evidence. Playwright covers paint/layout supplementally and does not substitute for installed-app proof (see `skills/optimus-native-ui-testing`). Regenerate PF-00 to a tracked path to restore the parity claim.
- Project isolation honesty (configured vs enforced) with concurrent multi-project mutate lease residual
- Release updater: no in-app signed auto-update channel (ADR-0043); reinstall script is the upgrade path
- Terminal PTY: Linux multi-tab session store scaffold; full interactive I/O residual

## Leading product losses

1. Hermes strict parity gate (2063 contracts + performance scenarios) still unverified
2. Live multi-tab ConPTY I/O product UI
3. Live computer-use effectors under heavy approval
4. Live Discord/Slack bot transports (mock enqueue only)

## Current architecture truth

- **Default installed desktop:** Tauri + React workbench over Rust host
  (ADR-0028 lineage; Tauri-exclusive since 2026-08-05, spec-012).
- **Legacy rollback:** retired — no Wry/tao shell is staged by either
  installer; the `LegacyWry` action was removed.
- Native IPC: Tauri bridge (host_invoke) + ADR-0014 custom-protocol path;
  host HTTP mode is a test / external-shell transport path.
- Browser: agent `browser_*` effector (HTTP SSRF-safe; CDP when available)
  is separate from the Tauri/React preview webview.
- Artifacts: content-addressed store under `{home}/artifacts` with gallery/filters/export under `exports/` (program P25)
- Campaigns: sequential WriteFile/RunCommand plus leased child-agent coordinator (S7)
- Gateway: SQLite authority + config-gated live Telegram long-poll (`optimus gateway telegram run`) + Telegram mock + Discord/Slack mock enqueue (live Discord/Slack residual)
- Retrieval: two SQLite FTS5 lexical indexes (`sessions_fts`, `claims_fts`); the claim index narrows only — every hit is re-authorized against `claims` and labelled with its bitemporal standing (ADR-0072). No vector, embedding, graph, reranking, or GPU index.
- Capabilities: PRODUCT-COMPLETE + S7/Track Z scaffolds; Hermes gate not claimed
- Architecture quality marks: [architecture-marks.md](../runbooks/architecture-marks.md) (S+++ program)

## Baseline commands of record

```bash
just verify
```

`just verify` runs every gate above through `scripts/verify.sh`, the single
source of truth shared by the justfile, managed land, humans, and coding agents.
Narrower tiers: `just gates` · `just check` · `just test` ·
`just ui`.

The Hermes parity gate is deliberately excluded from `just verify` because it is
fail-closed by design; run it with `just parity`.

PF-00 baseline evidence: **absent**. The installed-app CUA baseline has never
been committed, which is why `desktop.native-cua` is `partial`. Regenerate it to
a tracked path (not gitignored `local/`) to restore the parity claim.

## Honest statement

Optimus has evidence-backed architectural wins and broad product/ecosystem scaffolds. It is **not yet Hermes-strict-parity**: the ledger currently contains 4 partial and 1 missing capability row (packs.breadth re-marked by ADR-0068: breadth claimed through refusing scaffolds was cosmetics, and the scaffolds are gone), but Hermes feature-contract and performance gates remain unverified (optimus-native claims only for a small first batch). The Hermes parity version therefore remains `null`; it cannot become `0.19.0` until the full inventory, comparative, security, cost, durability, packaging, and native-platform gates pass.
