---
knowledge_type: plan
status: current
owns:
  - docs/plans/s-plus-plus-plus-program.md
watches:
  - docs/architecture/architecture-marks.md
  - docs/architecture/system-overview.md
  - docs/plans/s-plus-trust-spine.md
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/specialist_vertical.rs
covers:
  - docs/plans/s-plus-plus-plus-program.md
depends_on:
  - docs/architecture/architecture-marks.md
  - docs/plans/s-plus-trust-spine.md
  - docs/decisions/0001-kernel-and-work-graph.md
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
validated_by:
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-kernel/tests/specialist_vertical.rs
  - scripts/check-observability-gate.py
last_verified_commit: null
---

# S+++ program — lowest-to-highest dimension plan

**Authority for architecture S+++ sequencing after Phases 0–5:** this document.

Phases 0–5 (truth freeze through causal observability) are complete in
[s-plus-trust-spine.md](./s-plus-trust-spine.md). This plan is the **next program**:
raise every architecture mark from its **current grade to S+++**, working
**lowest grade first**, then higher grades.

This is **not** a Hermes product-parity plan and **not** a daily-app feature
checklist. Product surface work stays in [full-app-microtasks.md](./full-app-microtasks.md).
If a product task would lower an architecture mark, the architecture phase wins.

## Rules (non-negotiable)

1. **Grade order.** Work dimensions in the sequence below. Do not open the next
   dimension’s implementation wave until the current dimension’s **exit gate**
   is green and `architecture-marks.md` is updated in the same change set.
2. **Honest grading.** Planned work is never graded Confirmed or S+++. A mark
   moves only when source + tests + docs agree under adversarial review.
3. **Strengthened S+++ bar.** Historical exit criteria in
   `architecture-marks.md` were **minimum floors** for earlier phases. This plan
   defines **adversarial S+++ criteria** per dimension. Meeting the old floor
   while still graded B/A is expected until this program closes the residual.
4. **Hard dependency exceptions.** A lower-grade dimension may **temporarily**
   land at **S** (not S+++) when a structural residual is owned by a later
   dimension. The residual must be named, product-visible, and tracked. Final
   program gate still requires every mark at S+++.
5. **Spine reuse.** Every durable effect still flows Work Graph → SmartDeny →
   exact terminal outcome. No second approval system, second tool catalog, or
   renderer-granted authority.
6. **Regression hold.** Completing a later dimension must not drop an earlier
   mark. Each phase ends with a **hold suite** for already-elevated marks.
7. **ADR + EM.** Material boundary changes get an ADR. After behaviour lands,
   refresh Engineering Memory (`check` → owned knowledge → `generate` →
   `validate --quick`).
8. **No Hermes-parity requirement for architecture S+++.** Release/parity
   gating stays fail-closed; architecture S+++ does not require full ledger
   green.
9. **Naming planes.** Program phase `P##` is **not** a GitHub PR number and
   **not** an ADR number. Delivery uses `PR #N` / local `pr/N-…` assigned by
   GitHub. Coding agents must follow
   [artifact-naming.md](../contributing/artifact-naming.md) and `AGENTS.md`.

## Current grades and work order

Baseline: `architecture-marks.md` as of 2026-07-25.

| Order | Dimension | Current (post-P12) | Target | Program phase |
|:---:|---|:---:|:---:|---|
| 1 | Multi-agent readiness | **S+++** | S+++ | P10 (done; S+++ after P12 residual) |
| 2 | Control-plane modularity | **S+++** | S+++ | P11 (done) |
| 3 | Security boundary design | **S+++** | S+++ | P12 (done) |
| 4 | Domain modularity | **S+++** (post-P13) | S+++ | P13 (done) |
| 5 | Observability / eval | **A-** | S+++ | P14 |
| 6 | UI architecture | **A-** | S+++ | P15 |
| 7 | Doc / claim hygiene | **A-** | S+++ | P16 |
| 8 | Release / parity gating | **A** | S+++ | P17 |
| 9 | Durability / crash safety | **A+** | S+++ | P18 |
| — | **All-marks adversarial review** | mixed | **all S+++** | P19 |

Phase numbers continue from trust-spine 0–5; P6–P9 are reserved for any
interim hold/fix if needed. Execution starts at **P10**.

```text
P10 Multi-agent (done → S+++)
  → P11 Control-plane (done → S+++)
  → P12 Security (done → S+++)
  → P13 Domain modularity (done → S+++)
  → P14 Observability (A-)   ← next
  → P15 UI (A-)
  → P16 Doc hygiene (A-)
  → P17 Release gates (A)
  → P18 Durability (A+)
  → P19 Final S+++ review board
```

