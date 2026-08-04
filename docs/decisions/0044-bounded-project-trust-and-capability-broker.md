---
doc_id: decisions-0044-bounded-project-trust-and-capability-broker
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-26 - Program: program P30
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-policy/src/command_class.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/policy_bridge.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/project_trust.rs
  - crates/optimus-kernel/src/dev_run.rs
  - crates/optimus-host/src/chat.rs
  - apps/optimus-ui/src/components/workbench/Composer.tsx
depends_on:
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
validated_by:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-policy/tests/command_classification.rs
  - crates/optimus-kernel/tests/dev_run_trust.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
---

# ADR-0044: Bounded project trust and capability broker

- **Status:** Accepted
- **Date:** 2026-07-26
- **Program:** program P30

## Context

ADR-0031 correctly made project writes and commands high-risk under SmartDeny
with exact-effect identities, root binding, and durable receipts. That safety
milestone is binary at the product surface: every ordinary project mutation
pauses, and the only low-level alternatives are SmartDeny versus Unrestricted.
Composer defaults (`offline-echo`, Ask before effects) and an access menu that
lists Full access first reinforce a permission wall for normal work.

Peer products moved toward workspace-scoped autonomy with approval only at the
boundary. Optimus should follow that usability direction without discarding its
Rust-owned exact-effect ledger. Authorization must be separable from auditing:
every action still gets a canonical digest and receipt; not every action needs a
fresh human click when a durable project trust grant already covers it.

## Decision

1. **Separate authorization from auditing.** Exact effect digests, project root
   hashes, and durable receipts remain mandatory for host-mutating work. A
   previously granted **project trust profile** may satisfy authorization
   automatically. Recording an action does not require pausing for it.

2. **Introduce product autonomy profiles** (orthogonal to
   [`CommandFsEnvelope`](0035-command-capability-envelope.md)):

   | Profile | Product role | Authorization behaviour |
   |---|---|---|
   | `standard` | Recommended default | Auto-allow ordinary project FS mutate + confined project commands; ask for external, sensitive, or unusually destructive work |
   | `review_changes` | Renamed Ask before effects | Reads free; project writes and commands ask |
   | `read_only` | Analysis | Deny mutations |
   | `full_project` | Advanced | Broader project-local autonomy; still protect credentials, host system, external side effects |
   | `unrestricted_host` | Expert break-glass | Not a routine composer choice; maps to explicit unrestricted policy + host envelope |

   Autonomy answers **when Optimus asks**. Containment answers **what an approved
   process can reach**. Selecting a more autonomous profile must not silently
   switch command FS to `UnrestrictedHost`.

3. **Central Capability Broker** lives in `crates/optimus-policy`. Every tool,
   workflow, subagent, MCP tool, and connector eventually requests authority
   through this deterministic component. The kernel remains the control-plane
   waist; UI and tools must not own a second policy system.

4. **Capability classes, not command-string allowlists.** The broker classifies
   work with ids such as `fs.project.write`, `process.project.execute`,
   `git.remote.push`, `network.localhost.owned`, `system.modify`. Constraints
   (root hash, paths, domains, ports, deletion scope, session binding) attach to
   grants.

5. **Project scope ≠ project trust grant.** Opening a project still creates
   Rust-owned root scopes (ADR-0031). A separate durable **trust grant** answers
   what Optimus may do automatically inside those roots. Checked-in
   `.optimus/project.toml` may recommend a profile and declare stack/commands; it
   must not grant credentials, outside-project access, or unrestricted execution.
   Durable grants live in Optimus-owned state outside the repository.

   *Where a grant is read (R30.5).* One place: `Kernel::open_dev_run_session`.
   A grant is a statement about engineering runs inside an authorized worktree,
   not about every session that names the project, so a chat session on a
   trusted project still asks. `unrestricted_host` cannot be made durable at
   all — break-glass that survives a restart is not break-glass. An expired
   grant reads as *no grant*, never as its own profile, and a store that cannot
   be read is an error rather than a silent narrowing or widening.

   *What a command is (R30.6).* Authorizing `cargo test` and
   `cargo install ripgrep` against the same capability made a project-scoped
   grant cover a host change. `optimus-policy::command_class` separates
   **sync** (reproduces a committed lockfile), **add** (chooses a new
   dependency, reaches a registry), and **host install** (writes outside the
   project, and answers to `system.modify`). Unrecognized programs stay
   `process.project.execute`: guessing wide is safe, guessing narrow is not.

