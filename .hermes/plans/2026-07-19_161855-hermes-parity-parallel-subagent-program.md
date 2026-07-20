# Optimus Agent — Hermes Parity-Plus Parallel Subagent Program

> **For Hermes:** Execute with `subagent-driven-development`, one bounded vertical ticket at a time, using the orchestration rules below.
> **Planning scope:** This document optimizes the accepted Hermes-parity-plus program for multi-subagent execution. It does not authorize commit, push, publication, credential use, or remote tracker mutation.

**Goal:** Deliver every Hermes capability through Optimus with equal-or-better behavior, while maximizing safe subagent parallelism and routing each task by difficulty to the least expensive reasoning level that preserves correctness.

**Architecture:** Preserve the Rust kernel, Work Graph, MetaMemory evidence fence, SmartDeny, progressive capability packs, and Windows-native WebView2 shell. Work is organized as dependency-aware vertical tracer bullets. The controller owns integration and verification; one implementation writer owns the live shared workspace while two read-only agents prepare or review immutable candidates in parallel.

**Tech stack:** Rust workspace (`optimus-kernel`, `optimus-runtime`, `optimus-graph`, `optimus-memory`, `optimus-skills`, `optimus-packs`), `optimus-cli`, WebView2 desktop, Playwright, SQLite, CDP/Chromium, Windows CUA, provider and gateway adapters.

**Audit reconciliation:** Three independent read-only audits completed after the first draft. This revision treats current code and ADR-0014 as authority over stale phase/blueprint prose: the desktop is **tao + wry WebView2**, not Tauri/React; approvals, campaigns, Files list/read, and Terminal Phase A already exist; the leading losses are CDP/shared browser, artifacts, Telegram/durable delivery, MCP, ConPTY, general durable child-agent steps, and comparative evaluation. Current Playwright source contains 34 test declarations; execution must establish the actual passing baseline rather than copying historical counts.

### Current implementation truth at plan time

| Surface | State |
|---|---|
| Rome shell, routes, pane persistence; session search/local pins/rename/delete; local Projects UI | Implemented |
| Approvals, sequential campaigns, cron list/add/tick, Files list/read, Terminal Phase A | Implemented |
| Cron enable/remove desktop binding, thinking/tool timing, project→real root binding | Partial |
| Packs/skills/eval backends | Implemented; desktop console stub |
| MetaMemory engine | Implemented; desktop explorer absent |
| HTTP browser effector | Implemented but limited; shared CDP preview absent |
| Gateway queue/webhook | Implemented; Messaging UI and Telegram/Discord absent |
| Artifacts, Logs backend, MCP/plugins, ConPTY, comparative Hermes runner | Absent or honest stub |
| Campaign child-agent turns, DAG fan-out, worker leases, handoff artifacts | Absent; existing campaign verdict is narrow |

---

## 1. Non-negotiable execution constraints

1. **One live-workspace writer.** The user's established workflow is one writer with parallel read-only agents. Never dispatch overlapping writers into `E:/Projects/Optimus Agent`.
2. **Three-slot maximum:**
   - Slot W — one implementation writer.
   - Slot S — read-only specification/evidence/preflight agent.
   - Slot Q — read-only quality/security/test reviewer.
3. **Controller authority:** only the parent/controller mutates the task ledger, reconciles files, runs canonical gates, accepts work, or performs any authorized Git/remote action.
4. **Exact target binding:** every child brief names the absolute repository root, allowed paths, prohibited actions, focused tests, and expected evidence.
5. **No summary-as-proof:** child claims are leads. The controller reads changed paths and reruns focused and broad gates.
6. **No fake parity:** interfaces, placeholders, injected callbacks, disabled UI, and mocked adapters do not satisfy an end-to-end ticket.
7. **No nested spawn:** leaf workers do not delegate. Fan-out is controlled only by the parent.
8. **Current source-control constraint:** on 2026-07-19, `git status` at the project root returned “not a git repository.” Until repository authority is established, do not use Codex CLI or worktree-based concurrent writers. Use file fingerprints/snapshots for review identity.
9. **Windows verification:** desktop milestones require `scripts/rebuild-install-relaunch.sh` and native CUA observation, not HTTP-only proof.
10. **Shared hotspots:** `apps/optimus-desktop/ui/index.html`, `apps/optimus-desktop/src/ipc.rs`, `crates/optimus-kernel/src/lib.rs`, workspace manifests, and lockfiles have exclusive ownership. No other agent may write while a ticket owns one.
11. **Temporary source artifact:** `crates/optimus-kernel/src/.hermes-tmp.ACg6QB` must be classified and preserved or removed by the controller during PF-00; no worker may assume it is disposable.
12. **Current architecture authority:** ADR-0014 plus the running tao+wry code outrank `optimus-exceeds-hermes.md` where it still describes Tauri 2/React. Documentation is reconciled only after executable evidence exists.

