---
doc_id: architecture-versioning-parity
doc_type: explanation
plane: current
status: current
authority: canonical
summary: Versioning, release/parity gates, SOTA scorecard, and honest current status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: architecture
owns:
  - scripts/tools/optimus_version.py
  - scripts/gates/check-parity-ledger.py
  - scripts/gates/check-architecture-marks.py
---

# Versioning and parity

## Versioning

**Status:** active, fail-closed  
**Optimus product version:** `0.1.0`  
**Tracked Hermes target:** `0.19.0` at upstream revision `8967e73e`  
**Verified Hermes parity version:** none

## Why there are two versions

Optimus has an independent product version and a separate Hermes parity version.
They answer different questions:

- **Optimus product version** is normal SemVer from `Cargo.toml`.
- **Hermes target version** is the exact Hermes release currently being audited.
- **Hermes parity version** is `null` until every gate in this document passes for one immutable Optimus revision.

A normal Optimus development release may use any honest independent version. If
its three-part numeric SemVer core equals the tracked Hermes number, the release
check refuses it unless the Hermes parity claim is verified. Prerelease or build
suffixes do not disguise that collision. This prevents an accidental or
marketing-only numerical match.

Example while work is incomplete:

```text
Optimus Agent 0.1.0
Hermes target: 0.19.0
Hermes parity: unverified
Frozen Hermes feature contracts: 2063
```

## Non-negotiable parity invariant

Optimus may claim `Hermes parity: X.Y.Z` only when the exact candidate:

1. implements or strictly exceeds **every** feature contract frozen from Hermes
   `X.Y.Z`;
2. has executable, revision-bound evidence for each feature contract;
3. has no `missing` or `partial` row in the human parity rollup;
4. matches or beats Hermes success rate and deterministic quality score;
5. matches or beats Hermes p50 and p95 wall latency and time-to-first-token;
6. matches or beats Hermes cost per successful task and peak resident memory;
7. passes the required comparison scenarios on the same machine, model,
   provider, permissions, and paired randomized task order;
8. uses fresh evidence from a clean, immutable Optimus revision; and
9. has a completed audit against the official Hermes documentation.

There are **no feature waivers**. An equivalent Optimus design is allowed, but
it must prove the same user-visible outcome and edge behavior. A missing Hermes
feature cannot be traded for an unrelated Optimus advantage.

## Sources of truth

| File | Purpose |
|---|---|
| `docs/architecture/optimus-version.json` | Version target, claim, release rules, and benchmark thresholds |
| `docs/architecture/hermes-baselines/hermes-0.19.0.json` | Frozen machine inventory for Hermes 0.19.0 |
| `docs/architecture/hermes-manual-capabilities.json` | Non-CLI product capabilities curated from official docs/source |
| `docs/architecture/hermes-feature-evidence.json` | Per-feature Optimus evidence bound to a commit |
| `docs/architecture/hermes-performance-evidence.json` | Raw paired benchmark samples and protocol provenance |
| `docs/architecture/parity-capability-ledger.json` | Human-readable capability rollup and ownership |
| `scripts/tools/optimus_version.py` | Capture, validation, status, release, and promotion gate |
| `scripts/gates/check-parity-ledger.py` | Rollup validation plus version-system integrity check |

Executable evidence outranks prose. Architecture documents are not parity
proof unless a claim also names a passing trajectory and an existing evidence
artifact.

## Frozen Hermes inventory

The v0.19.0 baseline contains **2,063 distinct contracts** and has SHA-256:

```text
cafbcf313b4fbd7885b4df9b888a2539885d8d62ec55e6df1cf88dc0e66cf725
```

It inventories:

- recursively discovered CLI commands and options;
- slash commands, aliases, and subcommands;
- toolsets and statically registered tools;
- provider catalog entries;
- bundled messaging platforms; and
- non-CLI capabilities from the official product surface.

The source capture is tied to official commit `8967e73e`, not to the locally
modified Hermes checkout. Normalized ID collisions are retained as independent,
deterministically suffixed contracts; capture never drops one silently. MCP
server tool names are intentionally dynamic and unbounded, so the frozen
contract covers MCP client/server behavior rather than arbitrary third-party
runtime names.

The machine capture has zero warnings. The separate official-documentation
inventory audit remains `pending`, so parity is blocked even if someone were to
populate evidence prematurely.

## Per-feature evidence contract

`hermes-feature-evidence.json` maps each frozen feature ID to a claim. A passing
claim has this shape:

```json
{
  "cli.command.example": {
    "status": "verified",
    "evidence": ["path/to/current/test-or-report"],
    "trajectory": "cargo:package/test-name",
    "verified_at": "2026-07-23T12:00:00Z",
    "optimus_revision": "40-character-git-commit-sha"
  }
}
```

Rules:

- Every baseline ID must be present and `verified`.
- Evidence paths must exist.
- A named executable trajectory is mandatory.
- Evidence older than 30 days does not pass.
- All feature claims must refer to the same clean Optimus revision.
- Unknown IDs are schema errors, not ignored extensions.

`missing`, `partial`, `not-applicable`, `waived`, and prose-only evidence never
pass the parity gate.

## Comparative performance contract

The performance report stores raw paired samples. It does not accept manually
entered aggregate claims. Every required scenario needs at least 30 paired
samples across at least three distinct seeds:

1. cold start;
2. single-turn response;
3. multi-tool turn;
4. long session;
5. session resume;
6. scheduled job;
7. browser task; and
8. delegated task.

Each sample contains `hermes` and `optimus` records with `success`, a
reproducible `quality_score`, and the metrics required by that scenario.
The gate recomputes all statistics.

Hard thresholds:

| Axis | Requirement |
|---|---|
| Success rate | Optimus ≥ Hermes |
| Deterministic quality | Optimus ≥ Hermes |
| Wall time p50 and p95 | Optimus / Hermes ≤ 1.0 |
| TTFT p50 and p95 | Optimus / Hermes ≤ 1.0 |
| Cost per successful task | Optimus / Hermes ≤ 1.0 |
| Peak RSS p50 and p95 | Optimus / Hermes ≤ 1.0 |

The report must also affirm same machine, same model, same provider, same tool
permissions, and randomized paired order. It must hash the dataset, deterministic
grader, benchmark harness, Hermes binary, and Optimus binary, identify the
machine/provider/model, and record each sample's case ID, seed, and execution
order. Both `hermes-first` and `optimus-first` samples are required. Evidence is
valid for 30 days and must target the exact Hermes baseline and Optimus commit.

## Commands

```bash
# Human and machine-readable status
python3 scripts/tools/optimus_version.py status
python3 scripts/tools/optimus_version.py status --json

# Structural integrity; incomplete parity is reported but is not an error
python3 scripts/tools/optimus_version.py validate

# Strict full-parity gate; expected to fail until all work is complete
python3 scripts/tools/optimus_version.py gate

# Release preflight. Development versions pass; false matching claims fail.
python3 scripts/tools/optimus_version.py release-check

# Existing rollup plus version-system integrity
python3 scripts/gates/check-parity-ledger.py

# Architecture S+++ claim hygiene (not Hermes product parity)
python3 scripts/gates/check-architecture-marks.py

# Record parity only after all blockers are gone
python3 scripts/tools/optimus_version.py promote --reviewer "reviewer identity"

# Built CLI status
optimus version
optimus version --json
```

Both `scripts/rebuild-install-relaunch.sh` and
`scripts/rebuild-install-relaunch.ps1` run `release-check` before build/binary
selection, then run it again and revalidate both selected binary versions
immediately before stopping or replacing an installed application. Their
`VERSION.txt` and `install-meta.json` record the target, parity value, claim
status, and frozen feature count.

## Capturing a clean Hermes baseline

Never capture from a dirty or locally patched Hermes tree. Use an exact detached
worktree and the installed Hermes virtualenv only as the dependency runtime:

```bash
source_repo="$HOME/.hermes/hermes-agent"
clean_source="$(mktemp -d /tmp/optimus-hermes-0.19.0-XXXXXX)"
git -C "$source_repo" worktree add --detach "$clean_source" 8967e73e
python3 scripts/tools/optimus_version.py capture-hermes \
  --hermes-source "$clean_source" \
  --hermes-python "$source_repo/venv/bin/python"
git -C "$source_repo" worktree remove --force "$clean_source"
```

Capture updates the baseline hash in the version manifest and both evidence
files. Existing evidence is therefore invalidated whenever the baseline bytes
change.

## When Hermes publishes a new version

1. Update `hermes_target` to the new exact version, release date, and upstream
   revision.
2. Reset `parity_claim` to `unverified` with null metadata.
3. Capture a clean baseline from that exact revision.
4. Re-audit the official docs and mark the inventory audit complete only after
   resolving every discrepancy.
5. Add evidence for every new or changed feature contract.
6. Re-run all paired comparison scenarios on one immutable Optimus revision.
7. Run `validate`, the repository test suites, `gate`, and `release-check`.
8. Use `promote` only after the gate has no error or blocker.

