---
doc_id: plans-competitive-bottleneck-audit
doc_type: history
plane: history
status: historical
authority: historical
summary: 2026-08-01 audit pinning every bottleneck and barrier limiting Optimus Agent against Hermes Agent v0.19.0 and OpenClaw (Aug 2026), with brainstormed solutions per item; evidence from live gates, the parity ledger, the Hermes reference checkout, and sourced OpenClaw research.
reviewed_on: 2026-08-01
review_by: never
knowledge_type: plan
owns:
  - docs/plans/competitive-bottleneck-audit.md
covers:
  - docs/plans/competitive-bottleneck-audit.md
depends_on:
  - docs/current/roadmap.md
  - docs/architecture/sota-scorecard.md
  - docs/architecture/north-star-2026-07.md
  - docs/architecture/parity-capability-ledger.json
---

# Competitive bottleneck & barrier audit — 2026-08-01

Question: what limits Optimus Agent from becoming better than both **Hermes
Agent** (v0.19.0, the fail-closed parity target) and **OpenClaw** (the
dominant open-source personal agent, ~385k stars, Aug 2026)?

Method: three parallel investigations on 2026-08-01 — (1) live-gate and
document sweep of this tree, (2) read-only profile of
`Development/reference/hermes-agent-read-only`, (3) sourced web research on
OpenClaw's current state. Every bottleneck below carries evidence; solutions
are brainstorm options, not decisions. Items are labeled B-«area»-«nn» so
later plans can reference them without renumbering.

"Better than both" is defined here as: (a) the strict Hermes parity gate
passes or is consciously re-scoped by decision, and (b) Optimus beats
OpenClaw where OpenClaw is weakest (safety-by-default, durability, cost
honesty, memory integrity) while matching the small set of OpenClaw/Hermes
features that create daily reach (channels, automation, memory, voice).

## 0. Where Optimus already wins (the assets to build on)

The parity ledger claims exactly four evidence-backed structural wins:
crash-resumable Work Graph durability, evidence-fenced bitemporal
MetaMemory, outcome-gated permission-closed Skills, durable approval-gated
SmartDeny effects (`docs/architecture/sota-scorecard.md`). These map
directly onto the competitors' documented weaknesses: OpenClaw's dominant
criticisms are security-for-non-experts, runaway token costs, and memory
pollution; Hermes's evidenced weaknesses include 24k-line god files,
process-global state hazards, an open SQLite fd-leak class, and no scored
self-eval harness. The strategy every solution below serves: **keep the
governance moat, remove the reach and speed barriers.**

---

## 1. Strategic barriers (decide these first — they gate everything else)

