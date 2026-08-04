---
doc_id: decisions-0020-work-graph-integrity-and-loopback-security
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0020: Work Graph integrity and loopback security, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - Cargo.toml
  - crates/optimus-store/src/lib.rs
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/cancellation.rs
  - crates/optimus-runtime/tests/command_capture.rs
  - crates/optimus-runtime/tests/crash_resume.rs
  - apps/optimus-cli/src/gateway_http.rs
  - apps/optimus-cli/tests/gateway_http.rs
  - apps/optimus-desktop/src/main.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/src/bridge.rs
  - apps/optimus-desktop/e2e/support.js
depends_on:
  - docs/decisions/0019-capability-files-and-unified-campaign-authority.md
validated_by:
  - crates/optimus-store/src/lib.rs
  - crates/optimus-runtime/src/campaign.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-runtime/tests/cancellation.rs
  - crates/optimus-runtime/tests/command_capture.rs
  - crates/optimus-runtime/tests/crash_resume.rs
  - apps/optimus-cli/tests/gateway_http.rs
  - apps/optimus-desktop/src/server.rs
  - apps/optimus-desktop/e2e/**
---

# ADR-0020: Work Graph integrity and loopback security

- **Status:** Accepted
- **Date:** 2026-07-20

## Context

ADR-0019 unified campaign and Work Graph authority but left later projection/event
atomicity, terminal uniqueness, concurrent campaign ownership, effect crash
classification, cancellation, exact-action approvals, and loopback authorization
open. Those gaps permit split durable state, duplicate terminal claims, stale
owners, unsafe command replay, transferable authorization, and browser/local
process access to effectful HTTP surfaces.

## Decision

1. Work Graph schema version 7 rejects malformed/future metadata before mutation
   and migrates supported predecessors transactionally.
2. Accepted job/node projection transitions and their events commit in one SQLite
   transaction. Legacy partial state is diagnosed and quarantined before work.
3. Terminal job events use a storage-enforced unique terminal slot.
4. Campaign schema version 4 stores owner, token, generation, and lease deadline.
   Claim, renew, release, expiry, and stale-owner fencing are transactional.
5. Effect attempt identity and intent are durable before external I/O. Built-in
   writes use temporary replacement plus receipts. A prepared command found after
   process loss is `ambiguous` and cannot be blindly replayed.
6. Job/node/campaign cancellation is durable and idempotent. Running commands
   must terminate and reap their owned Windows process tree before cancellation
   finalizes. `Interrupted` remains resumable rather than terminal.
7. SmartDeny decisions bind to exact job ID, node ID, and SHA-256 of persisted
   effect JSON. Grants carry actor and finite expiry; denial and revocation are
   durable decisions with actor/reason evidence. Authorization cannot transfer
   to changed effects or later nodes.
8. Desktop HTTP is test/development-only, requires `--development-http` and
   `OPTIMUS_HTTP_TOKEN`, rejects unsafe requests without exact loopback origin and
   CSRF header, and emits no wildcard CORS. Gateway HTTP requires a separate
   `OPTIMUS_GATEWAY_TOKEN` and validates browser-origin requests.
9. Both HTTP surfaces cap request bodies and fixed-window request rates, bound
   aggregate work, omit home paths from health output, and return stable redacted
   errors while retaining local operator diagnostics.

## Alternatives considered

- Keep application-level terminal/event idempotency without database constraints.
- Use transferable job-wide approvals and rely on the pending-action display.
- Treat loopback binding as sufficient authorization.
- Replay all prepared effects after a crash.

These alternatives were rejected because retries, crashes, local processes, and
hostile browser origins cross the assumptions they depend on.

## Reasons

The runtime must fail closed at durable and authorization boundaries. Storage
constraints and exact identities remain enforceable across process loss, while
prompt/UI conventions do not. Explicit ambiguity is safer than inventing a
command result, and explicit development credentials are safer than treating
network topology as identity.

## Consequences

- Repeated cancellation, resume, run, recovery, and recomputation preserve one
  terminal outcome.
- A crash cannot produce an accepted projection without its transition event.
- Concurrent campaign processes cannot both hold valid execution authority.
- Command crash uncertainty remains explicit instead of being rewritten as
  success or silently replayed.
- Local HTTP clients and Playwright harnesses must supply explicit tokens; desktop
  browser fetch plumbing adds bearer and CSRF headers only in development HTTP
  mode.
- Existing job-scoped rows in the legacy `approvals` table do not authorize
  exact actions.

## Risks

The historical model-provider and pre-ownership Windows process boundaries below
are superseded in part by ADR-0021. Cooperative transport cancellation and
suspended Job Object ownership are now implemented; synchronous connection/write
abort remains unresolved.

- Effect execution and durable success marking are not one transaction; built-in
  write convergence and command ambiguity handling reduce but do not eliminate
  at-least-once external-effect semantics.
- Windows command-tree cancellation uses bounded tree termination and root reap;
  the runner does not yet create every process suspended inside a kill-on-close
  Job Object before first instruction.
- Cancellation does not yet interrupt model-provider calls or a future child-agent
  hierarchy.
- Approved commands are not `cap-std` file effects; filesystem reach is governed
  by `CommandFsEnvelope` (ADR-0035 / P12), not ambient host FS by default.
- The legacy `approvals` table remains for schema compatibility but is not an
  authorization source.

## Verification

- Atomic rollback, quarantine, terminal uniqueness, cancellation, exact approval,
  denial/revocation/expiry/non-transfer, command ambiguity, and campaign lease
  regressions.
- Authenticated gateway real-process smoke including a 401 probe.
- Desktop HTTP security, body/rate, redaction, and bounded stream-worker tests.
- Playwright browser verification with development token injection and no
  health-path disclosure.

## Evaluation evidence

Focused RED/GREEN regressions cover each invariant, followed by package suites,
strict Clippy, authenticated real-process gateway smoke, and Playwright browser
execution. Engineering Memory semantic guards reject superseded schema-v3,
job-scoped approval, split transition/event, and unauthenticated-loopback claims.

## Conditions for reconsideration

Revisit when introducing a universal effect protocol, OS-enforced process
sandbox/Job Object ownership, remote API exposure, model-call cancellation,
parallel child agents, or removal of the legacy approval table.

## Relevant code

- `crates/optimus-store/src/lib.rs`
- `crates/optimus-graph/src/lib.rs`
- `crates/optimus-runtime/src/lib.rs`
- `crates/optimus-runtime/src/campaign.rs`
- `apps/optimus-cli/src/gateway_http.rs`
- `apps/optimus-desktop/src/server.rs`
- `apps/optimus-desktop/src/bridge.rs`

## Relevant tests

- `crates/optimus-runtime/tests/approvals_surface.rs`
- `crates/optimus-runtime/tests/cancellation.rs`
- `crates/optimus-runtime/tests/crash_resume.rs`
- `apps/optimus-cli/tests/gateway_http.rs`
- `apps/optimus-desktop/e2e`

## Addendum (2026-07-25)

Filesystem reach of **approved** `RunCommand` is refined by **ADR-0035**
(P12 command capability envelope). SmartDeny / exact-grant integrity in this
ADR remains authoritative for approvals.
