---
knowledge_type: contract-risk-register
status: current
owns:
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/fs_sandbox.rs
  - crates/optimus-kernel/src/gateway.rs
  - crates/optimus-kernel/src/cron.rs
  - crates/optimus-kernel/src/agent.rs
  - crates/optimus-kernel/src/workflow.rs
  - crates/optimus-packs/src/lib.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-cli/src/gateway_http.rs
watches:
  - crates/optimus-kernel/src/**
  - crates/optimus-runtime/src/**
  - crates/optimus-store/src/**
  - crates/optimus-graph/src/**
  - apps/optimus-desktop/src/**
  - apps/optimus-electron/**
  - apps/optimus-ui/src/ipc/**
covers:
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/execution.rs
  - crates/optimus-kernel/src/fs_sandbox.rs
  - crates/optimus-kernel/src/gateway.rs
  - crates/optimus-kernel/src/cron.rs
  - crates/optimus-kernel/src/agent.rs
  - crates/optimus-kernel/src/workflow.rs
  - crates/optimus-packs/src/lib.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-cli/src/gateway_http.rs
depends_on:
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0019-capability-files-and-unified-campaign-authority.md
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
  - docs/decisions/0021-owned-execution-and-causal-delivery.md
  - docs/decisions/0031-safe-project-work-loop.md
validated_by:
  - crates/optimus-runtime/tests/cancellation.rs
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-kernel/tests/integrity_integration.rs
  - apps/optimus-cli/tests/gateway_http.rs
  - apps/optimus-desktop/e2e/03-runtime-and-sessions.spec.js
last_verified_commit: 09fddbc1b60a6b37f9f80680988ea5036a9b8eec
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
  runtime polling stops new nodes, terminates and reaps active platform-owned
  command trees (Unix process groups or Windows Job Objects), and
  propagates through campaign-created jobs/uncreated steps. Active providers
  receive a cooperative token and Codex SSE checks it on bounded read intervals.
  Desktop native/HTTP stream delivery failure requests the same token;
  full/disconnected bounded channels stop later progression and settle the
  accepted turn/execution as cancelled. Explicit desktop Stop is capability-local:
  HTTP aborts only its fetch, while native mode signals one exact bounded active
  stream ID without queueing behind chat workers.
- **Boundary:** synchronous transport connect/write abort and a future parallel
  child hierarchy remain unresolved. Durable agent invocations can synchronize
  cancellation to cooperative tokens at bounded owner-controlled loop points.
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
- **Evidence:** project writes and commands additionally persist the canonical
  workspace hash. Approval execution reopens the matching Rust-authorized root
  and rejects a foreign or changed workspace before any effect.
- **Evidence:** in-transcript decisions repeat run, call, job, node, node index,
  and effect digest. Kernel mismatch tests reject stale or substituted actions;
  denial terminalizes the paused turn without executing the effect, while an
  exact approval produces one terminal lifecycle event and durable receipt.
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
- **Confirmed current behaviour:** new project roots require a short-lived,
  single-use grant staged by a native folder picker. Canonical authorized roots
  persist under Rust ownership; absent scope does not fall back to shared work.
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

### C-17 Native preview annotation and overlay isolation

- **State:** Confirmed current behaviour for the Electron user-preview path.
- **Evidence:** remote content runs in a sandboxed `WebContentsView` without a
  Node preload; permissions, downloads, popups, insecure remote HTTP, and
  privileged schemes are denied. Annotation is user-triggered and one-shot,
  consumes the selected click, length-bounds every returned string, omits HTML
  and selectors, supports cancellation/expiry, and projects only a readable
  bounded note into the composer.
- **Evidence:** compiled Electron tests prove the child view is hidden while
  renderer Settings is open and restored afterward, preventing native pixels
  from covering approval/settings controls.
- **Boundary:** this is the user preview, not the Rust agent Browser effector;
  cookies, history, and automation identity are not shared.
- **Owner:** Electron main/preload plus React Browser surface.

## Priority 1

### C-08 Canonical tool output/error/cancel/replay schema

- **State:** Confirmed current behaviour.
- **Evidence:** versioned `ToolOutcome` envelopes carry exact call/tool identity,
  success/failure/cancelled/ambiguous kind, bounded summary/data/artifacts,
  structured error, replay class, and optional durable provenance. Kernel tool
  messages serialize the envelope and execution manifests retain it.
- **Boundary:** the `data` JSON value remains tool-specific rather than a
  per-tool static output schema.
- **Owner:** packs/protocol/kernel/runtime.

### C-09 Universal agent lifecycle

- **State:** Confirmed current behaviour for the typed contract, immutable
  registry, and durable invocation lifecycle; no built-in specialists exist.
- **Evidence:** bounded versioned requests/results, canonical tools, permission
  subset closure, context/evidence/artifact references, cancellation, retry
  lineage, exactly one terminal result, ambiguity, reopen validation, and exact
  runtime-effect provenance.
- **Boundary:** no specialist router, parallel scheduler, or child hierarchy.
- **Owner:** kernel agent module; runtime remains effect authority.

### C-10 General workflow lifecycle

- **State:** Confirmed current behaviour for versioned definitions, immutable
  registry, capability conformance, and owner adapters.
- **Evidence:** typed triggers/JSON-schema I/O, acyclic dependencies, bounded
  retries/timeouts, cancellation, approvals, rollback declaration,
  observability, exact terminal outcomes, optional agent references, and
  fail-closed adapters for jobs/campaigns/cron/gateway.
- **Boundary:** there is no universal workflow executor or cross-store
  transaction; execution remains adapter-owned.
- **Owner:** kernel workflow contract plus existing subsystem owners.

### C-11 Model routing and fallback

- **State:** Confirmed current behaviour for canonical provider/model ownership,
  capabilities, local-only privacy, bounded cost, explicit fallback, and
  persisted decisions across CLI/desktop/cron/gateway.
- **Boundary:** health, measured cost/latency, evaluation-driven selection, and
  runtime-failure fallback remain unresolved.
- **Owner:** kernel routing with provider adapters retained.

### C-12 Credential storage and local transport

- **State:** Confirmed current behaviour for Windows DPAPI credential envelopes,
  one-time plaintext migration, corruption fencing, user-only fallback file
  permissions, and bounded authenticated loopback transports.
- **Boundary:** non-Windows encrypted storage and local IPC process identity are
  unresolved.
- **Owner:** kernel credential boundary plus desktop/gateway transports.

### C-13 Deterministic replay and provenance

- **State:** Confirmed current behaviour for bounded fixture replay.
- **Evidence:** versioned immutable bundles bind one terminal source manifest,
  canonical trace, dependency hashes, ordered stages, content-addressed bounded
  fixtures, and expected terminal evidence. Planning fails closed on missing,
  duplicate, corrupt, reordered, drifted, or unsupported evidence.
- **Evidence:** the offline executor has no provider/network/process/runtime/
  approval/writable-workspace handle, compares exact stage inputs and fixture
  bytes, stops at first mismatch, and persists one immutable terminal report.
- **Boundary:** fixture comparison does not rerun or reproduce live model,
  network, process, browser, or destructive effects; independent stores do not
  share a transaction.
- **Owner:** kernel replay/execution/trace plus runtime provenance.

### C-14 Memory clock, sensitivity, retention, and erasure

- **State:** Confirmed current behaviour for injected monotonic UTC time,
  sensitivity/allowed-use gates, conservative migration, correction
  preservation, deterministic retention, scoped tombstone/privacy erase,
  idempotency, and sanitized audit inspection.
- **Boundary:** at-rest encryption, repository-wide erasure, compaction, archival,
  and quota remain unresolved.
- **Owner:** memory plus security/observability.

### C-17 Cron and gateway claim/delivery semantics

- **State:** Confirmed current behaviour for local cron and file-adapter gateway.
- **Evidence:** cron claims, renewals, releases, expiry takeover, disable fencing,
  and completion compare exact owner/generation/token/deadline state. Gateway
  SQLite owns idempotent message ingestion, leases, attempt history, terminal
  outcome and outbound JSON; files are deterministic reconciled materializations.
- **Evidence:** bounded gateway retries dead-letter the third failed attempt;
  delivery acknowledgement states are persisted and queryable.
- **Boundary:** guarantees of an external channel broker remain unresolved.
- **Owner:** kernel cron/gateway plus future scheduler/delivery runtime.

### C-18 Session causality around durable effects

- **State:** Confirmed current behaviour for accepted-turn lifecycle and durable
  tool-result progression.
- **Evidence:** before another model step, the tool message and a normalized link
  to provider call ID, job, node, attempt, effect hash, terminal outcome, and
  receipt hash commit in one `sessions.db` transaction. Conflicting call/attempt
  provenance rolls back the snapshot update.
- **Evidence:** accepted turns settle exactly once as success/failure/cancelled;
  interrupted accepted turns resume without duplicating the user segment.
- **Evidence:** every typed tool lifecycle transition commits to the execution
  store before stream delivery. Stable event IDs make duplicate reconnect
  delivery idempotent; session reload attaches ordered events to the owning
  assistant turn without exposing provider protocol messages.
- **Boundary:** no transaction spans `optimus.db`, `sessions.db`, or agent stores;
  causal links reference previously committed authoritative attempts.
- **Owner:** kernel/session/runtime.

## Coverage rule

A Priority-0 fix is incomplete until it has a focused regression, an integration
case across the owning boundary, updated coverage metadata, and refreshed
Engineering Memory. Do not weaken a test or relabel the behavior merely to close
a gap.
