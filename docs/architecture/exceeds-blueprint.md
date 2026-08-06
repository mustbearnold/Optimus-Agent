---
doc_id: architecture-exceeds-blueprint
doc_type: explanation
plane: current
status: current
authority: canonical
summary: Historical north-star blueprint for exceeding Hermes; planned components, not proof of implementation.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
depends_on:
  - docs/architecture.md
---

# Optimus exceeds Hermes (north-star blueprint)

## Optimus vs Hermes (measured)

> **Documentary status (P16, updated 2026-07-27): SUPERSEDED** by
> north-star-2026-07.md (atticked) via the
> [#59 wayfinder map](https://github.com/mustbearnold/Optimus-Agent/issues/59).
> Historical **blueprint / mission prose** — evidence of what was once
> intended, not a statement of truth. The "strictly better than Hermes"
> success criteria below were retired by
> [#63](https://github.com/mustbearnold/Optimus-Agent/issues/63); Hermes is no
> longer the yardstick. **Do not grade as Confirmed current behaviour.** For
> live topology and grades use [system-overview.md](../architecture.md) and
> [architecture-marks.md](../runbooks/architecture-marks.md).

**Mission:** Rebuild the personal agent category so Optimus exceeds Hermes Agent on *every* axis that matters in production: reliability, learning quality, memory integrity, cost, latency, security, multi-agent durability, desktop UX, Ubuntu-first quality, cross-platform discipline, evalability, and long-horizon autonomy — without sacrificing Hermes’ genuine strengths (provider freedom, gateway breadth, skills loop, cache discipline).

This is not a Hermes fork with a coat of paint. It is a greenfield architecture that **imports Hermes product lessons and rejects Hermes structural debt**.

---

## 0. North star

**Optimus is a durable operator runtime with a measured learning loop.**

- Hermes optimizes for: *self-improving single agent + many surfaces*.
- Optimus optimizes for: *verified work completion + compounding capability under budget, with evidence-native memory and crash-safe multi-agent campaigns*.

Success definition (must all be true):

1. **Parity-plus product surface** — everything Hermes users rely on daily (chat, tools, skills, cron, gateway, desktop, MCP, profiles) works at least as well.
2. **Strictly better closed loop** — skills and memory only promote when outcome metrics improve; bad skills cannot accumulate silently.
3. **Strictly better memory** — bitemporal evidence store is native, not a plugin afterthought; recalled content never becomes action authority.
4. **Strictly better economics** — progressive context loading + measured cache policy + tool-schema budgets cut tokens 2–4× on long sessions vs Hermes defaults.
5. **Strictly better durability** — jobs, subagents, and campaigns survive process death; resume is first-class.
6. **Strictly better security** — deny-by-default capabilities, skill sandboxing, provenance-bound authority, packaging integrity.
7. **Strictly better Windows** — native first-class host, not POSIX port with scars.
8. **Strictly better proof** — every capability has a hard real benchmark (agent trajectory suites, not only unit mocks).

---

## 1. What Hermes gets right (keep / surpass)

| Hermes strength | Why it matters | Optimus stance |
|---|---|---|
| Cache-stable system prompt | Biggest cost lever on long sessions | Keep invariant; make **progressive loading** compatible with caching via staged cache breakpoints |
| Skills as procedural memory | Compounding value over time | Keep; add **outcome-gated promotion**, tests, versioning, rollback |
| Provider-agnostic core | Avoid vendor lock-in | Keep; stronger adapter contract + capability matrix per model |
| Multi-surface same identity | CLI/gateway/desktop feel like one agent | Keep; one **Kernel**, many thin surfaces |
| Profiles isolation | Multi-persona / multi-tenant local | Keep; stronger tenant principal model |
| Cron + webhooks + kanban | Real operator durability hooks | Keep; unify under one **Job/Campaign runtime** |
| Narrow core tool waist (intent) | Every core tool taxes every call | Enforce harder than Hermes actually does today |
| Plugin/MCP edges | Extensibility without core bloat | First-class capability packs with signed manifests |

---

## 2. Where Hermes loses (and Optimus attacks)

### 2.1 Structural debt

Observed live tree shape (2026-07-18 local install):

- `run_agent.py` ~6.5k LOC
- `cli.py` ~15.8k LOC
- `gateway/run.py` ~21.9k LOC

These are accretion god-modules. They force every change through high-conflict files, make invariants hard to test, and couple product surface to agent loop.

**Optimus rule:** no module > ~800 LOC without a forced split. Core loop files are pure state machines with injectable ports.

### 2.2 Learning loop is unmeasured

Hermes creates skills after complex tasks. Quality is model-dependent. Skill explosion and mediocre skills are known failure modes. Curator is mostly inactivity hygiene, not outcome science.

**Optimus:** every skill is a versioned artifact with preconditions, postconditions, optional executable checks, and rolling success stats. Promotion to “always-load candidates” requires measured improvement or human pin.

### 2.3 Memory is too thin + too dangerous if thickened naively

Hermes core memory is intentionally tiny (~2.2k / ~1.4k chars) and frozen for cache. Deep memory is pluggable (Honcho, Mem0, etc.) and easy to get wrong (vector RAG as truth, last-write-wins, authority laundering).

**Optimus:** ships MetaMemory-class substrate natively:

- immutable experience ledger
- bitemporal claims (valid time + transaction time)
- procedural memory separate from semantic claims
- evidence packets on recall (never bare blobs)
- **recalled content is DATA, never instruction or capability**
- action requires live capability tokens, not remembered preference

### 2.4 Delegation is not durable

Hermes `delegate_task` is process-local. Parent exit kills children. Cron is durable but separate. Users experience “long mission” fragility.

**Optimus:** one **Durable Work Graph** for turns, tools, subagents, cron, and multi-day campaigns. Process is replaceable; graph is not.

### 2.5 Context tax

Hermes sends large tool schemas + skill catalogs + static guidance every turn. Progressive loading has been proposed but is not the architecture.

**Optimus:** **Capability Router** with:

- tiny always-on tool waist (file/terminal/web/memory/job/clarify)
- demand-loaded capability packs (browser, computer-use, home, office, …)
- skill *index* always available; skill *body* loaded on hit
- cache breakpoints designed so pack activation is an explicit, rare event (or new turn segment), not silent mid-prefix mutation

### 2.6 Security posture

Public analysis and user reports: powerful defaults, skill creation risks, container approval edge cases, “allow-all” feel. Packaging integrity for sidecars is a recurring class of bugs in adjacent desktop agent work.

**Optimus defaults:**

- capability-based sandbox (not YOLO culture)
- skills cannot expand privileges
- outbound network allowlists per profile
- signed skill/plugin manifests
- Windows package: pinned runtimes, full license closure, hash-verified sidecars

### 2.7 Dual-runtime desktop pain

Hermes desktop = Electron shell over Python agent. Two ecosystems, two update stories, Windows quirks multiply.

**Optimus desktop primary:** Tauri 2 + Rust kernel services + React UI (Heracles-class lessons), conversation-first like Codex desktop — progressive disclosure of tools/terminal/files, not IDE cosplay. CLI remains first-class for headless/VPS.

### 2.8 Windows second-class residue

Even with native Windows support, Hermes carries POSIX assumptions (test runner, PTY, path, env scrubbing).

**Optimus:** Windows x64 is tier-0 CI. Linux/macOS tier-0 equally. No feature merges without green on both families for that surface.

### 2.9 Reliability of long autonomous runs

Field reports: crons break, gateway breaks, fix-one-break-three, token burn, weak models thrash.

**Optimus:** supervisor tree (s6-like semantics even on Windows via a native supervisor), health probes, auto-restart with backoff, deterministic replay of failed tool segments, budget circuit breakers.

---

## 3. Architecture — deep modules at clean seams

### 3.1 Layer cake

```
┌─────────────────────────────────────────────────────────────┐
│ Surfaces: CLI · TUI · Desktop (Tauri) · Gateway · ACP · API │
├─────────────────────────────────────────────────────────────┤
│ Orchestration API (session, turn, job, approval, stream)    │
├─────────────────────────────────────────────────────────────┤
│ Kernel                                                      │
│  ├─ Conversation FSM                                        │
│  ├─ Context Assembler (cache tiers)                         │
│  ├─ Model Router + Provider Adapters                        │
│  ├─ Capability Router (tools/skills packs)                  │
│  ├─ Policy Engine (approvals, sandbox, budgets)             │
│  └─ Learning Controller (skill/memory promotion)            │
├─────────────────────────────────────────────────────────────┤
│ Durable Runtime                                             │
│  ├─ Work Graph (turns, tools, children, cron, campaigns)    │
│  ├─ Event Log (append-only)                                 │
│  ├─ Supervisor / Process Manager                            │
│  └─ Checkpoint & Rollback                                   │
├─────────────────────────────────────────────────────────────┤
│ Cognitive Stores                                            │
│  ├─ Core Pin (tiny, curated, cache-stable)                  │
│  ├─ MetaMemory (ledger + bitemporal + projections)          │
│  ├─ Session Transcript (FTS + scroll API)                   │
│  ├─ Skills Registry (versioned procedures)                  │
│  └─ Artifacts (files, screenshots, builds)                  │
├─────────────────────────────────────────────────────────────┤
│ Effectors                                                   │
│  Terminal · Files · Browser · Computer-Use · MCP · Net      │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Deep module interfaces (small surface, large behavior)

#### `Kernel::turn(input) -> TurnResult`

Caller knows: messages in, stream events out, approvals may pause, budgets enforced.  
Hidden: provider retries, tool dispatch, compression, memory writes, skill suggestions.

#### `WorkGraph::spawn(spec) -> JobId` / `resume(job_id)`

Caller knows: durable id, status machine, delivery target.  
Hidden: process placement, retries, child isolation, crash recovery.

#### `MetaMemory::recall(query, purpose) -> EvidencePacket`

Caller knows: purpose enum (`inform`, `constraint`, `procedure_lookup`, …).  
Fails closed on `action_authorize` purpose.  
Hidden: hybrid retrieval, conflicts, temporal views.

#### `Skills::resolve(task_signal) -> SkillHit[]` then `load(id, version)`

Index is cheap and cache-stable; bodies are demand-loaded and spilled out of hot context after use summaries.

#### `Policy::authorize(action, context) -> Decision`

Unified gate for shell, net, file write, browser, messaging send, skill install, memory export.

#### `ContextAssembler::build(snapshot) -> PromptLayers`

Explicit layers:

1. **L0 Immutable identity** (soul, safety, schema waist) — max cache life  
2. **L1 Session constants** (profile, cwd policy, enabled pack ids)  
3. **L2 Working set** (goal, todos, open errors) — may refresh on segment boundaries  
4. **L3 Ephemeral** (tool results, recalls, skill bodies) — never poison L0/L1

### 3.3 Language / runtime split (pragmatic)

| Layer | Language | Why |
|---|---|---|
| Kernel + Work Graph + Policy + Supervisor + MetaMemory engine | **Rust** | Crash isolation, concurrency, Windows service quality, single binary sidecars, auditability |
| Provider adapters + high-level tool orchestration glue | **Rust** primary; thin Python optional host for scientific/user scripts | One runtime for production path |
| Surfaces UI | **React/TS** (Tauri desktop, web dashboard) | Fast iteration, Heracles/Codex-class UX |
| User code execution sandbox | Isolated subprocess (Python/Node/etc. as tools, not core) | Don’t put the agent brain in the same process users can wreck |
| Skills content | Markdown + optional WASM/scripts with capability manifests | Portable procedural memory |

**Why not pure Python like Hermes?**  
Python is excellent for research loops and plugins, terrible as the long-lived Windows supervisor + multi-agent process fabric once you care about durability and memory safety. Optimus can still *run* Python tools and offer a Python SDK; the **kernel is not Python**.

**Migration bridge:** Optimus speaks Hermes-compatible session import + skill import + OpenAI-compatible tool loop so users can claw-migrate and hermes-migrate.

---

## 4. Exceeding Hermes axis-by-axis

### 4.1 Agent loop

**Hermes:** classic tool loop in a mega-module; compression when near limit; max turns config.

**Optimus:**

- Explicit **Conversation FSM**: `Idle → Reasoning → ToolDispatch → AwaitApproval → Compressing → HandedOff → Done/Failed`
- **Segmented turns**: tool bursts can finalize a “segment” and re-open with preserved L0/L1 cache
- **Steer/interrupt** as first-class events (Hermes has these; Optimus makes them durable in the work graph)
- **Verification phase** optional but default for coding/ops goals: after claim-done, run declared checks before user-facing success
- **Circuit breakers**: repeated identical tool failure, budget burn rate, thrash detector → degrade to plan/ask, not infinite loops

### 4.2 Tools & capabilities

**Hermes:** 60–80+ tools; toolsets; MCP; browser; computer-use; etc. Schema cost is real.

**Optimus Capability Packs:**

| Pack | Examples | Load mode |
|---|---|---|
| `core` | read/write/search/patch, terminal, process, web_search/extract, clarify, todo, memory, jobs | Always |
| `browser` | CDP/harness browser | On demand |
| `desktop` | computer-use, UI automation | On demand + elevated policy |
| `media` | vision, imagegen, TTS/STT, video | On demand |
| `office` | docs/sheets/pptx | On demand |
| `social` | X search, messaging send | On demand + stricter policy |
| `devex` | git/gh deep tools, kanban, PR workflows | On demand |
| `mcp:*` | per-server generated pack | On demand, allowlisted tools |

Pack activation rules:

- Model requests pack via `need_capability(pack)` tool (tiny schema)
- Or router heuristic from user utterance
- Activation is a **segment boundary** event (logged, user-visible, cache-aware)
- Hard cap on concurrent loaded packs per session

### 4.3 Skills (procedural memory 2.0)

Hermes skills are markdown procedures the model may load and patch.

Optimus skill object:

```yaml
id: windows-rust-lnk1104
version: 3
status: candidate | proven | pinned | deprecated
preconditions: ["os=windows", "toolchain=msvc"]
steps: ...
checks:
  - type: command
    run: "cargo test -q"
    expect_exit: 0
metrics:
  uses: 12
  successes: 10
  avg_tokens: 48000
  last_verified: 2026-07-18
provenance:
  created_from_job: job_...
  parent_skills: []
permissions_required: [terminal, fs_workspace]
```

Rules that beat Hermes:

1. **Create as candidate only** after complex success
2. **Auto-prove** by replaying checks on next similar task or synthetic fixture
3. **Promote to proven** only after N successes or human pin
4. **Never** let skill text grant new permissions
5. **Curator is metric-driven**, not only TTL
6. Compatibility with agentskills.io + Hermes skill import

### 4.4 Memory (MetaMemory-native)

Exceed Hermes four-layer story by making the deep substrate *correct*:

1. **Core Pin** — tiny USER/AGENT pins, cache-stable, curated (Hermes-like size discipline retained on purpose)
2. **Working Memory** — goal stack, constraints, open errors (session-local, durable)
3. **Ledger** — append-only events (messages, tools, decisions, outcomes)
4. **Episodic** — trajectories with attempts/outcomes
5. **Semantic bitemporal claims** — “user prefers X” with valid-time, corrections, conflicts preserved
6. **Procedural** — skills registry
7. **Artifacts** — files with manifests
8. **Meta-memory** — which recalls helped/harmed; retrieval feedback loop

Recall returns **EvidencePacket**:

- current vs historical vs transition
- conflicts explicit
- citations to ledger ids
- trust/authority fields
- abstain when insufficient

Security invariants (non-negotiable):

- origin-bound authority
- evidence ≠ instruction
- no durable action capability in memory rows
- scope filters before top-k
- privacy erasure across all projections
- no destructive summarization as sole evidence

Hardware note (user constraint class): default local embeddings / rerank must fit **RTX 5070 12GB** with headroom for concurrent agent work; heavy models optional, not required.

### 4.5 Multi-agent & long-horizon work

**Hermes gaps:** leaf/orchestrator delegation, kanban board, cron — three systems users must mentally unify; children not durable.

**Optimus Work Graph unifies:**

- interactive turns
- background tool processes
- subagents
- cron ticks
- multi-day campaigns
- human approval nodes

Properties:

- every node has idempotency key, budget, policy snapshot, parent, delivery
- crash → supervisor restarts worker → graph resumes
- subagent contexts are isolated stores with explicit handoff artifacts (not “summary vibes only”)
- parent can await, poll, or subscribe
- **no nested spawn bomb** without quota; depth and fanout are hard-metered
- campaign templates: “repo completion”, “research watch”, “release train”

### 4.6 Scheduling & gateway

Keep Hermes platform breadth ambition, but:

- gateway adapters are **plugins with contract tests** and chaos suites
- one message bus; adapters are pure I/O
- backpressure and per-chat queues (no thundering herd tool storms)
- home-channel delivery is transactional with the work graph
- pairing/auth is capability tokens, not ambient trust forever

### 4.7 Desktop UX (beat Electron Hermes + match Codex feel)

Primary UX principles (aligned with Heracles/Codex desktop taste):

- conversation-first
- compact status (model, cost, job health, sponsor/earnings slot if needed)
- progressive disclosure: terminal/files/browser as drawers, not permanent IDE chrome
- live job/subagent watch without stealing focus
- mid-turn steer that feels instant
- native notifications only for approvals / true blockers
- Windows installer: MSI/NSIS with **complete** license + native dependency closure

### 4.8 Cost & performance

Targets vs Hermes default long-coding session:

| Metric | Hermes baseline class | Optimus target |
|---|---|---|
| Tool schema tokens/turn | all enabled tools | core + ≤2 packs |
| Skill body tokens | easy over-injection | index always; body on hit; spill after |
| Cache hit rate | high if disciplined | ≥ Hermes on L0/L1; pack switches are rare segments |
| Useless tool thrash | common on weak models | thrash detector + tool-result hashing |
| Aux model use | ad hoc | explicit tiers: extract/classify local small; reason frontier |

Additional levers:

- result spill to disk with handles (Hermes-like) but typed handles the model must re-fetch intentionally
- provider prompt-cache breakpoint API awareness (Anthropic/OpenAI/xAI as available)
- local small models for routing, skill suggest, memory extract on GPU-class machines

### 4.9 Security & safety

Default profile: **smart-deny**, not smart-allow.

- filesystem jail per workspace + explicit escapes
- network policy per profile (default: no arbitrary egress from code sandbox)
- high-risk tools always approval-gated (even in “yolo” only if profile explicitly `unrestricted` and local single-user)
- skills/plugins: signed manifests, permission declarations, no ambient admin
- secret redaction on by default and **not** toggleable by the model mid-session (Hermes lesson — keep)
- prompt-injection scanning on project files and web/tool output; untrusted content fenced
- audit log exportable and hash-chained (tamper-evident; optional external anchor)

### 4.10 Eval, benchmarks, self-improvement science

Hermes learns; Optimus **measures learning**.

Built-in eval harness:

1. **Trajectory suite** — frozen tasks with graders (file state, tests pass, HTTP contracts)
2. **Memory suite** — LongMemEval-class + bitemporal correction probes + poisoning tests
3. **Skill regression** — promoted skills must keep passing fixtures
4. **Gateway chaos** — disconnects, duplicate webhooks, partial sends
5. **Windows GUI / computer-use** — real apps first (TrueCUA philosophy): installed apps, DOM/CDP/a11y before pixels
6. **Cost suite** — tokens and wall time budgets as first-class scores
7. **Security suite** — authority laundering, path traversal, skill privilege escape

Learning loop only writes *proven* improvements into default load paths. Everything else stays candidate or personal pin.

### 4.11 Developer experience & codebase health

- deep modules, deletion test, interface = test surface
- contract tests at every seam (provider, tool pack, memory, gateway adapter)
- `optimus doctor` stronger than hermes doctor: reproduces common break classes
- schema-first config (versioned migrations, no silent defaults that flip live↔demo)
- single writer conventions for worktrees; campaign locking
- docs as executable examples

---

## 5. Product surfaces (parity map)

| Surface | Hermes | Optimus |
|---|---|---|
| CLI chat | yes | yes, thinner, faster start |
| Ink/TUI | yes | yes or repl-first; not a second monolith |
| Desktop | Electron | **Tauri 2 native** primary |
| Gateway 20+ platforms | yes | yes, adapter SDK + cert suite |
| ACP / IDE | yes | yes |
| Dashboard | yes | yes, job/memory/skill observability first |
| OpenAI-compatible proxy | yes | yes |
| Profiles | yes | yes + org/tenancy hooks |
| Cron | yes | Work Graph schedules |
| Kanban multi-agent | yes | Campaign board on Work Graph |
| MCP client/server | yes | yes, pack-gated |
| Plugins | yes | signed capability packs |
| RL / datagen hooks | Nous-specific | optional research pack, not core waist |

---

## 6. Data & identity model

- **Principal**: user / agent / profile / service
- **Workspace**: directory + policy + secrets scope
- **Session**: interactive conversation
- **Job**: durable work unit (may outlive session)
- **Campaign**: graph of jobs toward a goal
- **Artifact**: content-addressed outputs
- **MemoryEntity**: typed, scoped, bitemporal
- **SkillVersion**: immutable version; pointer moves

Session search remains free FTS (Hermes session_search lesson: don’t tax aux LLM for scrollback).

---

## 7. Migration & coexistence

Day-one importers:

- Hermes `state.db` sessions → Optimus transcript ledger
- Hermes skills directories → candidate skills (not auto-proven)
- Hermes MEMORY.md/USER.md → core pin + semantic claims with low confidence until confirmed
- OpenClaw/Claude/Codex project files (AGENTS.md, CLAUDE.md) — load as workspace constitution

Coexistence: Optimus can run beside Hermes; different home dir (`OPTIMUS_HOME`). No destructive takeover.

---

## 8. Delivery plan (build order that cannot lie)

### Phase 0 — Spine (2–3 weeks)

Vertical slice only:

- Rust kernel turn loop with one provider
- core tool waist (fs + terminal) on Windows + Linux
- sqlite event log + session resume
- CLI surface
- golden trajectory: “create repo file, run test, pass”

**Exit:** crash process mid-tool; resume; finish task.

### Phase 1 — Durable jobs + policy (2 weeks)

- Work Graph + supervisor
- approvals
- budgets / circuit breakers
- checkpoints

**Exit:** kill -9 during multi-step job; resume from last committed node.

### Phase 2 — MetaMemory MVP (3–4 weeks)

- ledger + claims + recall evidence packets
- core pin integration cache-safe
- correction, conflict, forget
- adversarial security probes green

**Exit:** bitemporal “prefers X → prefers Y” and poisoning tests pass.

### Phase 3 — Skills 2.0 + learning controller (2–3 weeks)

- import Hermes skills
- candidate/proven lifecycle
- curator metrics
- skill permissions enforced

**Exit:** skill improves graded task tokens/success vs baseline; bad skill does not promote.

### Phase 4 — Capability packs + browser/desktop (3 weeks)

- pack loader + schema budget
- browser pack
- computer-use pack (real app benchmarks)

**Exit:** schema tokens/turn ≤ target; browser task suite pass rate ≥ Hermes baseline on same model.

### Phase 5 — Gateway + multi-platform (ongoing)

- adapter SDK
- Telegram/Discord/Slack first
- chaos tests

### Phase 6 — Desktop Tauri (parallel after Phase 1)

- conversation-first UI
- job watch
- approval UX
- packaging integrity (pinned runtimes, licenses)

### Phase 7 — Eval harness & public benchmarks (continuous from Phase 0)

- every phase adds graded tasks
- weekly regression gate blocks release

### Phase 8 — Breadth pack explosion

Only after waist is stable: office, home automation, media, RL, etc.

---

## 9. Explicit non-goals (for v1)

- Becoming a multi-tenant SaaS cloud agent OS on day one
- Replacing all IDEs
- Unlimited autonomous self-modification of kernel code in production profiles
- Claiming “AGI” or unbounded self-improvement without graders
- Shipping 20 messaging platforms before Work Graph resume is boringly reliable

---

## 10. Competitive one-liner matrix

| Axis | Hermes | Optimus |
|---|---|---|
| Core identity | Self-improving personal agent | Verified durable operator with measured learning |
| Brain host | Python monolith accretion | Rust kernel + thin surfaces |
| Memory | Small pins + plugins | MetaMemory-native evidence store |
| Skills | Create/patch freely | Outcome-gated versions + permissions |
| Multi-agent | Soft delegation + kanban | Unified durable Work Graph |
| Context | Cache-stable but heavy schemas | Cache tiers + progressive packs |
| Desktop | Electron + Python | Tauri conversation-first |
| Security default | Powerful/permissive leaning | Capability deny-by-default |
| Proof | Tests + doctor | Trajectory/memory/security/cost gates |
| Windows | Supported | Tier-0 equal citizen |

---

## 11. First decisions to lock before code floods in

1. **Kernel language:** Rust (recommended) vs Python rewrite discipline  
2. **Desktop:** Tauri-first vs CLI-only MVP then UI  
3. **Memory:** embed MetaMemory in-process vs separate memory daemon  
4. **Compatibility:** hard requirement to import Hermes state on week 1?  
5. **Default approval posture:** smart-deny vs Hermes-like smart  
6. **Model default:** provider-agnostic empty vs batteries-included OAuth path  
7. **Scope of v1 gateway:** none / one platform / three platforms  

---

## 12. Recommendation (label + rationale)

**Recommendation: Build Optimus as a Rust-kernel, MetaMemory-native, Work-Graph durable operator with Hermes skill/session import and Tauri conversation-first desktop — not a Hermes fork.**

Rationale: Hermes’ product insight (learning loop + multi-surface personal agent) is correct, but its Python accretion core, unmeasured skill promotion, thin/plugin memory, non-durable delegation, and schema-taxed context are structural ceilings. Exceeding “in every way” requires changing the waist, not polishing the shell.

---

## 13. Immediate next actions

1. Grill/lock the seven decisions in §11.  
2. Write ADR-0001 (kernel + work graph) and ADR-0002 (memory invariants).  
3. Scaffold monorepo: `crates/optimus-kernel`, `crates/optimus-memory`, `apps/cli`, `apps/desktop`, `packs/core`, `evals/`.  
4. Implement Phase 0 spine with crash-resume golden test on Windows.  
5. Import a sample Hermes skill pack as *candidates* and prove the promotion gate before building more features.

---

*Document status: architecture blueprint for empty project tree `E:\Projects\Optimus Agent` as of 2026-07-18. Not an implementation claim.*