6. **SmartDeny remains the pause mechanism.** Under SmartDeny, the broker
   decides Allow / Ask / Deny / Unavailable. Allow under a trust profile inserts
   a durable exact-effect authority receipt (actor
   `trust_profile:<profile>`) and continues. Ask still surfaces exact-effect
   approval. `PolicyMode::Unrestricted` remains test/break-glass auto-grant for
   all high-risk effects and is not the product default.

7. **Composer surface.** Primary choices: Standard (default), Review changes,
   Read only. Full project under Advanced. Unrestricted host under Expert only.
   Product release defaults move toward Auto provider/model + Standard + Confined
   containment; diagnostic offline-echo remains available for tests.

8. **Follow-on (same program, not this ADR’s full exit):** owned-localhost
   leases, same-run continuation frames, structured failure taxonomy, checkpoint
   manifests, layered readiness, first-run smoke. See
   [reliability-autonomy-program.md](../../_attic/plans/reliability-autonomy-program.md).

## Consequences

- Positive: ordinary project edit/test loops no longer require per-file pauses
  under Standard while audit and confinement stay first-class.
- Positive: Full project is no longer confused with unrestricted host FS.
- Negative: broker vocabulary must stay closed and versioned; incomplete
  capability mapping falls closed to Ask or Deny.
- Positive: the approval prompt can now say *which* act it is approving —
  "install a binary on your system" reads differently from "run a command in
  this project", and before R30.6 both rendered the same.
- Residual: owned-localhost browser leases (R30.7) remain a program P30
  follow-on. Package-manager structured capabilities (R30.6), the durable
  trust grant store (R30.5), and product Auto routing defaults (R30.8) have
  landed.