A previously verified older Hermes parity version may remain historical, but it
must not be presented as parity with the newly tracked release.

## Current honest status

Optimus `0.1.0` tracks Hermes `0.19.0`, but parity is **unverified**:

- feature contracts verified: `0 / 2063` under the new strict per-feature schema;
- rollup rows below parity: `37 / 51`;
- required performance scenarios passing: `0 / 8`;
- official-documentation inventory audit: pending.

This is intentional. The version system exists to prevent the number from
advancing ahead of the product and evidence.


## Release and parity gates

Date: 2026-07-25  
Planes: program **P17** · grade mark **Release / parity gating** · delivery **PR #27**

**Status:** Confirmed operator contract for merge hygiene vs product release.  
Architecture **S+++** for this mark does **not** require full Hermes parity
(2,063 feature contracts). It requires that gates remain **fail-closed**, that
operators know which command is merge-safe vs release-blocking, and that grade
claims cannot greenwash missing phase evidence.

## Two version questions (do not collapse)

| Question | Answer lives in | Green means |
|---|---|---|
| May I ship an ordinary Optimus development build? | `optimus_version.py release-check` | Product SemVer is honest and does not falsely equal Hermes without a verified claim |
| May I claim Hermes parity `X.Y.Z`? | `optimus_version.py gate` + evidence corpus | Every frozen feature, rollup row, and performance scenario is proven on a clean revision |

Full policy: the merged Versioning section above.

## Gate matrix

| Gate | Command | Pre-merge (PR / local) | Pre-release (ship binary / install) | Pre-parity-claim | Notes |
|---|---|:---:|:---:|:---:|---|
| Version structural integrity | `python3 scripts/tools/optimus_version.py validate` | ✅ | ✅ | ✅ | Incomplete parity is reported; structural errors fail |
| Development release honesty | `python3 scripts/tools/optimus_version.py release-check` | ✅ | ✅ (required by installer) | ✅ | Passes independent SemVer; blocks numeric Hermes collision without verified claim |
| Strict Hermes parity | `python3 scripts/tools/optimus_version.py gate` | ❌ (expected red until complete) | ❌ unless claiming parity | ✅ required | Fail-closed; **not** an architecture S+++ blocker |
| Parity ledger rollup | `python3 scripts/gates/check-parity-ledger.py` | ✅ | ✅ | ✅ | Evidence paths must exist; `parity`/`win` need trajectory |
| Architecture marks claim hygiene | `python3 scripts/gates/check-architecture-marks.py` | ✅ | ✅ | optional | Fails if a mark is graded **S+++** without done phase / required paths |
| Observability | `python3 scripts/gates/check-observability-gate.py` | ✅ when touching kernel/runtime/packs/eval | recommended | optional | Cargo integrity + causal/export surface |
| Surface contract | `python3 scripts/gates/check-surface-contract.py` | ✅ when touching desktop/tauri/ui | recommended | optional | Wire set ⊇ renderer union; schema + registry dump pinned (spec-015 A5) |
| Domain modularity | `python3 scripts/gates/check-domain-modularity.py` | ✅ when touching packs/kernel/store | recommended | optional | Single `ToolDesc` catalog / plane separation |
| Crate layers | `python3 scripts/gates/check-crate-layers.py` | ✅ when touching crate graph | recommended | optional | Control-plane peel deps |
| Engineering Memory | `python3 scripts/tools/engineering_memory.py check` (+ `generate` / `validate` when stale) | ✅ | ✅ | optional | Generated maps must not be hand-edited |
| Runtime / pack hold suites | `cargo test -p optimus-runtime` / `optimus-kernel` / `optimus-packs` as touched | ✅ scoped | full workspace before major ship | optional | See program hold suites |
| Installer re-gate | `scripts/rebuild-install-relaunch.*` | n/a | ✅ | n/a | Runs `release-check` before binary selection and again before replace |

Legend: ✅ expected green for that class of change · ❌ not required (and often red by design).

## What architecture S+++ does **not** require

- Full Hermes feature inventory green (`gate` PASS).
- Every parity-ledger row at `parity` or `win`.
- Performance scenario suite complete.
- Marketing claim that Optimus “is” Hermes `X.Y.Z`.

Those remain **product parity** work under `program:parity` / version promote /
Track Z after product-complete. The Release mark grades the **gate system**
(fail-closed scripts, docs, claim hygiene), not product completeness.

**Sources of truth (do not collapse):**