### B-STRAT-01 — The yardstick contradiction
The north-star retired Hermes as a yardstick ("no criterion's pass/fail may
depend on observing Hermes", `docs/architecture/north-star-2026-07.md`),
yet the release ratchet still binds to the 2,063-contract Hermes gate and
the fail-closed parity version (`scripts/optimus_version.py` gate: BLOCKED,
0/2063 feature evidence, 0/8 performance scenarios). A blocked gate for a
discarded benchmark is wired into release semantics, so "release" is
undefined.
**Solutions:** (a) Decide by ADR: either re-adopt the Hermes gate as the
release bar and fund the inventory audit, or re-scope the version gate to
the thesis axes and demote the Hermes gate to an informational scorecard.
(b) If re-scoped, define an Optimus-native release bar (own capability
ledger rows green + performance baselines vs *own previous release*, not vs
Hermes). (c) Either way, stop carrying `parity unverified` in README as the
headline — replace with the chosen bar and its live status.

### B-STRAT-02 — No performance evidence exists at all
0/8 parity performance scenarios have ever been run (cold-start,
single-turn, multi-tool-turn, long-session, session-resume, scheduled-job,
browser-task, delegated-task). Optimus literally does not know if it is
fast or slow versus anything.
**Solutions:** (a) Build the paired-benchmark harness the version gate
already specifies (≥30 paired samples, ≥3 seeds, machine fingerprint) and
run it against Hermes locally — the reference checkout plus built desktop
artifact already sit in `Development/reference/`. (b) Start smaller:
wall/TTFT/cost per scenario against Optimus's own last release to create a
regression ratchet immediately. (c) Wire p50/p95 into the evidence ledger
so every future land can cite trend data.

### B-STRAT-03 — No distribution or ecosystem story
OpenClaw's moat is 25+ channels, ~13k ClawHub skills, one-click hosting
partners, and 100k-member community; Hermes ships installers for three
OSes, Docker, Nix, Termux. Optimus ships a user-scoped Ubuntu install with
a reinstall script as the upgrade path (ADR-0043: no auto-updater).
**Solutions:** (a) Ship the signed updater (already ADR'd as partial) —
without update velocity, every other fix ships slowly. (b) Pick ONE
additional distribution surface for 2026 (deb repo, Flatpak, or Docker) —
not all. (c) Defer a public skill registry until the product moat is
proven, but design skill packaging now so third-party skills are possible
later (the Skills lifecycle law is already a differentiator: outcome-gated,
permission-closed — ClawHavoc-proof by construction; OpenClaw had ~12%
malicious skills in early 2026).

---

## 2. Capability gaps (reach: what users can simply *do* elsewhere)

### B-CAP-01 — 14 of 31 catalog tools are non-dispatchable scaffolds
Six packs (Desktop, Media, Devex, Social, Home, Office) declare 12 tools
and ship **0** (`scripts/check-tool-coverage.py`: 17 dispatchable, 14
refused; `docs/architecture/capability-baseline-2026-07.md`). Hermes ships
83 live tools; OpenClaw's plugin surface is effectively unbounded.
**Solutions:** (a) Triage the 14 into "ship in 90 days" vs "delete from
catalog" — a catalog row that refuses is worse than absence (it teaches the
model false affordances and pays prompt cost). (b) Priority order by
competitor-overlap × user demand: `vision_analyze`, `image_generate`,
`tts`, `desktop_screenshot/click/type` (see B-CAP-05), then devex
(`gh_pr`, `git_deep`). (c) For each shipped tool, add a parity-ledger
trajectory in the same land (prevents re-widening the scaffold gap).

### B-CAP-02 — Messaging gateway is mock-only
Live Discord/Slack/Telegram transports are mock enqueue
(`docs/architecture/sota-scorecard.md` losses row). Hermes runs ~20
platforms through one gateway; OpenClaw 25+; "the gateway is the phone"
(north-star) — but the gateway doesn't ship, so Optimus has no presence
away from the desk, and no mobile story at all (B-CAP-11).
**Solutions:** (a) Ship ONE live transport end-to-end first (Telegram is
the conventional lowest-friction bot API; Hermes and OpenClaw both started
there) with the durable-delivery law applied — exactly-once outbound is
already flagged unresolved, and a delivery ledger like Hermes's
`delivery_ledger.py` is the proven pattern. (b) Reuse the SmartDeny
approval spine for remote-initiated effects — remote surfaces with
approvals is precisely where OpenClaw is weakest (21k exposed instances,
CVE-2026-25253) and Optimus can be structurally safer. (c) Treat the
`optimus-ops` gateway scaffold's zero integration tests (B-QUAL-03) as a
blocker for this work, not an afterthought.

### B-CAP-03 — No voice in or out
`tts` is a refused scaffold; no STT exists. Hermes: 9 TTS / 6 STT
providers, 2 fully local; OpenClaw: wake words, talk mode, meetings.
**Solutions:** (a) Local-first minimal pair: faster-whisper STT + one local
TTS (Hermes bundles NeuTTS/KittenTTS locally — same route keeps the
local-first thesis). (b) Voice as a pack with its own budget/approval
class, not a bolt-on to chat. (c) Defer meeting-bot territory; it's
ecosystem-expensive and neither competitor's is a daily driver.

### B-CAP-04 — No vision or media generation
`vision_analyze`, `image_generate` refused; both competitors ship both.
**Solutions:** (a) Vision first (screenshot understanding unblocks
computer-use verification, B-CAP-05, and browser evidence); route through
existing provider adapters rather than new ones. (b) Image/video generation
last — differentiating value is low for a productivity agent; wire only
when a pack consumer exists.

### B-CAP-05 — Computer use has no committed baseline and scaffold-only effectors
`desktop_screenshot/click/type` refused; `desktop.native-cua` is the only
ledger row with an empty evidence array; the PF-00 installed-app CUA
baseline "has never been committed"; the governing skill documents only the
legacy Wry shell and carries `author: Hermes` (`skills/optimus-native-ui-
testing/SKILL.md`).
**Solutions:** (a) Rewrite the native-UI-testing skill for the Electron
default shell first — it gates all installed-app proof. (b) Commit PF-00
against the current installed build even if scores are poor; an honest red
baseline unblocks the ratchet. (c) Ship the three effectors under heavy
approval (the sota-scorecard already names "live computer-use effectors
under heavy approval" as a leading-product loss).

### B-CAP-06 — Browser output defect + bounded-HTTP facade
Live defect: `browser_navigate` exceeds the 128 KiB canonical outcome
ceiling and aborts researched turns after 70s (`MAX_TOOL_OUTCOME_DATA_BYTES`,
`crates/optimus-packs/src/lib.rs:199`; evidence file round-2). The kernel
retains an HTTP browser facade (marks residual); no preview UI; CDP
trajectory unclassified.
**Solutions:** (a) Accept the already-Proposed page/result budget decision
(ADR-0048) and implement extract-then-truncate: readability extraction +
link table within budget, full page to an artifact. (b) Never spend 70s
then fail: stream-size check aborts early or downgrades to summary. (c)
Retire the kernel HTTP facade onto `optimus-browser` (marks residual
already names this). (d) Add the browser-reliability benchmark the eval map
lists as missing.

### B-CAP-07 — MCP is mock-only egress
Mock client, "live spawn residual"; ingress ruled out by decision. Hermes
is MCP client + MCP server + ACP editor server; OpenClaw ships MCP apps.
**Solutions:** (a) Ship live stdio spawn for the existing mock client
contract — smallest step, largest unlock (every MCP server becomes an
Optimus capability without new tool code). (b) Keep ingress ruled out; it
is a real attack surface and OpenClaw's incident history is the cautionary
tale — document this as a deliberate safety win, not a gap. (c) Adopt
Hermes's catalog pattern: curated, pinned MCP manifests with threat
screening at save and spawn (`mcp_security.py` is the reference).

### B-CAP-08 — No retrieval subsystem exists
"No vector, embedding, full-text, graph, reranking, or GPU index exists"
(`docs/maps/memory-and-retrieval.md`); no relevance ranking, no semantic
search, no retrieval eval dataset. Memory/sessions/skills/project-knowledge
remain five distinct stores with no universal recall and no cross-store
transaction. Both competitors ship working recall (Hermes: FTS5 + 8
pluggable providers; OpenClaw: file-based memory + memsearch).
**Solutions:** (a) SQLite FTS5 over sessions + memory first — boring,
local, zero new deps, and it is what Hermes actually ships as default. (b)
Then a thin universal-recall facade (one query API over the five stores)
before any vector work; embeddings only after the retrieval eval dataset
exists (the map explicitly requires the baseline first). (c) Optimus's
bitemporal evidence-fenced memory is the differentiator vs OpenClaw's
documented memory-pollution problem — build the recall UX to *show*
provenance and staleness, which neither competitor does.

### B-CAP-09 — Multi-agent is registered-only
No model-chosen specialist router, no parallel child execution, closed
dispatch table (system-overview debts 1-2; C-09/C-10 boundaries). Hermes:
delegate_task with roles/depth/concurrency + kanban swarm + MoA; OpenClaw:
subagents + Codex delegation.
**Solutions:** (a) Open the dispatch table minimally: model-chosen routing
over the *registered* specialists first (routing maturity is already a
roadmap priority) — no open-ended spawn yet. (b) Parallel ready-node
execution inside the existing Work Graph rather than a new orchestrator —
durability is already the win; parallelism inside it beats a bolt-on. (c)
Cap child hierarchies exactly like Hermes (`max_spawn_depth`,
`max_concurrent_children`) — proven ergonomics, easy to gate.

### B-CAP-10 — Model routing has no fallback, accounting, or local models
No provider/model fallback, no token accounting, no live billing, no
local-model/GPU adapters (`docs/maps/model-routing.md`; C-11). Hermes: 32
providers, failover, cost guard, credential pool; OpenClaw: failover +
local inference; its cost blowouts are a top user complaint — an opening.
**Solutions:** (a) Runtime-failure fallback chain first (config-declared,
like Hermes `fallback_model`). (b) Token/cost accounting in the events
ledger; surface per-turn cost in the UI — "cost honesty" directly attacks
OpenClaw's weakness. (c) One local adapter (Ollama/OpenAI-compatible
loopback) — note this collides with localhost-denied-by-default (B-SEC-04),
which needs the ADR-0060 grant path finished. (d) Skip GPU adapters until
an eval consumer exists (their own map says the GPU eval row is N/A).

### B-CAP-11 — No mobile access of any kind
"The gateway is the phone. No mobile client ships" — but the gateway is
mock (B-CAP-02), so the thesis currently resolves to *nothing on the
phone*. OpenClaw: iOS/Android/WearOS apps; Hermes: Termux + any chat app.
**Solutions:** (a) B-CAP-02(a) IS the mobile plan — one live messaging
transport gives phone reach with zero mobile code; sequence it first. (b)
Do not build a native client in 2026 (already correctly deferred).

### B-CAP-12 — Terminal PTY is partial
"Full interactive I/O residual"; parity row partial; Hermes ships 6
terminal backends including hibernating serverless ones.
**Solutions:** (a) Finish single-backend local interactive PTY end-to-end
in the workbench (already a named leading-product loss). (b) Skip exotic
backends; local + (later) SSH covers the real product's audience.

### B-CAP-13 — Cron exists but lifecycle is unproven; no event triggers
`cron.lifecycle` is untrajectoried; no webhook/event-driven triggers are
documented anywhere in the tree (inferred absent). OpenClaw's "wake only
when something changes" and Hermes's webhook runs are core daily-value
features.
**Solutions:** (a) Add the cron lifecycle trajectory (cheap, closes a
ledger row). (b) A single localhost webhook trigger endpoint under the
existing approval law gives event-driven automation without a gateway; it
depends on the ADR-0060 localhost grant path (B-SEC-04).

### B-CAP-14 — Skills: powerful law, empty shelf
The Skills lifecycle law (outcome-gated, permission-closed) is a claimed
structural win, but exactly 2 repository skills exist, and there is no
product skill catalog. Hermes ships 183 skills + hub + Curator; OpenClaw
~13k (with a 12%-malicious incident to its name).
**Solutions:** (a) Author 10-15 first-party product skills for the top
synthetic-lab journeys — the lab personas already define demand. (b) Adopt
a Curator-like usage/staleness reviewer bounded to agent-authored skills
(Hermes's never-delete/archive-only rule is the right shape and matches
the no-deletion-by-age law already in this repo's culture). (c) Market the
security contrast explicitly when any sharing surface ships.

### B-CAP-15 — Hermes migration path is unproven
`hermes_import.rs` exists; `migration.hermes` is untrajectoried. Hermes
itself ships `hermes claw migrate` to poach OpenClaw users — the playbook
works and Optimus has it half-built for Hermes.
**Solutions:** (a) Trajectory the existing Hermes import against the
reference checkout's real state shapes. (b) Add an OpenClaw importer
(memory files are plain Markdown in `~/.openclaw/workspace` — trivially
importable, and OpenClaw's user base is enormous and security-anxious).

### B-CAP-16 — Project scope: configured vs enforced
`projects.scope` partial ("concurrent multi-project mutate lease
residual"); C2 stands at 0/82 host methods carrying scope assertions with
the counter never having moved.
**Solutions:** (a) Make C2 movement a standing land-review item: any land
touching a host method must assert its scope (ratchet the allowlist down
mechanically). (b) Implement the mutate lease; it is the named residual.

---

## 3. Performance & turn-loop bottlenecks

### B-PERF-01 — Turn budget of 8 steps starves ordinary turns
ADR-0047 (Proposed, 8 → 32) states it plainly; approval round-trips consume
budget. **Solution:** accept and implement; trivially the highest
value-per-line change in the backlog. Gate: verify long-turn evidence in
the synthetic lab afterward.

### B-PERF-02 — 48k-char history budget, sized for chat not tools
ADR-0048 (Proposed, 48k → 200k + page budgets). **Solution:** implement
together with B-CAP-06; they are the same decision. Add a context-use
telemetry line per turn so budget tuning stops being guesswork.

### B-PERF-03 — Approval friction: the Standard default is decided, gated, and unreachable
Four synthetic humans reproduced 2-3 approvals for harmless confined
writes; the TUI has no bounded command to select `standard`. The
north-star calls the shipped `#[default] ReviewChanges` "drift to be
fixed, not documented around" — but ADR-0044's post-0059 amendment gates
the universal Standard default on "code-enforced arbitrary-process network
and scoped credential authority", and ADR-0059's own risks section names
ambient credential inheritance as a known boundary gap. The default
therefore cannot honestly flip yet; the two authorities also disagree and
need reconciling.
**Solutions:** (a) Near-term, sanctioned: add the bounded
profile-selection command to the TUI so users can *choose* Standard (the
desktop composer already offers it; the TUI evidence names the absence).
(b) Implement approval-resumes-turn (ADR-0046, Proposed) so a granted
approval doesn't also cost the user a turn. (c) Unblock the real gate:
scoped credential authority + arbitrary-process egress enforcement
(B-SEC-02) — then flip the default per the amendment, and record the
north-star/ADR-0044 reconciliation in the same land. Flipping before (c)
would widen autonomy past what the decision record permits.

### B-PERF-04 — Measured latencies are uncompetitive and unexplained to the user
47s aggregate for six web searches; 25s/15s ordinary conversational turns;
70s to fail on a big page; no progress display beyond a repeated label
(evidence round-2, priority #3).
**Solutions:** (a) Streaming progress events per tool call in TUI/desktop
(observability law already mandates ordered events — render them). (b)
Parallelize independent tool calls inside a turn (needs B-CAP-09(b)
ready-node parallelism). (c) Profile the turn loop against the B-STRAT-02
baseline; nobody has measured where the time goes.

### B-PERF-05 — Synchronous HTTP cannot be cancelled mid-request
`ureq` connection/write cannot be force-aborted; cancellation law is
otherwise a product pillar. **Solutions:** (a) Move provider calls to the
async client with cancellation tokens; or (b) wrap sync calls in abortable
workers with a hard timeout as an interim. Either way, add a cancellation
trajectory to the ledger.

### B-PERF-06 — Verification wall-clock and caching
`just ui` ~2min; cargo/npm/Playwright caches uncached in CI-style runs;
sccache/shared-cache work is a pending backlog row; every land pays full
verify.
**Solutions:** (a) Do the pending cache work (sccache + reused browsers).
(b) Per-stage duration telemetry (also already pending) to find the next
bottleneck honestly. (c) Consider an incremental verify tier keyed on the
impact-select map for inner-loop use only (land keeps full verify).

---

## 4. Quality & evidence barriers

### B-QUAL-01 — Eval coverage is documentation-shaped, not capability-shaped
Three suites exist (docs-authority, repository-orientation, synthetic-user
lab); zero product-capability evals; the eval map lists seven missing
dimensions (retrieval, grounding, browser reliability, cost/latency, tool
schema conformance, routing, workflow). Hermes has no scored harness either
— this is an *open* differentiation lane, not just a gap.
**Solutions:** (a) Stand up the "planned evaluation gate" (candidate-aware
versioned baselines) the map already specifies. (b) First three capability
evals by leverage: tool-outcome schema conformance (cheap, mechanical),
browser task reliability, longitudinal continuity (only one returning
persona exists — add more). (c) Publish scores in the README the way
coverage badges work; "the agent that measures itself" is a credible
marketing line against both competitors.

### B-QUAL-02 — 37/50 parity rows have no runnable trajectory
Shrink-only counter pinned at 37. **Solution:** trajectory-per-land rule
(any land touching a capability area must add or extend its trajectory) +
a monthly floor (e.g. -3/month) enforced by ratchet, same mechanism as the
module-size law.

### B-QUAL-03 — Zero integration tests in seven crates including the gateway scaffold
`optimus-ops` (3,903 LOC: gateway, cron, PTY, Hermes import, channels) and
`optimus-workflow` (5,200 LOC) have no integration tests at all.
**Solution:** block new capability work in an untested crate until it gains
a harness (the gateway plan B-CAP-02 makes this urgent); start with
contract tests over the IPC/enqueue seams which already have fixtures.

### B-QUAL-04 — Replay cannot reproduce live effects
C-13: fixture comparison does not rerun live model/network/process/browser
effects; stores share no transaction. **Solutions:** (a) Record-replay at
the provider adapter seam (cassettes) to make model-loop regressions
testable offline. (b) Cross-store transaction is a research item — scope it
to memory+events first, where the integrity win lives.

### B-QUAL-05 — Evidence culture stops at the desk
Synthetic-user lab is strong (five personas, adaptive rounds) but only one
longitudinal persona exists, and no evidence covers messaging/mobile/voice
journeys because the surfaces don't exist (circular with B-CAP-02/03).
**Solution:** each new surface lands with a lab persona in the same
program; extend the cohort schema now.

---

## 5. Architecture debts

### B-ARCH-01 — optimus-kernel is the mega-module the blueprint criticized
34% of the tree's Rust at baseline and grown since (55 files, 22.5k LOC).
**Solutions:** (a) Declare a kernel budget (share-of-tree ratchet like the
module-size law). (b) Extract the two seams the docs already name: the
HTTP browser facade (to `optimus-browser`) and OAuth flows (to a provider
crate). (c) No big-bang split; move one consumer-visible seam per program.

### B-ARCH-02 — optimus-engineering: 9,917 LOC nobody can reach
Zero workspace consumers; seven accepted ADRs and a program describe a
vertical no shipped binary invokes; component DB says "excluded from
installed default members" while Cargo.toml lists it in default-members
(live contradiction).
**Solutions:** (a) Decide within one program: integrate through an approved
product route, or archive the crate and mark the ADR series superseded —
the component row already defines exactly these two exits and a review-by
date of 2026-10-31. (b) Fix the default-members contradiction immediately
(one-line land). (c) If archived, harvest its durable-task substrate ideas
into the Work Graph backlog rather than deleting knowledge.

### B-ARCH-03 — Seven Proposed ADRs are decided-but-undelivered product value
0045-0051: host/transport registry, approval-resumes-turn, step budget,
context budgets, honest module measurement, Radix overlays, Electron/Tauri
posture. Three of them (0046/0047/0048) are the direct fixes for the
top UX complaints.
**Solution:** a "Proposed→Accepted or Rejected" sweep with implementation
lands for the three UX ADRs first; a Proposed ADR older than two programs
should be re-decided or withdrawn as a standing rule.

### B-ARCH-04 — North-star C-criteria not moving
C2 at 0/82 with no movement; C5 at 5 allowlisted violations; C4's script
still only checks the desktop surface; C3's electron always-spawn noted.
**Solutions:** (a) Every criterion needs a ratchet enforced in verify (C2
and C5 have counters but no per-land floor). (b) Extend the IPC-matrix
script to the four surfaces so C4 is measurable at all.

### B-ARCH-05 — Surface reach: built capability the faces can't touch
82 host methods; Electron reaches 70; TUI ~8. Cron, gateway, artifacts,
memory, campaigns, packs, skills, MCP, project scopes are all unreachable
from the TUI.
**Solutions:** (a) TUI hub work is already ADR-0045's subject — deliver it.
(b) Prioritize by journey evidence: memory recall and cron are the two the
synthetic personas actually reached for. (c) Publish the reach matrix as a
generated doc so the gap is visible per land.

### B-ARCH-06 — Fragmented state stores, no universal memory
Five distinct systems named in status.md; no cross-store transaction; the
roadmap already owns this. **Solution:** B-CAP-08(b)'s recall facade is
step one; unification of write paths is a later program — do not attempt a
grand merge before recall proves the seams.

### B-ARCH-07 — Exactly-once external delivery unresolved
Named debt (system-overview #9) — becomes load-bearing the day B-CAP-02
ships. **Solution:** durable outbound obligation ledger (Hermes's
`delivery_ledger.py` docstring literally cites the incidents it prevents);
build it before the first live transport, not after the first incident.

### B-ARCH-08 — optimus-graph question deliberately unanswered
Widest contract (8 consumers), 537 lines, "no ticket decided it."
**Solution:** decide it (thin waist is fine; write the ADR either way so
the question stops recurring in every baseline).

### B-ARCH-09 — 14 grandfathered oversized modules
Ratchet exists and works; largest is optimus-store at 1,655.
**Solution:** keep the ratchet; opportunistic shrink when a module is
already open for capability work (B-CAP items touch most of them);
no dedicated refactor program — Hermes shows where ignoring this ends
(24k-line files), but a standing law already prevents that here.

### B-ARCH-10 — Electron-only preview weld
ADR-0051 (Proposed): only the in-process preview welds the shell to
Electron; 335 MB Hermes desktop and OpenClaw's TS footprint show the cost
of never deciding. **Solution:** accept 0051's posture (Electron now,
Tauri when the preview leaves the shell) and keep the seam clean — cheap
optionality.

---

## 6. Security & platform barriers

### B-SEC-01 — Credential storage debt
Plain-JSON auth store (ADR-0011 debt); non-Windows encrypted storage
unresolved (C-12); no at-rest encryption/backup contract for SQLite.
**Solutions:** (a) OS keyring via a thin adapter (secret-service/DPAPI)
with the plain-JSON path as documented fallback. (b) Backup contract doc +
`optimus backup` command; small, closes a named map gap. This is table
stakes for the "safer than OpenClaw" claim.

### B-SEC-02 — Network policy does not cover provider/OAuth calls
Allowlist/proxy/TLS-pinning gaps; ambient credential inheritance named as a
known boundary gap (ADR-0059 risks).
**Solutions:** (a) Route provider adapters through the same egress policy
object as tools (one chokepoint). (b) Credential scoping per project grant
— the broker design already anticipates it.

### B-SEC-03 — Platform asymmetry
FS confinement Linux-only; Windows Job Object residual; `os.rs` unsupported
paths; primary target is Ubuntu.
**Solutions:** (a) Declare a supported-platform matrix honestly in README
(Ubuntu full, Windows degraded-with-list) — honesty is the brand. (b)
Windows Job Object work only when a Windows user journey exists in the lab.

### B-SEC-04 — Localhost denied by default blocks local ecosystems
ADR-0060 issuance incomplete; IPv4/HTTP-only. Blocks local models
(B-CAP-10), webhooks (B-CAP-13), Home Assistant-class integrations.
**Solution:** finish the grant issuance path with per-origin durable grants
under SmartDeny; this single unlock feeds three capability items.

### B-SEC-05 — SSRF residual named in the map
Hostname-resolution edge documented. **Solution:** resolve-then-pin
(connect to the vetted IP, not the re-resolved name); add the trajectory.

### B-SEC-06 — Security as an unweaponized advantage
Optimus's approval spine, consequence-bounded autonomy, and refusal-based
tooling are exactly what OpenClaw's incident history (exposed gateways,
CVE-2026-25253, ClawHavoc, Moltbook) proves the market lacks — but no
document positions Optimus this way externally.
**Solution:** a public threat-model/security posture doc (Hermes's
SECURITY.md candor is the style to beat) + a self-audit command
(`optimus security audit`, mirroring what third parties built for OpenClaw
as SecureClaw). Cheap, differentiating, honest.

---

## 7. Process & velocity barriers

### B-PROC-01 — Governance ceremony taxes every change
Per-turn: orient → docs-check → docs-refresh → docs-generate → em-check →
em-context → em-generate → validate; 31 gates + 24 self-tests; naming-plane
gates; single-channel delivery. This is the moat AND the tax — OpenClaw
ships 13 releases a month; this repo's honest cost must drop without
dropping the honesty.
**Solutions:** (a) Auto-chain the mechanical steps (`just docs-fix` =
check+refresh+generate for reported ids; `just em-fix` likewise) — the
*review* stays human/agent, the choreography shouldn't be. (b) B-PERF-06
caching. (c) Measure land-cycle wall-clock as a first-class metric
(the temporal graph can already answer it).

### B-PROC-02 — Delivery tooling has named expressiveness gaps
"If checkpoint/land/undo cannot express the operation, report the
limitation" — this session alone hit two (worktree config repair,
cleanup symlink refusal).
**Solution:** ledger of tooling-limitation reports (a `land/limitations/`
receipt type); monthly triage into managed-tool fixes like ADR-0067's.

### B-PROC-03 — Environment fragility outside managed land
Nine skip branches (tmux, npm ci ×3, chromium, display); Playwright cwd
sensitivity; xvfb 640×480 default; gitignored-bundle staleness (#107). A
fresh worktree failed a land this very day on missing node_modules.
**Solutions:** (a) `just setup-worktree` that provisions node_modules +
browsers + checks tmux/display and prints a green/red table. (b) Make
worktree creation managed (`just worktree-new <name>`) — also fixes the
config.worktree gap permanently. (c) Skip-report already exists; wire it
into orient so agents see environment debt at turn zero.

### B-PROC-04 — The live tier is expensive so it's rare
Real-model smoke needs installed credentials and spends tokens; browser
success needs public web. **Solution:** scheduled weekly live run with a
receipt (cron exists; budget-capped), so live evidence ages ≤7 days
instead of "whenever someone remembers."

### B-PROC-05 — Institutional knowledge concentration
Two repo skills; deep contracts live in AGENTS.md and individual heads.
**Solution:** B-CAP-14(a)'s skill-authoring push applies to development
skills too (the worktree-provisioning and land-recovery procedures learned
this week are exactly skill-shaped).

---

## 8. Sequencing sketch (solutions above, ordered by unlock value)

1. **Decide B-STRAT-01** (yardstick) — everything else inherits its
   definition of done.
2. **The decided-but-undelivered set** (B-PERF-01/02 via ADRs 0047/0048,
   plus B-PERF-03(a) TUI Standard selection and (b) approval-resumes-turn
   via ADR-0046): largest UX gain, zero new design. The Standard *default*
   flip stays gated behind B-SEC-02 per ADR-0044's amendment.
3. **B-CAP-06 browser fix** — live defect on the researched-turn path.
4. **B-STRAT-02 performance baseline** — cannot claim "better" unmeasured.
5. **B-CAP-01 tool triage + B-CAP-04(a) vision** — reach.
6. **B-SEC-04 localhost grants** → unlocks local models, webhooks.
7. **B-CAP-02 one live transport** (with B-ARCH-07 delivery ledger and
   B-QUAL-03 gateway tests) — the presence gap vs both competitors.
8. **B-CAP-08(a) FTS recall** — memory reach with provenance UX.
9. **B-ARCH-02 engineering-crate decision** before its 2026-10-31 review.
10. **B-STRAT-03(a) signed updater** — velocity multiplier for all above.

Everything not sequenced here is ratchet-shaped: attach it to lands that
touch the area (trajectories, scope assertions, module shrink, C-criteria).
