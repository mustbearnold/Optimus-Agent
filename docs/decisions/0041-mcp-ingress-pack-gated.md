---
doc_id: decisions-0041-mcp-ingress-pack-gated
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0041: Pack-gated MCP ingress (no second tool catalog), including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-ops/src/mcp.rs
  - crates/optimus-host/src/extensibility.rs
depends_on:
  - docs/decisions/0036-domain-modularity-single-catalog.md
  - docs/decisions/0016-canonical-tool-contract.md
validated_by:
  - crates/optimus-ops/src/mcp.rs
---

# ADR-0041: Pack-gated MCP ingress (no second tool catalog)

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

Program P27 requires third-party tools via MCP without inventing a parallel tool
registry. ADR-0036 freezes a single ToolDesc catalog under optimus-packs.

## Decision

1. MCP stdio and HTTP clients live in `optimus-ops` (not the kernel waist).
2. Server tool offers map to allowlisted ToolDesc rows under a pack permission
   ceiling. Unmapped offers are dropped; collisions with built-in ToolId fail
   closed.
3. Mapped MCP tools use `ToolInvocation::Unavailable` until a host effector is
   registered under SmartDeny — advertisement never implies unrestricted
   execution.
4. HTTP MCP requires public http(s) URLs (no private/metadata destinations).
5. Stdio MCP is mock-first for product exit; live child spawn bounds follow the
   command capability envelope in later hardening.

## Consequences

- Desktop IPC `mcp_status` / `mcp_tools` expose mapped tools only.
- Domain modularity and pack budget gates remain the authority for tool ads.

## Documentation completion addendum (2026-07-31)

## Alternatives considered

The pre-decision behaviour and an ad hoc implementation were considered. Both were rejected because they leave the boundary described by this decision implicit, inconsistently enforced, or unobservable.

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Risks

Implementation can drift from the accepted boundary while the prose remains unchanged. Source-bound documentation checks, the relevant tests below, and the full repository gate are the mitigation.

## Evaluation evidence

- `crates/optimus-ops/src/mcp.rs`

## Conditions for reconsideration

Reconsider when the named boundary or threat model changes and a replacement preserves typed enforcement, observability, deterministic failure, and regression coverage.

## Relevant code

- `crates/optimus-ops/src/mcp.rs`
- `crates/optimus-host/src/extensibility.rs`

## Relevant tests

- `crates/optimus-ops/src/mcp.rs`