---

## 2. Difficulty-decided reasoning selection

Difficulty changes decomposition, prompt depth, review strength, and model wave. It must not be a cosmetic label.

| Tier | Name | Selection criteria | Intended routing | Required review |
|---|---|---|---|---|
| D0 | Mechanical | One obvious local edit; no state, auth, concurrency, migration, external adapter, or UX contract | Controller/direct deterministic tools; lowest-cost reasoning (`Luna` class) | Focused check; optional read-only diff scan |
| D1 | Bounded | One module and one behavior; reversible; established neighboring pattern | Fresh leaf implementer; economy/balanced reasoning (`Luna` or `Terra`) | Spec review; focused tests |
| D2 | Cross-layer | API + durable state + UI/CLI or several consumers; bounded external integration | Fresh leaf implementer with explicit invariants; balanced/deep reasoning (`Terra`) | Parallel spec and quality review after candidate freeze; integration smoke |
| D3 | Critical | Security/authority, durable state machine, concurrency, crash recovery, migration, filesystem safety, provider auth, CUA, or irreversible side effects | `Sol`-class deepest reasoning in controller; first dispatch two read-only design/adversarial lanes, then one small writer packet | Spec + quality/security + failure-injection review; canonical broad gates |

### 2.1 Selection algorithm

Score one point for each condition:

- touches persistence/schema/migration;
- crosses three or more runtime layers;
- has ambiguous or contradictory reference behavior;
- handles credentials/auth/approvals/permissions;
- involves concurrency, cancellation, restart, or idempotency;
- invokes external side effects or third-party protocol behavior;
- has filesystem/network escape risk;
- changes a public pack/tool/IPC contract;
- lacks an established neighboring implementation;
- lacks a deterministic correctness oracle;
- overlaps a shared hotspot, generated output, formatter, manifest, or lockfile;
- failure can corrupt user work or silently misdeliver a message;
- acceptance requires native GUI or real adapter evidence.

Routing:

- `0` points → D0
- `1–2` → D1
- `3–5` → D2
- `6+`, or any authority/corruption boundary → D3

The controller may only promote difficulty, never demote below the score without recording why.

### 2.2 Real model-selection limitation

Hermes `delegate_task` children inherit the parent model; there is no per-call model/reasoning selector. Therefore:

1. Group implementation dispatches into **difficulty-homogeneous waves** when actual model switching matters.
2. Switch the parent/session model before a wave if a different Sol/Terra/Luna class is desired.
3. Within a mixed wave, keep the parent on the highest required tier and save cost by reducing prompt/review depth for easier work.
4. Codex CLI may be used only after a valid Git repository exists and its live CLI confirms supported model/reasoning flags. Do not invent flags.
5. Current chat runtime is `gpt-5.6-sol` via `openai-codex`; child delegations dispatched from it inherit that runtime.

---

## 3. Parallel pipeline

### 3.1 Steady-state three-slot pipeline

| Slot | While ticket N is being implemented |
|---|---|
| W | Implement ticket N using TDD and only its allowed paths |
| S | Preflight ticket N+1: inspect exact seams, refine acceptance checks, identify blockers; read-only |
| Q | Review frozen candidate N−1 for quality/security or validate benchmark evidence; read-only |

At the end of W:

1. Stop writes and inventory exact changed paths.
2. Run the focused gate.
3. Freeze a candidate identity. Preferred: Git tree/worktree. Until Git exists: copy only relevant files plus task-start fingerprints into `local/tmp/reviews/<ticket>-<timestamp>/` and make reviewers read that snapshot.
4. Dispatch S and Q reviews in parallel against the same immutable candidate.
5. Controller resolves findings serially; any material fix creates a new candidate generation and invalidates review on changed blobs.
6. Run canonical integration gate.
7. Mark ticket complete and advance the frontier.

