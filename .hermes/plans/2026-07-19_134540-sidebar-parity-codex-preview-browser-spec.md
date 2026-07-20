# Optimus Desktop — Full Sidebar Parity + Codex-Class Preview Browser

> **For Hermes:** Use subagent-driven-development after this plan is accepted.  
> **Mode this turn:** ULTRAPLAN / ULTRATHINK / **spec only** — no product code execution.  
> **User directive:** Left sidebar = full Hermes Desktop left-rail functionality. Right sidebar = full Hermes right-rail **plus** ChatGPT/Codex app preview browser with all advanced capabilities. Leave no stone unturned. If Optimus lacks a backend, **specify and build it**.

**Goal:** Specify and sequence a complete Optimus Desktop dual-rail control surface that (1) matches or exceeds Hermes Desktop left-nav + session/operator chrome, (2) matches Hermes right-rail files/terminal/artifacts, and (3) ships a first-class **in-app Preview Browser** with Codex-class annotation, local-dev, multi-tab, and agent-loop integration — all on Optimus’s WebView2 + Rust kernel stack, Rome aesthetic, ADR-0014 native IPC, CUA-proven installs.

**Architecture:** Keep **tao + wry WebView2** shell. Expand `ui/index.html` (or split modules) into a multi-pane shell with durable state in `OPTIMUS_HOME`. Add **kernel/desktop IPC** for FS, PTY, browser effector, artifacts, gateway, memory, packs. Preview Browser is a **right-rail primary tab** backed by a **real browser engine surface** (prefer WebView2 child / CDP-controlled Chromium; not fake iframe-only). Agent tools (`browser_*`, annotate, screenshot) share the same browser session as the UI preview.

**Tech Stack:** Rust (`optimus-desktop`, `optimus-kernel`, `optimus-runtime`, browser effector), WebView2 host, Playwright HTTP tests, native install + CUA, optional `portable-pty`, CDP (`chromiumoxide` / Playwright protocol / WebView2 DevTools), SQLite for session/artifact/browser state.

---

## 0. ULTRATHINK — product truth

### 0.1 Why sidebars decide daily use
Hermes Desktop wins on **operator chrome density**: left rail is a control room (sessions, capabilities, messaging, artifacts, search, pins, signal); right rail is a **workspace companion** (files, terminal, previews). ChatGPT Codex desktop wins on **agent×code×browser loop**: multi-agent, file tree, terminal, **in-app browser with comment annotations on rendered pages**, PR review, computer use. Optimus already wins kernel axes (Work Graph, MetaMemory fence, SmartDeny, packs, campaigns, Codex OAuth correctness). The gap is **surfaces that bind hands to that kernel every minute**.

### 0.2 Non-negotiables (Optimus memory + ADRs)
1. Function > cosmetic polish for new backends; **sidebar IA that surfaces real backends is function**.
2. After every desktop rebuild: `bash scripts/rebuild-install-relaunch.sh` → **native CUA proof** (not HTTP-only).
3. **ADR-0014:** native `http://optimus.localhost/` uses `window.ipc`, never `fetch` for IPC.
4. Rome aesthetic: zero radius, gold-on-obsidian, no logo clutter in titlebar.
5. Codex OAuth: nested `reasoning.effort` only; Sol/Terra/Luna.
6. Secrets never leave redaction; FS/browser SSRF and path allowlists mandatory.
7. SmartDeny for terminal, FS write, browser navigate outside allowlist, computer-use.
8. MetaMemory is **data not authority** — browser/memory UIs must not execute recalled content as instructions.

### 0.3 Honest Optimus baseline (today)
| Surface | State |
|---|---|
| Left: New / Capabilities / Messaging / Artifacts routes | Scaffold; Capabilities partially real (approvals/campaigns/doctor); Messaging/Artifacts stubs |
| Left: search, PINNED, SESSIONS, SIGNAL/CRON | Partial (pins localStorage; cron operator basic) |
| Right: Files / Artifacts | **Stub text only** |
| Term pane | **Stub** |
| Preview browser | **Missing** |
| FS / PTY / CDP IPC | **Missing or HTTP-browser only** |
| Gateway messaging UI | Stub |
| Status bar live doctor counts | Done (S1) |

### 0.4 External references (capabilities to match)
**Hermes Desktop** (docs + product image + community): chat-first window; left sidebar navigation; multiple simultaneous agent conversations; messaging providers; artifacts; project folder structures; multi-project/workspaces; session list hygiene (archive, search by id, pin, concurrent multi-profile sessions, cross-profile `@session` links); control-room density (setup, chat, memory, tools, scheduling, logs, backups); active sessions prioritization; deliverable/artifact attachments.

**ChatGPT / Codex app (2025–2026):** multi-agent parallel work; worktrees; file tree; multi-terminal; SSH alpha; GitHub PR review side panel; inline diff edit/accept/reject; **in-app browser** for localhost and public pages; **comment annotations on rendered pages as agent instructions**; computer use (screen/cursor); plugins; memory; file preview for PDFs/sheets/docs; project/workspace switcher.

