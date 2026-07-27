---
knowledge_type: decision
status: current
covers:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-host/src/chat.rs
  - apps/optimus-ui/src/components/workbench/Composer.tsx
depends_on:
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/plans/reliability-autonomy-program.md
validated_by:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/tests/project_trust_profile.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
last_verified_commit: null
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
   [reliability-autonomy-program.md](../plans/reliability-autonomy-program.md).

## Consequences

- Positive: ordinary project edit/test loops no longer require per-file pauses
  under Standard while audit and confinement stay first-class.
- Positive: Full project is no longer confused with unrestricted host FS.
- Negative: broker vocabulary must stay closed and versioned; incomplete
  capability mapping falls closed to Ask or Deny.
- Residual: package-manager structured capabilities, owned-localhost browser
  leases, and durable project trust grant store beyond session profile are
  program P30 follow-ons.

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

## Reconsideration

Revisit if Standard auto-allow produces user-visible unsafe host effects under
Confined envelopes, or if broker latency blocks the turn loop.
