---
knowledge_type: contract-risk-register
status: current
covers:
  - crates/optimus-store/src/**
  - crates/optimus-graph/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-kernel/src/**
  - crates/optimus-packs/src/**
  - apps/optimus-desktop/src/**
  - apps/optimus-cli/src/gateway_http.rs
depends_on:
  - docs/decisions/**
validated_by:
  - crates/**/tests/**
  - apps/optimus-desktop/e2e/**
last_verified_commit: null
---

# Highest-risk behavioural contracts

This is a prioritized gap register, not a claim that every listed contract is
implemented. “Documented” means an ADR or test expresses part of the behavior;
it does not replace executable enforcement.

## Priority 0

### C-01 Cancellation propagation and terminalization

- **State:** Confirmed current behaviour for Work Graph jobs, commands,
  campaigns, and cooperative model-provider calls.
- **Evidence:** durable idempotent requests cancel pending work atomically;
  running commands observe cancellation and terminate/reap their Windows process
  tree; campaign cancellation propagates to created jobs and uncreated steps.
  Active providers receive a cooperative token and Codex SSE checks it on
  bounded read intervals.
- **Boundary:** synchronous transport connect/write abort and future child-agent
  propagation remain unresolved.
- **Required contract:** idempotent cancellation; propagation to child work and
  running tools; bounded graceful/forced stop; no new stages; inspectable partial
  artifacts; lock/lease release; exactly one terminal event.
- **Owner:** runtime/graph/store, then future workflow/agent layers.
- **Minimum regression:** cancel before start, during command, during approval,
  after terminal, and process restart.

### C-02 Exactly one terminal outcome/event

- **State:** Confirmed current behaviour.
- **Evidence:** terminal events use a storage-enforced unique slot. Repeated
  cancel, resume, run, recovery, and recomputation preserve one terminal row.
- **Required contract:** one durable terminal outcome for every accepted
  execution; repeated recomputation/resume/cancel cannot append another.
- **Owner:** graph/store/runtime.

### C-03 Action-bound approval

- **State:** Confirmed current behaviour.
- **Evidence:** every decision is bound to job ID, node ID, and SHA-256 of the
  persisted effect JSON, with actor, creation time, expiry, denial, revocation,
  and ledger events. Changed effects and later nodes cannot reuse a grant.
- **Required contract:** request includes exact side effects and effect hash;
  grant is actor/time/scope bound, non-transferable, expirable/revocable, and
  retained in trace; denial has a defined outcome.
- **Owner:** runtime/store/security policy.

### C-04 Runtime filesystem confinement

- **State:** Confirmed current behaviour for built-in runtime file effects.
- **Confirmed current behaviour:** only normal components are accepted; empty,
  current-directory, absolute, parent, root, and platform-prefix paths are
  denied. `WriteFile` and `AssertFileEquals` resolve from a retained `cap-std`
  workspace directory capability. Root replacement and existing linked targets
  or missing descendants below Windows junctions/Unix symlinks are rejected by
  public-effect tests. Runtime and `FsRoots` share one secret-basename predicate.
- **Boundary:** approved arbitrary child processes are not filesystem-sandboxed.
- **Owner:** runtime; align with kernel `FsRoots`.

### C-05 Loopback API authorization

- **State:** Confirmed current behaviour for current loopback surfaces.
- **Evidence:** desktop HTTP requires explicit development mode, a 32-character
  bearer, exact unsafe-request origin, and CSRF header; wildcard CORS is absent.
  Gateway HTTP requires a separate bearer and validates supplied origins. Both
  cap bodies/rates/aggregate work and redact external errors and health paths.
- **Required contract:** explicit development-only gating, origin/auth token,
  request-size/rate bounds, CSRF/local-origin defense, secret redaction, and
  deployment prohibition unless hardened.
- **Owner:** desktop/server and CLI gateway.

### C-06 Fail-closed persisted workflow decoding

- **State:** Confirmed current campaign behaviour.
- **Confirmed current behaviour:** invalid campaign/step UUIDs, statuses,
  timestamps, indices, optional job IDs, step JSON, expected counts, noncontiguous
  indices, and missing/partially reassigned plans return errors. Full campaign
  decoding and exact plan-completeness validation precede runtime/workspace
  effects. Campaign schema v4 provides sequential transactional migrations,
  malformed/future-version rejection, all-or-nothing legacy import,
  non-executing diagnostics, and deterministic projection repair.
- **Owner:** runtime campaign store.

### C-07 Provider call envelope and whole-batch authorization

- **State:** Confirmed current behaviour, documented by ADR-0016 and focused
  regressions; remains high risk.
- **Contract currently enforced:** strict supported provider variants,
  non-empty/unique IDs, canonical names, exact advertised set, loaded/available
  descriptor, supported schema validation, and all-sibling prevalidation before
  effects.
- **Owner:** kernel/packs/provider adapters.
- **Future work:** property/fuzz tests for malformed JSON/SSE and schema subsets.

### C-15 Atomic projection and event-ledger transitions

- **State:** Confirmed current behaviour.
- **Confirmed current behaviour:** creation and later accepted projection/event
  transitions commit atomically. Legacy partial state is diagnosed and
  quarantined before execution, and terminal-event uniqueness is enforced by
  storage.
- **Owner:** store/graph/runtime.

### C-16 Campaign-to-job consistency and recovery

- **State:** Confirmed current behaviour for campaign/job handoff and recovery.
- **Confirmed current behaviour:** campaign plans and Work Graph jobs live in
  `optimus.db`; plan creation is transactional; every new step persists its own
  UUID as deterministic job UUID; complete job creation is transactional and
  idempotently discoverable after a crash. Campaign reads derive status from job
  state in one read transaction. Resume creates an absent fixed job, reuses an
  existing one, and targets crash recovery only at that job. Diagnostics report
  missing/invalid identities and repair projection drift without executing work.
- **Confirmed current behaviour:** exact owner/token/generation/deadline leases
  fence concurrent runners, expire, renew, release, and reject stale owners.
- **Remaining boundary:** effect execution and durable success are at-least-once.
- **Owner:** runtime campaign/work-graph boundary.

## Priority 1

### C-08 Canonical tool output/error/cancel/replay schema

- **State:** Unknown or unresolved behaviour.
- **Risk:** model and UI consumers receive tool-specific strings/JSON without a
  canonical typed output/error envelope.
- **Owner:** packs/protocol/kernel/runtime.

### C-09 Universal agent lifecycle

- **State:** Unknown or unresolved behaviour; no specialist agent abstraction.
- **Required fields:** typed task/context/constraints/permissions/tools/
  completion/cancellation input and typed result/evidence/artifacts/actions/
  unresolved/confidence/cost/trace output.
- **Owner:** future protocol/orchestrator/agent SDK.

### C-10 General workflow lifecycle

- **State:** Partially implemented through jobs/campaigns/cron/gateway.
- **Required fields:** versioned trigger, typed I/O, states, dependencies,
  retries, timeouts, cancellation, approvals, validation, terminal outcomes,
  rollback, observability, and eval coverage.
- **Owner:** runtime plus future workflow package.

### C-11 Model routing and fallback

- **State:** Unknown or unresolved behaviour.
- **Risk:** provider semantics differ by surface; costs/privacy/capabilities are
  not policy inputs.
- **Owner:** future model router with current adapters retained.

### C-12 Credential storage and local transport

- **State:** Unknown or unresolved behaviour.
- **Evidence:** OAuth tokens are stored as plain JSON; local effectful transports
  have no user/process authorization contract.
- **Owner:** auth/security/desktop/gateway.

### C-13 Deterministic replay and provenance

- **State:** Partially implemented effect/event persistence only.
- **Required contract:** version and hash every execution dependency, classify
  nondeterminism, retain stable references, and never claim exact replay for
  model/external stages.
- **Owner:** observability/runtime/protocol.

### C-14 Memory clock, sensitivity, retention, and erasure

- **State:** Partially implemented temporal claims and erase modes; fixed event
  timestamps and no end-to-end policy.
- **Owner:** memory plus security/observability.

### C-17 Cron and gateway claim/delivery semantics

- **State:** Confirmed current behaviour for local cron and file-adapter gateway.
- **Evidence:** cron claims, renewals, releases, expiry takeover, disable fencing,
  and completion compare exact owner/generation/token/deadline state. Gateway
  SQLite owns idempotent message ingestion, leases, attempt history, terminal
  outcome and outbound JSON; files are deterministic reconciled materializations.
- **Boundary:** external channel delivery acknowledgements and a dead-letter
  retry policy do not yet exist.
- **Owner:** kernel cron/gateway plus future scheduler/delivery runtime.

### C-18 Session causality around durable effects

- **State:** Confirmed current behaviour for successful durable tool-result
  progression.
- **Evidence:** before another model step, the tool message and a normalized link
  to provider call ID, job, node, attempt, effect hash, terminal outcome, and
  receipt hash commit in one `sessions.db` transaction. Conflicting call/attempt
  provenance rolls back the snapshot update.
- **Boundary:** no transaction spans `optimus.db` and `sessions.db`; failed turns
  before a durable tool result and general restart continuation remain partial.
- **Owner:** kernel/session/runtime.

## Coverage rule

A Priority-0 fix is incomplete until it has a focused regression, an integration
case across the owning boundary, updated coverage metadata, and refreshed
Engineering Memory. Do not weaken a test or relabel the behavior merely to close
a gap.