Optimus must not clone Electron/Codex proprietary UI pixel-for-pixel; it must achieve **feature completeness** on its own stack.

---

## 1. Information architecture (target shell)

```
┌─ SINGLE TITLEBAR ──────────────────────────────────────────────────────────┐
│ [drag | session title ……] [Tasks][Logs][Files][Term][Browser][theme][─□×] │
├─ LEFT RAIL (288–320px) ─┬─ MAIN STACK ──────────────┬─ RIGHT RAIL (320–420) ┤
│ PRIMARY NAV              │ route pages OR chat       │ TAB STRIP:            │
│  + New session           │                           │  Files | Browser |    │
│  ○ Chat (implicit)       │ chat: messages + tools    │  Artifacts | Git/PR   │
│  ○ Capabilities          │   + subagent strip        │  Preview meta         │
│  ○ Messaging             │                           ├───────────────────────┤
│  ○ Artifacts             │ capabilities/messaging/…  │ TAB BODY (stacked)    │
│  ○ Memory (NEW)          │                           │                       │
│  ○ Projects (NEW)        │                           │ Browser: toolbar +    │
│  ○ Settings/Doctor       │                           │  WebView + annotate   │
│ ─────────────────        │                           │  overlay + device     │
│ Search sessions          │                           │ Files: tree + preview │
│ PINNED                   │                           │ Term: (below or tab)  │
│ SESSIONS n/m  [active↑]  │                           │                       │
│ PROFILES (optional)      │                           │                       │
│ SIGNAL / Gateway / Cron  │                           │                       │
│ Approvals badge          │                           │                       │
├──────────────────────────┴───────────────────────────┴───────────────────────┤
│ OPTIONAL SPLIT: terminal drawer full-width OR right-bottom                   │
├─ STATUS PLINTH ─────────────────────────────────────────────────────────────┤
│ gw · agents · cron · approvals · tokens · model · home · version · browser  │
└─ COMPOSER (chat route) ─────────────────────────────────────────────────────┘
```

**Layout persistence:** `localStorage optimus.ui.layout` + optional `{home}/ui/layout.json` for cross-device later.  
**Widths:** left 288 default (240–360); right 360 default (280–520); browser min 360.  
**Collapse rules:** right hidden by default on first run OR last-user; keyboard shortcuts below.  
**Nothing may push composer/status out of viewport** (hard flex + overflow locks from S1).

---

## 2. LEFT SIDEBAR — exhaustive Hermes parity spec

### 2.1 Primary navigation (top block)
| Item | Behavior | Backend required |
|---|---|---|
| **+ New session** | Creates kernel session; route→chat; focus composer; optional template picker | `new_session` ✅ |
| **Chat** | Focus last/active session; route chat | sessions ✅ |
| **Capabilities** | Packs, schema budget, approvals, campaigns, CUA/doctor tools, skills | packs list/activate ⚠️ build; approvals/campaigns ✅ |
| **Messaging** | Channels, inbox/outbox, webhook, connect Telegram/etc. | gateway ✅ partial; adapters ⚠️ build |
| **Artifacts** | Global + per-session deliverables browser | artifacts store ⚠️ build |
| **Memory** | MetaMemory explorer (evidence cards, search, tombstone) | metamemory IPC ⚠️ build |
| **Projects** | Workspace roots, AGENTS.md context, multi-repo | project registry ⚠️ build |
| **Settings / Doctor** | Home path, auth import, theme, shortcuts, about, diagnostics | doctor/auth ✅; settings file ⚠️ |

**UX details**
- Active nav: left gold inlay bar (Rome).
- Badges: Messaging unread count; Approvals pending; Cron failures; Memory conflicts.
- Keyboard: `Ctrl+1…8` jump nav; `Ctrl+N` new session; `Ctrl+/` command palette (see 2.8).
- Collapsed left icon-rail mode (56px) optional later; v1 full labels.

### 2.2 Session search
| Requirement | Detail |
|---|---|
| Filter | Title, id substring, packs, preview snippet |
| Search by exact id | Hermes parity: paste full uuid → jump |
| Scope toggle | All / Pinned / Archived / Active-only |
| Debounce | 120ms |
| Empty | “No sessions match” |
| Backend | Client filter OK to 500; server FTS if >500 ⚠️ `sessions_search` |

### 2.3 PINNED section
| Requirement | Detail |
|---|---|
| Storage v1 | `localStorage optimus.ui.pins` ✅ |
| Storage v2 | DB column `sessions.pinned` + sync ⚠️ |
| Order | User drag-reorder (v1: pin order array) |
| Pin control | ★ on row; click must not open session |
| Visibility | Hidden section when empty OR show “Pin sessions for quick access” |
| Bug parity | Avoid Hermes bug: pinned must appear even when filtered out of main list unless search excludes it intentionally — **pins always visible when query empty; when query non-empty, pin subset that matches** |

