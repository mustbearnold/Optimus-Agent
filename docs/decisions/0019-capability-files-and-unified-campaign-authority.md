---
doc_id: decisions-0019-capability-files-and-unified-campaign-authority
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0019: Capability files and unified campaign authority, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - Cargo.toml
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-store/src/lib.rs
  - crates/optimus-kernel/src/fs_sandbox.rs
  - apps/optimus-cli/src/main.rs
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
depends_on:
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/contracts/high-risk-contracts.md
validated_by:
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-store/src/lib.rs
  - scripts/test_engineering_memory.py
last_verified_commit: b59b90766fd3b001725dd1542a05326a1d4b4894
---

# ADR-0019: Capability files and unified campaign authority

- **Status:** Accepted
- **Date:** 2026-07-20

## Context

ADR-0018 closed deterministic path escapes and permissive campaign decoding but
recorded four remaining gaps: filesystem checks were preflight-only, runtime
writes did not share the kernel secret-basename policy, campaign persistence had
no general schema/repair framework, and campaign state was committed separately
from Work Graph jobs.

Those gaps left concurrent path replacement and campaign crash windows outside
the implemented contract.

## Decision

1. `Runtime` opens and retains a `cap_std::fs::Dir` for the canonical workspace.
   `WriteFile` creation/writes and `AssertFileEquals` opens resolve relative to
   that directory capability. Ambient authority is used only to establish the
   initial workspace handle.
2. Relative paths still accept only normal components. The runtime owns the
   canonical case-insensitive secret-basename predicate, and kernel `FsRoots`
   reuses that exact predicate.
3. Campaign tables live in `optimus.db` with Work Graph jobs. A versioned
   `campaign_meta` schema advances through transactional `v0→v1→v2→v3`
   migrations and rejects malformed or future versions before migration.
4. Existing `campaigns.db` data is imported read-only and all-or-nothing into
   `optimus.db`; an import marker is committed with the copied rows. The legacy
   file is retained as evidence/rollback input and is not a live authority.
5. Campaign creation commits the campaign, complete ordered plan, expected step
   count, and deterministic step-to-job identity in one transaction. For new
   plans, each job UUID equals its step UUID.
6. Work Graph job creation serializes the complete graph first and commits job,
   nodes, and creation events in one store transaction.
7. Campaign status is a read-transaction projection of authoritative Work Graph
   job status. Legacy campaign/step status columns are cache fields only.
8. Resume checks the deterministic job identity: absent jobs are atomically
   created, existing jobs are reused, and only a running job owned by the step is
   marked interrupted before resume. No global recovery sweep is performed.
9. `diagnose` scans without executing work, distinguishes repairable projection
   drift from irreparable executable corruption, and `repair` synchronizes only
   deterministic cache fields. CLI `campaign diagnose|repair` exposes both.

## Reasons

- A retained directory capability removes path re-resolution between validation
  and file open and either pins or prevents replacement of the workspace root.
- One secret predicate prevents kernel/runtime policy drift.
- Deterministic job IDs turn the campaign-to-job handoff into an idempotent
  lookup rather than a two-database bookkeeping race.
- Deriving campaign state from jobs removes duplicate terminal authority.
- Explicit schema versions and honest diagnostics preserve unknown/corrupt data
  instead of inventing executable defaults.

## Alternatives considered

### Continue canonicalization immediately before each file operation

Rejected because it remains a time-of-check/time-of-use preflight and follows a
workspace root replaced after runtime startup.

### Attach `campaigns.db` to `optimus.db`

Rejected because independent live authorities and WAL/multi-file commit behavior
would retain avoidable crash and reconciliation complexity.

### Keep random job IDs and reconcile nullable handoffs

Rejected because a crash after job creation but before storing the random ID can
orphan the job. A step UUID is already a stable, durable idempotency key.

### Silently repair malformed plans

Rejected. Only projections derivable from authoritative jobs are repairable;
identities, executable JSON, plan cardinality, and unknown schema versions fail
closed.

## Consequences

### Positive

- Root replacement, linked-ancestor, traversal, prefix, current-directory, and
  secret-basename regressions are denied at the public effect seam.
- New campaign plans and Work Graph jobs are all-or-nothing at creation.
- Crash after job creation or while a node is running resumes the same job.
- `get`, `list`, `status`, and `run` report one job-derived campaign outcome.
- Legacy data has a deterministic migration and operator-visible diagnostics.

### Negative

- `cap-std` and its platform dependencies increase the runtime dependency set.
- Opening a runtime retains an OS directory handle for its lifetime.
- Legacy projection columns remain for compatibility and require optional repair
  after externally driven job transitions.
- Invalid/future schema and irreparable plan corruption require operator action.

## Risks and limitations

The transition/event and campaign-lease limitations below describe the accepted
ADR-0019 candidate. ADR-0020 supersedes them with atomic later transitions,
storage terminal uniqueness, and schema-v4 fenced campaign leases.

- Arbitrary approved `RunCommand` programs are not filesystem-sandboxed; the
  directory capability governs built-in file effects, not child-process syscalls.
- Effect execution and durable success marking are not one transaction; crash
  recovery remains at-least-once for interrupted effects.
- General Work Graph status-update plus event-append transitions are still
  separate statements. Job creation is atomic, but C-15 remains partially open
  for non-creation transitions and terminal-event uniqueness.
- Concurrent process ownership/leases for the same campaign are not yet defined.

## Evaluation evidence

- root replacement cannot redirect `WriteFile`
- Windows junction/Unix symlink linked-ancestor denial
- secret-basename denial for writes and assertions
- future campaign schema rejection and transactional legacy migration
- late-node failure rolls back complete Work Graph job creation
- deterministic handoff survives crash after job creation
- targeted recovery resumes a running campaign node
- campaign `get` and `list` derive status from Work Graph jobs
- diagnostics classify and repair projection drift without executing work
- Engineering Memory validation rejects superseded split-store/path-policy claims
- strict workspace Clippy and full workspace tests

## Relevant code

- `crates/optimus-runtime/src/lib.rs`
- `crates/optimus-runtime/src/campaign.rs`
- `crates/optimus-graph/src/lib.rs`
- `crates/optimus-store/src/lib.rs`
- `crates/optimus-kernel/src/fs_sandbox.rs`
- `apps/optimus-cli/src/main.rs`
- `scripts/engineering_memory.py`

## Relevant tests

- `crates/optimus-runtime/tests/path_confinement.rs`
- inline campaign migration, corruption, recovery, authority, and repair tests in
  `crates/optimus-runtime/src/campaign.rs`
- atomic late-node rollback test in `crates/optimus-store/src/lib.rs`
- ADR-0019 semantic guard tests in `scripts/test_engineering_memory.py`

## Conditions for reconsideration

Revisit when adding rename/delete effects, child-process filesystem sandboxing,
workflow leases, exactly-once effect protocols, or atomic Work Graph transition
and terminal-event storage.
