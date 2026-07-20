---
knowledge_type: decision
status: current
covers:
  - Cargo.toml
  - crates/optimus-store/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/src/codex_oauth.rs
  - crates/optimus-kernel/src/cron.rs
  - crates/optimus-kernel/src/gateway.rs
  - crates/optimus-kernel/src/session.rs
depends_on:
  - docs/decisions/0020-work-graph-integrity-and-loopback-security.md
validated_by:
  - crates/optimus-runtime/tests/cancellation.rs
  - crates/optimus-kernel/tests/kernel_turn.rs
  - crates/optimus-kernel/tests/session_resume.rs
  - crates/optimus-kernel/src/cron.rs
  - crates/optimus-kernel/src/gateway.rs
last_verified_commit: null
---

# ADR-0021: Owned execution and causal delivery

- **Status:** Accepted
- **Date:** 2026-07-20

## Context

ADR-0020 left four explicit integrity boundaries. Windows commands could execute
before process-tree ownership was established. Model completions had no
cancellation input. Cron due selection and completion were separate operations.
Gateway reply publication and inbox archival were separate file effects. Session
snapshots also had no normalized identity linking durable tool messages to Work
Graph effect attempts.

These gaps allowed escaped child work, duplicate scheduler/delivery ownership,
stale completion, and transcripts that could not prove which durable attempt
caused an effect.

## Decision

1. On Windows, runtime commands launch suspended, enter a private kill-on-close
   Job Object, and resume only after assignment succeeds. Cancellation, timeout,
   and normal root exit terminate the Job and verify its active process count is
   zero before settlement.
2. `ModelProvider` receives a cloneable cooperative cancellation token through a
   cancellable streaming seam. Existing turn APIs create a never-cancelled token;
   callers that own cancellation use `turn_with_sink_cancellable`. Codex SSE uses
   bounded read intervals and checks the token between reads/events.
3. Cron uses transactional owner/generation/token/deadline claims. Claim,
   renewal, release, disable fencing, expiry takeover, and exact completion are
   storage-enforced. `tick_cron` executes only claimed rows.
4. Gateway JSON inbox files are an adapter, not the state authority. SQLite
   idempotently ingests message UUIDs and owns claims, attempt history, terminal
   outcomes, and outbound JSON. Outbox/archive files are deterministic
   materializations repaired by reconciliation without rerunning a turn.
5. Session snapshots that advance past a durable tool result atomically insert a
   normalized causal link from provider tool-call ID to job, node, effect attempt,
   effect hash, terminal outcome, and optional receipt hash. Receipt bodies are
   not duplicated into the link ledger.
6. Reusing a tool-call ID or effect attempt with conflicting provenance fails
   closed and rolls back the session snapshot update.

## Alternatives considered

- Keep post-launch `taskkill` only. Rejected because a child can escape before
  ownership and root-process exit is not proof of descendant cleanup.
- Poll cancellation only between model calls. Rejected because an active
  cooperative provider would remain uninterruptible.
- Add lock files around cron and gateway directories. Rejected because lock-file
  expiry and compare-and-swap semantics are weaker and harder to reconcile than
  SQLite transactions.
- Treat outbox files as the gateway authority. Rejected because publication and
  archival cannot be one filesystem transaction.
- Embed full effect receipts in session links. Rejected because it duplicates
  potentially sensitive bounded command output; hashes and authoritative runtime
  identities retain provenance without duplicating contents.

## Reasons

The selected boundaries establish ownership before execution, serialize claims
where identity lives, and make terminal projection plus provenance inspectable.
They preserve the existing file adapter and provider traits while adding narrow,
typed capabilities instead of broad process/job grants.

## Consequences

- `windows-sys` is a direct Windows-only runtime dependency.
- Gateway state now includes `gateway/gateway.db`; file directories remain the
  external adapter and operator-visible materialization.
- Cron and gateway stale owners cannot publish terminal state after takeover.
- A crash after gateway terminal commit but before file publication is repaired
  without another model turn.
- Tool-heavy sessions persist intermediate causal segments before the next model
  step, rather than only at final assistant text.

## Risks

- `NtResumeProcess` is an NT native API used because `std::process::Child` does
  not retain the primary thread handle needed by `ResumeThread`.
- Synchronous `ureq` connection establishment/write cannot be force-aborted by
  the cooperative token. Codex cancellation is bounded after its response stream
  opens; adapters with native cancellation can implement a stronger seam.
- Cron and gateway leases are finite. Long-lived workers must renew or accept
  stale-owner rejection.
- No transaction spans `optimus.db` and `sessions.db`; the session link records a
  previously committed authoritative attempt and atomically gates only session
  progression.

## Evaluation evidence

- Windows tests prove injected pre-assignment failure cannot execute a marker,
  resumed children are Job members, and cancellation/timeout leave no late work.
- Kernel tests prove an in-flight cooperative provider observes cancellation.
- Cron tests cover concurrent claims, expiry takeover, stale completion, disable
  fencing, and legacy migration.
- Gateway tests cover exclusive claims, renewal/release, takeover fencing,
  commit-before-materialization recovery, and conflicting materialization.
- Session tests cover exact terminal-attempt links and transaction rollback on
  conflicting provenance.

## Conditions for reconsideration

Reconsider the native Windows launch implementation if Rust exposes primary
thread ownership or a maintained dependency provides equivalent suspended
create/assign/resume guarantees. Reconsider the cooperative provider seam when
all production adapters use an async transport with native request abort.
Reconsider SQLite gateway authority only if an external broker provides stronger
transactional claim/outbox semantics with local offline operation.

## Relevant code

- `crates/optimus-runtime/src/lib.rs`
- `crates/optimus-kernel/src/codex_oauth.rs`
- `crates/optimus-kernel/src/cron.rs`
- `crates/optimus-kernel/src/gateway.rs`
- `crates/optimus-kernel/src/session.rs`

## Relevant tests

- `crates/optimus-kernel/tests/kernel_turn.rs`
- `crates/optimus-kernel/tests/session_resume.rs`