### 3.2 D3 pipeline

D3 work is deliberately more serialized:

1. S: invariant/architecture review, read-only.
2. Q: adversarial failure/security test design, read-only, in parallel with S.
3. Controller resolves design and cuts the D3 ticket into packets no larger than one seam:
   - regression/failure test;
   - pure state transition or contract;
   - durable adapter/journal/filesystem boundary;
   - UI/CLI wiring;
   - real integration proof.
4. W implements exactly one packet.
5. Controller verifies before assigning the next packet.
6. Final S/Q reviews run in parallel on the frozen integrated candidate.

High difficulty means **smaller packets**, not a broader autonomous prompt.

### 3.3 Target dependency shape

Contract-first implementation should create policy-neutral domain engines that never depend on desktop or CLI code:

```text
optimus-browser    optimus-terminal    optimus-artifacts
optimus-gateway    optimus-mcp         optimus-projects
optimus-evals
        │
        ├── policy-neutral contracts ──→ optimus-runtime / optimus-packs
        └──────────────────────────────→ optimus-kernel
                                                ↓
                                     CLI / Desktop IPC / Desktop UI
```

Browser, terminal, MCP, artifacts, and gateway engines receive policy decisions or capability handles; they do not import the kernel to ask for authority. Root manifests, `Cargo.lock`, facades, registries, and global UI shell remain controller-owned integration surfaces.

### 3.4 Timeout rule

- First timeout: treat workspace as unknown; inventory files/processes and run the smallest focused test.
- Preserve verified partial work; redispatch only the missing checklist.
- Second timeout on the same write surface: stop redispatching writers. Controller finishes the bounded repair and still requests independent frozen-candidate reviews.

---

## 4. Parallelization-enabler wave (must come first)

These tickets make later work safer and more parallel.

| ID | Ticket | Difficulty | Blocked by | Ownership | Deliverable / gate |
|---|---|---:|---|---|---|
| PF-00 | Establish authoritative source-control and baseline protocol | D2 | None | Controller only | Identify intended repo root/branch or explicitly adopt snapshot fingerprints; no silent `git init`; canonical status command documented |
| PF-01 | Executable parity capability ledger | D1 | None | `docs/architecture`, `evals` only | Every claimed Hermes capability has `missing/partial/parity/win`, evidence path, trajectory ID, and owner; stale scorecard entries fail a check |
| PF-02 | Split desktop monofile and monolithic Playwright suite into stable modules without behavior change | D2 | PF-00 | Desktop UI/e2e files exclusively | Extract styles, state/router, IPC client, chat, left rail, right rail, composer and domain spec files; all established Playwright behaviors remain green in HTTP/native modes and native app paints correctly |
| PF-03 | Split desktop IPC router into domain modules without contract change | D2 | PF-00 | `apps/optimus-desktop/src` exclusively | Domain handlers for sessions, runtime, files, browser, gateway, memory, skills; IPC contract regression tests green |
| PF-04 | Canonical tool/pack descriptor and invocation contract | D3 | PF-00 | `optimus-packs`, kernel tool dispatch | One descriptor source drives schemas, policy, runtime invocation, UI catalog, and eval identity; unknown/unloaded tools fail closed |
| PF-05 | Immutable candidate/review harness | D2 | PF-00 | scripts/local review tooling | Produces candidate manifest, path hashes, focused command log, start/end identity; reviewers can prove target unchanged |
| PF-06 | Split CLI command families without behavior change | D2 | PF-00 | `apps/optimus-cli/src` exclusively | Extract gateway/cron/provider/session/campaign command handlers from `main.rs`; CLI contract tests and help snapshots remain green |

**Safe initial frontier:** PF-00 and PF-01 can proceed with controller/read-only assistance. PF-02, PF-03, PF-04, PF-05, and PF-06 must not run as concurrent live writers in the current checkout. While the writer advances one of them, the two read-only lanes should preflight the first CDP and MCP contract slices or review the preceding frozen candidate.

---

## 5. Ship A — Daily-driver agent loop

