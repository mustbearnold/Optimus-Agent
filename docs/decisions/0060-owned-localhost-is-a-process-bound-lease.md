---
knowledge_type: decision
status: current
covers:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/src/owned_localhost.rs
  - crates/optimus-runtime/src/process_ownership.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-browser/src/lib.rs
depends_on:
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0040-shared-browser-contract.md
  - docs/decisions/0044-bounded-project-trust-and-capability-broker.md
  - docs/decisions/0059-standard-autonomy-is-consequence-bounded.md
validated_by:
  - crates/optimus-policy/src/lib.rs
  - crates/optimus-runtime/src/owned_localhost.rs
  - crates/optimus-kernel/src/browser.rs
  - crates/optimus-browser/src/lib.rs
last_verified_commit: null
---

# ADR-0060: Owned localhost is a process-bound lease (program P30)

- **Status:** Accepted
- **Date:** 2026-07-31
- **Program:** program P30

## Context

Standard project autonomy needs to start a development server and inspect it
without asking the user to approve each page load. Treating localhost as safe
would do the opposite of confinement: unrelated databases, desktop APIs,
debuggers, metadata proxies, and services from another project all live behind
the same name.

The existing capability vocabulary includes `process.project.serve`,
`network.localhost.owned`, and `browser.localhost.owned`, but a name is not an
authority. Before this decision those capabilities had no exact socket,
process-tree, project, session, run, generation, or expiry constraint.

Browser checks also have to happen before connection. An HTTP client that
validates only the final URL has already contacted a redirect target, and a CDP
browser that checks only navigation can still load private subresources or
pivot after a click.

## Decision

Owned-localhost access is an ephemeral capability joining one exact listener to
one Optimus-owned process tree and one agent execution:

1. A typed binding carries the HTTP scheme, exact numeric loopback address and
   port, project scope and canonical-root identity, session and run identity,
   retained process-tree identity, generation, and expiry.
2. Initially the only leased origin is `http://127.0.0.1:<port>`. `localhost`,
   other loopback spellings, adjacent ports, IPv6, wildcard listeners, private
   ranges, and HTTPS do not inherit that authority.
3. Only a structured project-serve effect may request the capability. Ordinary
   terminal argv, stdout saying “ready,” a bare PID, or a URL supplied by a
   model cannot mint it.
4. The runtime may issue a lease only after the listener is live and proven to
   belong to the retained systemd scope or Windows Job Object. Unsupported
   ownership proof fails closed.
5. The lease is for the **AgentEffector** domain. It is never transferred to
   UserPreview, page JavaScript, renderer storage, URLs, headers, or model
   arguments.
6. Every network request must meet the same authority before connection. CDP
   Fetch interception is the current pre-request seam; final URLs after
   navigation and clicks are revalidated afterwards as defense in depth.
   Public destinations continue through public-browser authority; private
   destinations remain denied.
7. Expiry, cancellation, run settlement, process exit, listener loss, project
   revocation, browser close, or runtime restart revokes the lease. Port or PID
   reuse never revives it.
8. Issuance, use, refusal, revocation, and process cleanup produce ordered audit
   evidence without storing an opaque bearer in model-visible data.

The policy broker remains pure. It verifies that a coherent constraint envelope
accompanies the three owned-localhost capabilities, rejects that envelope on
unrelated capabilities, and copies it into applied constraints. Runtime code
remains responsible for proving ownership, liveness, generation, and expiry
rather than trusting publicly serializable fields.

## Reasons

- A project/session/run/process/socket tuple is the minimum authority that
  distinguishes the selected dev server from another local service.
- Numeric loopback plus an exact port avoids DNS rebinding and hostname alias
  ambiguity.
- Reusing the broker's exact constraints and receipts keeps one authority and
  audit plane instead of creating a browser-owned permission system.
- Per-request checks are required because a page can create network effects
  without changing the top-level address bar.

## Delivery boundary

This decision lands in stages without weakening the default deny:

- **Landed foundation:** exact broker constraints; public-only browser
  constructors; an explicit CDP authority for one exact numeric origin;
  attached-tab CDP request interception; final-URL checks after navigation and
  clicks; manual HTTP redirects validated before the next connection; and a
  runtime registry where serialized bindings are non-bearer receipts. The
  registry requires exact live membership, the same opaque execution context,
  generation, expiry, and retained-listener liveness on every use. Revocation
  fences new uses, waits boundedly for active use guards, then cleans the
  retained owner or quarantines a failed cleanup for retry, and records ordered
  evidence. Neither the verified-listener proof nor the opaque execution context
  has a production constructor yet, so the substrate cannot issue product
  authority by itself.