### Dependency honesty (exceptions allowed by Rule 4)

| Dependency | Handling |
|---|---|
| Multi-agent command specialists need strong command FS envelope | P10 ships write-only + optional **sandboxed** command path that reuses whatever containment exists; full adversarial FS confinement is **P12**. P10 may exit at **S** interim if command residual remains, then re-grade to S+++ after P12. |
| Clean specialist hosting wants thinner kernel | P10 may keep code in `optimus-kernel` modules; P11 **must** extract agent/workflow (and related) crates so control-plane S+++ is real. |
| Observability “every terminal turn reconstructible” | Already largely true; P14 closes distributed-trace export and cross-store identity honesty without claiming false distributed transactions. |
| Doc hygiene last among A- marks | P16 re-audits after behaviour phases so docs do not freeze mid-flight. |

---

## Shared definition of S+++

A dimension is **S+++** only when all of the following hold:

1. **Invariant enforcement:** the critical properties are enforced in code, not
   only documented.
2. **Adversarial residual empty:** no known structural hole remains that a
   hostile model, compromised renderer, or crash mid-effect can exploit within
   that dimension’s scope (or the residual is an explicit, tested Unrestricted
   break-glass path with operator-visible policy).
3. **Tests:** focused unit/integration tests + at least one gate script or
   offline harness that would fail if the invariant regresses.
4. **Docs match:** system-overview, maps, marks, and ADRs use Confirmed language
   only for implemented behaviour.
5. **No silent second path:** no bypass of SmartDeny, packs catalog, project
   authority, or Work Graph for durable host effects.

---

## P10 — Multi-agent readiness (B → S+++)

**Owner packages:** `optimus-kernel` (then peels in P11), `optimus-runtime`,
`optimus-packs`, CLI vertical surface, optional desktop read-only projection.

### Why still B

One vertical (`workspace_writer` + `write_file_handoff`) proves the spine.
There is no general specialist router, parallel/leased child hierarchy, DAG
workflow executor for registered definitions, or cancel tree across children.

### Adversarial S+++ criteria

1. **≥2** immutable registered specialists with distinct permission ceilings and
   tool sets, both executed through Work Graph + SmartDeny + provenance links.
2. **≥2** workflow definitions with explicit cancellation, retry, timeout,
   approval, and terminal policies; owner adapters declare unsupported
   capabilities rather than inventing them.
3. A **bounded DAG executor** for registered workflow definitions (not free-form
   model plans): topological order, per-node Work Graph jobs, fenced leases,
   exactly one terminal outcome per node and per workflow run.
4. **Parent/child cancel tree:** cancelling a parent invocation cancels
   in-flight children and prevents new children; storage enforces one terminal
   per invocation identity.
5. **Handoff artifacts** are content-addressed, permission-checked, and linked
   from child terminal results to parent context without sharing writable
   session mutation authority.
6. **No SmartDeny bypass:** specialists cannot grant themselves host mutation
   outside skill/approval policy.
7. EM agent count ≥ 2 with tests; workflow registry reflects executed paths.
8. CLI (and optional desktop) can list verticals and run the second vertical
   offline with ScriptedModel or deterministic fixture.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| M1 | ADR: multi-agent execution model (DAG vs free children, lease, handoff) | done — `docs/decisions/0033-multi-agent-dag-execution.md` |
| M2 | Second specialist (`workspace_reader`) | done — seed + tests |
| M3 | Second + third workflows (`read_file_handoff`, `write_then_read_handoff`) | done |
| M4 | Generic `WorkflowRunStore`: run id, node states, lease, terminal uniqueness | done — `workflow_run.rs` |
| M5 | DAG scheduler over registered definitions only | done — `run_registered_workflow` |
| M6 | Parent cancel propagates; child cannot outlive parent terminal | done — cancel tests |
| M7 | Artifact handoff contract tests (hash match both nodes) | done — `workflow_dag.rs` |
| M8 | CLI `vertical` surface for write/read/write-then-read; EM ≥2 agents | done |
| M9 | Update marks: Multi-agent → **S** (S+++ deferred to P12 residual) | done |

### Interim grade rule

If command specialists still rely on P12 FS residual: grade **S** after M1–M9
for write-only / fully confined paths, document residual, re-open grade to
**S+++** in P12 exit.