| ID | Vertical tracer bullet | D | Blocked by | Primary ownership | Acceptance gate |
|---|---|---:|---|---|---|
| A-01 | Provider catalog + connected/disconnected state + connect route | D3 | PF-03, PF-04 | provider adapters + desktop provider UI | Catalog shows complete canonical provider/model ownership; disconnected providers disabled; Codex/OpenAI-compat/offline real smokes |
| A-02 | Core file tools from model request to sandboxed effect and tool card | D3 | PF-04 | `fs_sandbox`, tool loop, chat cards | Agent lists/reads/writes/patches project fixture; traversal/symlink/secret probes denied; replay durable |
| A-03 | Terminal/process tools from model request to streamed/cancellable job | D3 | PF-04 | runtime command/process + chat cards | Agent starts, streams, cancels bounded command; timeout/restart/capture truncation tests green |
| A-04 | Web search/extract tools in the turn loop | D2 | PF-04 | kernel web + tool loop | Tool call reaches a real safe endpoint, citations survive transcript, network policy enforced |
| A-05 | Harden the existing SmartDeny approval UI for exact resume across all new effects | D3 | A-02, A-03, PF-03 | runtime approvals + desktop | Existing modal remains green; denied write/command/browser/MCP effect explains scope, grant/deny works, exact job resumes once, no duplicate effect |
| A-06 | Session FTS and jump-to-message | D2 | PF-03 | session store + desktop search | Search title/content/ID; result opens exact message; pagination and >500-session fixture green |
| A-07 | Thinking, tool cards, steering, and interrupt stream contract | D2 | PF-02, A-02, A-03 | desktop chat stream | Thinking separate from final; cards show status/duration/result; mid-turn steer and interrupt persist |
| A-08 | Provider failover and per-model capability enforcement | D3 | A-01, A-02–A-04 | provider router | Unsupported tools/reasoning fail before request; configured fallback preserves transcript/tool identity |
| A-09 | Bounded, redacted desktop/kernel Logs drawer | D2 | PF-02, PF-03 | desktop logs IPC/UI | Existing placeholder becomes a real capped tail with source/severity filters; missing logs are honest; secrets redacted |
| A-10 | Durable session archive, restore, runtime-state sorting, and pins | D3 | A-06, PF-03 | session store + desktop sessions | Additive migration preserves transcripts; archive/restore/filter, active-first state, and durable pins survive restart; profile links remain a separate D-06 concern |

Parallel preflight/review is encouraged, but A-02 and A-03 are serialized because both cross the core tool loop. A-06 may be implemented while another lane only reviews, not writes.

**Ship A gate:** A real desktop turn reads a fixture, writes a change after approval, runs a test, uses web evidence, streams cards, survives reload, and is searchable afterward on offline and Codex-capable paths.

---

## 6. Ship B — Hands: browser, files, terminal, CUA

| ID | Vertical tracer bullet | D | Blocked by | Primary ownership | Acceptance gate |
|---|---|---:|---|---|---|
| B-01 | CDP browser lifecycle and persisted tab model | D3 | PF-04 | new browser crate + kernel adapter | Launch/attach/close Edge/Chromium, create/select/close tabs, crash cleanup, no orphan listener |
| B-02 | Shared browser tools + preview UI session | D3 | B-01, PF-02, PF-03 | browser crate + desktop browser module | User and agent operate same tab; navigate/snapshot/click/type/screenshot; localhost and allowlist policy proven |
| B-03 | Page annotations to structured agent instruction | D3 | B-02, D-04 | browser annotation + artifacts/chat | Element/region annotation persists selectors/a11y/bbox/screenshot; sending creates fenced structured context |
| B-04 | Files tree, preview, write, rename, delete | D3 | A-02, PF-02 | desktop files module + fs tools | Real project tree; text/image/markdown preview; edits use same sandbox and SmartDeny; symlink escape denied |
| B-05 | Multi-terminal ConPTY/xterm surface | D3 | A-03, PF-02 | terminal backend + desktop terminal module | Create/write/resize/close multiple terminals, stream under load, cancel safely, native Windows smoke |
| B-06 | Native background CUA pack | D3 | PF-04, A-05 | desktop/CUA pack | A11y-first capture/click/type in installed apps, verified background behavior, foreground escalation only on signal, approval gates |
| B-07 | Browser/CUA hard benchmark suite | D3 | B-02, B-06 | evals only after contracts freeze | Real installed-app and website tasks, deterministic evidence bundles, Hermes baseline comparison |

