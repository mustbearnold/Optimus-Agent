---
doc_id: decisions-0036-domain-modularity-single-catalog
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0036: Domain modularity — single catalog and memory planes (P13), including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-packs/**
  - crates/optimus-memory/**
  - crates/optimus-skills/**
  - crates/optimus-store/**
  - crates/optimus-kernel/src/lib.rs
  - scripts/gates/check-domain-modularity.py
depends_on:
  - docs/decisions/0002-memory-invariants.md
  - docs/decisions/0005-skills-2.md
  - docs/decisions/0016-canonical-tool-contract.md
  - docs/decisions/0017-engineering-memory-separation.md
validated_by:
  - crates/optimus-kernel/tests/domain_modularity.rs
  - crates/optimus-packs/tests/packs_budget.rs
  - crates/optimus-skills/tests/skills_lifecycle.rs
  - crates/optimus-memory/tests/metamemory_mvp.rs
  - crates/optimus-runtime/tests/skill_bridge.rs
  - scripts/gates/check-domain-modularity.py
---

# ADR-0036: Domain modularity — single catalog and memory planes (P13)

- **Status:** Accepted
- **Date:** 2026-07-25

## Context

Domain modularity was **A-**: deep modules existed, but adversarial review still
hunted for second tool catalogs, memory/session/skill/EM plane confusion, store
schema leaks into chat UI, and pack policy bypasses via surface helpers.

P13 requires **S+++** when each plane has a single owner, host effects remain
SmartDeny-gated without plane-confused grants, and tests + a merge-adjacent
script would fail if a second catalog or grant path appears.

## Decision

1. **`optimus-packs::ToolDesc` / `ToolId` / `ToolInvocation` is the only tool
   catalog.** Kernel re-exports `ToolDesc` as `ToolSchema` for provider mapping
   only; dispatch resolves `resolve_loaded_tool` then matches on
   `ToolInvocation` — never free-text tool registries.
2. **Memory planes stay separate** (session, MetaMemory, skills, Work Graph,
   Engineering Memory). MetaMemory `RecallPurpose::ActionAuthorize` fails closed.
   Skills grant only class-scoped permissions (`FsWorkspace` → writes,
   `Terminal` → commands). Engineering Memory and session rows never issue
   SmartDeny grants.
3. **Store** owns Work Graph projections only — no chat message tables.
4. **Gate:** `scripts/gates/check-domain-modularity.py` plus
   `crates/optimus-kernel/tests/domain_modularity.rs` and existing packs/skills/
   memory hold suites.

## Consequences

- Positive: Domain mark can move to **S+++** with executable evidence.
- Positive: Coding agents and reviews have a greppable forbidden-pattern gate.
- Residual: product knowledge / retrieval indexes remain unimplemented (not a
  modularity hole — absence of a second catalog, not missing features).

## Alternatives considered

- **Merge skills into packs.** Rejected: procedural vs capability catalogs are
  different planes (ADR-0005 / ADR-0016).
- **Allow memory Action use for “safe” claims.** Rejected: memory never grants
  live capability.

## Risks

- New surfaces inventing local tool lists. Mitigated by domain modularity script
  and code review against AGENTS laws.

## Conditions for reconsideration

- If MCP tools become first-class, they must enter through packs descriptors,
  not a parallel registry.

## Documentation completion addendum (2026-07-31)

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Evaluation evidence

- `crates/optimus-kernel/tests/domain_modularity.rs`
- `crates/optimus-packs/tests/packs_budget.rs`
- `crates/optimus-skills/tests/skills_lifecycle.rs`
- `crates/optimus-memory/tests/metamemory_mvp.rs`
- `crates/optimus-runtime/tests/skill_bridge.rs`
- `scripts/gates/check-domain-modularity.py`

## Relevant code

- `crates/optimus-packs/**`
- `crates/optimus-memory/**`
- `crates/optimus-skills/**`
- `crates/optimus-store/**`
- `crates/optimus-kernel/src/lib.rs`
- `scripts/gates/check-domain-modularity.py`

## Relevant tests

- `crates/optimus-kernel/tests/domain_modularity.rs`
- `crates/optimus-packs/tests/packs_budget.rs`
- `crates/optimus-skills/tests/skills_lifecycle.rs`
- `crates/optimus-memory/tests/metamemory_mvp.rs`
- `crates/optimus-runtime/tests/skill_bridge.rs`
- `crates/optimus-store/src/capability_grants.rs` — unit tests for
  capability-grant TTL clamping ([8 h, 24 h] window), key/scope validation,
  and the revoke/renew/live lifecycle (2026-08-11).
- `scripts/gates/check-domain-modularity.py`