### 2.4 SESSIONS list
| Requirement | Detail |
|---|---|
| Sort default | `updated_at` desc |
| Active first | Running/streaming sessions pin to top (Hermes issue #46560 parity) ⚠️ needs `session.runtime_state` |
| Row fields | Title, msg count, relative time, packs chips, status dot (idle/run/err) |
| Actions (context menu / `⋯`) | Open, Pin/Unpin, Rename, Archive, Delete, Copy id, Export md/html, Open in new tab (multi-tab later) |
| Multi-select | Shift/Ctrl for bulk archive/delete (v1.1) |
| Archive | Soft-hide; “Show archived” toggle ⚠️ `archived_at` |
| Rename | Inline edit → `session_rename` ⚠️ |
| Delete | Confirm modal → `session_delete` ⚠️ |
| Streaming indicator | Pulse gold dot while chat_stream active for that id |
| Counts | Label `SESSIONS shown/total` ✅ pattern |

### 2.5 Profiles / multi-identity (Hermes multi-profile)
| Requirement | Detail |
|---|---|
| v1 | Single `OPTIMUS_HOME` only — show profile name “default” |
| v2 | Profile switcher: list `%LOCALAPPDATA%/optimus/profiles/*` or config profiles ⚠️ |
| Cross-session links | `@session:<profile>/<id>` resolve in chat ⚠️ parser |
| Concurrent | One desktop process per profile OR tabs — document choice: **v1 one process; v2 profile tabs** |

### 2.6 SIGNAL block (operator)
| Subsection | Contents | Backend |
|---|---|---|
| **Gateway** | State, last event, webhook URL copy, start/stop local serve | gateway_http ⚠️ UI bind; process control ⚠️ |
| **Cron** | List (cap 6+more), add, tick, enable/disable, delete, last status, next run | list/add/tick ✅; enable/delete ⚠️ if missing |
| **Approvals** | Pending count + jump to Capabilities approvals | ✅ |
| **Campaigns** | Active count + jump | ✅ |
| **Browser** | Preview session status (url, tabs, annotated) | ⚠️ new |
| **CUA** | cua-driver detected? last smoke | doctor field ⚠️ |

Hard caps: cron display ≤6, create ≤12 (or settings). Never overflow left rail.

### 2.7 Workspaces / Projects (Hermes multi-project)
| Requirement | Detail |
|---|---|
| Project = named root path(s) + optional git remote | |
| Switch project | Updates FS allowlist roots, terminal cwd default, browser bookmarks for localhost ports, AGENTS.md injection | ⚠️ `projects.json` |
| Sidebar section | Under search or above sessions: current project chip |
| Open folder | Native folder picker via tauri/tao dialog ⚠️ `dialog_open_folder` IPC |
| Recent projects | MRU 10 |

### 2.8 Command palette (left-adjacent, global)
| Requirement | Detail |
|---|---|
| Trigger | `Ctrl+K` / `Ctrl+Shift+P` |
| Commands | New session, routes, toggle panes, import codex, cron tick, run campaign, open url in preview, grant all approvals (dangerous confirm), doctor, theme |
| Fuzzy | client-side |
| Backend | pure UI + existing IPC |

### 2.9 Memory nav page (Hermes memory control-room parity)
| Requirement | Detail |
|---|---|
| List | Recent MetaMemory entries with type, trust, validity window |
| Search | Query API |
| Actions | Open detail, feedback helpful/harmful, tombstone, privacy erase |
| Safety | Render as evidence; never auto-execute |
| Backend | `memory_list`, `memory_get`, `memory_feedback`, `memory_forget` ⚠️ wrap MetaMemory crate |

### 2.10 Messaging nav page (Hermes messaging providers)
| Requirement | Detail |
|---|---|
| Channels list | configured adapters + status |
| Inbox | durable gateway messages |
| Outbox | pending/sent |
| Compose | optional send via adapter |
| Webhook | URL, secret redacted, copy, rotate |
| Connect flows | Telegram bot token import (secure), etc. phased |
| Backend | gateway inbox/outbox ✅; adapters ⚠️ phased Telegram first |

### 2.11 Artifacts nav page
| Requirement | Detail |
|---|---|
| Filters | session, type (pdf/png/csv/md/code), date |
| Preview | click → right rail Artifacts/Browser/Files preview |
| Open external | shell open |
| Reveal in tree | files pane |
| Backend | `{home}/artifacts/{session_id}/…` index SQLite ⚠️ |

### 2.12 Capabilities nav page (expand existing)
| Section | Detail |
|---|---|
| Schema budget | gold progress ✅ |
| Packs | list, enable/disable, token cost, activate_pack ⚠️ |
| Skills | list promoted/deprecated rates if available ⚠️ |
| Approvals | full panel ✅ |
| Campaigns | full panel ✅ |
| Tools catalog | kernel tool names + deny policy summary ⚠️ |
| Browser / CUA | effector health, allowlist domains, last run ⚠️ |
| Eval | run trajectory suite button ⚠️ |

### 2.13 Left footer
| Item | Detail |
|---|---|
| Status/Doctor button | modal or page with full doctor JSON pretty + copy |
| Auth banner | keep compact above SIGNAL or under nav |
| Disk/home path | truncated with tooltip |

### 2.14 Left-rail accessibility & a11y
- `aria-current` on active nav
- Listbox semantics for sessions
- Focus trap in modals
- Screen reader labels on pin/archive
- High contrast with Rome tokens

### 2.15 Left-rail backend IPC matrix (build if missing)

| IPC method | Purpose | Status |
|---|---|---|
| `sessions` / `new_session` / `get_session` | core | ✅ |
| `session_rename` | rename | ⚠️ |
| `session_archive` / `session_unarchive` | hygiene | ⚠️ |
| `session_delete` | delete | ⚠️ |
| `session_set_pinned` | durable pins | ⚠️ optional after localStorage |
| `sessions_search` | FTS | ⚠️ when scale |
| `session_runtime_list` | active/streaming flags | ⚠️ |
| `projects_list/create/switch` | workspaces | ⚠️ |
| `dialog_open_folder` | native picker | ⚠️ |
| `packs_list` / `pack_activate` | capabilities | ⚠️ |
| `skills_list` | capabilities | ⚠️ |
| `memory_*` | memory page | ⚠️ |
| `gateway_status` / `gateway_inbox` / `gateway_outbox` | messaging | ⚠️ bind |
| `gateway_serve_start/stop` | local webhook process | ⚠️ |
| `cron_list/add/tick` | signal | ✅ |
| `cron_set_enabled` / `cron_remove` | operator | ⚠️ if absent |
| `approvals_*` / `campaign_*` | ✅ |
| `artifacts_list` / `artifacts_get` | artifacts | ⚠️ |
| `doctor` | extended fields | partial ✅ |

---

## 3. RIGHT SIDEBAR — Hermes + Codex companion rail

### 3.1 Right-rail chrome
| Element | Spec |
|---|---|
| Tab strip | `Files` · `Browser` · `Artifacts` · `Git` (optional phase) · `Outline` |
| Tab state | Remember last tab per session + global default |
| Close/collapse | `]` or titlebar Files/Browser toggles; `Esc` focuses chat |
| Resize | Drag handle left edge; double-click reset |
| Split | Browser top / Files bottom optional user toggle |
| Empty collapsed | zero width, no residual gap |

### 3.2 FILES tab (Hermes files + Codex file tree)

#### 3.2.1 Tree
| Requirement | Detail |
|---|---|
| Roots | Project roots + `OPTIMUS_HOME` (tagged) |
| Expand/collapse | Lazy load children via `fs_list` |
| Icons | folder/file by extension (simple map) |
| Git status | M/A/D badges if git available ⚠️ `git_status_porcelain` |
| Multi-select | for batch open/copy path |
| Context menu | Open preview, Open in Browser (if html), Copy path, Copy relative, Reveal in OS explorer, New file/folder, Rename, Delete (SmartDeny), Refresh |
| Drag-drop | Drop files into chat as attachments ⚠️ |
| Watch | optional fs watch debounce refresh ⚠️ |
| Filter | fuzzy filename filter box |
| Keyboard | arrows, enter open, alt+up parent |

#### 3.2.2 File preview subpane
| Type | Renderer |
|---|---|
| text/code | Monaco or lightweight highlight.js / Prism in pre; line numbers |
| markdown | rendered md |
| image | img fit |
| pdf | pdf.js canvas |
| csv/xlsx | simple table first 200 rows |
| binary | hex/summary “cannot preview” |

#### 3.2.3 Edit (Codex-like, phased)
| Phase | Capability |
|---|---|
| A | Read-only preview |
| B | Edit text + Save via `fs_write` (SmartDeny + backup) |
| C | Inline annotations “ask Optimus about selection” → inject composer quote |
| D | Diff view vs HEAD |

#### 3.2.4 FS backend (mandatory build)
```
fs_list { root_id, path } -> { entries:[{name,path,kind,size,mtime,git?}] }
fs_read { path, max_bytes } -> { content, truncated, mime }
fs_write { path, content } -> { ok }  // SmartDeny
fs_mkdir / fs_rename / fs_delete
fs_reveal { path } // explorer
fs_roots -> configured roots
```
**Security:** canonicalize; must stay under roots; reject symlink escape; max read 1–2 MiB default; secret file name denylist (`.env`, `auth.json`) read requires explicit grant.

### 3.3 TERMINAL (Hermes/Codex multi-terminal)

#### 3.3.1 Placement
- Default: bottom drawer under main+right (`termPane`) full width when open  
- Alt: right-rail sub-tab “Term” for narrow focus  
- Multi-tabs: `Term 1`, `Term 2`, `+` (Codex multi-terminal parity)

#### 3.3.2 Phase A — Job stream (ship first)
| Feature | Spec |
|---|---|
| Run | `term_run { cmd, cwd, env? }` → job_id |
| Stream | stdout/stderr chunks via stream events or poll |
| Stop | `term_kill` |
| History | per-tab scrollback 5000 lines |
| Cwd | show + change |
| Allowlist/SmartDeny | block dangerous cmds; network policy |

#### 3.3.3 Phase B — Interactive PTY (required for “full” parity)
| Feature | Spec |
|---|---|
| PTY | ConPTY on Windows via `portable-pty` or similar |
| xterm.js | fit addon, webgl optional |
| Input | keystrokes, paste, resize SIGWINCH |
| Multiple sessions | map tab → pty id |
| SSH | later alpha (Codex parity later) |

#### 3.3.4 Terminal IPC
```
term_list / term_create / term_close
term_write { id, data }
term_resize { id, cols, rows }
term_run_job // phase A
stream events: term_out, term_exit
```

### 3.4 ARTIFACTS tab
| Feature | Spec |
|---|---|
| List | session-scoped + global recent |
| Types | charts, pdf, csv, images, code bundles, html reports |
| Actions | Preview, Open external, Insert to chat, Delete, Pin |
| Deliverable mode | agent can `artifact_publish` → appears here + optional messaging attach |
| Backend | index + filesystem layout under home |

### 3.5 GIT / PR tab (Codex PR Chat lite — phase)
| Feature | Spec |
|---|---|
| Status | branch, dirty files |
| Diff | file list + patch view |
| Actions | stage/commit (confirm), create PR via `gh` if available |
| PR Chat | side thread about PR diff (session pack) |
| Inline review | accept/reject agent patch hunks (Work Graph job) |

v1 can be read-only `git status` + `git diff`; full PR Chat is phase 2.

### 3.6 OUTLINE / SESSION TOOLS tab (optional)
- Jump to user/assistant/tool messages  
- Tool call index  
- Subagent/campaign steps  

---

## 4. PREVIEW BROWSER — Codex-class advanced specification

This is the **centerpiece** of the right rail beyond Hermes.

### 4.1 Product definition
An **in-app browser workspace** where:
1. User and agent share a real browsing context.
2. Local dev servers (`http://127.0.0.1:*`, `http://localhost:*`) and allowlisted http(s) URLs load.
3. User can **annotate** the rendered page; annotations become structured agent instructions.
4. Agent browser tools drive the **same** session (navigate, click, type, screenshot, snapshot).
5. Multi-tab, device chrome, network/console drawers, and screenshot/video capture exist.

### 4.2 Engine choice (decision — locked for plan)

| Option | Pros | Cons |
|---|---|---|
| A. Second WebView2 pane in desktop | Native Windows, fast | Annotation overlay harder; two WV2 |
| B. CDP-controlled Chromium (bundled or system Edge/Chrome) | Full DevTools protocol, screenshots, realistic | Bundle size / external dep |
| C. iframe only | Easy | **Insufficient** (CORS, no real localhost tooling, weak CDP) — **reject as sole engine** |

**Decision:** **Hybrid B primary** — desktop spawns/manages a Chromium/Edge instance via CDP for Preview Browser + agent tools; UI chrome in Optimus WebView2. Fallback A for simple doc preview if CDP unavailable (degraded mode banner).

Implementation sketch:
- `optimus-browser` crate: lifecycle, tabs, CDP client, screenshot, a11y snapshot, annotation hit-testing bridge.
- Desktop IPC proxies to crate.
- Agent `browser_*` tools call same crate (single source of truth).

### 4.3 Browser toolbar (every control)
| Control | Behavior |
|---|---|
| Back / Forward / Reload / Stop | per-tab history |
| Home | project default URL or `about:blank` |
| Omnibox | URL or search; security highlight https/http/localhost |
| Bookmark star | project-scoped bookmarks |
| Tab list | multi-tab strip under toolbar |
| New tab / Close tab | |
| Dual focus lock | “Follow agent” toggle — viewport tracks agent navigations |
| Device | Desktop / Tablet / Phone presets + custom size |
| Zoom | 50–200% |
| Annotate mode | toggle crosshair; click creates pin+comment |
| Select mode | normal interaction |
| Screenshot | viewport / full page → artifact |
| Copy URL | |
| Open external | system browser |
| Share to chat | send URL + optional screenshot to composer |
| Security badge | allowlisted / granted / blocked |
| Incognito tab | isolated partition (no shared cookies) optional |

### 4.4 Tabs model
```json
{
  "browser_session_id": "uuid",
  "tabs": [
    {
      "id": "tab_uuid",
      "title": "…",
      "url": "http://127.0.0.1:5173/",
      "favicon": "…",
      "status": "loading|complete|error",
      "can_go_back": true,
      "can_go_forward": false,
      "device": "desktop",
      "annotations": [],
      "console_error_count": 0,
      "is_agent_focus": true
    }
  ],
  "active_tab_id": "…"
}
```
Persist last session optionally in `{home}/browser/sessions/`.

### 4.5 Annotation system (Codex “comment on page” parity)

#### 4.5.1 Create
- Enter Annotate mode → click element or drag region.
- Resolve target via CDP `DOM.getNodeForLocation` + a11y role/name + CSS selector candidates + bounding box.
- Popover: comment text, severity (nit/issue/blocker), “Send to agent” / “Save only”.

#### 4.5.2 Annotation object
```json
{
  "id": "ann_uuid",
  "tab_id": "…",
  "created_at": "ISO-8601",
  "author": "user|agent",
  "body": "make this button 20px taller",
  "severity": "nit|issue|blocker",
  "target": {
    "backend_node_id": 123,
    "selector_candidates": ["button.primary", "text=Save"],
    "role": "button",
    "name": "Save",
    "bbox": {"x":0,"y":0,"w":0,"h":0},
    "xpath": "…",
    "outer_html_snip": "…"
  },
  "screenshot_artifact_id": null,
  "status": "open|sent|resolved|wontfix",
  "linked_job_id": null
}
```

#### 4.5.3 Send to agent
- Inserts a structured composer/system message:
  - Page URL + title
  - Annotation body
  - Target descriptors
  - Optional screenshot
- Optionally auto-starts a turn with tools enabled.
- Agent may call `browser_highlight` / `browser_click` using annotation id.

#### 4.5.4 Overlay UX
- Numbered pins `[1]…` on page (screen-space, reflow on resize/scroll via CDP box model refresh).
- Sidebar list of annotations for active tab; click focuses pin.
- Resolve/reopen; filter open-only.
- Export annotations JSON/markdown artifact.

### 4.6 Agent tool surface (must share session)

| Tool | Behavior |
|---|---|
| `browser_tabs` | list/create/close/select |
| `browser_navigate` | url, wait until | 
| `browser_back` / `forward` / `reload` |
| `browser_snapshot` | a11y tree + refs `@eN` compact |
| `browser_click` / `type` / `press` / `hover` / `scroll` | by ref or selector |
| `browser_fill` | clear+type |
| `browser_select` | option |
| `browser_screenshot` | viewport/full; store artifact |
| `browser_console` | messages + optional evaluate |
| `browser_wait` | selector/url/network idle |
| `browser_annotate_read` | list user annotations |
| `browser_highlight` | flash target |
| `browser_pdf` | print to PDF artifact |
| `browser_set_device` | viewport emulation |
| Network stubs later | route/abort (advanced) |

**Policy:** SmartDeny + domain allowlist; localhost free within user projects; public net needs grant or settings allowlist; block `file://` except under project roots with grant.

### 4.7 Devtools drawers (advanced Codex/dev parity)
| Drawer | Contents |
|---|---|
| Console | level filter, clear, click → reveal |
| Network | method, status, url, type, waterline simple; click headers |
| Issues | security mixed content, console errors summary |
| Accessibility | show a11y tree parallel to snapshot |

### 4.8 Local development workflow
| Feature | Spec |
|---|---|
| Port discovery | scan project common ports / user-registered dev URLs |
| “Open localhost” menu | 5173, 3000, 8080, custom |
| Auto-reload follow | optional when files change |
| Login walls | do not automate password managers; user interacts in Select mode |
| File-backed pages | `fs` html preview can “Open in Browser” via local static server ⚠️ `preview_serve` ephemeral 127.0.0.1 |

### 4.9 Computer use relationship
- Preview Browser ≠ full OS computer-use.
- Status bar shows both.
- Agent chooses browser tools for web; CUA tools for native apps.
- Spec must keep boundaries clear in Capabilities docs.

### 4.10 Browser backend IPC (mandatory build)

```
browser_session_start / browser_session_stop / browser_session_status
browser_tab_create / browser_tab_close / browser_tab_list / browser_tab_select
browser_navigate { tab_id, url }
browser_history { op: back|forward|reload|stop }
browser_snapshot { tab_id, mode: a11y|dom|compact }
browser_screenshot { tab_id, full_page? }
browser_act { tab_id, kind, ref|selector|coords, text? }
browser_console_list / browser_evaluate
browser_annotation_add / list / update / delete
browser_annotation_send_to_chat { id, auto_turn? }
browser_set_device { tab_id, preset|width,height,dpr }
browser_bookmarks_* 
browser_allowlist_get/set
```

Stream events to UI:
`browser_tab_updated`, `browser_console`, `browser_download`, `browser_annotation_changed`, `browser_agent_focus`.

### 4.11 Security & privacy (non-optional)
1. **SSRF:** block link-local metadata IPs except explicit localhost user intent; no cloud metadata.
2. **Allowlist modes:** `localhost_only` | `allowlist` | `open` (open requires settings + warning).
3. **Partitions:** agent profile vs user profile cookies optional isolation.
4. **Downloads:** save under `{home}/downloads/` with scan/extension policy; never auto-execute.
5. **Permissions:** geolocation/camera/mic denied by default.
6. **Redaction:** auth headers not shown in network drawer raw by default.
7. **Audit log:** navigations + grants in `{home}/logs/browser_audit.jsonl`.

### 4.12 Performance
- One Chromium shared; tabs as targets.
- Snapshot size caps (compact a11y default).
- Thumbnail idle tabs optional.
- Tear down browser process on app exit; crash recover.

### 4.13 Degraded modes
| Condition | UI |
|---|---|
| No Chromium/CDP | Banner + iframe preview limited + disable annotate |
| Navigate denied | Inline error + “Request approval” |
| Agent driving | “Agent controlling” badge; optional lock user input |

### 4.14 Acceptance tests — browser
1. Open Browser tab → blank.
2. Navigate `http://127.0.0.1:<static>` served by test fixture → title shows.
3. Annotate element → pin visible → Send to chat → message contains selector+body.
4. Agent tool `browser_snapshot` returns refs for same tab.
5. Screenshot artifact appears in Artifacts.
6. Multi-tab isolate histories.
7. Blocked external nav in localhost_only mode.
8. CUA still independent.
9. PW + native install proof.

---

## 5. Cross-cutting: chat integration with rails

| Interaction | Spec |
|---|---|
| Drop file from Files → composer | attach path reference |
| Browser “Share to chat” | url + screenshot |
| Annotation send | structured block |
| Tool cards | open target file in Files; open url in Browser |
| Code block “Preview HTML” | ephemeral preview_serve → Browser tab |
| Campaign outputs | appear in Artifacts + Files |
| Subagent strip | click → campaign detail in Capabilities |
| @file / @url references | chip autocomplete from rails |

---

## 6. Titlebar & status bar extensions

### Titlebar buttons (single header remains)
`Tasks` · `Logs` · `Files` · `Term` · `Browser` · theme · window controls  
No logo/name (user rule). Session title draggable region.

### Status plinth fields
`gw` · `agents` · `cron` · `appr` · `tokens` · `model` · `home` · `ver` · **`br`** (browser tabs count / url host) · **`pty`** (term sessions)

---

## 7. Rome UX rules for new chrome
- Zero border-radius everywhere.
- Gold hairlines for pane separators.
- Section labels uppercase Trajan tracking.
- Browser toolbar stone background; omnibox inscribed.
- Annotation pins gold; resolved mute.
- Active file tree row gold left bar.
- Terminal void background, mono ink.

---

## 8. Data layout under OPTIMUS_HOME
```
{home}/
  sessions.db
  cron.db
  campaigns.db
  artifacts/{session_id}/…
  downloads/
  browser/
    allowlist.json
    bookmarks.json
    sessions/
    audit.jsonl
  projects.json
  ui/layout.json
  logs/
  auth.json
```

---

## 9. Phased delivery (implementation plan)

> Each task = small, TDD, install+CUA at phase ends. Prefer subagent-driven-development.

### Phase P0 — Spec freeze & doctor flags
**Objective:** Document degraded capabilities in doctor.  
**Files:** `ipc.rs` doctor fields `files:false`, `pty:false`, `preview_browser:false` until true.  
**Test:** doctor JSON keys exist.

### Phase P1 — FS allowlist + Files tree read-only
**Tasks:**  
1. RED: kernel unit tests path escape rejected.  
2. Implement `fs_roots/list/read`.  
3. UI tree + text preview.  
4. PW + CUA open Files.  
**Files:** `crates/optimus-kernel/src/fs_sandbox.rs`, `ipc.rs`, `bridge.rs`, `ui/index.html`, e2e.

### Phase P2 — Files write/rename/delete + reveal
SmartDeny integration; OS reveal; context menu.

### Phase P3 — Terminal Phase A job stream
`term_run`/`kill`/stream; xterm-less pre first; then xterm.js read-only stream.

### Phase P4 — Terminal Phase B ConPTY
Interactive multi-tab; resize; paste.

### Phase P5 — Artifacts store + UI tabs
Publish API; list/preview; chat attachment.

### Phase P6 — Left session hygiene
rename/archive/delete/active-first; durable pins optional.

### Phase P7 — Projects switcher
`projects.json`, folder dialog, root injection to FS/term.

### Phase P8 — Messaging page bind gateway
inbox/outbox/webhook; honest adapter stubs.

### Phase P9 — Memory page
MetaMemory IPC + evidence UI.

### Phase P10 — Packs/skills full Capabilities
activate_pack; tool catalog.

### Phase P11 — Preview Browser engine bootstrap
Chromium/Edge CDP manager; one tab navigate localhost fixture.

### Phase P12 — Browser UI chrome + multi-tab
toolbar, device, screenshot artifacts.

### Phase P13 — Annotations v1
create pin, list, send to chat.

### Phase P14 — Agent browser tools unified
All tools hit same session; snapshot refs; PW agent smoke.

### Phase P15 — Devtools drawers + bookmarks + allowlist settings

### Phase P16 — Git status/diff lite

### Phase P17 — PR review lite (gh)

### Phase P18 — Command palette + keyboard map polish

### Phase P19 — Performance/virtualization (session list, tree)

### Phase P20 — Scorecard + phase doc + CUA full matrix

---

## 10. Detailed task bites (first buildable slice)

### Task A1: `fs_sandbox` module
**Files:** Create `crates/optimus-kernel/src/fs_sandbox.rs`; export from lib.  
**Tests:**  
- allow read under root  
- deny `../` escape  
- deny symlink out (if applicable)  
- deny `.env` without grant flag  

### Task A2: IPC fs_*  
**Files:** `apps/optimus-desktop/src/ipc.rs`, `bridge.rs`  
**Test:** HTTP mode Playwright list home.

### Task A3: Files tree UI  
**Files:** `ui/index.html`  
**Test:** toggleRight → tree non-stub; click file → preview text.

### Task B1: browser crate skeleton  
**Files:** `crates/optimus-browser/` (new) CDP connect to Edge channel.  
**Test:** unit mock; integration ignored without browser.

…(Implementers expand each phase into 2–5 min tasks at execution time using this spec as authority.)

---

## 11. Verification matrix (definition of done)

### Left rail DoD
- [ ] All primary nav destinations real or honest-but-complete operator pages (no “coming soon” without actions where backend exists)
- [ ] Session search by title and id
- [ ] Pin/archive/rename/delete
- [ ] Active sessions sort
- [ ] SIGNAL: gateway, cron CRUD-ish, approvals/campaigns badges
- [ ] Projects switch changes FS roots
- [ ] Memory/Messaging/Artifacts usable for daily paths
- [ ] Overflow impossible under spam

### Right rail DoD
- [ ] Files tree + preview + secure write path
- [ ] Terminal multi-tab Phase B or documented Phase A limit with working jobs
- [ ] Artifacts list/preview
- [ ] Git status at least

### Preview Browser DoD
- [ ] Multi-tab real engine
- [ ] Localhost + allowlist nav
- [ ] Annotate → agent instruction loop
- [ ] Shared agent tools
- [ ] Screenshot artifacts
- [ ] Device emulation
- [ ] Console drawer
- [ ] Security mode enforced
- [ ] Native CUA proof scripted checklist

### Global
- [ ] PW suite green  
- [ ] `rebuild-install-relaunch`  
- [ ] CUA screenshots evidence  
- [ ] Scorecard Desktop + Browser axes updated toward WIN  

---

## 12. Risks & mitigations
| Risk | Mitigation |
|---|---|
| Scope megaproject | Phases P1–P20; ship Files+TermA before Browser |
| CDP flaky on Windows | Edge channel stable CDP; health in doctor |
| Annotation drift on responsive layouts | refresh boxes on scroll/resize; store multiple selectors |
| Secret exfil via FS/browser | denylist + grants + audit |
| PTY complexity | Phase A first |
| index.html megabundle | split `ui/js/{shell,files,browser,term}.js` served by custom protocol |
| Electron envy | measure by workflow completion not pixel clone |

---

## 13. Open decisions (defaults if “go ahead”)
1. **Browser engine:** CDP Edge/Chromium hybrid (**default**).  
2. **Terminal:** Phase A then B (**default**).  
3. **Pins durable DB:** after localStorage (**default**).  
4. **Multi-profile:** v2 (**default v1 single home**).  
5. **PR Chat:** after Git lite (**default**).  
6. **Monaco vs textarea:** preview Monaco only if size OK; else highlight.js first.  
7. **Rome:** retained.  

---

## 14. Files likely to change (execution)
| Path | Role |
|---|---|
| `apps/optimus-desktop/ui/index.html` (+ future `ui/js/*`, `ui/css/rome.css`) | shell |
| `apps/optimus-desktop/src/{main,ipc,bridge,server}.rs` | host IPC |
| `crates/optimus-kernel/src/{fs_sandbox,lib,browser_tool,artifacts}.rs` | backends |
| `crates/optimus-browser/` | NEW CDP browser runtime |
| `crates/optimus-runtime/` | job hooks for term/browser grants |
| `apps/optimus-desktop/e2e/*.js` | PW |
| `docs/architecture/phase-19-sidebar-preview-browser.md` | milestone |
| `docs/architecture/sota-scorecard.md` | living score |
| `docs/decisions/0015-preview-browser-cdp.md` | ADR |
| `docs/decisions/0016-fs-sandbox-allowlist.md` | ADR |

---

## 15. Commands of record
```bash
export TEMP='C:/Users/mustb/AppData/Local/Temp'
export TMP='C:/Users/mustb/AppData/Local/Temp'
export CARGO_TARGET_DIR='E:/Projects/Optimus Agent/local/tmp/cargo-target'
cd "E:/Projects/Optimus Agent"
cargo test -p optimus-kernel -p optimus-browser -- --test-threads=1
cargo build -p optimus-desktop
cd apps/optimus-desktop && npx playwright test
cd "E:/Projects/Optimus Agent" && bash scripts/rebuild-install-relaunch.sh --dev
# then CUA checklist on installed OptimusAgent
```

---

## 16. Handoff

This document is the **authoritative product+technical spec** for:
- full **Hermes-class left sidebar**
- full **Hermes-class right sidebar**
- **Codex-class Preview Browser** (annotations, multi-tab, shared agent session, devtools, security)

**Plan complete and saved.**

Ready to execute with **subagent-driven-development** starting at **Phase P0→P1 (FS sandbox + Files tree)** unless you redirect to Browser-first (P11) — not recommended before Files.

**Default on “go ahead”:** P0 doctor flags → P1 FS → P2 writes → P3 term A → P11 browser engine spike in parallel once FS green.