**Ship B gate:** From one conversation, Optimus edits a local app, starts it in terminal, opens it in the shared preview browser, accepts a page annotation, repairs it, and verifies via browser/CUA without focus theft.

---

## 7. Ship C — Operator: schedules, gateways, unattended work

| ID | Vertical tracer bullet | D | Blocked by | Primary ownership | Acceptance gate |
|---|---|---:|---|---|---|
| C-01 | Rich schedule model and crash-safe due-job leasing | D3 | PF-04 | cron/work graph | Cron expressions, interval/one-shot, pause/resume/remove, lease ownership, restart recovery, no double-fire |
| C-02 | Delivery contract joining Work Graph to gateway outbox | D3 | C-01 | graph + gateway | Transactional completion→outbox handoff; duplicate/retry/idempotency chaos tests |
| C-03 | Telegram adapter with persisted pairing/auth state | D3 | C-02 | gateway adapter + secure config | Real opt-in message→session→reply smoke; secrets redacted; reconnect and duplicate webhook tests |
| C-04 | Messaging and schedule desktop surfaces | D2 | C-01–C-03, PF-02, PF-03 | desktop messaging/settings modules | Connect/status/inbox/outbox/job history/failures; no secret rendering; real adapter state only |
| C-05 | Discord and Slack adapters via certified SDK | D2 each | C-03 | separate adapter modules | Same adapter contract/chaos suite passes; each adapter is an independent ticket during execution |
| C-06 | Supervisor health/restart and operator notifications | D3 | C-01–C-04 | runtime supervisor + desktop status | Failed workers/adapters restart with bounded backoff; only true blockers notify; state visible |

**Ship C gate:** A scheduled task survives process restart, runs once, and delivers through Telegram with auditable state; failures surface in desktop and retry without duplicate sends.

---

## 8. Ship D — Extensibility, knowledge, artifacts

| ID | Vertical tracer bullet | D | Blocked by | Primary ownership | Acceptance gate |
|---|---|---:|---|---|---|
| D-01 | Native MCP client as per-server capability pack | D3 | PF-04, A-05 | MCP crate/pack | Stdio + HTTP server discovery, generated schemas under budget, allowlisted tools, disconnect/reconnect, one real server smoke |
| D-02 | Signed plugin/pack manifest and permission lifecycle | D3 | D-01 | packs/security/config | Install/enable/disable/remove; signature and permissions displayed; tamper/privilege escalation rejected |
| D-03 | Bind existing packs/skills/eval backends into a real Capabilities console | D2 | PF-02, PF-03 | packs/skills/eval IPC + desktop capabilities | Inspect/activate packs for the active session, show schema budget errors, show candidate/proven/pinned/deprecated skills, and persist visible eval reports; skills cannot grant permission |
| D-04 | Content-addressed artifact store and desktop gallery | D3 | PF-03 | new artifact crate/store + desktop | Publish/list/preview/open/delete/pin; session provenance and hashes; browser/screenshots/files integrate |
| D-05 | MetaMemory browse/recall/correct/feedback/forget UI | D3 | PF-02, PF-03 | memory IPC + desktop memory | EvidencePacket fence visible; current/historical/conflicts; privacy erase tests; recalled data never authorizes action |
| D-06 | Bind the existing local Projects UI to durable project/profile scope | D3 | A-06, B-04 | new projects registry + desktop | Persist validated roots; switch project/profile changes FS roots, session/memory scope, terminal cwd and browser localhost policy; no cross-scope bleed |
| D-07 | Generalize existing durable campaigns into real child-agent DAG orchestration | D3 | A-05, D-04 | campaign/work graph + desktop | Add AgentTurn/AwaitChild/PublishArtifact semantics, isolated child session, worker lease, handoff artifact, parallel dependency edges, crash resume, and depth/fanout quotas; existing WriteFile/RunCommand steps remain compatible |

**Ship D gate:** Connect an MCP server, load one skill, delegate a task, produce a provenance-bearing artifact, recall/correct memory safely, and reopen all state after restart.

---

## 9. Ship E — Remaining Hermes breadth

Each adapter/surface is a separate vertical ticket after the shared contract is stable.