- Decision 7 reached the surface on 2026-07-29 (#118). *Both* shipped composers
  offer the five profiles — the React workbench with Standard first, Full
  project under Advanced and Unrestricted host under Expert, and the Wry
  desktop composer (`apps/optimus-desktop/ui/index.html`, reached through
  `OPTIMUS_ELECTRON_UI=legacy`) with the same vocabulary and `standard`
  pre-selected. The strangler of ADR-0028 is unfinished (#106), so a claim about
  "the composer" is worth only as much as its narrower reading; the gate reads
  both files for that reason. `full` and `host` no longer parse to
  `UnrestrictedHost` in either crate, so a stale sender of the old menu's first
  value falls closed to Review changes rather than receiving the machine. Two
  values do not survive a reload — legacy `full`, and `unrestricted_host`
  itself, both restoring to Standard, which extends §5's "break-glass that
  survives a restart is not break-glass" from durable grants to the composer's
  own persistence. The legacy Wry composer now applies the same restore filter
  as the React composer; its former `smart_deny` value migrates to Review
  changes rather than upgrading to Standard. Both migration tables are exact,
  and their gate rejects computed, spread, duplicate, or unclassifiable
  properties, so legacy `full` cannot drift upward to Full project behind a
  syntax shape the verifier ignores. The Wry menu renders Full project under
  Advanced and Unrestricted host under a warning-coloured Expert section
  instead of presenting a flat authority list. Its options carry the same
  explanatory hints as React, and the break-glass warning is included in the
  option's accessible name rather than being visual-only. The CLI's former
  `open` policy alias is also removed:
  only the explicit word `unrestricted` disables effect checks.
  `scripts/gates/check-autonomy-profiles.py` holds both menus, both persistence paths,
  both profile parsers, and the CLI policy parser against this vocabulary on
  every verify; its Wry rendering proof is scoped to the live access branch.
  Desktop Playwright coverage independently checks the rendered group order,
  warning semantics, and persisted-value migrations after reload.

## Alternatives considered

### Keep binary SmartDeny / Unrestricted only

Rejected. Forces either constant approval friction or break-glass over-grant.

### LLM as primary permission authority

Rejected. Optimus’s differentiator is deterministic Rust policy plus exact
receipts. Models may request capabilities; only the broker grants them.

### Auto-allow without exact-effect receipts

Rejected. Would discard ADR-0031’s audit architecture.

## Risks

- Auto-allowed destructive deletes if classification is too coarse — mitigate with
  delete-scope constraints and Full project vs Standard distinction.
- UI still labels “Full access” as unrestricted in old clients — map legacy
  `full` carefully: prefer `full_project` for product; keep explicit
  `unrestricted` / `unrestricted_host` for break-glass.

## Conditions for reconsideration

Revisit if Standard auto-allow produces user-visible unsafe host effects under
Confined envelopes, or if broker latency blocks the turn loop.

## Relevant code

- `crates/optimus-policy/src/lib.rs` — profiles, capability ids, broker
- `crates/optimus-policy/src/command_class.rs` — sync vs add vs host install
- `crates/optimus-kernel/src/project_trust.rs` — the durable grant store
- `crates/optimus-kernel/src/dev_run.rs` — the one place a grant is read
- `crates/optimus-runtime/src/policy_bridge.rs` — effect → broker request

## Relevant tests

- `crates/optimus-policy/tests/command_classification.rs` — a host install no
  longer rides on a project grant
- `crates/optimus-kernel/tests/dev_run_trust.rs` — the same write pauses
  without a grant and lands with one; chat does not inherit it
- `crates/optimus-runtime/tests/project_trust_profile.rs` — Standard
  auto-allows a project write; Review changes still pauses
- `crates/optimus-runtime/tests/approvals_surface.rs` — what an approval shows

## Addendum — consequence-bounded command classification (2026-07-31)

[ADR-0059](0059-standard-autonomy-is-consequence-bounded.md) tightens R30.6
without changing Standard's product role. Recognised git remote operations,
network/remote clients, and command-string shell forms such as `sh -c` no
longer inherit `process.project.execute`; Standard asks at those identified
boundaries. Uncheckpointed project deletes are now irreversible and ask under
Standard.

The original sentence “guessing wide is safe” is retained above as decision
history, not as the current security claim. Unknown project binaries and
scripts can still reach the shared network and inherited credential variables,
so classification is defense in depth rather than proof of containment. The
universal Standard default remains gated on code-enforced arbitrary-process
network and scoped credential authority. Direct project builds, tests, and
transparent script argv remain automatic.

## Addendum — Auto is a selection, not a provider identity (2026-07-31)

R30.8 implements decision 7 without inventing an `auto` provider or model.
`Auto` is the release-surface selection. At the start of each turn the canonical
router resolves it once to a connected concrete provider in this order: Codex
OAuth, configured OpenAI-compatible, then the deterministic offline provider.
An Auto model is absence of an override, so the selected provider supplies its
canonical default. Every durable route decision and execution record therefore
continues to name the concrete provider and model actually used.

Auto selection is readiness-based routing, not cross-provider retry. A provider
failure after selection does not silently send the prompt to another provider.
Codex access that is already expiring and cannot refresh is not ready. New
explicit provider and model choices remain exact and sticky; the one legacy
migration converts pre-Auto, unchosen Offline preference residue to Auto. A fresh
credential-less or fixture home resolves Auto to offline, which keeps first-run
chat and offline verification deterministic; adding a credential makes a later
Auto turn select the corresponding live provider without rewriting the user's
durable selection.

## Documentation completion addendum (2026-07-31)

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Evaluation evidence

- `crates/optimus-policy/src/lib.rs`
- `crates/optimus-policy/tests/command_classification.rs`
- `crates/optimus-kernel/tests/dev_run_trust.rs`
- `crates/optimus-runtime/tests/project_trust_profile.rs`
- `crates/optimus-runtime/tests/approvals_surface.rs`
