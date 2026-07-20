# Optimus vs Hermes — evidence-backed SOTA scorecard

Updated: 2026-07-19 · PF-01 ledger baseline

**Source of truth:** `docs/architecture/parity-capability-ledger.json`  
**Validator:** `python scripts/check-parity-ledger.py`  
**Rule:** executable current-repository evidence outranks architecture blueprints and historical phase prose. A `parity` or `win` row requires both an existing evidence path and a named trajectory.

## Current ledger summary

| State | Count | Meaning |
|---|---:|---|
| **win** | 4 | Current executable evidence demonstrates a structural advantage over Hermes |
| **parity** | 10 | A bounded Hermes-equivalent capability has current executable evidence |
| **partial** | 11 | Useful implementation exists, but the Hermes behavior/surface is incomplete |
| **missing** | 26 | No complete executable path exists yet |
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
- tao+wry Windows desktop shell
- Installed native paint/accessibility baseline
- Sandboxed Files list/read
- Bounded terminal job stream
- Sequential durable write/command campaigns
- Deterministic offline eval suite

## Material partials

- Pack budget and model tool loop: descriptors and handlers are not yet one fail-closed canonical contract
- Provider catalog: connected/disconnected breadth and capability enforcement remain incomplete
- Thinking/tool cards: lifecycle/timing separation remains incomplete
- Session hygiene: FTS jump, archive, active sort, and durable pins remain incomplete
- Web search/extract breadth
- HTTP browser effector without shared CDP session
- Cron lifecycle and desktop CRUD
- Gateway queue without leases/delivery receipts
- Durable project→policy/root binding
- Slash-command/command-palette unification

## Leading product losses

1. Shared-session CDP Preview Browser
2. Content-addressed Artifacts store and gallery
3. Durable Telegram delivery with leases/receipts and ambiguous-send handling
4. Pack-gated stdio/HTTP MCP client
5. Interactive multi-tab ConPTY terminal
6. General durable child-agent DAG execution with leases/handoff artifacts
7. Comparative Hermes-vs-Optimus trajectory runner
8. Files mutation path, Memory UI, Skills/Packs console, Messaging UI, Logs backend
9. Profiles, CUA pack, media/voice, ACP/TUI/proxy, migration, updater, and ecosystem breadth

## Current architecture truth

- Desktop: **tao + wry WebView2**, not Tauri/React
- Native IPC: ADR-0014 `window.ipc`; HTTP mode is a test path
- Browser today: SSRF-safe HTTP effector; CDP Preview Browser absent
- Campaigns today: durable sequential `WriteFile`/`RunCommand`; not general subagent parity
- Gateway today: durable local inbox/outbox + loopback webhook; no Telegram adapter or delivery receipts
- Capabilities today: packs/skills/eval backends exist; desktop console remains incomplete

## Baseline commands of record

```bash
python scripts/check-parity-ledger.py
cargo test --workspace -- --test-threads=1
cargo build -p optimus-desktop
cd apps/optimus-desktop && npx playwright test
```

PF-00 baseline evidence: `local/tmp/baselines/PF-00-report.md`.

## Honest statement

Optimus has evidence-backed architectural wins and several parity slices. It is **not yet better than Hermes in every way**: the ledger currently contains 11 partial and 26 missing capabilities. The parity-plus claim remains blocked until every row is `parity` or `win` and the final exact candidate passes comparative, security, cost, durability, packaging, and native Windows gates.