### Hold suite

- Existing `specialist_vertical` tests green
- Runtime approvals + cancellation suites green
- No new tool catalog in kernel

### Out of scope for P10

- Open-ended model-chosen specialist spawning without registry
- MCP tools as specialists
- Product-complete multi-agent UI consoles (UI phase may add projection only)

---

## P11 — Control-plane modularity (B+ → S+++)

**Owner packages:** new/ peeled crates; `optimus-kernel` shrinks to turn waist.

### Why still B+

Eval and ops are extracted. Kernel still owns agents, workflows, artifacts,
browser, routing, credentials, project authority, specialist verticals, and the
~2.5k-line turn `lib.rs` waist.

### Adversarial S+++ criteria

1. **Kernel public surface** is turn/provider/tool dispatch/session projection
   (+ thin re-exports only where surfaces need ergonomics).
2. **Separate crates with public APIs and no reverse deps into kernel turn
   internals** for at least:
   - agents + specialist verticals + invocation ledger
   - workflow definitions + DAG run executor (from P10)
   - artifacts
   - browser effector (HTTP/CDP)
   - routing + route telemetry (or routing stays kernel-adjacent with clear
     module boundary and zero tool-catalog duplication)
3. **Dependency rule enforced in CI or script:** `optimus-eval` may depend on
   kernel; kernel must not depend on eval; ops must not depend on kernel;
   agent/workflow crates must not depend on desktop/electron.
4. **No second ToolDesc catalog** appears in any peeled crate.
5. Surfaces (`cli`, `desktop`) compile against the new crate graph without
   behaviour change (golden turn + campaign + vertical tests).
6. Architecture maps and system-overview topology match the crate graph.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| C1 | Crate map ADR (names, deps, re-export policy) | done — ADR-0034 |
| C2 | Peel `optimus-agent`: registry + invocation ledger | done |
| C3 | Peel `optimus-workflow`: definitions + DAG run store + verticals | done |
| C4 | Peel `optimus-artifacts` | done |
| C5 | Browser: CDP remains `optimus-browser`; kernel facade HTTP+factory retained | done (partial by design) |
| C6 | Dependency lint script `scripts/check-crate-layers.py` | done |
| C7 | Kernel modules removed for peels; re-export waist | done |
| C8 | Ownership map + system-overview + marks → Control-plane **S+++** | done |

### Hold suite

- P10 multi-agent tests still pass after moves
- `cargo test -p optimus-kernel` / eval / runtime focused suites
- Offline integrity gate still green

### Out of scope for P11

- New product features
- OTel (P14)
- Command sandbox (P12)

---

## P12 — Security boundary design (A- → S+++)

**Owner packages:** `optimus-runtime`, `optimus-graph`, `optimus-kernel` /
project authority, desktop/electron transport, security map.

### Why still A-

SmartDeny covers host-mutating effects; path preflight and skill class grants
exist. **Residual:** approved `RunCommand` is not under the same `cap-std`
directory capability as file effects. Linux uses `systemd-run` + `bwrap` but
currently **`--bind / /`** (full root visible)—workspace cwd is not a true FS
envelope. Windows Job Objects own the process tree, not the filesystem.
Network egress policy is incomplete for provider/search/browser as a whole.

### Adversarial S+++ criteria

1. Every host-mutating Work Graph effect is high-risk under SmartDeny **or**
   requires explicit **Unrestricted** policy mode that is operator-visible and
   tested as break-glass.
2. Approvals remain exact job/node/effect-hash bound; non-transferable.
3. **Command capability envelope (all supported OS):**
   - **Linux:** bwrap (or Landlock+seccomp equivalent) where workspace is the
     only writable tree; system paths ro as needed for dynamic linker; no full
     root rw bind; network optional and policy-gated; nested breakout tests
     fail closed.
   - **Windows:** documented Job Object + integrity/AppContainer or equivalent
     constrained token **or** explicit product-visible residual with
     fail-closed default that refuses unconstrained shell when
     `WorkIsolationMode` requires confinement.
   - Env sanitisation retained; secret basenames still denied for file effects.
4. Renderer / Electron never grants FS or project authority.
5. SSRF and redirect checks remain for browser tools; add **shared egress
   policy hooks** for browser/search at least (provider TLS may remain adapter-
   local if documented).
6. Security map has **zero “known structural hole”** without either a closed
   fix or a tested Unrestricted break-glass path.
