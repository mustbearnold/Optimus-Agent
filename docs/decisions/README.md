---
knowledge_type: decision-index
status: current
covers:
  - docs/decisions/*.md
depends_on:
  - docs/architecture/system-overview.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: null
---

# Architecture decision index

This index preserves existing decisions and exposes their current documentary
state. Source and tests still determine whether an accepted design is fully
implemented.

| ID | Decision | Documentary status | Implementation interpretation |
|---|---|---|---|
| 0000 | Locked defaults | Historical locked-defaults note; not full ADR shape | Mixed product constraints; verify individually. |
| 0001 | Kernel language and Work Graph spine | Accepted | Confirmed current core. |
| 0002 | Memory invariants | Accepted; full store originally phased | Confirmed current memory core with documented gaps. |
| 0003 | Policy, budgets, bounded commands | Accepted | Confirmed current core. |
| 0004 | MetaMemory MVP | Accepted | Confirmed current memory core. |
| 0005 | Skills 2.0 | Accepted | Confirmed current skill registry. |
| 0006 | Capability packs | Accepted | Confirmed current pack core; many catalog tools remain unavailable. |
| 0007 | Provider-agnostic turn loop | Accepted | Confirmed current behavior. |
| 0008 | OpenAI-compatible provider | Accepted | Confirmed current adapter. |
| 0009 | Durable sessions | Accepted | Confirmed current behavior. |
| 0010 | Context compression | Accepted | Confirmed current behavior with limited evaluation. |
| 0011 | Codex OAuth | Accepted | Confirmed current adapter and plain-JSON auth-store debt. |
| 0012 | Kernel effectors | Partly superseded by canonical contract | Use ADR-0016 plus current source. |
| 0013 | Command capture | Accepted | Confirmed current behavior. |
| 0014 | Native WebView IPC mode | Accepted | Confirmed current native/HTTP split. |
| 0015 | Preview Browser via CDP | Design accepted; implementation phased | Planned behavior; current browser is bounded HTTP and preview UI is absent. |
| 0016-A | Canonical tool/pack contract | Accepted for PF-04 | Confirmed current canonical contract; independent security review remains separate delivery evidence. |
| 0016-B | Filesystem sandbox allowlist | Accepted, described as in progress | `FsRoots` reads are implemented; runtime write confinement is governed separately by ADR-0018. |
| 0017 | Repository-local Engineering Memory | Accepted | Implemented by docs, skill, generator, tests, and generated indexes. |
| 0018 | Fail-closed runtime path and campaign decoding | Accepted; limitations superseded by 0019 | Historical normal-component and strict-decoding decision; see 0019 for current filesystem and campaign authority. |
| 0019 | Capability files and unified campaign authority | Accepted; limitations superseded by 0020 | Retained workspace capability, shared secret policy, unified campaign authority, deterministic handoff, and job-derived campaign status. |
| 0020 | Work Graph integrity and loopback security | Accepted | Atomic transitions, terminal uniqueness, schema-v4 campaign leases, durable attempts/cancellation, exact-action approvals, and authenticated bounded loopback APIs. |
| 0021 | Owned execution and causal delivery | Accepted | Suspended Job Object command ownership, cooperative model cancellation, leased cron/gateway attempts, reconciled transactional outbox, and session-to-effect provenance. |

## Known documentary debt

- **Confirmed current behaviour:** two files use ADR number `0016`. They remain
  unchanged to preserve history; use the A/B labels only in this index.
- **Confirmed current behaviour:** ADRs 0000–0016 predate the full template and
  omit one or more modern fields such as alternatives, risks, evaluation
  evidence, or reconsideration conditions.
- **Planned behaviour:** new ADRs use the full template. Existing ADRs may gain
  non-destructive addenda, but must not be rewritten to conceal prior reasoning.
- **Unknown or unresolved behaviour:** no automated source proves that every
  historical accepted ADR remains implemented; contract/source/test maps must
  be consulted for each change.
