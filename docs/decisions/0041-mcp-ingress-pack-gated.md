---
knowledge_type: decision
status: current
covers:
  - crates/optimus-ops/src/mcp.rs
  - crates/optimus-host/src/extensibility.rs
depends_on:
  - docs/decisions/0036-domain-modularity-single-catalog.md
  - docs/decisions/0016-canonical-tool-contract.md
validated_by:
  - crates/optimus-ops/src/mcp.rs
last_verified_commit: null
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
