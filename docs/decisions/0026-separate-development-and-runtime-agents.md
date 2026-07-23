---
knowledge_type: decision
status: current
covers:
  - AGENTS.md
  - OPTIMUS_AGENTS.md
  - crates/optimus-kernel/src/lib.rs
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
validated_by:
  - crates/optimus-kernel/src/lib.rs
last_verified_commit: null
---

# ADR-0026: Separate development AGENTS.md from Optimus runtime constitution

- **Status:** Accepted
- **Date:** 2026-07-22

## Context

Optimus has a repository `AGENTS.md` used by developers and coding agents that
work on the source tree. Users also chat with the installed Optimus product.
Mixing those instruction surfaces causes the product agent to either invent
development laws or deny project constitutions incorrectly.

## Decision

Keep two explicit files:

1. `AGENTS.md` — development-only engineering laws for the Optimus repository.
2. `OPTIMUS_AGENTS.md` — product runtime constitution injected into Optimus chat
   system prompts.

The kernel embeds `OPTIMUS_AGENTS.md` into new/refreshed system messages. It does
not inject repository development `AGENTS.md` into product turns.

When a user later selects a third-party project workspace, that project may have
its own `AGENTS.md` / `HERMES.md` / `CLAUDE.md`. Those are project constitutions
and remain distinct from both files above.

## Alternatives considered

- Inject repository `AGENTS.md` into product sessions. Rejected because it leaks
  build-only rules into user workspaces.
- Keep one combined instruction file. Rejected because repository and product
  ownership boundaries are different.

## Reasons

The split makes the instruction source explicit, testable, and stable across
installation. It also prevents project-local constitutions from being confused
with Optimus's own runtime contract.

## Risks

- The embedded runtime constitution can drift from its source file.
- New session construction paths can accidentally inject the development file.

## Evaluation evidence

Kernel tests assert that the embedded system prompt contains
`OPTIMUS_AGENTS.md` content and excludes development-only `AGENTS.md` rules.

## Conditions for reconsideration

Reconsider only if product and repository sessions gain a typed instruction
registry that preserves the same source and ownership separation.

## Relevant code

- `crates/optimus-kernel/src/lib.rs`
- `AGENTS.md`
- `OPTIMUS_AGENTS.md`

## Relevant tests

- `crates/optimus-kernel/src/lib.rs` unit tests for runtime system instructions

## Consequences

- Product answers about “which agents file do you use?” can name
  `OPTIMUS_AGENTS.md` as the runtime constitution.
- Development workflows continue to open root `AGENTS.md`.
- Engineering Memory remains development knowledge and is not auto-injected into
  product chat.
