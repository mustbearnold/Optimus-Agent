---
knowledge_type: plan
status: current
owns:
  - docs/plans/product-complete-program.md
watches:
  - docs/architecture/parity-capability-ledger.json
  - docs/architecture/sota-scorecard.md
  - docs/architecture/architecture-marks.md
  - docs/architecture/system-overview.md
  - docs/plans/full-app-microtasks.md
  - docs/maps/security-and-approvals.md
  - docs/contracts/high-risk-contracts.md
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-ops/src/gateway.rs
  - apps/optimus-ui/**
  - apps/optimus-electron/**
covers:
  - docs/plans/product-complete-program.md
depends_on:
  - docs/plans/s-plus-plus-plus-program.md
  - docs/plans/s-plus-trust-spine.md
  - docs/plans/full-app-microtasks.md
  - docs/architecture/architecture-marks.md
  - docs/decisions/0001-kernel-and-work-graph.md
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/decisions/0027-settings-driven-work-isolation.md
  - docs/decisions/0028-electron-react-shell-rust-host.md
  - docs/decisions/0029-react-workbench-and-electron-preview-view.md
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0033-multi-agent-dag-execution.md
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0036-domain-modularity-single-catalog.md
  - docs/decisions/0038-ui-ipc-architecture.md
validated_by:
  - scripts/optimus_version.py
  - scripts/check-parity-ledger.py
  - scripts/check-architecture-marks.py
  - scripts/check-domain-modularity.py
  - scripts/check-crate-layers.py
  - scripts/check-desktop-ipc-matrix.py
  - scripts/check-observability-gate.py
last_verified_commit: null
---

# Product-complete program — P20–P29

**Execution authority for finishing the daily Optimus app after architecture
S+++ (program P10–P19):** this document.

Architecture quality marks are **all S+++**
([board](../evidence/s-plus-plus-plus-review-2026-07-25.md)). That climb is
**complete and historical** for mark exits. It remains a **hold constraint**
(never demote marks for product speed).

| Authority | Document | Role |
|---|---|---|
| **This program** | `product-complete-program.md` | Phase exit gates P20–P29 → PRODUCT-COMPLETE |
| Task queue | [full-app-microtasks.md](./full-app-microtasks.md) | Microtasks `S*.*` under phase exits |
| Gap ledger | [parity-capability-ledger.json](../architecture/parity-capability-ledger.json) | Product row state |
| Architecture hold | [architecture-marks.md](../architecture/architecture-marks.md) | Marks stay S+++ |
| Architecture history | [s-plus-plus-plus-program.md](./s-plus-plus-plus-program.md) | P10–P19 done |
| Merge/ship honesty | [release-and-parity-gates.md](../architecture/release-and-parity-gates.md) | `release-check` ≠ Hermes `gate` |

This is **not** a Hermes strict-parity plan (Track Z after product-complete).
This is **not** an architecture re-climb. Product incompleteness is not an
architecture demotion.

## Naming planes (mandatory)

Always say **program P20** (etc.) in new prose.

| Plane | Token | Authority |
|---|---|---|
| Program | **program** `P20`…`P29` | **this document** (active); historical S+++ = P10–P19 |
| Decision | `ADR-NNNN` | `docs/decisions/` |
| Delivery | `PR #N` / `pr/N-…` | GitHub (never force PR# = phase) |
| Plan microtask | `S*.*` | `full-app-microtasks.md` |
| Historical product slice | `docs/specifications/phase-20*`, `phase-21*`, evidence `phase-20-*` | cite by **full path only** |
| Spec-local subheads | old `P20A` / `P21A` in historical specs | **not** program plane |
| Grade / mark | `S+++` … | architecture-marks (hold only; no product “S+++”) |

Do **not** renumber historical `phase-20*` files. Historical phase-20d packaging
≠ **program P29**. Bare `P20` next to CDP/artifacts without “program” is forbidden.

PR title planes cheat-sheet:

```text
✨ feat(packs): program P21 fail-closed tool registry
🏗️ architecture: program P22 files.mutate effect taxonomy
```

## Rules (non-negotiable)

1. **Spine reuse.** Durable host effects: Work Graph → SmartDeny → exact terminal.
   No second approval system, second tool catalog, or renderer-granted FS authority.
2. **Single ToolDesc authority.** MCP/third-party tools enter only via
   `optimus-packs` (ADR-0036).
3. **Honest grading.** Ledger rows move only with evidence path + trajectory.
   Planned ≠ Confirmed. Architecture marks do not auto-promote for product features.
4. **Hold suite** every phase close (commands below). Phase exits require
   `docs/architecture/product-complete-pNN-verification.md` plus dated
   `docs/evidence/product-complete-*` artifacts. `local/tmp/` is raw capture only.
5. **Architecture wins ties.** If a product task would demote an S+++ mark, split
   or redesign; do not ship the hole.
6. **One primary program phase per PR** (hold/fix exception only). Multi-PR per
   phase is OK.
7. **Naming planes** on every branch, commit, PR title, and microtask reference.
8. **Security map obligation.** When a phase changes approvals, effects,
   browser/network, credentials, gateway, MCP, or project authority:
   - update [security-and-approvals.md](../maps/security-and-approvals.md) in-phase
   - update touched rows in [high-risk-contracts.md](../contracts/high-risk-contracts.md)
   - residuals remain labeled product-visible where true
9. **EM refresh** after material behaviour:
   `engineering_memory.py check → owned knowledge → generate → validate --quick`.
10. **No install / push / commit** unless the user explicitly asks during a session.

## Program shape

```text
program P20  Authority + ship-surface freeze           ≈ S0
program P21  Fail-closed tool contract + pack budget   ≈ S1.1,1.2,1.4
program P22  Files mutate + project isolation enforce  ≈ S1.3 + S2.12–14
program P23  Coordinated browser + search/annotations  ≈ S1.5–1.8 → parity
program P24  Daily chat + session hygiene              ≈ S2.1–2.5
program P25  Artifacts gallery/export + cron UI        ≈ S2.6–2.11
program P26  Consoles (skills, memory, logs, packs)    ≈ S3
program P27  Extensibility (ordered sub-exits)         ≈ S4
program P28  Gateway receipts → Telegram → messaging   ≈ S5 active
program P29  Product ship + ledger close               ≈ S6
             = PRODUCT-COMPLETE
                  → later: S7 operator depth · Track Z
```

**Isolation pull-forward is deliberate:** `projects.scope` is owned by
**program P22** with `files.mutate` (Security hold), not P24/P25.

### Parallelism (exit gates ≠ single queue lock)

After **program P21** green:

| Work | May start when |
|---|---|
| P22 mutate+isolation | after P21 (prefer first if single agent) |
| P23 coordinated browser | after P21 (not a blocker for P27/P28) |
| P24 chat/session | after **P21** (tool cards need envelope) |
| P25 artifacts/cron | after P21 |
| P26 consoles | after P21; prefer P22 if claiming real mutate |
| P27 extensibility | after **P21 only** (not P23) |
| P28 messaging | independent of P23; internal receipt order only |
| P29 ship | after **P21–P28 all green** (including P23) |

## Residual ownership

| Surface | Ledger | Status | Owner |
|---|---|---|---|
| Tool contract / pack budget | `core.tool-loop`, `core.pack-budget` | **parity** (kernel) | **P21 done**; packs **console** residual → **P26** |
| `files.mutate` | `files.mutate` | **parity** (kernel) | **P22 done**; concurrent lease residual |
| Project isolation enforce | `projects.scope` | **partial** (honesty fields) | concurrent multi-project lease residual **S2.14** |
| Browser / search | `browser.*`, `web.search` | **parity** | **P23 done** (ADR-0040 coordinated dual-domain; not shared CDP) |
| Chat / session hygiene | `chat.thinking-tools`, `session.search-hygiene` | Partial | **P24** |
| Artifacts / cron UI | `artifacts.store-ui`, `cron.lifecycle` | Partial | **P25** |
| Skills / Memory / logs / commands | `skills.ui`, `memory.ui`, `desktop.logs`, `surface.commands` | Missing UI | **P26** |
| Provider catalog / failover | `provider.catalog`, `provider.failover` | Partial / Missing | **P27** |
| MCP + signed packs | `mcp.client`, `plugins.signed` | Missing | **P27** |
| Gateway / Telegram / messaging UI | `gateway.queue`, `telegram`, `ui` | Partial / Missing | **P28** |
| Install + updater | `release.updater` | Missing / Partial packaging | **P29** |
| Already parity/win product rows | streaming, durable session, shell, files.read, terminal.job, … | **HOLD** | re-evidence only if install claim changes |
| S7 / Track Z | profiles, open subagents, PTY, CUA, Hermes gate, … | Deferred | **After P29** |

**Anchors to reuse (do not reinvent):**

- Packs: `crates/optimus-packs/`, `tests/packs_budget.rs`
- Mutate spine: kernel `ToolInvocation` → runtime `Effect`; `fs_sandbox.rs`;
  `approvals_surface`, `path_confinement`
- Browser: `optimus-browser`, kernel `browser.rs`, Electron preview security tests,
  ADR-0015/0029 (P23 must supersede ambiguous “one session” language)
- Sessions: `session.rs`, desktop `ipc/sessions.rs`
- Artifacts: `optimus-artifacts`, `ArtifactsSurface.tsx`, ADR-0025
- Cron: `optimus-ops` `cron.rs`, desktop `ipc/scheduling.rs`
- Gateway: `optimus-ops` `gateway.rs`, CLI `gateway_http.rs`, UI `MailPage.tsx` shell
- Skills/memory crates exist; React consoles do not yet

---

## program P20 — Authority + ship-surface freeze

**Goal:** Install this document as execution authority; close Stage 0 residuals
honestly (done with evidence or named residual with owner).

**Microtasks:** S0.2–S0.5 (S0.1 already done).

**Scope**

- This program file; plans README authority flip; multi-program plane in
  `AGENTS.md` + `docs/contributing/artifact-naming.md`.
- Banner + S*→P* map on full-app-microtasks.
- Pointers in architecture-marks, system-overview; optional next-action on S+++
  program; relationship note on release-and-parity-gates.
- Doc-only note that historical ADR-0015 “one shared session” is **not** current
  product law until program P23 lands a superseding SharedBrowserContract
  (ADR-0029 remains the accepted two-path honesty).
- `docs/architecture/product-complete-p20-verification.md`.
- **Not** installed-app cutover (program P29). **Not** historical phase-20* renames.

**Exit gate**

- Authority docs agree on a single execution sentence.
- `python3 scripts/optimus_version.py release-check`
- `python3 scripts/check-parity-ledger.py`
- `python3 scripts/check-architecture-marks.py`
- S0 items done or residual-owned in verification md.
- EM current for touched docs.

**Ledger:** hygiene only.

**S+++ hold:** Doc hygiene, Release/parity gating.

---

## program P21 — Fail-closed tool contract + pack budget

**Ledger → parity:** `core.tool-loop`, `core.pack-budget`
(console product story may finish in P26).

**Microtasks:** S1.1, S1.2, S1.4.

**Scope**

- Fail-closed `ToolDesc` ↔ handler registry (no advertised available tool without
  handler).
- **Extend** existing `ToolOutcome` envelope for all available tools — **no
  parallel envelope type** (high-risk C-08 already Confirmed for the type).
- Schema-token pack budget hard reject + progressive `activate_pack`.
- Unavailable catalog placeholders stay non-advertised.
- `activate_pack` cannot authorize sibling calls in the same model response
  (security map).

**Exit gate**

- Registry + envelope + budget tests green.
- `check-domain-modularity.py` + crate layers green.
- Ledger trajectories updated.
- `docs/architecture/product-complete-p21-verification.md`.

**S+++ hold:** Domain, Security, Control-plane.

**Unlocks:** P24 tool cards; P27 MCP/packs; honest ads for later phases.

---

## program P22 — Files mutate + project isolation enforce

**Ledger → parity:** `files.mutate`, `projects.scope`.

**Microtasks:** S1.3 + S2.12–S2.14 (isolation pull-forward).

### Effect-taxonomy ADR (mandatory before first mutate PR)

```text
- Enumerate exact new Effect + ToolInvocation variants (e.g. Mkdir, Rename,
  Delete, Patch / Project* twins). No free-text path effects.
- is_high_risk MUST include every host-mutating file op (delete/rename/patch/
  mkdir-create). Assert-only stays non-high-risk.
- Project* variants MUST persist workspace_sha256 and reopen via
  project_authority (ADR-0031); foreign/changed root fails before effect.
- Rename: both source and dest confined under cap-std; no cross-root rename;
  secret-basename policy on both sides.
- Delete/patch crash: no success terminal without receipt; partial patch is
  failed/ambiguous per Work Graph rules — never silent half-apply success.
- Skill grants: FsWorkspace covers write/mkdir/patch/rename/delete class;
  no new grant plane.
- Campaign StepKind + approvals_surface + path_confinement + high-risk-contracts
  C-03/C-04 updated in same phase.
- Specialists: do not silently widen workspace_writer ceiling; new tools require
  explicit descriptor/ceiling change + tests (ADR-0033 registered-only).
- Update docs/maps/security-and-approvals.md Filesystem boundaries on exit.
```

Isolation-only changes without new effect kinds do **not** require the effect ADR.

### Isolation slice honesty (in vs out)

```text
IN: project_bound FS enforcement for project effects (roots from
  project_authority, not renderer catalog); status/doctor show configured_mode
  vs enforced_mode independently; allow_concurrent_projects=false denies
  concurrent *mutating* work across projects (ADR-0027), not merely hiding a
  second tab if another path can still mutate.
OUT of P20–P29: isolated_profiles sealed homes, cross-profile deny, profile
  migration/recovery (remain After P29 / S7).
Forbidden: Settings label or status bar “Isolated” when runtime still shared.
Exit tests must prove enforced_mode false under shared even if UI projects exist.
```

**Exit gate**

- Mutate suite: approval required, path confinement, crash non-replay.
- Isolation enforcement + doctor/UI enforced mode tests.
- Effect ADR (if taxonomy grew) + security map + high-risk contracts.
- `product-complete-p22-verification.md`.

**S+++ hold:** Security, Durability, UI (no renderer roots), Multi-agent ceilings.

---

## program P23 — Coordinated browser + search/annotations

**Ledger → parity (required):** `browser.cdp`, `browser.annotations`,
`browser.http`, `web.search`.

**Microtasks:** S1.5–S1.8.

### SharedBrowserContract ADR gate (mandatory before code)

Accepted ADR-0029 §9 and high-risk C-17 keep user preview ≠ agent Browser.
Historical ADR-0015 “one shared session” is **not** product law until superseded.

```text
- Supersede ADR-0015 “one shared session” language; amend ADR-0029 with an
  explicit SharedBrowserContract that does NOT merge trust domains by default.
- Allowed product claim: coordinated navigation / paint parity / annotation→composer
  via host-owned protocol (URL/state events), not shared Chromium cookie jar,
  storage partition, or agent automation of the user WebContentsView.
- Forbidden without separate break-glass ADR + tests:
  - Agent CDP attached to Electron preview WebContentsView partition
  - Shared cookies/localStorage/IndexedDB between user preview and agent effector
  - Elevating preview permissions/downloads/popups “because agent needs it”
- Agent browser_* remains Work Graph/effector path; HTTP fallback keeps
  network_policy::assert_public_http_url (pre-DNS + post-redirect).
- Preview security tests remain merge-blocking
  (apps/optimus-electron/test/preview-security.test.cjs + browser-policy).
- Exit prose must say “coordinated preview + agent browser” unless evidence
  proves a single CDP target under the new ADR.
- Do not grow a second effect authority in Electron main (Rust remains
  authoritative — ADR-0029).
- Update docs/maps/security-and-approvals.md Browser/network boundary on exit.
```

Also: annotation → composer only via explicit “Add to prompt”; gallery of prior
notes; web search extract schema + stable provenance URL.

**Exit gate**

- Navigate parity / coordination proof; annotation regression; HTTP SSRF tests
  without CDP; preview security tests pass.
- SharedBrowserContract ADR accepted; security map updated.
- Ledger trajectories to **parity**.
- `product-complete-p23-verification.md`.

**S+++ hold:** Security, UI, Observability.

**Does not block:** program P27, program P28.

---

## program P24 — Daily chat + session hygiene

**Depends on:** program P21.

**Ledger → parity:** `chat.thinking-tools`, `session.search-hygiene`.

**Microtasks:** S2.1–S2.5.

**Scope**

- Thinking blocks separate from assistant text.
- Tool cards: start / stream / success / fail / cancel + duration using
  **persisted** execution lifecycle events / stable call IDs (ADR-0031 themes) —
  not renderer-only coalescing as sole truth.
- Session FTS; archive/unarchive; durable pins + sort (distinct from presentation
  pins in layout store).

**Exit gate**

- Component/stream fixtures + session reopen tests.
- IPC matrix green if methods added (ADR-0038).
- `product-complete-p24-verification.md`.

**S+++ hold:** UI, Observability, Doc hygiene (no overclaim).

---

## program P25 — Artifacts gallery/export + cron workbench

**Ledger → parity:** `artifacts.store-ui`, `cron.lifecycle`.

**Microtasks:** S2.6–S2.11.

**Scope**

- Thumbnails, type/label filters, single export (host save/copy), bulk zip.
- Cron list/pause/resume/remove/create/history in React (`optimus-ops` owner).

**Export confinement:** host save dialog only; content-addressed store paths;
no zip-slip; secret-basename exclusion.

**Exit gate**

- Store + UI + cron validation/lease tests; e2e smoke list/create/pause.
- Cron leases not bypassable via UI-only paths.
- `product-complete-p25-verification.md`.

**S+++ hold:** Durability, Control-plane (ops peel), UI IPC, Security (export).

---

## program P26 — Consoles (surface existing backends)

**Ledger → parity:** `skills.ui`, `memory.ui`, `desktop.logs`, `surface.commands`;
packs console completes `core.pack-budget` product story.

**Microtasks:** S3.1–S3.5.

**Scope**

- Skills console: list, pin, deprecate, outcome counts (permissions stay closed).
- Memory explorer: claims, evidence, correct, forget — **recall is data, never
  ActionAuthorize**.
- Bounded redacted logs drawer.
- Capabilities console: activate/deactivate via **same kernel pack APIs** as CLI
  (activate cannot authorize sibling calls in the same model response).
- Unified slash-command registry + command palette (CLI↔desktop same registry).

**Exit gate**

- IPC + React + security tests (memory/skills non-auth).
- No second tool list invented in UI.
- `product-complete-p26-verification.md`.

**S+++ hold:** Domain, Security, Doc hygiene.

---

## program P27 — Extensibility (ordered internal exits)

**Depends on:** program P21 only (not P23).

**Ledger → parity:** `provider.catalog`, `provider.failover`, `mcp.client`,
`plugins.signed`.

**Microtasks:** S4.1–S4.7.

### Internal ordered sub-exits

| Sub-exit | Focus |
|---|---|
| **P27.a** | Rust provider/model catalog + connect state + capability flags → UI |
| **P27.b** | Capability-aware ordered failover (scripted offline) |
| **P27.c** | Pack-gated stdio MCP (mock server) |
| **P27.d** | Pack-gated HTTP MCP transport |
| **P27.e** | Signed pack manifests + permission ceiling → SmartDeny |

### MCP + signed pack security law

```text
- MCP never installs a second tool catalog. Server tools map to ToolDesc rows
  under optimus-packs; advertisement ≡ handler; unavailable remain non-advertised
  (ADR-0036 / program P21).
- MCP adapters emit only ToolInvocation / Work Graph effects. No direct FS,
  process, or network side effects outside existing effectors + SmartDeny.
- Stdio MCP child: bounded spawn (cwd, env strip aligned with command path),
  kill-on-cancel, output bounds; not UnrestrictedHost by default.
- HTTP MCP: assert_public_http_url (or stricter allowlist); no private/metadata
  destinations; body/time bounds; no wildcard redirect to loopback.
- Allowlist = intersection(pack permission ceiling, server offer, host policy).
  Name collisions with built-in ToolId fail closed.
- Signed packs: default reject unsigned; verify signature before load; document
  trust root + key rotation in ADR; permission ceiling cannot exceed SmartDeny
  classes.
- Crate placement decided in ADR (packs peel or new crate); check-crate-layers.py
  green; kernel waist does not grow MCP protocol.
- Failover cannot authorize statically denied candidates (routing invariant).
- Security map + domain modularity gates required on phase exit.
```

**ADRs expected:** MCP ingress; signed packs; optional provider catalog authority.

**Exit gate**

- All sub-exits green; mock MCP + transport + crypto/load tests; offline failover;
  UI consumes catalog; domain modularity + crate layers.
- `product-complete-p27-verification.md`.

**S+++ hold:** Domain, Security, Control-plane.

---

## program P28 — External channel (gateway → Telegram → UI)

**Independent of program P23.**

**Ledger → parity:** `gateway.queue`, `gateway.telegram`, `gateway.ui`.

**Microtasks:** S5.1–S5.4 (S5.5–S5.6 Discord/Slack remain parked).

### Fixed order + transport/security freeze

```text
1. Outbox delivery receipts + attempt leases → gateway.queue parity
2. Ambiguous-send recovery CLI + doctor
3. Telegram adapter: claim → turn → receipt (mock first, then config-gated live)
4. Messaging UI bound to real inbox/outbox

Transport + security:
- Default Telegram path: outbound long-poll / Bot API client from optimus-ops
  (no public listen port). Gateway SQLite remains local delivery authority
  (ADR-0021).
- Webhook mode (if any in P28): local bind 127.0.0.1 only + reverse-proxy in
  front; secret path/token; separate bearer; no wildcard CORS; rate/body caps
  (extend ADR-0020). Document as optional; mock tests cover both.
- Claim/attempt/receipt leases are local; external exactly-once remains
  product-visible residual (S+++ residual table) — doctor surfaces ambiguous
  sends; never ledger “parity” on external EO.
- Adapter cannot auto-grant SmartDeny or mint project roots.
- Update security map Desktop/gateway boundary + high-risk gateway semantics
  on exit.
```

**Exit gate**

- Gateway tests; adapter mock suite; messaging fixture e2e; doctor ambiguous-send.
- No false external exactly-once claims.
- `product-complete-p28-verification.md`.

**S+++ hold:** Durability (local leases/receipts), Security, Observability.

---

## program P29 — Product ship + PRODUCT-COMPLETE

**Depends on:** program P21–P28 all green.

**Microtasks:** S6.1–S6.5.

### Must-move ledger set (product-critical)

**Partial → parity:** `provider.catalog`,
`chat.thinking-tools`, `session.search-hygiene`, `web.search`, `browser.http`,
`browser.cdp`, `browser.annotations`, `cron.lifecycle`, `gateway.queue`,
`artifacts.store-ui`, `surface.commands`.

**HOLD (already parity from program P21–P22):** `core.tool-loop`, `core.pack-budget`,
`files.mutate`.

**Still partial:** `projects.scope` (honesty only; concurrent lease residual).

**Missing → parity:** `provider.failover`, `desktop.logs`,
`gateway.telegram`, `gateway.ui`, `mcp.client`, `plugins.signed`, `skills.ui`,
`memory.ui`, `release.updater` (**or** explicit no-updater ADR + honest residual —
prefer honest ADR unless signing chain is real).

**HOLD (already parity/win):** `core.work-durability`, `core.memory-integrity`,
`core.skills-lifecycle`, `core.smartdeny`, `provider.openai-compat`,
`provider.codex-oauth`, `chat.streaming`, `session.durable`, `desktop.shell`,
`desktop.native-cua`, `files.read`, `terminal.job`, `campaign.sequential`,
`eval.offline`. Re-evidence installed React path only if claim changes; do not
demote without regression proof.

**Scope**

- Installed Electron packaging + desktop entry default React (user-authorized
  install verification only).
- Native paint/a11y baseline on **installed** Electron
  (`skills/optimus-native-ui-testing`).
- Doctor: shell mode, isolation enforcement, gateway, packs.
- Signed updater + rollback **or** documented no-updater ADR.
- Scorecard + ledger pass for product-critical rows.
- **Not** Hermes `optimus_version.py gate` PASS.

**PRODUCT-COMPLETE means** a user can install Optimus and, without CLI-only
workarounds for core paths: streaming chat, stop/cancel, thinking+tool cards,
file mutate under approval, terminal jobs, coordinated browser, session hygiene,
artifacts, cron, skills/memory consoles, pack activate, MCP-gated tool if
configured, Telegram message path with receipts honesty.

**Exit board:** `docs/evidence/product-complete-p29-board-YYYY-MM-DD.md`  
Architecture marks still all S+++ (`check-architecture-marks.py`).

**Out of P29:** S7 (profiles, open subagents, PTY, CUA, migration), Track Z
(Hermes comparative, 2063 contracts, TUI/ACP/proxy, media, pack breadth).

---

## Cross-phase hold suite

```bash
python3 scripts/optimus_version.py release-check
python3 scripts/check-parity-ledger.py
python3 scripts/check-architecture-marks.py
python3 scripts/engineering_memory.py check
# when touched:
python3 scripts/check-domain-modularity.py
python3 scripts/check-crate-layers.py
python3 scripts/check-desktop-ipc-matrix.py
python3 scripts/check-observability-gate.py
cargo test -p optimus-runtime -p optimus-kernel -p optimus-packs -- --test-threads=1
```

Plus phase-specific proofs. Evidence pattern:

- `docs/architecture/product-complete-pNN-verification.md`
- `docs/evidence/product-complete-pNN-*-YYYY-MM-DD.{md,txt,json}`
- Final board: `docs/evidence/product-complete-p29-board-YYYY-MM-DD.md`

## Dependency honesty

| Dependency | Handling |
|---|---|
| MCP needs tool contract | P27 after P21 |
| P24 tool cards need envelope | P24 after P21 |
| Telegram needs receipts | P28 internal order |
| P27 ↛ P23 | extensibility not blocked on browser |
| P28 ↛ P23 | messaging not blocked on browser |
| P29 ← P23 | product-complete includes coordinated browser |
| P29 ← P21–P28 | full critical path |
| Isolation with mutate | P22 deliberate pull-forward |
| Profiles / open multi-agent / PTY | After P29 |

## Explicit non-claims

- Hermes parity version `0.19.0` or `optimus_version.py gate` PASS
- Open-ended model-spawn specialists / universal workflow executor
- Discord/Slack, ACP, TUI, OpenAI proxy, vision/voice, CUA, multi-tab PTY
- Multi-DB distributed transactions or OTLP
- External messaging exactly-once across remote process death
- Shared cookie/storage partition between preview and agent browser (unless
  break-glass ADR + tests)
- `isolated_profiles` sealed homes inside P20–P29
- Demoting or “re-proving” architecture S+++ via product feature count
- Equating historical `phase-20*` / `phase-21*` specs with program P20/P21

## Relationship to other plans

| Plan | Status |
|---|---|
| [s-plus-trust-spine.md](./s-plus-trust-spine.md) | Done (foundation 0–5) |
| [s-plus-plus-plus-program.md](./s-plus-plus-plus-program.md) | Done (architecture S+++ P10–P19) — hold constraint |
| [full-app-microtasks.md](./full-app-microtasks.md) | Current task queue under this program’s exits |
| [engineering-memory-phases.md](./engineering-memory-phases.md) | EM system; orthogonal |

## Failure handling

If a later adversarial review finds a structural hole that demotes an architecture
mark: demote **that mark only**, open `P19.x` / hold fix owned by the dimension,
and pause product phases that depend on the broken invariant. Do not keep S+++
by silence (same rule as the P19 board).

If a product phase cannot meet ledger **parity** without greenwashing: leave the
row `partial`/`missing` with a named residual; do not flip state.

## Immediate next action

1. **program P20–P23 done** (tool contract, files.mutate, coordinated browser).
2. Open **program P24** (daily chat + session hygiene) or parallel P25–P28; residual S2.14 concurrent mutate lease remains optional.

## Success definition

**Program P20–P29 complete** when the P29 board records PRODUCT-COMPLETE and
architecture marks remain S+++. Optimus is then a shippable daily personal
operator agent; ecosystem breadth and Hermes numeric parity are a separate
subsequent program.