| ID | Capability | D | Blocked by | Gate |
|---|---|---:|---|---|
| E-01 | Vision/image generation pack | D2 | PF-04, D-04 | Image input/output appears in transcript and artifacts with provider capability checks |
| E-02 | TTS/STT/voice composer pack | D2 | PF-04, D-04 | Record/transcribe/respond/speak with cancellation, device errors, and artifact provenance |
| E-03 | OpenAI-compatible proxy/server surface | D3 | A-01–A-05 | External client executes streamed tool turn under policy with session durability |
| E-04 | ACP/IDE bridge | D3 | A-02–A-07, D-06 | IDE client opens project-scoped session and receives tool/stream state safely |
| E-05 | TUI/headless operator surface | D2 | A, C | Chat/tools/approvals/jobs usable without desktop; no second business-logic implementation |
| E-06 | Slash-command/command-palette parity | D1 | A, C, D | Canonical command registry drives desktop/TUI help and actions |
| E-07 | Hermes session/skill/memory importer | D3 | D-03, D-05, D-06 | Dry-run report, idempotent import, conflicts preserved, source untouched |
| E-08 | Kanban/campaign board | D2 | D-07 | Work Graph is single authority; board is projection, not separate scheduler |
| E-09 | Signed updater/release channel | D3 | all release gates | Verify signature, rollback interrupted update, preserve home, installed native smoke |
| E-10 | Remaining messaging adapters | D2 each | C-05 SDK | One ticket per platform with contract + chaos + real opt-in smoke |
| E-11 | Office/devex/social/home packs | D2 each | PF-04, D-02, D-04 | One pack per domain, budgeted schemas, real fixture task, explicit permissions |

---

## 10. Ship P — Proof required for “better in every way”

This ship runs continuously; it is not deferred until the end.

| ID | Proof ticket | D | Blocked by | Acceptance |
|---|---|---:|---|---|
| P-01 | Frozen Hermes-vs-Optimus capability/trajectory manifest | D2 | PF-01 | Same task, same model/provider where possible, declared environment, deterministic graders |
| P-02 | Feature trajectory added with every vertical ticket | D1–D3 | corresponding ticket | No capability may move to parity/win without executable evidence |
| P-03 | Crash/restart/failure-injection suite | D3 | A–D stateful tickets | Kill at each side-effect boundary; no corruption/duplication/silent loss |
| P-04 | Authority/security suite | D3 | A-05, B, C, D | Path escape, prompt injection, memory authority laundering, plugin escalation, secret redaction |
| P-05 | Context/cost/latency suite | D2 | PF-04, A | Schema tokens, cache breaks, wall time, tool thrash measured against Hermes baseline |
| P-06 | Native Windows product suite | D3 | each desktop ship | Install/relaunch, resize/minimize, real CUA, no focus theft, no black screen, process cleanup |
| P-07 | Release claim generator | D2 | P-01–P-06 | Produces scorecard only from current evidence; any missing/loss axis blocks “better everywhere” claim |

---

## 11. Dependency waves and maximum useful concurrency

### Wave 0 — Truth and seams
- Writer: PF-01, then PF-02, PF-03, PF-04, PF-05 serially by hotspot.
- S lane: baseline capability audit and next-ticket seam discovery.
- Q lane: behavior-preservation review and test-gap design.

### Wave 1 — Daily driver
- Critical sequence: A-01 → A-02 → A-03 → A-05 → A-08.
- Side frontier: A-04, A-06, A-07 when their owned paths are free.
- P-02/P-04/P-05 are parallel read-only/test-design lanes until their writer turn.

### Wave 2 — Hands + operator foundations
- Critical sequences: B-01 → B-02 → B-03; C-01 → C-02 → C-03.
- Because one writer is authoritative, alternate B and C tickets to let S/Q preflight and review the other domain.
- B-04/B-05 can begin after A-02/A-03; B-06 remains Sol/D3.

### Writer parallelism boundary

The architecture can eventually support many domain lanes, but the **current execution policy remains one writer**. True multi-writer fan-out is permitted only if the user separately authorizes isolated Git worktrees/clones, each has an exclusive path/manifest contract, and the controller serializes manifest/facade integration. Without that gate, “parallel” always means one writer plus two read-only agents.

### Wave 3 — Extensibility
- D-01/D-02, D-03, D-04, D-05 have distinct domain ownership but are still serialized in the live workspace.
- After immutable worktrees are explicitly authorized and operational, D-03/D-04/D-05 are the first candidates for true parallel writers because their crate/UI modules can be disjoint.

