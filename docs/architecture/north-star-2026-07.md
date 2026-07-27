# Optimus Agent — North Star (2026-07)

> **Documentary status:** Current and canonical. Supersedes
> [optimus-exceeds-hermes.md](./optimus-exceeds-hermes.md) (2026-07-18), which
> is retained as history only. Every claim here was settled on the
> [#59 wayfinder map](https://github.com/mustbearnold/Optimus-Agent/issues/59)
> across issues #60–#66; each section links its deciding ticket. Measured
> baseline: [capability-baseline-2026-07.md](./capability-baseline-2026-07.md)
> at tree `14d8f39`. Claim status: statements about current behaviour are
> **Confirmed** against that baseline; the success criteria and the ledger
> transition are **Planned**.

## Thesis

> **Optimus is one durable core with many faces: every surface speaks the same
> contract, and none of them can lose track of which project it's in.**

Decided in [#61](https://github.com/mustbearnold/Optimus-Agent/issues/61). Both
halves were already half-built and never named as the point:

- **Project integrity** — the origin of the project. Hermes Agent muddled
  concurrent projects and switched between them unprompted; that frustration is
  why Optimus exists. Latent in `ProjectAuthorityStore` /
  `optimus-kernel/src/project_authority.rs`, the `optimus-policy` capability
  broker, ADR-0044, and the `work_isolation` / `allow_concurrent_projects`
  settings.
- **One core, many faces** — `crates/optimus-host` is the exact IPC method
  registry every surface speaks (ADR-0045); `handle_ipc` is deliberately
  transport-agnostic. Reaching a new surface needs a transport, not an
  architecture.

Explicitly rejected as the thesis: the measured learning loop (may return as a
feature, never as the point); "faster, more efficient, more accurate, higher
quality" (that is the *bar* every decision is judged against, not the thesis);
durability as the headline (it is *how* multi-project work survives, not *why*
someone picks Optimus).

## What "feels like Hermes" means

Decided in [#64](https://github.com/mustbearnold/Optimus-Agent/issues/64).
Scoped to the interaction loop; the load-bearing moment is the **interrupt**.

- Interrupt is upgraded to a **durable partial turn** — an `optimus-host`
  contract change. An IPC call against durable state, never a signal against a
  live process ([#66](https://github.com/mustbearnold/Optimus-Agent/issues/66)).
  This binds unscoped on every surface. General form: **latency is a surface
  property, capability is a contract property.**
- **Going without asking on ordinary project work stays.** Safety is bounded by
  project scope, not per-call prompts. Default posture is **`Standard`**;
  `FullProject` rejected as default (allows by externality); `/yolo` stays as
  break-glass. Known drift: the shipped `#[default] ReviewChanges` contradicts
  ADR-0044 and is to be fixed, not documented around.
- Dropped under default-deny: auto-skill-creation, per-turn schema resend (the
  latter already answered by progressive pack loading, which shipped —
  `PackBudgetConfig`, `activation_snapshot()`).

"Feels like Hermes" is checkable, not vibes (#64's acceptance checks): an
interrupted run leaves a durable partial turn; an ordinary session completes
with zero approval cards; external, host, and credential effects still raise a
card; no skill is ever created unasked.

## Surfaces

Decided in [#62](https://github.com/mustbearnold/Optimus-Agent/issues/62) and
[#66](https://github.com/mustbearnold/Optimus-Agent/issues/66). **Surface =
entry point, not face.** Anything that can start a turn is a surface.

Four surfaces, no more:

1. **Terminal** — one binary, two faces. Bare `optimus` opens the TUI
   (`apps/optimus-cli/src/main.rs:518`); the CLI is the launcher.
2. **Desktop** — one surface (`optimus-desktop`), two shells; Electron ships,
   native tao/wry is dev/e2e. `optimus-electron` and `optimus-ui` are transport
   clients, not surfaces.
3. **Gateway** — and the gateway **is** the phone. No mobile client ships;
   default path is a long-poll client with no public listen port
   (`crates/optimus-host/src/messaging.rs:139-140`); identity is the fail-closed
   `allowed_chat_ids` allowlist bound to project scope. A native client is
   deferred, not precluded.
4. **Cron** — a surface despite no human at the other end; project scope binds
   it where a prompt cannot.

MCP is egress only; **MCP ingress is ruled out**. Platform rule: **no platform
is claimed without a CI job that builds it.**

## The contract rule

Decided in [#65](https://github.com/mustbearnold/Optimus-Agent/issues/65). An
app in `apps/` may **name** core types but may not **construct or open** core
state. The host is a library — `handle_ipc` already is embedded mode, so
ADR-0045's "embedded mode" exception is withdrawn. `apps/optimus-cli`'s 6
violations sit on a shrinking allowlist; a compile-time seal is **revisited**
once it empties (#65 adopted the ratchet now, not the seal). A surface may serve the contract to another (desktop→Electron):
serving a transport is not owning the core.

## Success criteria

Decided in [#63](https://github.com/mustbearnold/Optimus-Agent/issues/63).
**Hermes is not the yardstick** — it is a design reference (#64) and an import
target (`hermes_import.rs`), nothing more. No criterion's pass/fail may depend
on observing Hermes. The guard against a self-authored bar is structural:

> Every criterion is either **failing the day it is written** or a **monotone
> counter with a named enforcing script**. Green-at-authoring with no counter is
> banned.

| # | Criterion | Check | State at authoring |
|---|---|---|---|
| C1 | Five projects in one core, zero bleed; project selection changes only via explicit IPC call, never as a turn side effect | integration test, ratchets 1→5 | red — no test references `ProjectScope` |
| C2 | Every host method carries a project-scope assertion | script counter 0→82 | red — gate doesn't exist |
| C3 | One core per home — a second surface probes and attaches, never spawns | test + probe path | red — `apps/optimus-electron/main.cjs:213` always spawns |
| C4 | All 82 host methods classified across all four surfaces (`unclassified → 0`); the 22 critical methods may never be N/A on a human-facing surface | generalised `check-desktop-ipc-matrix.py` | red — terminal reaches 8 of 22 |
| C5 | `apps/` layering allowlist shrinks to zero | `check-crate-layers.py` extended to `apps/` | red — no `apps/` coverage yet |
| C6 | CI green at the SHA; one workflow runs `just verify` on push + PR; **a skipped gate fails the build** | `.github/workflows/verify.yml` | red — zero workflows exist |

C6 is the prerequisite: until it lands, C1–C5 are claims, and the platform rule
claims no platforms at all. Sequence: C6 → C5 + C2 → ledger re-key → C1 + C3.

C4 rider: the durable-partial-turn interrupt (#64) is a registry method and
must appear in this matrix as reachable on **all four surfaces** — #66 binds it
unscoped.

**Not criteria** (and why): parity-plus surface (dies with the yardstick),
learning loop (rejected by #61), memory (demoted — own ticket), economics (no
baseline without Hermes; progressive loading already shipped), durability
(supporting, already evidenced), security (it *is* C1+C2 per ADR-0044),
Windows (governed by the CI-job rule), per-capability benchmarks (survives only
as the runnable-trajectory requirement).

**Parity ledger transition:** `parity-capability-ledger.json` is re-keyed, not
retired — `hermes_reference` → `thesis_axis`, trajectories must be runnable
(13 of 51 were at decision time), landing green with the unclassified count
pinned at 37 — #63 measured 38, one of which was the deleted `eval.comparative`
row — and only able to shrink. `crates/optimus-eval/src/comparative.rs` and the
`eval.comparative` row are deleted; `migration.hermes` stays.

## Disposition of prior documents

- `optimus-exceeds-hermes.md` — superseded by this document; kept as history,
  not rewritten.
- The 49 ADRs — left standing as history. Load-bearing for this document:
  ADR-0044 (bounded project trust) and ADR-0045 (host contract). Where an ADR
  conflicts with a decision here, the ticket linked here wins (e.g. ADR-0045's
  embedded-mode exception, withdrawn by #65).
- `CONTEXT.md` — still describes Phase 0; redrawn when the delivery-order
  ticket lands, not before.
- `optimus-graph` — the widest contract in the workspace (8 consumers) at 537
  lines. The baseline asked whether that is waist discipline or an under-built
  centre; no ticket decided it, so the question is handed to the
  delivery-order effort (#67) rather than answered here.

## Out of scope

Building anything this document names — that is the delivery-order effort's
job. Hermes Agent / Hermes Next code is off-limits per AGENTS.md; read-only
inspection of `~/.hermes` is permitted for import compatibility only.