7. Adversarial test pack: path escape, grant replay, skill over-grant, nested
   process escape, symlink/junction, project root swap after approve.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| S1 | ADR: command capability envelope + Unrestricted break-glass | ADR |
| S2 | Linux bwrap profile: workspace rw, minimal ro system, deny full-root rw | runtime tests (incl. write-outside-workspace fails) |
| S3 | Windows confinement path or fail-closed settings mode | runtime tests / cfg |
| S4 | Product settings: isolation modes map to envelope strength | settings + docs |
| S5 | Nested systemd-run / breakout regression remains red for attacker | cancellation/security tests |
| S6 | Shared egress allow/deny helper for browser + search | unit tests |
| S7 | Re-grade Multi-agent to S+++ if interim S from P10 | marks |
| S8 | Security map + marks → Security **S+++** | docs |

### Hold suite

- Approvals surface, path confinement, kernel turn security cases
- Desktop project scope token tests
- Gateway/desktop loopback token rules unchanged

### Out of scope for P12

- Full local MCP host sandboxing (future product)
- Encrypting all credentials on Linux (document residual if still platform-
  limited; only blocks S+++ if it is a *runtime authorization* hole—prefer
  separate credential grade note rather than blocking FS S+++)

---

## P13 — Domain modularity (A- → S+++)

**Owner packages:** `optimus-packs`, `optimus-memory`, `optimus-skills`,
`optimus-store`, any accidental kernel dual catalogs.

### Why still A-

Deep modules exist, but adversarial review still looks for: second tool
catalogs, memory/skills/session confusion, store schema ownership leaks into
surfaces, packs policy bypassed by kernel special cases.

### Adversarial S+++ criteria

1. **Single ToolDesc authority:** only `optimus-packs` defines tool identity;
   kernel/dispatch only resolves advertised `ToolId`s.
2. **Memory planes remain separate** with tests that forbid using Engineering
   Memory, session rows, or skills as authorization for host effects.
3. Skills cannot expand declared permissions; grant paths class-scoped.
4. Store owns Work Graph projections only; no chat UI schema in store.
5. Pack activation cannot authorize sibling tool calls in the same model step
   (existing invariant—keep gate tests).
6. Public module docs + ownership map list exact crate boundaries; EM tool
   registry matches packs available set.
7. No “god” helpers in surfaces that reimplement pack validation.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| D1 | Audit kernel for duplicate tool schema / ad-hoc tool names | grep + fix or document |
| D2 | Pack budget + availability golden tests as merge-adjacent gate | packs tests / script |
| D3 | Memory plane separation tests (session ≠ memory ≠ skills ≠ EM) | kernel/memory tests |
| D4 | Skill permission ceiling fuzz/table tests | skills + runtime |
| D5 | Ownership map + system-overview domain table → Domain **S+++** | done — ownership map + system-overview Domain modularity (P13) + marks |

### Hold suite

- Packs budget tests, skills lifecycle, metamemory MVP

---

## P14 — Observability / eval (A- → S+++)

**Owner packages:** `optimus-eval`, kernel trace/execution (or peeled), CLI
trace/eval, gate scripts.

### Why still A-

Offline integrity + causal reconstruction exist; no OTel export; multi-DB
identity is reconciled not transactional; live-effect replay is intentionally
out of scope.

### Adversarial S+++ criteria

1. Every terminal turn has reconstructible causal chain from durable stores
   (`optimus trace show` / `load_causal_turn`) including security denials when
   classifiable.
2. Offline integrity suite remains a **merge gate** for kernel/runtime/packs.
3. **Export path:** versioned OTLP/JSON export of trace spans **or** an
   explicitly documented “local-only S+++” with machine-readable export of the
   same causal graph (choose one in ADR; must be deterministic and redacted).
4. Execution manifest ↔ root trace binding remains atomic on turn start;
   interrupted resume validates identity.
5. Eval reports remain fail-closed on hash/metric/trace requirements.
6. No log-only authority: tests prove reconstruction without stderr logs.
7. Document honestly: fixture replay does not re-run live providers (not a
   hole if stated and gated).

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| O1 | ADR: export format (OTLP vs local causal export) | ADR |
| O2 | Implement export CLI + redaction | CLI tests |
| O3 | Strengthen observability gate for new export invariants | `check-observability-gate.py` |
| O4 | Causal tests for denial codes + cancel terminals | causal_trace tests |
| O5 | Marks → Observability **S+++** | docs |

### Hold suite

- `check-observability-gate.py`, eval compare/report offline paths

---

## P15 — UI architecture (A- → S+++)