### Wave 4 — Breadth factory
- Use stable adapter/pack contracts.
- Up to three agents may run in parallel only in separate authorized worktrees, one adapter/pack each.
- Parent merges one candidate at a time and reruns integration gates after each merge.

### Wave 5 — Claim freeze
- Stop all writers.
- S: exact specification/parity audit.
- Q: security/quality/reproducibility audit.
- Controller: full test/build/install/CUA/trajectory suite and release scorecard.

---

## 12. Ticket brief template

Every implementation dispatch must include:

- Ticket ID, complete objective, user-visible result.
- Difficulty score, tier, and why.
- Absolute root: `E:/Projects/Optimus Agent`.
- Exact allowed write paths and protected paths.
- Current candidate/baseline identity.
- Relevant existing types/functions already inspected.
- Invariants and explicit non-goals.
- RED test command and expected failure reason.
- Focused GREEN command.
- Controller-owned broad gates (worker must not burn time on them unless requested).
- Prohibition on commit, push, credentials, installs, external sends, unrelated edits, and nested delegation.
- Required output: changed paths, RED/GREEN evidence, unresolved risks; no unverified completion claim.

Review briefs bind to one immutable candidate generation and prohibit edits, builds, network calls, and broad repository searches unless specifically required.

---

## 13. Gates

### Per-ticket revision gate
1. Focused RED observed for intended missing behavior.
2. Minimal implementation passes focused test.
3. Controller reads every changed path.
4. Frozen-candidate specification review passes.
5. Frozen-candidate quality/security review passes for D2/D3.
6. Relevant crate/package tests pass.

### Per-ship integration gate
1. `cargo test --workspace -- --test-threads=1`
2. `cd apps/optimus-desktop && npx playwright test`
3. Any live-adapter tests are opt-in and explicitly configured.
4. `bash scripts/rebuild-install-relaunch.sh`
5. Native CUA verification on installed executable.
6. Capability ledger and trajectories updated from executable evidence.

### Abort gates
- Workspace changes while an unaccounted writer is active.
- Candidate identity changes during review.
- Secret/credential data appears in logs or UI.
- External message/write happens without explicit opt-in.
- Same write surface times out twice.
- “Parity” depends only on a stub, mock, disabled UI, or injected interface.

---

## 14. Immediate frontier

1. **PF-00:** resolve repository authority, disposition the temporary kernel-side file, and establish the exact passing baseline. Current root is not recognized as Git; do not silently initialize it.
2. **PF-01:** build the executable parity capability ledger from current code, replacing stale phase claims.
3. **PF-02/PF-03/PF-06:** modularize desktop UI/e2e, desktop IPC, and CLI hotspots without feature changes.
4. **PF-04:** unify pack descriptors and tool invocation before adding more adapters.
5. Start independent contract-first backend slices after the decomposition gate: one localhost CDP tab, one stdio MCP fixture tool, one content-addressed artifact, one durable gateway receipt, one ConPTY tab, and one comparative trajectory packet.
6. Integrate root manifests/facades serially, then bind each engine to kernel/runtime and finally to its desktop module.

During PF-00–PF-04, use the read-only slots immediately: one agent freezes the CDP/MCP/gateway reference contracts and adversarial cases while the other inventories exact tests and consumers. This preserves multi-agent throughput without risking shared-workspace corruption.

The first execution wave should use one Sol-class writer for PF-02/PF-03/PF-04 (D2/D3) and two parallel read-only agents: one behavior-preservation/spec lane and one test/security lane. Easy D0/D1 cleanup discovered during these tickets is queued for a later Luna/Terra-class wave rather than interrupting the critical writer.

---

## 15. Definition of program completion

Optimus may claim “every Hermes capability, better in every way” only when:

1. every capability ledger row is `parity` or `win` with current executable evidence;
2. no Hermes surface remains a stub or CLI-only substitute where Hermes has a usable product surface;
3. each adapter has contract, chaos/failure, and safe real-smoke evidence;
4. durability, authority, privacy, context cost, latency, accessibility, and Windows-native behavior meet the declared comparative gates;
5. all writers are stopped and the exact release candidate passes spec, quality/security, workspace, build, install, CUA, trajectory, and scorecard gates;
6. remaining differences are documented intentional product choices with demonstrated equal-or-better user outcomes—not omitted functionality.