| Question | Authority |
|---|---|
| Architecture mark exits / hold | [architecture-marks.md](../runbooks/architecture-marks.md); history s-plus-plus-plus-program.md (atticked) (P10–P19 done) |
| Daily-app phase exits → PRODUCT-COMPLETE | product-complete-program.md (atticked) (program P20–P29) + ledger |
| Merge vs ship vs Hermes claim | this matrix (`release-check` vs `gate`) |

## Operator quick paths

### Before opening / merging a PR

```bash
python3 scripts/tools/optimus_version.py release-check
python3 scripts/gates/check-parity-ledger.py
python3 scripts/gates/check-architecture-marks.py
python3 scripts/tools/engineering_memory.py check
# plus dimension gates for the files you touched
```

### Before install / ship of a development build

```bash
python3 scripts/tools/optimus_version.py release-check
python3 scripts/gates/check-parity-ledger.py
# installer scripts re-run release-check around binary selection
```

### Only when claiming Hermes parity

```bash
python3 scripts/tools/optimus_version.py validate
python3 scripts/gates/check-parity-ledger.py
python3 scripts/tools/optimus_version.py gate          # must PASS
python3 scripts/tools/optimus_version.py release-check
python3 scripts/tools/optimus_version.py promote --reviewer "…"
```

## Sources of truth

| Artifact | Role |
|---|---|
| `docs/architecture/optimus-version.json` | Product version, Hermes target, claim, release rules |
| `docs/architecture/parity-capability-ledger.json` | Human rollup (51 rows); not the 2,063-feature gate |
| `docs/architecture/architecture-marks.md` | Architecture quality grades (S+++ climb) |
| `docs/plans/s-plus-plus-plus-program.md` | Phase exit criteria; P17 owns this matrix |
| `scripts/tools/optimus_version.py` | Capture / validate / gate / release-check / promote |
| `scripts/gates/check-parity-ledger.py` | Rollup + version integrity |
| `scripts/gates/check-architecture-marks.py` | S+++ claim ↔ phase done / path existence |

## Related verification

- s-plus-plus-plus-p17-verification.md (atticked)
- sota-scorecard (parity planning rollup, not architecture grades; merged above)


## SOTA scorecard

Updated: 2026-07-28 · thesis-axis re-key (north-star C-criteria); 13/50 runnable trajectories, unclassified pinned at 37 shrink-only; projects.scope+updater+pty+native-cua partial

**Status banner:** This scorecard is a **parity/planning rollup**, not the
architecture quality grade sheet. For modular architecture grades (S+++ climb)
see [architecture-marks.md](../runbooks/architecture-marks.md). For current topology and
Confirmed behaviour see [system-overview.md](../architecture.md).

**Default product shell (Confirmed):** Tauri + React over Rust host
(exclusively — no Electron, no Wry rollback since 2026-08-05, spec-012).
The block below is the dated 2026-07-28 pre-cutover snapshot: treat its
Electron/Wry default-shell claims as historical, not current — do not
read them as the default install path.

**Source of truth:** `docs/architecture/parity-capability-ledger.json`  
**Validator:** `python scripts/gates/check-parity-ledger.py`  
**Rule:** executable current-repository evidence outranks architecture blueprints and historical phase prose. A `parity` or `win` row requires an existing evidence path; every row's trajectory is either runnable (`cargo:`/`playwright:`, resolved to a real target by the validator) or pinned on the validator's shrink-only unclassified list.
**Release-version gate:** `docs/architecture/optimus-version.json` plus `python scripts/tools/optimus_version.py gate`. The 50 rows below are a planning rollup, not sufficient for a product-level Hermes parity claim. The strict v0.19.0 baseline contains 2,063 per-feature contracts.

## Current ledger summary

