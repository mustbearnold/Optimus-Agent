---
doc_id: docs-home
doc_type: reference
plane: current
status: current
authority: canonical
summary: This is the documentation front door. A current answer must be reachable from this page; documents not surfaced here or by the generated catalog are supporting detail or history, never hidden authority.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# Optimus Agent documentation

This is the documentation front door. A current answer must be reachable from
this page; documents not surfaced here or by the generated catalog are
supporting detail or history, never hidden authority.

## Start with the answer you need

| Need | Canonical starting point |
|---|---|
| What Optimus is now | [Current status](current/status.md) |
| Where Optimus is going | [Current roadmap](current/roadmap.md) |
| How the system works | [System overview](architecture/system-overview.md) |
| How coding agents develop Optimus | [Development law](../AGENTS.md) |
| How Optimus behaves for users | [Runtime constitution](../OPTIMUS_AGENTS.md) |
| How changes reach main | [Managed delivery](contributing/managed-delivery.md) |
| What a path is, whether it ships, or whether it can be removed | [Repository components](repository-components.md) |
| What changed when, what is stale, or what should be cleaned up | [Temporal project knowledge](project-knowledge.md) |
| Which decision governs something | [Decision index](decisions/README.md) |
| Which source/test owns a subsystem | [Repository map](maps/repository-and-ownership.md) |
| Why an old document says something different | [History policy](current/history-policy.md) |

## Documentation types

The current knowledge layer separates the four practitioner needs described by
Diátaxis while retaining engineering governance records:

- **Tutorial** — guided learning for a newcomer.
- **How-to** — steps for a specific outcome.
- **Reference** — precise facts, contracts, commands, and current state.
- **Explanation** — architecture, reasoning, and conceptual relationships.
- **Decision** — permanent ADR history and its current documentary status.
- **Evidence** — bounded observations for a named candidate and date.
- **History** — superseded plans, phase records, and preserved context.

## Current authority pack for AI agents

Load the smallest relevant set, in this order:

1. [Current status](current/status.md)
2. [Current roadmap](current/roadmap.md) only for prioritization questions
3. [System overview](architecture/system-overview.md) for architecture
4. one relevant map, contract, ADR, or how-to selected by `just docs-context`

Do not load all of `docs/`, all ADRs, raw evidence, or generated Engineering
Memory into a prompt. History explains why; it does not override current
source, tests, constitutions, or the current authority pack.

## Find documentation

```text
just docs-check
just docs-search "approval behaviour"
just docs-context architecture.overview
just docs-benchmark
just orient
just explain-path apps/optimus-desktop
just project-status
just cleanup-candidates
```

`docs-check` fails on missing metadata, orphaned current authority, ambiguous
canonical topics, invalid local links or anchors, stale generated views,
unreviewed source-binding changes, expired reviews, and retrieval regressions.
`docs-benchmark` proves that representative fresh-agent questions retrieve the
expected authority within a bounded top-three result set.

## Standards baseline

This system applies the four documentation purposes from
[Diátaxis](https://diataxis.fr/start-here/), docs-as-code discoverability and
ownership ideas described by
[Backstage TechDocs](https://backstage.io/docs/features/techdocs/concepts/), and
the failure classes covered by tools such as
[Vale](https://docs.vale.sh/) and [Lychee](https://github.com/lycheeverse/lychee).
Optimus keeps its mandatory gate repository-local and deterministic: network
URL health and optional prose-style audits supplement, but never weaken, the
offline authority, link, staleness, and retrieval checks.