**Owner packages:** `optimus-electron`, `optimus-ui`, `optimus-desktop` host,
IPC matrix scripts.

### Why still A-

Default Electron+React shell and IPC matrix exist; residual risk is registry
drift, legacy Wry dual paths, preview vs agent browser confusion, incomplete
critical-method coverage, presentation state mistaken for authority.

### Adversarial S+++ criteria

1. One default install story; legacy Wry optional and not required for daily
   path tests.
2. **IPC matrix gate:** host registry ⊇ Electron allowlist = React
   `DesktopMethod` types; fails CI when drifted.
3. Critical methods covered: sessions, chat stream/cancel, approval resolve,
   project scopes, approvals, fs, settings, doctor, vertical/status surfaces
   that exist.
4. Preview `WebContentsView` remains sandboxed and **not** agent browser;
   product copy and contracts stay distinct.
5. Renderer cannot mint project roots or approvals.
6. Stream cancel honesty: Stop signals cooperative token; UI shows terminal
   from host, not optimistic local success.
7. Native UI skill path remains for install verification when claiming shell
   changes.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| U1 | Expand IPC matrix to 100% host methods or explicit `legacy_only` tags | matrix script |
| U2 | React/Electron e2e for approval resolve + project scope deny | e2e |
| U3 | Preview security tests (no file, no node integration) | electron tests |
| U4 | Install script truth matches marks (Electron primary) | rebuild script review |
| U5 | Marks → UI **S+++** | docs |

### Hold suite

- `check-desktop-ipc-matrix.py`, electron allowlist tests, desktop e2e subset

---

## P16 — Doc / claim hygiene (A- → S+++)

**Owner packages:** docs only (+ EM refresh). Run **after** P10–P15 behaviour
settles; may do light fixes earlier but final S+++ is here.

### Why still A-

Status legends are strong; known debt remains (duplicate ADR `0016`, ownership
map specialist line, scorecard lag vs system-overview).

### Adversarial S+++ criteria

1. system-overview, architecture-marks, sota-scorecard shell truth, install
   scripts, and this plan **agree**.
2. Planned never graded Confirmed; every architecture debt item in
   system-overview is either closed or explicitly residual with owner phase.
3. Duplicate ADR numbers resolved (renumber or alias with redirects).
4. Ownership map matches crate graph post-P11.
5. EM `report` shows `stale_documents: 0` after refresh.
6. Blueprint docs (`optimus-exceeds-hermes.md` etc.) carry status banners where
   they mix plan vs current.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| H1 | Fix ADR 0016 collision | decisions/ |
| H2 | Refresh ownership map specialists + crate list | maps/ |
| H3 | Align sota-scorecard dated claims with system-overview | scorecard |
| H4 | Banner pass on historical phase notes | architecture/ |
| H5 | EM generate + validate; marks → Doc **S+++** | EM + marks |

### Hold suite

- No behaviour change required; `engineering_memory.py check` clean

---

## P17 — Release / parity gating (A → S+++)

**Owner packages:** `scripts/optimus_version.py`, `check-parity-ledger.py`,
version JSON, CI docs.

### Why still A

Fail-closed gates exist; S+++ means the gate system is complete, documented,
and cannot be “greenwashed” by partial evidence.

### Adversarial S+++ criteria

1. Version gate and parity ledger gate remain fail-closed.
2. **Architecture marks gate:** script fails if marks claim S+++ without the
   phase checklist file sections marked done (or without required test path
   existence)—lightweight, not a full proof assistant.
3. Ledger rows cannot be `win`/`parity` without evidence path + trajectory
   (already true—keep tests).
4. Architecture S+++ explicitly does **not** require full Hermes parity;
   release notes templates state that.
5. Single operator doc: which gates run pre-merge vs pre-release.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| R1 | Document pre-merge vs pre-release gate matrix | docs/architecture or plans |
| R2 | Optional `check-architecture-marks.py` for S+++ claim hygiene | script + tests |
| R3 | Version/ledger tests remain green | existing scripts |
| R4 | Marks → Release **S+++** | marks |

---

## P18 — Durability / crash safety (A+ → S+++)

**Owner packages:** `optimus-store`, `optimus-graph`, `optimus-runtime`,
session coupling, campaign leases.

### Why still A+ (not S+++)

Core Work Graph durability is excellent. Residuals for adversarial S+++:

- Multi-DB homes lack unified backup/migration/retention story.
- Stream delivery loss can cancel after a durable effect commits but before
  transcript save (repair-on-open helps; prove all paths).