| State | Count | Meaning |
|---|---:|---|
| **win** | 4 | Current executable evidence demonstrates a structural advantage over Hermes |
| **parity** | 41 | A bounded Hermes-equivalent capability has current executable evidence |
| **partial** | 4 | Useful implementation exists, but the Hermes behavior/surface is incomplete |
| **missing** | 1 | No complete executable path exists yet |
| **total** | 50 | Capability rows tracked by the executable ledger |

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
- Electron + React default desktop shell (Wry legacy optional)
- Sandboxed Files list/read
- Bounded terminal job stream
- Sequential durable write/command campaigns
- Deterministic offline eval suite
- Store-backed causal reconstruction + local export (`optimus.causal.v1`)
- Fail-closed tool ads↔handler registry + progressive pack schema budget (program P21)
- Files mutate under SmartDeny (mkdir/rename/delete/patch + write) (program P22)
- Coordinated preview + agent browser under ADR-0040 (not shared CDP session) (program P23)
- Web search versioned extract + provenance URL (program P23)
- Annotation gallery + explicit Add to prompt (program P23)
- HTTP browser SSRF without CDP (program P23)
- Thinking blocks separate from assistant text + timed tool lifecycle cards (program P24)
- Session FTS, archive/unarchive, durable pins + sort (program P24)
- Memory FTS: free-text claim recall (`memory_search`) with per-hit standing/provenance, no new dependency (ADR-0072)
- Artifacts gallery, filters, export + bulk zip (program P25)
- Cron create/pause/resume/remove/history workbench (program P25)
- Skills/memory/packs consoles + redacted logs + command palette (program P26)
- Gateway outbox receipts, ambiguous-send recovery, mock Telegram adapter, messaging UI (program P28; external EO residual)
- Provider catalog + ordered failover, pack-gated MCP mock, signed packs (program P27)
- Product ship path: Electron install default, doctor shell/isolation/gateway/packs, ADR-0043 no auto-updater (program P29)
- S7: profile homes, leased child agents, CUA pack scaffold, Hermes importers
- Track Z: offline comparative runner, surface/media/breadth scaffolds, Discord/Slack mock adapters

## Material partials

- Installed native paint/accessibility (`desktop.native-cua`): the PF-00 installed-app CUA baseline is not committed, so the row carries no evidence. Playwright covers paint/layout supplementally and does not substitute for installed-app proof (see `skills/optimus-native-ui-testing`). Regenerate PF-00 to a tracked path to restore the parity claim.
- Project isolation honesty (configured vs enforced) with concurrent multi-project mutate lease residual
- Release updater: no in-app signed auto-update channel (ADR-0043); reinstall script is the upgrade path
- Terminal PTY: Linux multi-tab session store scaffold; full interactive I/O residual

## Leading product losses

1. Hermes strict parity gate (2063 contracts + performance scenarios) still unverified
2. Live multi-tab ConPTY I/O product UI
3. Live computer-use effectors under heavy approval
4. Live Discord/Slack bot transports (mock enqueue only)

## Current architecture truth

- **Default installed desktop:** Electron + React workbench over Rust `optimus-desktop --host-only` (ADR-0028). Not Tauri.
- **Legacy rollback:** tao + wry native shell (WebKitGTK / WebView2) via install “Legacy Wry” action.
- Native Wry IPC: ADR-0014 custom-protocol path; host HTTP mode is a test / Electron transport path.
- Browser: agent `browser_*` effector (HTTP SSRF-safe; CDP when available) is separate from the Electron sandboxed preview `WebContentsView`.
- Artifacts: content-addressed store under `{home}/artifacts` with gallery/filters/export under `exports/` (program P25)
- Campaigns: sequential WriteFile/RunCommand plus leased child-agent coordinator (S7)
- Gateway: SQLite authority + config-gated live Telegram long-poll (`optimus gateway telegram run`) + Telegram mock + Discord/Slack mock enqueue (live Discord/Slack residual)
- Retrieval: two SQLite FTS5 lexical indexes (`sessions_fts`, `claims_fts`); the claim index narrows only — every hit is re-authorized against `claims` and labelled with its bitemporal standing (ADR-0072). No vector, embedding, graph, reranking, or GPU index.
- Capabilities: PRODUCT-COMPLETE + S7/Track Z scaffolds; Hermes gate not claimed
- Architecture quality marks: [architecture-marks.md](../runbooks/architecture-marks.md) (S+++ program)

## Baseline commands of record

```bash
just verify
```

`just verify` runs every gate above through `scripts/verify.sh`, the single
source of truth shared by the justfile, managed land, humans, and coding agents.
Narrower tiers: `just gates` · `just check` · `just test` ·
`just ui`.

The Hermes parity gate is deliberately excluded from `just verify` because it is
fail-closed by design; run it with `just parity`.

PF-00 baseline evidence: **absent**. The installed-app CUA baseline has never
been committed, which is why `desktop.native-cua` is `partial`. Regenerate it to
a tracked path (not gitignored `local/`) to restore the parity claim.

## Honest statement

Optimus has evidence-backed architectural wins and broad product/ecosystem scaffolds. It is **not yet Hermes-strict-parity**: the ledger currently contains 4 partial and 1 missing capability row (packs.breadth re-marked by ADR-0068: breadth claimed through refusing scaffolds was cosmetics, and the scaffolds are gone), but Hermes feature-contract and performance gates remain unverified (optimus-native claims only for a small first batch). The Hermes parity version therefore remains `null`; it cannot become `0.19.0` until the full inventory, comparative, security, cost, durability, packaging, and native-platform gates pass.
