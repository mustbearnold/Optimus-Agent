# Optimus vs Hermes — evidence-backed SOTA scorecard

Updated: 2026-07-25 · P17 release gates; program P21 tool parity; program P22 files.mutate parity; projects.scope honesty partial; artifacts post-P11

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
| **parity** | 13 | A bounded Hermes-equivalent capability has current executable evidence |
| **partial** | 12 | Useful implementation exists, but the Hermes behavior/surface is incomplete |
| **missing** | 22 | No complete executable path exists yet |
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

## Material partials

- Provider catalog: connected/disconnected breadth and capability enforcement remain incomplete
- Thinking/tool cards: lifecycle/timing separation remains incomplete
- Session hygiene: FTS jump, archive, active sort, and durable pins remain incomplete
- Web search/extract breadth
- CDP preview browser + annotations (shared-session paint + gallery still incomplete)
- Content-addressed artifacts store (list/publish/preview/delete/filter + bulk delete; export/zip still incomplete)
- Cron lifecycle and desktop CRUD
- Gateway queue without leases/delivery receipts
- Project isolation honesty (configured vs enforced) with concurrent multi-project mutate lease residual
- Slash-command/command-palette unification

## Leading product losses

1. Pack-gated stdio/HTTP MCP client
2. Durable Telegram delivery with leases/receipts and ambiguous-send handling
3. Interactive multi-tab ConPTY terminal
4. General durable child-agent DAG execution with leases/handoff artifacts
5. Comparative Hermes-vs-Optimus trajectory runner
6. Memory UI, Skills/Packs console, Messaging UI, Logs backend
7. Profiles, CUA pack, media/voice, ACP/TUI/proxy, migration, updater, and ecosystem breadth

## Current architecture truth

- **Default installed desktop:** Electron + React workbench over Rust `optimus-desktop --host-only` (ADR-0028). Not Tauri.
- **Legacy rollback:** tao + wry native shell (WebKitGTK / WebView2) via install “Legacy Wry” action.
- Native Wry IPC: ADR-0014 custom-protocol path; host HTTP mode is a test / Electron transport path.
- Browser: agent `browser_*` effector (HTTP SSRF-safe; CDP when available) is separate from the Electron sandboxed preview `WebContentsView`.
- Artifacts: content-addressed blobs under `{home}/artifacts` with list IPC; gallery incomplete
- Campaigns today: durable sequential `WriteFile`/`RunCommand`; not general subagent parity
- Gateway today: durable local inbox/outbox + loopback webhook; no Telegram adapter or delivery receipts
- Capabilities today: packs/skills/eval backends exist; desktop console remains incomplete
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

Optimus has evidence-backed architectural wins and several parity slices. It is **not yet better than Hermes in every way**: the ledger currently contains 12 partial and 22 missing capabilities. Under the strict release-version schema, 0/2,063 Hermes feature contracts and 0/8 comparative performance scenarios are currently verified. The Hermes parity version therefore remains `null`; it cannot become `0.19.0` until every row is `parity` or `win` and the final exact candidate passes the per-feature, comparative, security, cost, durability, packaging, and native-platform gates.