- External exactly-once delivery (gateway off-box) unresolved—must either
  implement local exactly-once claims fully documented or scope S+++ to
  **process-local / local SQLite** durability only.

### Adversarial S+++ criteria

1. Crash at any phase → exactly one terminal outcome (jobs, campaigns,
   workflow runs, agent invocations).
2. Effect ↔ session coupling: transaction **or** deterministic repair-on-open
   for all durable tool effect links (prove with chaos tests).
3. Resume never invents success for `running` / ambiguous command attempts.
4. Process-tree ownership verified empty before settlement (Unix + Windows).
5. **Operator durability contract:** documented backup set of DB files +
   `optimus doctor` checks for schema version skew / quarantine.
6. Scope statement: external messaging exactly-once is **out of S+++ scope**
   unless implemented; local gateway leases remain Confirmed.
7. Chaos/property tests: kill mid-node, double-cancel, lease expiry takeover.

### Microtasks

| ID | Task | Exit evidence |
|---|---|---|
| Y1 | Doctor: multi-DB schema/version inventory + quarantine report | CLI tests |
| Y2 | Backup/restore runbook + optional `optimus doctor backup-list` | docs + CLI |
| Y3 | Chaos tests: kill during WriteFile/RunCommand/campaign step | runtime tests |
| Y4 | Workflow run + agent invocation crash matrix (post-P10) | agent/workflow tests |
| Y5 | Session repair coverage for all durable tool kinds | session_resume |
| Y6 | Marks → Durability **S+++** | marks |

### Hold suite

- Full runtime suite, campaign tests, cancellation tests

---

## P19 — Final all-marks S+++ review board

**Purpose:** adversarial pass after P10–P18. No new features.

### Checklist

1. Every row in `architecture-marks.md` is **S+++** with notes pointing at
   exit evidence.
2. Re-run hold suites for all dimensions in one report under `local/tmp/`.
3. Re-read security map and system-overview debt list: no unowned structural
   holes.
4. EM `report`: stale_documents=0; agent/tool counts match claims.
5. Install story + IPC matrix + observability + version/parity gates green.
6. Optional external reviewer (human) sign-off recorded in
   `docs/evidence/s-plus-plus-plus-review-YYYY-MM-DD.md`.

### Failure handling

If any mark fails adversarial review: **do not** keep S+++. Open a patch phase
`P19.x` owned by that dimension only; re-enter review board after green.

---

## Cross-cutting hold suites (run at every phase exit)

| Suite | Command / location |
|---|---|
| Runtime safety | `cargo test -p optimus-runtime` |
| Kernel turn + session | `cargo test -p optimus-kernel` |
| Packs | `cargo test -p optimus-packs` |
| Observability gate | `python3 scripts/check-observability-gate.py` |
| IPC matrix | `python3 scripts/check-desktop-ipc-matrix.py` |
| Parity/version (release hygiene) | `python3 scripts/check-parity-ledger.py` / `optimus_version.py gate` as applicable |
| EM | `python3 scripts/engineering_memory.py check` |

Exact CI wiring may lag; local green is required before mark moves.

---

## Suggested calendar shape (indicative, not a commitment)

| Phase | Focus | Rough effort band |
|---|---|---|
| P10 | Multi-agent platform | L (multiple verticals + DAG) |
| P11 | Crate peels | L (mechanical + dep lint) |
| P12 | Command FS envelope | L (OS-specific, high risk) |
| P13 | Domain audit | S–M |
| P14 | Export + obs gate | M |
| P15 | UI/IPC completeness | M |
| P16 | Doc hygiene | S–M |
| P17 | Release gates | S |
| P18 | Durability chaos + doctor | M |
| P19 | Review board | S |

L = multi-PR; M = one or few PRs; S = short.

---

## Relationship to other plans

| Plan | Role |
|---|---|
| [s-plus-trust-spine.md](./s-plus-trust-spine.md) | **Done** foundation Phases 0–5 |
| **This plan** | **Active** architecture S+++ Phases P10–P19 |
| [full-app-microtasks.md](./full-app-microtasks.md) | Product surface; yields to architecture sequencing on conflicts |
| [engineering-memory-phases.md](./engineering-memory-phases.md) | EM system evolution; use lenses each phase |

---

## Immediate next action

**P10–P13 done** (multi-agent / control-plane / security / domain → **S+++**).
Next: **P14** observability / eval (export path + gate strength).
