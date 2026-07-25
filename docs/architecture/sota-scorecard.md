# Optimus vs Hermes — evidence-backed SOTA scorecard

Updated: 2026-07-25 · P17 release gates; program P21–P28 product + program P27 extensibility parity; projects.scope honesty partial

**Status banner:** This scorecard is a **parity/planning rollup**, not the
architecture quality grade sheet. For modular architecture grades (S+++ climb)
see [architecture-marks.md](./architecture-marks.md). For current topology and
Confirmed behaviour see [system-overview.md](./system-overview.md).

**Default product shell (Confirmed):** Electron + React over Rust host; Legacy
Wry optional. Do not read “tao+wry Windows desktop shell” below as the default
install path.

**Source of truth:** `docs/architecture/parity-capability-ledger.json`  
**Validator:** `python scripts/check-parity-ledger.py`  
**Rule:** executable current-repository evidence outranks architecture blueprints and historical phase prose. A `parity` or `win` row requires both an existing evidence path and a named trajectory.
**Release-version gate:** `docs/architecture/optimus-version.json` plus `python scripts/optimus_version.py gate`. The 51 rows below are a planning rollup, not sufficient for a product-level Hermes parity claim. The strict v0.19.0 baseline contains 2,063 per-feature contracts.

## Current ledger summary

| State | Count | Meaning |
|---|---:|---|
| **win** | 4 | Current executable evidence demonstrates a structural advantage over Hermes |
| **parity** | 32 | A bounded Hermes-equivalent capability has current executable evidence |
| **partial** | 1 | Useful implementation exists, but the Hermes behavior/surface is incomplete |
| **missing** | 14 | No complete executable path exists yet |
| **total** | 51 | Capability rows tracked by the executable ledger |

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
- Electron + React default desktop shell (Wry legacy optional)
- Installed native paint/accessibility baseline
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
- Artifacts gallery, filters, export + bulk zip (program P25)
- Cron create/pause/resume/remove/history workbench (program P25)
- Skills/memory/packs consoles + redacted logs + command palette (program P26)
- Gateway outbox receipts, ambiguous-send recovery, mock Telegram adapter, messaging UI (program P28; external EO residual)
- Provider catalog + ordered failover, pack-gated MCP mock, signed packs (program P27)

## Material partials

- Project isolation honesty (configured vs enforced) with concurrent multi-project mutate lease residual

## Leading product losses

1. Interactive multi-tab ConPTY terminal
2. General durable child-agent DAG execution with leases/handoff artifacts
3. Comparative Hermes-vs-Optimus trajectory runner
4. Profiles, CUA pack, media/voice, ACP/TUI/proxy, migration, updater, and ecosystem breadth

## Current architecture truth

- **Default installed desktop:** Electron + React workbench over Rust `optimus-desktop --host-only` (ADR-0028). Not Tauri.
- **Legacy rollback:** tao + wry native shell (WebKitGTK / WebView2) via install “Legacy Wry” action.
- Native Wry IPC: ADR-0014 custom-protocol path; host HTTP mode is a test / Electron transport path.
- Browser: agent `browser_*` effector (HTTP SSRF-safe; CDP when available) is separate from the Electron sandboxed preview `WebContentsView`.
- Artifacts: content-addressed store under `{home}/artifacts` with gallery/filters/export under `exports/` (program P25)
- Campaigns today: durable sequential `WriteFile`/`RunCommand`; not general subagent parity
- Gateway today: SQLite authority with outbox receipts, ambiguous-send recovery, mock Telegram adapter, messaging UI (program P28); external EO residual honest
- Capabilities today: skills/memory/packs consoles (P26), messaging (P28), provider/MCP/signed packs extensibility (P27); live MCP child spawn + production key ops residual
- Architecture quality marks: [architecture-marks.md](./architecture-marks.md) (S+++ program)

## Baseline commands of record

```bash
python scripts/check-parity-ledger.py
python scripts/optimus_version.py validate
python scripts/optimus_version.py release-check
python scripts/check-architecture-marks.py
cargo test --workspace -- --test-threads=1
cargo build -p optimus-desktop
cd apps/optimus-desktop && npx playwright test
```

PF-00 baseline evidence: `local/tmp/baselines/PF-00-report.md`.

## Honest statement

Optimus has evidence-backed architectural wins and several parity slices. It is **not yet better than Hermes in every way**: the ledger currently contains 1 partial and 14 missing capabilities. Under the strict release-version schema, 0/2,063 Hermes feature contracts and 0/8 comparative performance scenarios are currently verified. The Hermes parity version therefore remains `null`; it cannot become `0.19.0` until every row is `parity` or `win` and the final exact candidate passes the per-feature, comparative, security, cost, durability, packaging, and native-platform gates.
