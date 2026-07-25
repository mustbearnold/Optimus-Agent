# Optimus vs Hermes — evidence-backed SOTA scorecard

Updated: 2026-07-25 · P17 release gates; PRODUCT-COMPLETE + S7 operator depth + Track Z scaffolds; Hermes gate unverified; projects.scope+updater+pty partial

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
| **parity** | 44 | A bounded Hermes-equivalent capability has current executable evidence |
| **partial** | 3 | Useful implementation exists, but the Hermes behavior/surface is incomplete |
| **missing** | 0 | No complete executable path exists yet |
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
- Product ship path: Electron install default, doctor shell/isolation/gateway/packs, ADR-0043 no auto-updater (program P29)
- S7: profile homes, leased child agents, CUA pack scaffold, Hermes importers
- Track Z: offline comparative runner, surface/media/breadth scaffolds, Discord/Slack mock adapters

## Material partials

- Project isolation honesty (configured vs enforced) with concurrent multi-project mutate lease residual
- Release updater: no in-app signed auto-update channel (ADR-0043); reinstall script is the upgrade path
- Terminal PTY: Linux multi-tab session store scaffold; full interactive I/O residual

## Leading product losses

1. Hermes strict parity gate (2063 contracts + performance scenarios) still unverified
2. Live multi-tab ConPTY I/O product UI
3. Live computer-use effectors under heavy approval
4. Live Discord/Slack bot transports (mock enqueue only)

## Current architecture truth

- **Default installed desktop:** Electron + React workbench over Rust `optimus-desktop --host-only` (ADR-0028). Not Tauri.
- **Legacy rollback:** tao + wry native shell (WebKitGTK / WebView2) via install “Legacy Wry” action.
- Native Wry IPC: ADR-0014 custom-protocol path; host HTTP mode is a test / Electron transport path.
- Browser: agent `browser_*` effector (HTTP SSRF-safe; CDP when available) is separate from the Electron sandboxed preview `WebContentsView`.
- Artifacts: content-addressed store under `{home}/artifacts` with gallery/filters/export under `exports/` (program P25)
- Campaigns: sequential WriteFile/RunCommand plus leased child-agent coordinator (S7)
- Gateway: SQLite authority + Telegram mock + Discord/Slack mock enqueue (live bots residual)
- Capabilities: PRODUCT-COMPLETE + S7/Track Z scaffolds; Hermes gate not claimed
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

Optimus has evidence-backed architectural wins and broad product/ecosystem scaffolds. It is **not yet Hermes-strict-parity**: the ledger currently contains 3 partial and 0 missing capability rows, but Hermes feature-contract and performance gates remain unverified (optimus-native claims only for a small first batch). The Hermes parity version therefore remains `null`; it cannot become `0.19.0` until the full inventory, comparative, security, cost, durability, packaging, and native-platform gates pass.
