---
doc_id: decisions-0085-user-sovereignty-over-workflow-and-security-posture
doc_type: decision
plane: decision
status: current
authority: record
summary: Optimus must never force the user to use it one way or another and must never force more security or less security — workflow and security posture are the user's choice, entirely and at all times (owner directive 2026-08-07). The runtime constitution gains a User sovereignty section; approval/permission postures become user-selectable requirements in spec-015 instead of product-mandated fences; product defaults must never override or trap an explicit user choice.
reviewed_on: 2026-08-07
review_by: 2026-11-07
knowledge_type: decision
covers:
  - OPTIMUS_AGENTS.md
  - specs/015-surface-protocol/spec.md
  - crates/optimus-policy/src/command_class.rs
depends_on:
  - docs/decisions/0081-truthful-approval-resolution-and-session-consent.md
  - docs/decisions/0026-separate-development-and-runtime-agents.md
---

# ADR-0085: User sovereignty over workflow and security posture

- **Status:** Accepted
- **Date:** 2026-08-07
- **Source:** Owner directive (2026-08-07): "Optimus Agent must never ever force
  the user to use it one way or another, it must be fully adaptive to the
  users needs, it must never force more security or less security — that is
  completely upto whatever user who uses optimus agent."

## Context

The product has historically shipped opinionated defaults: approval
resolution hard-fences SystemModify-class actions in every profile
(see ADR-0081 and the approval-latency-sandbox map), and workflow shaping
(agent orchestration, autonomy levels) is fixed by the runtime rather than
selected by the user. The owner's directive establishes that neither workflow
nor security posture may be forced on the user: the product must adapt to the
user's needs, and the security posture — in either direction — belongs to the
user, not to the product.

## Decision

1. The runtime constitution (`OPTIMUS_AGENTS.md`) gains a **User sovereignty**
   section: Optimus never forces a way of working; behaviour is fully adaptive
   to the user's needs; Optimus never forces more security or less security;
   security posture (approval depth, permission strictness, autonomy) is the
   user's choice; product defaults must never override or trap an explicit
   user choice about workflow or security posture.
2. Approval/permission posture becomes a **user-selectable** product surface
   (spec-015 requirement, phase-marked): profiles and sessions expose the
   user's chosen posture; no posture is hard-mandated by the runtime.
3. Defaults remain permissible as *starting* positions, but an explicit user
   choice always wins.

## Consequences

- Approval UX and kernel grants must expose posture selection instead of a
  fixed fence; implementation is tracked as a spec-015 requirement with
  testable acceptance criteria (phase-marked pending implementation).
- The constitution is a docs-DB document (`runtime-constitution`): this
  amendment cascades through docs refresh/generate and Engineering Memory.
- No product code changes land in this ADR itself; it records the decision and
  the requirement home so implementation follows the SDD loop.
