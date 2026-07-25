---
knowledge_type: implementation-plan
status: current
owns:
  - docs/architecture/parity-capability-ledger.json
  - docs/architecture/sota-scorecard.md
  - docs/architecture/system-overview.md
  - scripts/check-parity-ledger.py
watches:
  - apps/optimus-ui/**
  - apps/optimus-electron/**
  - apps/optimus-desktop/**
  - apps/optimus-cli/**
  - crates/optimus-kernel/**
  - crates/optimus-packs/**
  - crates/optimus-runtime/**
  - crates/optimus-skills/**
  - crates/optimus-memory/**
covers:
  - docs/architecture/parity-capability-ledger.json
  - docs/architecture/sota-scorecard.md
  - docs/architecture/system-overview.md
  - scripts/check-parity-ledger.py
depends_on:
  - docs/plans/product-complete-program.md
  - docs/architecture/parity-capability-ledger.json
  - docs/architecture/sota-scorecard.md
  - docs/architecture/system-overview.md
  - docs/specifications/react-workbench-electron-preview-cutover.md
  - docs/decisions/0027-settings-driven-work-isolation.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
validated_by:
  - scripts/check-parity-ledger.py
  - scripts/test_engineering_memory.py
last_verified_commit: null
---

# Full-app micro-tasks (efficiency-ordered)

> **Authority banner (program P20):** this file is the **task queue** (`S*.*`).
> **Phase exit gates** live in
> [product-complete-program.md](./product-complete-program.md) (**program P20–P29**).
> Architecture S+++ (P10–P19) is done and is a **hold**, not the next work queue.
> Historical specs named `docs/specifications/phase-20*` are **not** program P20.

**Authority for gaps:** `docs/architecture/parity-capability-ledger.json`

**Scorecard rollup:** `docs/architecture/sota-scorecard.md`

**How to use this plan:** pick microtasks under the **open program phase** exit
in `product-complete-program.md`. Within a stage, tasks marked `//` may run in
parallel when they do not share a write surface. One active coding outcome per
agent; mark status in this file when a stage closes.

### Stage → program phase map

| Stage / tasks | Program phase | Notes |
|---|---|---|
| S0 (S0.2 residual) | **program P20** | Authority + ship-surface freeze |
| S1.1, S1.2, S1.4 | **program P21** | Tool contract + pack budget |
| S1.3 + S2.12–S2.14 | **program P22** | Mutate + isolation pull-forward |
| S1.5–S1.8 | **program P23** | Coordinated browser → ledger **parity** |
| S2.1–S2.5 | **program P24** | Chat + session hygiene (after P21) |
| S2.6–S2.11 | **program P25** | Artifacts + cron UI |
| S3 | **program P26** | Consoles |
| S4 | **program P27** | Extensibility (ordered sub-exits; after P21 only) |
| S5 active | **program P28** | Gateway → Telegram → UI |
| S6 | **program P29** | PRODUCT-COMPLETE ship |
| S7 + Track Z | **After P29** | Operator depth / Hermes claim |
## Definition: “full app finished”

The **product-complete Optimus app** is finished when a user can install and
run the default Electron React workbench and, without CLI-only workarounds for
core paths:

1. Chat with streaming, stop/cancel, thinking + tool cards, and durable sessions.
2. Use the agent on real work: read/write files under policy, terminal jobs,
   web search, shared-session browser, approvals (SmartDeny).
3. Manage work: projects with enforced isolation, session search/archive/pins,
   artifacts gallery + export, cron CRUD, skills + memory consoles.
4. Extend safely: pack budget/tool contract, MCP client, provider catalog.
5. Reach the agent from at least one external channel (Telegram on the durable
   gateway) with a messaging UI.
6. Ship as a product: installed Electron default, packaging gates, signed updater
   or explicit no-updater honesty, doctor/logs.

**Explicitly out of the critical path** (Track Z — after product-complete):

- Hermes strict 2,063-feature / comparative performance release gate
- Discord/Slack, ACP, TUI, OpenAI proxy, vision/voice, CUA, interactive PTY
- General specialist control plane / parallel child hierarchy beyond one leased
  child-agent DAG
- GPU, OpenTelemetry export, multi-DB backup framework

Those items do not block calling the app finished for daily personal-agent use.
They block marketing “Hermes parity version = 0.19.0”.

## Efficiency rule

Order by **unlock value per unit work**:

```text
finish the ship surface
  → make the agent able to change the world (tools)
  → complete the daily workbench UX
  → surface existing backends (no new substrate)
  → extensibility (MCP/packs/providers)
  → external channel (gateway + Telegram)
  → install/update product path
  → optional operator depth (profiles, subagents)
  → parity-claim evidence (Track Z)
```

Never start Track Z or optional depth while Stages 0–3 still have open tasks.
Never invent specialist orchestration before tool contract + file mutate +
browser shared session are green.

## Status legend

| Mark | Meaning |
|---|---|
| `todo` | Not started |
| `doing` | Active (at most one per agent) |
| `done` | Focused proof green; ledger updated if row moves |
| `park` | Intentionally deferred (reason in notes) |

## Baseline (2026-07-24)

| Ledger state | Count |
|---|---:|
| win | 4 |
| parity | 10 |
| partial | 14 |
| missing | 23 |

**In flight:** branch `agent/react-workbench-cutover`; ArtifactsSurface polish
uncommitted. Repository React default exists; **installed** Electron cutover
does not.

---

## Stage 0 — Unblock the ship surface

**Why first:** every later UI/desktop task rides the React + Electron host. Open
WIP and docs drift burn parallel agents.

| ID | Status | Micro-task | Ledger / anchor | Proof |
|---|---|---|---|---|
| S0.1 | `done` | Land ArtifactsSurface polish + unit tests on cutover branch | `artifacts.store-ui` | `npm --prefix apps/optimus-ui test -- ArtifactsSurface` |
| S0.2 | `todo` | Green the React cutover verification matrix (repo only; no install) | ADR-0029, cutover spec | unit + Vite build + Electron policy + compiled-shell e2e + `cargo test -p optimus-desktop` |
| S0.3 | `done` | Regenerate Engineering Memory after cutover tree is stable | EM | `engineering_memory.py generate` + `validate --quick` → VALID/CURRENT on PR #30 |
| S0.4 | `done` | Align scorecard “architecture truth” with Electron React default | scorecard | Electron+React default banner; parity ledger green |
| S0.5 | `done` | Freeze cutover handoff: rollback = `OPTIMUS_ELECTRON_UI=legacy` | cutover spec | product-complete + ADR-0029 / electron README; no data rewrite |

**Stage exit:** repository default shell is React; verification matrix green or
explicitly deferred with reason; EM current for this tree.

---

## Stage 1 — Agent can do real work (tools critical path)

**Why second:** UI polish on a read-only / half-tool agent wastes cycles. File
mutation, tool contract, and shared browser are the shortest path from “chat
demo” to “usable operator”.

| ID | Status | Micro-task | Ledger | Proof |
|---|---|---|---|---|
| S1.1 | `done` | Fail-closed ToolDesc ↔ handler registry (no advertised tool without handler) | `core.tool-loop`, `core.pack-budget` | `ALL_DISPATCHABLE` + `assert_dispatch_registry_closed`; packs_budget + domain_modularity; program P21 |
| S1.2 | `done` | Universal tool outcome envelope for available tools | `core.tool-loop` | turn-loop wraps `ToolOutcome` + `validate_outcome`; activate/budget typed fail; residual: table-driven every-tool envelope (SHOULD) |
| S1.3 | `done` | `files.mutate`: write/patch/mkdir/rename/delete via SmartDeny exact-action | `files.mutate` | ADR-0039; path_confinement; Project* tools; program P22 |
| S1.4 | `done` | Schema-token pack budget hard reject + progressive activate | `core.pack-budget` | packs hard SchemaBudget/PackLimit; kernel progressive activate + typed budget deny; program P21 |
| S1.5 | `done` | Coordinated preview ↔ agent browser (ADR-0040 host protocol; **not** shared CDP session) | `browser.cdp` | BrowserCoordBus dual-domain tests; preview security; program P23 |
| S1.6 | `done` | Web search extract schema + provenance URL stable | `web.search` | offline fixture + unit; schema_version envelope; program P23 |
| S1.7 | `done` | Annotation → composer only via “Add to prompt”; gallery of prior notes | `browser.annotations` | React BrowserSurface tests; program P23 |
| S1.8 | `done` | HTTP browser fallback when Chromium absent remains SSRF-safe | `browser.http` | http_effector SSRF unit suite without CDP; program P23 |

**Stage exit:** agent can mutate project files under approval, browse with
**coordinated** (not merged-trust) preview + agent browser, and tool ads match
handlers. Move rows toward `parity` only with named trajectories.

**Parallel note:** S1.6–S1.8 can fan out after S1.1 lands; S1.3 and S1.5 are
the two highest-leverage serial items after S1.1.

---

## Stage 2 — Daily workbench complete (same shell, high surface area)

**Why third:** backends mostly exist; completing the React daily path is cheaper
than new substrates and is what users touch every minute.

| ID | Status | Micro-task | Ledger | Proof |
|---|---|---|---|---|
| S2.1 | `todo` | Thinking blocks separate from assistant text | `chat.thinking-tools` | transcript fixture component test |
| S2.2 | `todo` | Tool cards: start / stream / success / fail / cancel + duration | `chat.thinking-tools` | stream fixture test |
| S2.3 | `todo` | Session FTS over title + messages | `session.search-hygiene` | kernel FTS + UI |
| S2.4 | `todo` | Archive / unarchive sessions | `session.search-hygiene` | IPC + e2e |
| S2.5 | `todo` | Durable pins + sort (updated / pinned / archived) | `session.search-hygiene` | session reopen test |
| S2.6 // | `todo` | Artifacts image gallery thumbnails | `artifacts.store-ui` | React + e2e |
| S2.7 // | `todo` | Artifacts type/label filter chips | `artifacts.store-ui` | unit filter |
| S2.8 // | `todo` | Single-artifact export (host save/copy path) | `artifacts.store-ui` | IPC + e2e |
| S2.9 // | `todo` | Bulk zip export | `artifacts.store-ui` | kernel + UI confirm |
| S2.10 | `todo` | Cron list + pause/resume/remove in React | `cron.lifecycle` | cron tests + e2e |
| S2.11 | `todo` | Cron create form + per-schedule history | `cron.lifecycle` | validation + store tests |
| S2.12 | `done` | Project-bound FS honesty + Project* workspace hash | `projects.scope` | honesty fields; concurrent lease residual S2.14 |
| S2.13 | `done` | Status bar shows **enforced** isolation mode (not intent-only) | `projects.scope` | doctor/settings `enforced_mode`; legacy UI uses enforced label |
| S2.14 | `todo` | `allow_concurrent_projects=false` blocks second project open | `projects.scope` | IPC/e2e |

**Stage exit:** workbench is the daily driver without CLI for sessions, cron,
artifacts, or project scope. Artifacts and session rows can approach `parity`.

**Parallel note:** chat (S2.1–2.5), artifacts (S2.6–2.9), cron (S2.10–2.11),
and isolation (S2.12–2.14) are four disjoint write surfaces after Stage 0.

---

## Stage 3 — Surface existing backends (no new architecture)

**Why fourth:** skills, memory, and packs already have crates. Consoles convert
latent capability into product without inventing runtime.

| ID | Status | Micro-task | Ledger | Proof |
|---|---|---|---|---|
| S3.1 | `todo` | Skills console: list, pin, deprecate, outcome counts | `skills.ui` | IPC + React + skills tests |
| S3.2 | `todo` | Memory explorer: claims, evidence, correct, forget | `memory.ui` | memory crate + React; recall stays data |
| S3.3 | `todo` | Bounded redacted logs drawer | `desktop.logs` | redaction unit + e2e |
| S3.4 | `todo` | Capabilities console: pack activate/deactivate (not inspect-only) | `core.pack-budget` | e2e vs CLI parity |
| S3.5 | `todo` | Unified slash-command registry + command palette | `surface.commands` | registry test CLI↔desktop |

**Stage exit:** every major durable store has a truthful UI; logs support
supportability.

---

## Stage 4 — Extensibility (packs, MCP, providers)

**Why fifth:** after the app is usable end-to-end, MCP and catalog multiply
capability without custom code per tool. Depends on S1 tool contract.

| ID | Status | Micro-task | Ledger | Proof |
|---|---|---|---|---|
| S4.1 | `todo` | Rust-owned provider/model catalog + connect state | `provider.catalog` | kernel tests; UI consumes catalog |
| S4.2 | `todo` | Per-model capability flags (tools, vision, stream) drive UI | `provider.catalog` | schema + UI disable tests |
| S4.3 | `todo` | Capability-aware provider failover (ordered list) | `provider.failover` | scripted offline failover |
| S4.4 | `todo` | Pack-gated stdio MCP client (one server, allowlisted tools) | `mcp.client` | mock MCP integration |
| S4.5 | `todo` | Pack-gated HTTP MCP transport | `mcp.client` | transport test |
| S4.6 | `todo` | Signed pack manifests; unsigned rejected by default | `plugins.signed` | load + crypto unit |
| S4.7 | `todo` | Pack permission ceiling → SmartDeny (no privilege escalation) | `plugins.signed`, `core.pack-budget` | permission closure test |

**Stage exit:** third-party tools enter only through pack + MCP gates; providers
are one source of truth.

---

## Stage 5 — External channel (gateway → Telegram → UI)

**Why sixth:** multi-surface identity is a north-star requirement, but Telegram
before durable receipts creates double-send risk. Order is fixed.

| ID | Status | Micro-task | Ledger | Proof |
|---|---|---|---|---|
| S5.1 | `todo` | Outbox delivery receipts + attempt leases | `gateway.queue` | gateway tests |
| S5.2 | `todo` | Ambiguous-send recovery CLI + doctor | `gateway.queue` | CLI tests |
| S5.3 | `todo` | Telegram adapter: claim → turn → receipt (mock first) | `gateway.telegram` | adapter mock suite |
| S5.4 | `todo` | Messaging UI bound to real inbox/outbox | `gateway.ui` | fixture e2e |
| S5.5 | `park` | Discord adapter | `gateway.discord-slack` | after S5.4; not product-critical |
| S5.6 | `park` | Slack adapter | `gateway.discord-slack` | after S5.4; not product-critical |

**Stage exit:** user can message Optimus on Telegram; UI shows gateway truth.
Product-complete does **not** require Discord/Slack.

---

## Stage 6 — Install, ship, product honesty

**Why seventh:** repository-complete ≠ product-complete. Packaging is the last
mile after behavior exists (avoids re-proving install on a moving UI).

| ID | Status | Micro-task | Ledger / anchor | Proof |
|---|---|---|---|---|
| S6.1 | `todo` | Installed Electron packaging + desktop entry default React | ADR-0029 planned | install/relaunch skill; only when user authorizes install |
| S6.2 | `todo` | Native paint/a11y baseline on **installed** Electron | `desktop.native-cua` | CUA/PF-00-class evidence for React host |
| S6.3 | `todo` | Doctor reports shell mode, isolation enforcement, gateway, packs | supportability | doctor tests |
| S6.4 | `todo` | Signed updater + rollback **or** documented no-updater channel | `release.updater` | packaging test or explicit product decision ADR |
| S6.5 | `todo` | Ledger + scorecard pass for all Stage 0–6 product rows | ledger | every product row `parity` or `win` with trajectory |

**Stage exit:** **PRODUCT-COMPLETE.** User installs Optimus and uses the full
daily loop + Telegram without developer tooling.

---

## Stage 7 — Operator depth (post product-complete)

Run only after Stage 6 exit. Increases power; not required to call the app
finished.

| ID | Status | Micro-task | Ledger | Proof |
|---|---|---|---|---|
| S7.1 | `todo` | Profile-isolated homes | `profiles.isolation` | path + db isolation tests |
| S7.2 | `todo` | Cross-profile links deny-by-default | `profiles.isolation` | security test |
| S7.3 | `todo` | One leased child-agent campaign step + handoff artifact | `campaign.subagents` | runtime + agent ledger |
| S7.4 | `todo` | Cancel propagates to child invocation | `campaign.subagents` | cancel integration |
| S7.5 | `todo` | Bounded parallel fan-out N≤k | `campaign.subagents` | graph budget test |
| S7.6 | `todo` | Interactive multi-tab terminal (Linux first) | `terminal.pty` | platform-gated e2e |
| S7.7 | `todo` | Computer-use pack scaffold (heavy approval) | `desktop.cua` | offline fixture |
| S7.8 | `todo` | Hermes session importer | `migration.hermes` | import fixture |
| S7.9 | `todo` | Hermes skills/memory importer | `migration.hermes` | import fixtures |

---

## Track Z — Parity-claim and ecosystem breadth (not critical path)

Start **only** when Stage 6 is done and a release claim is needed. Parallelize
internally; never block product fixes for these.

| ID | Status | Micro-task | Ledger | Proof |
|---|---|---|---|---|
| Z.1 | `todo` | Comparative Hermes-vs-Optimus runner (1 scenario) | `eval.comparative` | same-task scorecard |
| Z.2 | `todo` | Expand performance scenarios to version-gate set | performance evidence | `optimus_version.py gate` |
| Z.3 | `todo` | Bind first batch of feature contracts to evidence | version gate | evidence JSON non-zero |
| Z.4 | `todo` | OpenAI-compatible proxy (chat first) | `surface.proxy` | HTTP tests |
| Z.5 | `todo` | Headless TUI (approvals + chat) | `surface.tui` | smoke |
| Z.6 | `todo` | ACP/IDE bridge thin adapter | `surface.acp` | contract test |
| Z.7 | `todo` | Vision analyze tool | `media.vision-image` | offline fixture |
| Z.8 | `todo` | Image generate pack | `media.vision-image` | offline fixture |
| Z.9 | `todo` | STT/TTS composer | `media.voice` | mock audio |
| Z.10 | `todo` | Breadth packs: one tool each office/devex/home | `packs.breadth` | pack activate |
| Z.11 | `todo` | Discord/Slack (if still parked in S5) | `gateway.discord-slack` | adapters + UI |

---

## Parked architecture (do not schedule as product tasks)

| Item | Why parked |
|---|---|
| Dedicated control-plane process | Kernel remains the waist |
| Built-in specialist roster + router | Contracts exist; no product need until S7 child-agent works |
| Universal workflow executor | Definitions ≠ execution |
| Shared multi-DB transactions/backup | Explicit unknown; product does not need it first |
| Force-abort mid-`ureq` connect | Platform limit; document only |
| GPU embeddings / OTEL export | Optional; CPU fallback and local evidence first |

---

## Critical-path diagram

```text
S0 ship surface
 └─► S1 tools (mutate + tool contract + shared browser)
      ├─► S2 workbench (// chat | artifacts | cron | isolation)
      ├─► S3 consoles (skills, memory, logs, packs, palette)
      └─► S4 extensibility (catalog, failover, MCP, signed packs)
           └─► S5 gateway receipts → Telegram → messaging UI
                └─► S6 install + updater honesty + ledger green
                     = PRODUCT-COMPLETE
                          ├─► S7 operator depth (optional)
                          └─► Track Z parity-claim / ecosystem
```

## Fastest “next session” queue

If only one agent is working, pull in this exact order (skip `done` items):

1. S0.2 (cutover matrix residual) if still open
2. S2.14 concurrent multi-project mutate lease residual (or skip)
3. S1.5 shared browser (program P23), then S2.1–S2.5 chat/session (P24)
4. S2.6–S2.11 artifacts/cron (P25) // S3 consoles (P26)
5. S4 extensibility (P27) after P21 (already done)
6. S5 gateway → Telegram (P28)
7. S6 install/updater (P29)

Skip ahead only when a listed dependency is already `done`.

## Task hygiene

1. One micro-task = one focused proof. No silent scope growth.
2. When a ledger row can move, update
   `docs/architecture/parity-capability-ledger.json` **and** the scorecard
   marker in the same change.
3. Bug fixes that regress a stage require a regression test (AGENTS.md law 18).
4. Do not commit, install, or push unless the user asks.
5. Mark status in **this file** when a task completes so the queue stays honest.

## Counts

| Stage | Tasks | Role |
|---|---:|---|
| S0 | 5 | Unblock |
| S1 | 8 | Agent power |
| S2 | 14 | Daily workbench |
| S3 | 5 | Consoles |
| S4 | 7 | Extensibility |
| S5 | 4 active (+2 park) | Messaging |
| S6 | 5 | Product ship |
| **Critical path total** | **48** | → product-complete |
| S7 | 9 | Operator depth |
| Track Z | 11 | Parity claim |
| **Full backlog** | **68** | including optional |

---

## Change log

| Date | Note |
|---|---|
| 2026-07-24 | Initial efficiency-ordered full-app micro-task plan recorded. |
