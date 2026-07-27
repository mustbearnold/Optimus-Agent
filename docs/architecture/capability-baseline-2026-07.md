# Capability baseline — July 2026

Evidence-backed answer to [#60](https://github.com/mustbearnold/Optimus-Agent/issues/60):
what Optimus Agent actually does today. Every claim below is measured from the
tree at commit `14d8f39`, not from `optimus-exceeds-hermes.md` and not from
memory.

Written to support [map #59](https://github.com/mustbearnold/Optimus-Agent/issues/59),
which needs a real baseline before success criteria can be set.

## Headline

**Three surfaces are declared; one is real.** The gap is not between crates that
exist and crates that don't — every crate has a consumer. The gap is between
`optimus-host`'s ~78 IPC handlers and the **8** of them the TUI actually calls.
The capability is built; the face hasn't reached it.

## The model-facing tool surface

`ToolInvocation` (`crates/optimus-packs/src/invocation.rs:14`) is the definitive
list. **17 real tools**, plus an explicit `Unavailable` variant:

| Group | Tools |
|---|---|
| Filesystem (9) | `read_file`, `search_content`, `find_files`, `list_dir`, `write_file`, `mkdir`, `delete_path`, `rename_path`, `patch_file` |
| Execution (1) | `terminal` |
| Knowledge (4) | `web_search`, `memory_recall`, `skill_resolve`, `activate_pack` |
| Browser (3) | `browser_navigate`, `browser_snapshot`, `browser_click` |

**Progressive loading is built, not proposed.** `optimus-packs` gates tools
behind a schema-token budget (`PackBudgetConfig`, `schema_tokens()`,
`activation_snapshot()` at `lib.rs:1203`), with `activate_pack` as the model's
own lever. The blueprint listed this as a gap versus Hermes; it shipped.

**Six packs are honest scaffolds.** Desktop, Media, Devex, Social, Home and
Office declare 12 tools between them, every one constructed via `unavailable(...)`
(`lib.rs:821-890`) and filtered out by `is_available()` (`lib.rs:454`). They are
named intentions with policy classes attached, not stubs that fail at runtime.

| Pack | Declared | Available |
|---|---:|---:|
| Core | 14 | 14 |
| Browser | 3 | 3 |
| Desktop / Media / Devex / Social / Home / Office | 12 | **0** |

## What the TUI can and cannot do

`apps/optimus-tui` is 4,504 lines and the only surface under active work.

**Can:** streaming transcript with inline tool rows; approval cards that resume
the paused turn (ADR-0046); `/providers`, `/provider`, `/model`, `/thinking`
with persistence across launches; `/approval`, `/yolo`, `/new`, `/frame`,
`/mouse`, `/help` (`src/commands.rs:74-83`); Ctrl-C interrupt with elapsed-time
spinner (`src/session.rs:262`); surrenderable mouse capture; session list and
search.

**Cannot:** everything else `optimus-host` exposes. The TUI references only
`approvals_release_yolo`, `providers_catalog`, `sessions`, `session`, `new`,
`logs`, `providers`, `session_id` — against a host surface of roughly 78
handlers including cron (6), gateway (7), artifacts (8), memory (4), campaigns
(4), packs (4), skills (3), MCP (2) and project scopes (2).

Those subsystems are reachable, tested, and invisible from the surface being
built to near-perfection.

## Crates: load-bearing vs aspirational

**No crate is orphaned.** Every one of the 15 has at least one workspace
consumer, so "aspirational" here means thin, not dead.

| Crate | LOC | Consumers |
|---|---:|---|
| `optimus-kernel` | 19,777 | eval, host, cli, desktop, tui |
| `optimus-runtime` | 7,496 | agent, eval, host, kernel, workflow, cli, desktop |
| `optimus-workflow` | 5,200 | kernel |
| `optimus-host` | 4,684 | desktop, tui |
| `optimus-ops` | 3,903 | kernel |
| `optimus-eval` | 3,465 | cli |
| `optimus-packs` | 2,424 | agent, eval, host, kernel, ops, workflow, cli, desktop |
| `optimus-store` | 1,937 | graph, runtime |
| `optimus-memory` | 1,860 | eval, kernel |
| `optimus-artifacts` | 1,057 | kernel, workflow |
| `optimus-agent` | 903 | kernel, workflow |
| `optimus-skills` | 659 | kernel, runtime, cli |
| `optimus-policy` | 595 | runtime |
| `optimus-browser` | 592 | kernel |
| `optimus-graph` | 537 | agent, eval, host, kernel, runtime, workflow, cli, desktop |

Two observations worth carrying into the north star:

1. **`optimus-kernel` holds 34% of the Rust in the tree** (19,777 of ~58,000
   lines) across 32 modules. It is the mega-module the blueprint criticised
   Hermes for, reproduced in Rust.
2. **`optimus-graph` is the most-depended-on crate at 537 lines.** The widest
   contract in the workspace is also one of the smallest — that is either
   excellent waist discipline or an under-built centre, and the north star
   should say which.

## Apps

| App | State |
|---|---|
| `optimus-tui` | 4,504 Rust LOC — the live surface |
| `optimus-cli` | 3,267 Rust LOC — consumes kernel, tui, eval |
| `optimus-desktop` | 2,922 Rust LOC — consumes host, kernel |
| `optimus-electron` | JS only — Playwright e2e, browser policy, preload |
| `optimus-ui` | TS/Vite only — no Rust |

## Limits of this baseline

Stated so nobody treats it as more than it is:

- Crate depth is measured by **line count and dependency edges**, not by
  behavioural coverage. A 3,903-line `optimus-ops` is not evidence that ops
  works, only that it exists and the kernel links it.
- `optimus-kernel` and `optimus-runtime` are characterised at **module
  granularity**. What each module actually provides at runtime was not traced
  call-by-call.
- The IPC-handler count is from match arms in `crates/optimus-host/src/`; a
  handler existing is not evidence it is correct.