- **Still required for R30.7:** the structured serve effect, listener ownership
  proof and atomic runtime-owned front listener, trusted kernel-to-runtime
  issuance context, lifecycle hook wiring and active expiry supervision, HTTP
  leased-origin connection, worker/service-worker and WebSocket egress coverage
  below a single tab target, restart orphan-process recovery, and one end-to-end
  serve/browse/revoke test. Generations are process-local until the retained
  listener/store projection makes cross-process exclusivity authoritative.

Until those remaining items land, no product path issues the new authority and
localhost stays denied. Program P30 microtask R30.7 therefore remains in
progress rather than done.

## Consequences

- Positive: Standard can eventually inspect its own dev server without making
  all localhost services reachable.
- Positive: redirect and click pivots are blocked on the existing public
  browser path now, independently of lease issuance.
- Positive: legacy serialized action targets and constraints remain readable;
  their absent lease defaults to no authority.
- Negative: a long-running owned-service lifecycle is separate from bounded
  terminal execution and requires explicit cleanup machinery.
- Negative: the first implementation is IPv4 HTTP only.
- Residual: this lease does not by itself remove general egress or ambient
  credentials from arbitrary `Confined` project processes. ADR-0059's broader
  Standard-default warning remains.

## Risks

- A future issuer could validate the binding's shape but fail to prove listener
  ownership. Mitigation: the product path remains absent until an adversarial
  ownership test proves a foreign process cannot receive a lease.
- Process exit and port reuse can race revocation. Mitigation: generation and
  retained process-tree identity are mandatory constraints, and cleanup must
  revoke before releasing the listener.
- Browser backends can drift. Mitigation: HTTP redirect and CDP request/final
  URL tests remain independent and both must pass the end-to-end acceptance.
- Chrome worker and service-worker targets may not share the initial tab's Fetch
  interception. Mitigation: they remain an explicit R30.7 residual until a
  lower network boundary or target-auto-attach test proves coverage.
- Public DNS validation and actual connection are not yet atomic in every
  adapter. This ADR does not widen that residual into localhost authority.

## Alternatives considered

### Allow localhost under Standard

Rejected. Hostname or subnet trust cannot distinguish the selected project's
server from unrelated local services.

### Infer a server from terminal argv or stdout

Rejected. Neither proves which process owns the listener, and bounded terminal
execution intentionally reaps descendants at completion.

### Validate only initial and final navigation URLs

Rejected. Redirects connect before a final check, and scripts, frames, styles,
images, workers, and clicks create independent requests.

### Share the Electron preview browser

Rejected. ADR-0040 separates UserPreview and AgentEffector storage and authority
domains. Coordination is not shared credentials or shared network authority.

## Evaluation evidence

- `cargo test -p optimus-policy` proves exact bindings are copied under Standard
  and missing, malformed, or transferred bindings fail closed.
- `cargo test -p optimus-browser --lib` proves the CDP request authority permits
  only one exact numeric origin while adjacent/private destinations stay denied.
- `cargo test -p optimus-kernel browser::tests --lib` proves HTTP redirects are
  resolved and checked before the next request.
- `cargo test -p optimus-runtime owned_localhost --lib` proves inactive,
  transferred, expired, revoked, stale-generation, dead-listener, foreign-root,
  and non-exact-origin bindings fail closed; it also proves revocation fences
  new use before bounded drain and ordered process cleanup.
- `just check` holds formatting, layering, module size, compilation, clippy, and
  Engineering Memory consistency for the combined slice.

## Conditions for reconsideration

Revisit the IPv4/HTTP-only limit when the runtime can prove IPv6 listener
ownership and tests address-family isolation. Revisit the process-tree binding
only if a broker-owned proxy or socket-activation design provides stronger
atomic ownership without reducing framework compatibility. Do not broaden the
lease based on hostname convenience alone.

## Relevant code

- `crates/optimus-policy/src/lib.rs` — exact binding and broker constraints
- `crates/optimus-runtime/src/owned_localhost.rs` — non-bearer live registry,
  use guards, expiry/generation fencing, revocation, cleanup, and audit evidence
- `crates/optimus-kernel/src/browser.rs` — pre-connect HTTP redirect validation
- `crates/optimus-browser/src/lib.rs` — immutable CDP request authority
- `crates/optimus-runtime/src/process_ownership.rs` — existing retained
  process-tree containment on which issuance must build

## Relevant tests

- `crates/optimus-policy/src/lib.rs` — broker binding and compatibility tests
- `crates/optimus-runtime/src/owned_localhost.rs` — registry and lifecycle tests
- `crates/optimus-browser/src/lib.rs` — exact CDP origin/request tests
- `crates/optimus-kernel/src/browser.rs` — manual redirect policy tests
