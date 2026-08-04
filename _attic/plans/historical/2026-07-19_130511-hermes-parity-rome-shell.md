---
doc_id: plans-historical-2026-07-19-130511-hermes-parity-rome-shell
doc_type: history
plane: history
status: historical
authority: historical
summary: Goal: Make Optimus Desktop a conversation-first Windows shell that matches Hermes Desktop information architecture and daily-use surfaces, exceeds Hermes on Optimus’s architectural wins (durability, memory fence, SmartDeny, packs,...
reviewed_on: 2026-07-31
review_by: never
---

# Optimus Desktop — Hermes Parity Shell + Digital Rome Aesthetic

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.
> **Mode:** Plan only (no execution in the planning turn).
> **User directive:** ULTRAPLAN / ULTRATHINK — Optimus UI should be a Hermes Agent Desktop reskin with **all features and functionality**, better in every way, with a **digital refined ancient Rome** aesthetic.

**Goal:** Make Optimus Desktop a conversation-first Windows shell that matches Hermes Desktop information architecture and daily-use surfaces, exceeds Hermes on Optimus’s architectural wins (durability, memory fence, SmartDeny, packs, campaigns), and applies a single refined “digital Rome” design system — without sacrificing function-first gates (native install + CUA proof).

**Architecture:** Keep the existing **tao + wry WebView2** host and Rust kernel IPC. Do **not** Electron-fork Hermes. Grow `apps/optimus-desktop/ui/` into a multi-pane shell (left rail · chat · optional right rails · status bar) driven by the same IPC surface (`ipc.rs` + kernel). Aesthetic is CSS design tokens + component language only (no new UI framework). Function wires existing Optimus backends first; new backends only where Hermes has a daily-use surface Optimus lacks.

**Tech Stack:** Rust (`optimus-desktop`, `optimus-kernel`, `optimus-runtime`), single-page `ui/index.html` (+ later split CSS/JS modules if file > ~2k LOC), Playwright HTTP mode, native install via `scripts/rebuild-install-relaunch.sh`, CUA via cua-driver on installed exe.

---

## 0. What the Hermes screenshot proves (target IA)

From the attached Hermes Desktop frame:

| Zone | Hermes surface | Optimus today | Gap |
|---|---|---|---|
| **Window chrome** | Single top bar: nav + tabs + window controls | Single custom titlebar (session + Tasks/Copy + win btns) | Close; needs tabs / logs entry |
| **Left primary nav** | New session, Capabilities, Messaging, Artifacts | Only sessions + cron + import | **Large** |
| **Session list** | Search, PINNED, SESSIONS count, SIGNAL section | Flat session list | Medium |
| **Main chat** | Thinking rows, tool cards, code blocks, subagent strip | Chat + coalesced tools + markdown | Medium polish / thinking UX |
| **Right files** | Home tree (MUSTB …) | None | **Large** |
| **Terminal** | Embedded pane | None | **Large** |
| **Composer** | Model · voice · send · “Push it further” | Provider/model/think/fast/access | Medium |
| **Status bar** | Gateway, agents, cron, tokens, version | Foot meta (home + phase) only | **Large** |
| **Multi-agent** | “1 Subagent”, Agents panel | Campaigns IPC exists, weak UI | Medium |
| **Aesthetic** | Dark glass / product chrome | Sharp glass / amber | Rome system not defined |

**Non-goals for v1 shell parity:** reimplement Hermes Electron stack; pixel-clone Hermes; ship Telegram/MCP in the first UI milestone if backends incomplete — expose **honest stubs** with doctor flags rather than fake green.

**Hard constraints (from project memory):**
1. Function > polish still applies for *new* backends — but shell IA that surfaces **already-shipped** kernel features is function, not polish.
2. After every desktop rebuild: `bash scripts/rebuild-install-relaunch.sh` → native CUA proof (not HTTP-only).
3. ADR-0014: native `optimus.localhost` uses `window.ipc`, not `fetch`.
4. Zero curved corners remain unless user revises (Rome = geometric, not soft blobs).
5. Codex OAuth: nested `reasoning.effort` only; models Sol/Terra/Luna.

---

## 1. Aesthetic system — “Digital Refined Ancient Rome”

### 1.1 Principles
- **Material:** obsidian stone + thin gold inlay + smoked glass overlays (not marble texture spam).
- **Geometry:** zero radius (already); columnar rhythm via **12px gutters**, hairline rules, inscribed panels.
- **Type:** Segoe UI body; uppercase micro-labels with Trajan-like tracking (`letter-spacing: 0.08–0.14em`) for section labels only.
- **Color tokens (dark default):**
  - `--rome-void: #0a0a0c` (base)
  - `--rome-stone: #141416`
  - `--rome-ash: #1c1c20`
  - `--rome-inlay: #c9a227` (SPQR gold; map from current `--accent`)
  - `--rome-inlay-dim: rgba(201,162,39,.14)`
  - `--rome-ink: #ece7dc` (warm off-white, not pure #fff)
  - `--rome-mute: #8a8578`
  - `--rome-ok: #5d9b7a` (laurel green)
  - `--rome-warn: #c9a227`
  - `--rome-danger: #b54a3c` (Pompeian red)
  - `--rome-glass: rgba(255,255,255,.04)`
  - `--rome-hairline: rgba(201,162,39,.18)`
- **Motifs (subtle, rare):**
  - 1px gold hairline under header / status bar
  - Section labels: `PINNED`, `SESSIONS`, `SIGNAL` style caps
  - Optional tiny laurel or SPQR mark **only** on empty states / about — never clutter titlebar (user rejected logo-in-chrome)
- **Motion:** 80–120ms ease-out; no bounce; reduced-motion respected.
- **Light theme:** warm travertine grays + same gold inlay (phase after dark lock).

### 1.2 Component language
| Component | Rome treatment |
|---|---|
| Tool card | Glass inscribed panel, gold left rail on `run`, laurel green on `ok` |
| Code block | Stone inset, mono, no radius |
| User bubble | Stone slab border hairline |
| Status bar | Full-width plinth; monospace metrics |
| Nav item | Left gold bar on active; mute label |
| Composer | Inlaid border (`--composer-ring` → gold-blue hybrid or pure gold) |

---

## 2. Target shell layout (ASCII)

```
┌─ titlebar (ONE header): [drag|session title········] [Tasks][···][theme][─□×]
├──────────────┬────────────────────────────────────┬─────────────────────────┤
│ LEFT RAIL    │ MAIN                               │ RIGHT (collapsible)     │
│ • New session│  chat scroll                       │ FILES tab | ARTIFACTS   │
│ • Capabilities│  thinking / tools / md            │ tree / preview          │
│ • Messaging  │                                    ├─────────────────────────┤
│ • Artifacts  │                                    │ TERMINAL (collapsible)  │
│ ─────────    │                                    │ xterm or pre+input      │
│ Search       │                                    │                         │
│ PINNED       │                                    │                         │
│ SESSIONS n/m │                                    │                         │
│ SIGNAL / GW  │                                    │                         │
│ Cron (cap)   │                                    │                         │
│ Status/Doctor│                                    │                         │
├──────────────┴────────────────────────────────────┴─────────────────────────┤
│ STATUS BAR: gateway · agents · cron · tokens · model · home · version       │
└─ composer docked above status (or overlapping bottom of main) ──────────────┘
```

**Default widths:** left `288px`, right `280px` (hidden until toggled), terminal height `180px` when open.  
**Persistence:** `localStorage` keys `optimus.ui.layout` (pane open/width).

---

## 3. Feature parity matrix (ordered by daily-use impact)

### P0 — Shell + surface what already works (function)
1. **IA scaffold** — left nav sections, main, right pane shell, status bar, single header (extend current).
2. **Sessions UX** — pin, search, counts, last-active sort, rename/delete if store supports (else hide).
3. **Status bar live** — doctor + auth + cron job count + campaign active + gateway flag + token estimate if available.
4. **Approvals UI** — list pending SmartDeny grants; Grant/Deny buttons → existing IPC.
5. **Campaigns UI** — create/run/status list in Capabilities or Agents drawer (IPC exists).
6. **Cron operator UI** — list/add/tick/enable in left SIGNAL or Cron section (not spam-only buttons).
7. **Thinking presentation** — stream “Thinking” rows separate from final answer (parse stream events).
8. **Tool cards v2** — Hermes-like duration, collapse, copy; keep coalesce ×N.
9. **Composer parity** — model ladder, thinking ladder, fast, access; optional “Continue” chip.
10. **Rome tokens** applied to entire shell.

### P1 — Near-Hermes product surfaces (need thin new backend or adapters)
11. **Files pane** — read-only tree under `OPTIMUS_HOME` + workspace roots; open text preview; no full IDE.
12. **Terminal pane** — PTY via `portable-pty` or reuse CommandCapture job streaming into UI; start with **job log tail** if PTY is heavy.
13. **Capabilities page** — pack list, schema token budget, activate_pack.
14. **Messaging / Gateway page** — inbox/outbox viewer + webhook URL; Telegram when adapter lands.
15. **Artifacts** — session-scoped file outputs registry (kernel write path → list).
16. **Logs drawer** — desktop + kernel log tail from `%LOCALAPPDATA%/optimus/logs`.
17. **Subagent strip** — campaign step progress under composer (like Hermes “1 Subagent”).

### P2 — Beat Hermes (Optimus-native)
18. **MetaMemory browser** — evidence cards, never instruction.
19. **Work Graph inspector** — job states, resume, approval edges.
20. **Eval panel** — run trajectory suite, show scorecard deltas.
21. **CUA / computer-use pack status** — doctor + one-click smoke.
22. **Multi-root workspace** — project switcher (Optimus projects).

### Explicitly later / do not fake
- Full CDP Browser-Use clone until effector exists.
- MCP marketplace until adapter pack exists.
- Voice mic until TTS/STT wired.

---

## 4. Implementation plan (bite-sized tasks)

### Task 1: Design tokens — Rome palette in CSS

**Objective:** Centralize digital-Rome tokens; map old vars.

**Files:**
- Modify: `apps/optimus-desktop/ui/index.html` (`:root` / light theme)

**Steps:**
1. Replace/alias `--bg-*`, `--accent*`, `--text*` with Rome tokens above; keep aliases so existing classes still work (`--accent: var(--rome-inlay)`).
2. Hairline utility: `.inlay-rule { border-color: var(--rome-hairline); }`.
3. Playwright: assert `getComputedStyle(document.documentElement).getPropertyValue('--rome-inlay')` non-empty.
4. Rebuild-install not required until Task 3 batch (or batch with Task 2).

---

### Task 2: Shell grid scaffold (no new backends)

**Objective:** DOM/CSS for left rail / main / right / terminal / status bar; panes toggle.

**Files:**
- Modify: `apps/optimus-desktop/ui/index.html` (body structure + CSS + minimal JS)

**Layout IDs (canonical):**
- `#titlebar` (exists)
- `#leftRail`, `#navPrimary`, `#sessionSearch`, `#pinnedList`, `#sessionList`, `#signalPanel`
- `#main`, `#chat`, `#composerWrap`
- `#rightPane`, `#filesTree`, `#artifactList`
- `#termPane`, `#termOut`, `#termIn`
- `#statusBar` with `#stGateway #stAgents #stCron #stTokens #stModel #stHome #stVer`
- Toggles: `#toggleRight`, `#toggleTerm` in titlebar actions

**Steps:**
1. Playwright test: `shell has leftRail main statusBar`; right/term hidden by default; toggles open them; statusBar height ≤ 28px; nothing overflows `100vh` with cron spam (reuse layout lock test).
2. Implement markup + CSS grid; `localStorage` layout persistence.
3. Pass PW; install only if shipping mid-milestone.

---

### Task 3: Left nav — primary destinations (routes)

**Objective:** In-shell “pages”: `chat` | `capabilities` | `messaging` | `artifacts` (chat is default).

**Files:**
- Modify: `ui/index.html` JS router `state.route`
- No new Rust yet — pages can show doctor-backed empty states

**Steps:**
1. Test: click Capabilities → `#page-capabilities` visible, chat hidden; New session stays on chat route.
2. Implement nav buttons + page containers.
3. Wire New session to existing `newSession`.

---

### Task 4: Sessions list parity (pin + sections)

**Objective:** PINNED / SESSIONS grouping + search (client-side pin in `localStorage` until store field exists).

**Files:**
- Modify: `ui/index.html` `renderSessions`
- Optional later: `optimus-kernel` session `pinned` column — **YAGNI** until pins must sync across machines

**Steps:**
1. Test: pin session → appears under PINNED; search filters both.
2. Implement pin toggle on thread row (context menu or ★).
3. Show `SESSIONS {shown}/{total}` like Hermes.

---

### Task 5: Status bar — live doctor/auth/cron

**Objective:** Bottom plinth always shows operator truth.

**Files:**
- Modify: `ui/index.html` bootstrap + poll every 15s
- Modify: `ipc.rs` `doctor_json` if fields missing (`gateway`, `cron_jobs`, `campaigns_active`)

**Steps:**
1. Extend doctor JSON:
```rust
"cron_jobs": n,
"campaigns_active": n,
"gateway": true/false,
"approvals_pending": n,
```
2. UI bind; PW assert `#stCron` matches cron_list length after add.
3. Native smoke: status shows Codex ready when auth present.

---

### Task 6: Approvals panel (function — backend exists)

**Objective:** Pending SmartDeny approvals visible and actionable in desktop.

**Files:**
- Modify: `ui/index.html` (SIGNAL or modal)
- Modify: `ipc.rs` if `approvals_list` / `approvals_grant` naming differs — align with CLI

**Steps:**
1. PW/IPC test: empty list idle; after injecting pending (or scripted), Grant clears.
2. UI: list + Grant / Deny; on grant call existing resume path.
3. Badge on status bar `approvals_pending`.

---

### Task 7: Campaigns / Agents drawer

**Objective:** Hermes-like multi-agent visibility using campaign store.

**Files:**
- Modify: `ui/index.html`
- Reuse: campaign IPC from phase 16

**Steps:**
1. List campaigns; Create (name + steps JSON minimal); Run; show status.
2. Composer sub-strip: `N steps · status` when active.
3. PW: campaign create/run still green + UI list non-empty.

---

### Task 8: Cron operator panel (replace spam buttons)

**Objective:** Proper cron management; keep hard caps (display 6, create max 12).

**Files:**
- Modify: `ui/index.html` SIGNAL/Cron section
- Optional: `cron_delete` / `cron_set_enabled` IPC if missing

**Steps:**
1. If no delete IPC, add `cron_remove` + `cron_set_enabled` in kernel/cron + ipc.
2. UI rows: name, every, enabled toggle, last status, Run now.
3. PW: add → list → tick → enabled toggle; layout still capped.

---

### Task 9: Chat presentation — Thinking + tool cards v2

**Objective:** Match Hermes readability: Thinking label rows, timed tool cards, no JSON.

**Files:**
- Modify: `ui/index.html` stream handler + CSS
- Modify: stream event mapping if kernel emits reasoning deltas

**Steps:**
1. On stream: if event type reasoning/thinking → append `.think-row` (collapsed by default).
2. Tool cards: show duration ms when `tool` end event has timing; gold rail while run.
3. PW: formatRich + coalesce still pass; add think-row unit test via evaluate.

---

### Task 10: Composer + titlebar actions parity

**Objective:** One header remains; add Logs toggle + Right/Term toggles; composer model UX.

**Files:**
- Modify: `ui/index.html` titlebar chips
- Keep send absolute pin; controls wrap

**Steps:**
1. Titlebar: `Tasks` `Logs` `Files` `Term` icons (no logo).
2. Composer: ensure Sol/Terra/Luna + thinking ladder + fast; remove redundant ACCESS if unused by kernel or wire it.
3. PW geometry tests updated for new chips.

---

### Task 11: Files pane (read-only tree)

**Objective:** Browse workspace roots safely.

**Files:**
- Create IPC: `fs_list`, `fs_read` in `ipc.rs` with path allowlist (`OPTIMUS_HOME`, optional project roots from config)
- UI: `#filesTree` render

**Security:**
- Reject `..`, absolute paths outside roots, symlink escape; max read 256 KiB text.

**Steps:**
1. Unit tests on allowlist.
2. PW: list home → see `sessions` or known file; read small file.
3. CUA: open Files pane on native.

---

### Task 12: Terminal pane (phase A — command job stream)

**Objective:** Operator terminal without boiling the ocean.

**Phase A (this plan):** Run allowlisted commands via existing CommandCapture / job API; stream stdout to `#termOut`.  
**Phase B (later):** Interactive PTY (`portable-pty`) — separate plan.

**Files:**
- IPC: `term_run` { cmd, cwd } → job id; `term_poll`
- UI: input box + output pre

**Steps:**
1. Deny network exfil commands by SmartDeny.
2. PW offline: `echo hi` appears in termOut (HTTP mode may stub).
3. Native CUA: open Term, run `echo optimus-term-ok`.

---

### Task 13: Capabilities page

**Objective:** Show packs, schema token budget, activate_pack.

**Files:**
- IPC: `packs_list`, `pack_activate` (wire to optimus-packs)
- UI page

**Steps:**
1. Doctor already has schema tokens — display progress bar (rome gold fill).
2. List packs enabled/disabled.

---

### Task 14: Messaging / Gateway page

**Objective:** Surface durable gateway + webhook.

**Files:**
- IPC: `gateway_status`, `gateway_inbox_list` (from phase 14/16)
- UI: webhook URL `http://127.0.0.1:PORT/...`, recent inbox rows

**Steps:**
1. If server not running, show Start instructions / `optimus gateway serve` copy button.
2. No fake “connected to Telegram” without adapter.

---

### Task 15: Artifacts + Logs

**Objective:** Session artifacts list + log tail drawer.

**Files:**
- IPC: `artifacts_list(session_id)`, `logs_tail(bytes)`
- Ensure kernel writes artifacts to `{home}/artifacts/{session}/`

**Steps:**
1. PW with offline chat that writes a file artifact (or seed file).
2. Logs drawer shows last N lines.

---

### Task 16: Rome pass on all components

**Objective:** Consistent inscribed glass, hairlines, section labels, status plinth.

**Files:**
- `ui/index.html` CSS only (+ empty state copy)

**Steps:**
1. Visual checklist PW: no `border-radius` ≠ 0; tool card max-width; gold active nav.
2. Native CUA screenshot compare (manual).

---

### Task 17: Scorecard + docs

**Objective:** Update living scorecard desktop axis toward WIN.

**Files:**
- `docs/architecture/sota-scorecard.md`
- `docs/architecture/phase-18-hermes-parity-shell.md` (new)

**Steps:**
1. Record PW count, CUA evidence path, remaining P2 gaps.

---

### Task 18: Full verification gate

**Objective:** Ship criterion for “reskin milestone”.

**Commands:**
```bash
export TEMP='C:/Users/mustb/AppData/Local/Temp'
export TMP='C:/Users/mustb/AppData/Local/Temp'
export CARGO_TARGET_DIR='E:/Projects/Optimus Agent/local/tmp/cargo-target'
cd "E:/Projects/Optimus Agent"
cargo test -p optimus-kernel -p optimus-desktop -- --test-threads=1
cd apps/optimus-desktop && npx playwright test
cd "E:/Projects/Optimus Agent" && bash scripts/rebuild-install-relaunch.sh --dev
# CUA: capture OptimusAgent window — single header, status bar, Files toggle, cron capped
```

**Acceptance (milestone “Hermes-parity shell v1”):**
- [ ] One header only (no logo strip, no second topbar)
- [ ] Left: New / Capabilities / Messaging / Artifacts + PINNED + SESSIONS + SIGNAL
- [ ] Status bar live fields non-placeholder when doctor healthy
- [ ] Approvals + Campaigns + Cron operable from UI
- [ ] Files + Term panes open (Term phase A OK)
- [ ] Rome tokens applied; 0 radius; tool cards compact
- [ ] PW all green; native install relaunch; CUA confirms chrome
- [ ] Scorecard Desktop product: **TIE or WIN path** with evidence

---

## 5. File map (expected touch set)

| Path | Role |
|---|---|
| `apps/optimus-desktop/ui/index.html` | Shell, Rome CSS, router, panes |
| `apps/optimus-desktop/src/main.rs` | Window chrome (done); maybe denser min size |
| `apps/optimus-desktop/src/ipc.rs` | doctor fields, fs_*, term_*, packs_*, gateway_*, logs_*, cron_remove |
| `apps/optimus-desktop/src/bridge.rs` | expose new optimus.* methods |
| `apps/optimus-desktop/e2e/desktop.spec.js` | shell / status / panes / rome tests |
| `crates/optimus-kernel/src/*` | fs allowlist, artifacts, log helper, cron enable/remove |
| `docs/architecture/phase-18-hermes-parity-shell.md` | milestone doc |
| `docs/architecture/sota-scorecard.md` | living scorecard |
| `scripts/rebuild-install-relaunch.sh` | unchanged entry |

Optional split (if `index.html` > ~2500 LOC mid-flight):
- `ui/css/rome.css`, `ui/js/shell.js`, `ui/js/chat.js` loaded as custom protocol assets — **only if needed**.

---

## 6. Risks & tradeoffs

| Risk | Mitigation |
|---|---|
| Scope explosion to “full IDE” | Files read-only; Term phase A non-interactive; no editor Monaco until P2 |
| UI polish before function | P0 tasks wire **existing** backends first; aesthetic tokens are cheap and parallel |
| PTY complexity on Windows | Defer interactive PTY; job stream first |
| Path traversal in fs_* | Allowlist + canonicalize + tests |
| Performance of single HTML | rAF chat already; virtualize session list if >200 |
| User wanted function>polish | Treat Hermes IA as **function** for daily use; Rome is thin CSS layer |
| Electron parity expectation | Document WebView2 choice; beat Hermes on native weight + kernel |

---

## 7. Open questions (defaults if “go ahead”)

1. **Right pane default:** hidden (default) vs open files — **default hidden**.
2. **Terminal:** phase A job stream vs block on PTY — **phase A**.
3. **Pins:** localStorage vs DB — **localStorage** until sync needed.
4. **Messaging:** UI shell now + Telegram later — **shell now**.
5. **Voice controls:** omit until STT — **omit**.
6. **Brand mark:** never in titlebar; optional empty-state only — **yes**.

---

## 8. Execution order (recommended sprints)

| Sprint | Tasks | Outcome |
|---|---|---|
| **S1** | 1–5, 10, 16 (partial), 18 smoke | Rome shell IA + status bar + sessions |
| **S2** | 6–9, 13 | Approvals, campaigns, cron, chat polish, capabilities |
| **S3** | 11–12, 14–15, 17–18 | Files, term A, messaging, artifacts/logs, scorecard |

Each sprint ends with: PW + rebuild-install-relaunch + CUA checklist.

---

## 9. ULTRATHINK — why this beats a pure reskin

Hermes Desktop wins today on **breadth of panes and operator chrome**. Optimus already wins on **kernel truth** (Work Graph, MetaMemory fence, SmartDeny, packs, campaigns, Codex OAuth correctness).  

The winning move is **not** cloning Electron. It is:

1. **Steal Hermes IA** (where the eye/hand go every minute).  
2. **Bind every pane to Optimus-native durable backends** (so the shell is honest).  
3. **Skin with Rome** so the product is unmistakably Optimus — austere, inscribed, gold-on-obsidian — not a Hermes theme pack.  
4. **Keep proof discipline** (Playwright + native CUA) so “looks like Hermes” never regresses into ADR-0014-class lies.

That is how Optimus becomes “Hermes desktop, better in every way” rather than “prettier chat box.”

---

## 10. Handoff

Plan complete and saved.

**Ready to execute using subagent-driven-development** — fresh subagent per task with spec compliance then code quality review — **or** continuous single-agent S1 execution under YOLO.

**Default if user says go ahead:** start **Sprint S1 (Tasks 1→5, 10, partial 16)** immediately on `E:/Projects/Optimus Agent`.
