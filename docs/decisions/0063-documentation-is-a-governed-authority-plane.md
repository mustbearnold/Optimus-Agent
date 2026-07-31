---
doc_id: decisions-0063-documentation-is-a-governed-authority-plane
doc_type: decision
plane: decision
status: current
authority: record
summary: Establishes typed documentation planes, exclusive authority routes, durable source-binding verification, deterministic discovery, and retrieval evaluation.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - docs/README.md
  - docs/current/**
  - docs/authority-routes.json
  - docs/catalog.json
  - docs/verification-lock.json
  - scripts/docs_system.py
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/decisions/0032-engineering-memory-compact-lenses.md
  - docs/decisions/0061-generated-engineering-memory-is-a-disposable-cache.md
validated_by:
  - scripts/test_docs_system.py
  - evals/docs-authority/questions-v1.json
---

# ADR-0063: Documentation is a governed authority plane

- **Status:** Accepted
- **Date:** 2026-07-31

## Context

Optimus had extensive documentation but no dependable front door, universal
lifecycle metadata, exclusive topic authorities, durable semantic-staleness
record, or measured answer retrieval. Old plans and dated evidence could look
as authoritative as current architecture. Engineering Memory accelerated
source discovery, but its generated baseline could not by itself prove that a
human or agent had revalidated changed prose against changed source.

## Decision

1. Every document has a stable id, documentation type, knowledge plane,
   lifecycle, authority class, summary, review date, and next review deadline.
2. The planes are `current`, `work`, `decision`, `evidence`, and `history`.
   Default retrieval excludes evidence and history.
3. Common question domains have exactly one current canonical authority in
   `docs/authority-routes.json`. Canonical documents without a route fail the
   repository gate.
4. `docs/README.md` is the human and agent front door. A deterministic catalog,
   bounded search, and route-context command provide progressively deeper
   discovery without dumping the tree into model context.
5. Every current or planned document and each available source binding are
   recorded in `docs/verification-lock.json`. Ordinary generation and
   validation never refresh this lock. A named `docs-refresh` is the explicit
   review act.
6. Local links, reference links, heading anchors, repository containment,
   metadata, review expiry, generated indexes, authority routes, and a
   fresh-question retrieval benchmark are mandatory offline gates.
7. The four practitioner forms—tutorial, how-to, reference, and explanation—
   follow the Diátaxis distinction. Decisions, evidence, and history remain
   additional governance record types.

## Reasons

Documentation authority must be machine-checkable because filename intuition
and generated freshness both failed to distinguish current truth from an
extensive historical record. Explicit routes make ambiguity a failing contract;
the durable lock separates semantic review from deterministic generation; and
retrieval evaluation measures the experience a fresh agent actually receives.

## Consequences

- A fresh coding agent can ask by intent and receive a bounded authoritative
  pack rather than guess from filenames.
- Old work remains auditable without quietly steering current development.
- A prose or bound-source edit cannot look freshly verified merely by running a
  generator.
- Current documentation carries an expiring semantic review obligation.
- Adding a canonical authority requires adding its exclusive route and a
  representative retrieval question where appropriate.

## Alternatives considered

- **A hand-maintained index only:** discoverable, but incapable of detecting
  drift, contradictions, broken anchors, or retrieval regressions.
- **Generated freshness from the current tree:** always green immediately after
  generation and therefore not evidence of semantic review.
- **Delete all historical material:** cleaner at first glance, but destroys
  decision and delivery provenance.
- **Make network link and prose services mandatory:** harms deterministic
  offline development. External URL and style audits may supplement this gate;
  they do not replace its local authority guarantees.

## Risks

- Metadata can become ceremonial. Review expiry, route exclusivity and the
  retrieval benchmark make neglected metadata observable.
- Keyword routing can overfit its fixture. The suite uses natural questions,
  retains top-three diagnostics and must grow when real agent failures appear.
- A reviewer can acknowledge a stale document incorrectly. Named refresh makes
  the act auditable but cannot replace technical judgment; source and tests
  therefore remain higher authority.
- Broad source globs could absorb ignored build output and become timing
  dependent. Bindings therefore enumerate only Git-tracked and non-ignored
  candidate files, never caches, dependencies, or compiled output.

## Evaluation evidence

- All repository Markdown documents pass metadata, lifecycle, local link,
  anchor, containment, generated-view and source-binding checks.
- Sixteen representative fresh-agent questions resolve to their expected
  canonical authority at rank one.
- Unit tests cover duplicate anchors, broken local fragments, orphan canonical
  documents, routing ambiguity and non-mutating stale-lock validation.

## Conditions for reconsideration

The catalog no longer fits comfortably in local deterministic search, or
measured retrieval shows that a repository-local index cannot maintain at
least 95% top-one authority resolution for the maintained question suite.

## Relevant code

- `scripts/docs_system.py`
- `docs/authority-routes.json`
- `docs/catalog.json`
- `docs/verification-lock.json`

## Relevant tests

- `scripts/test_docs_system.py`
- `evals/docs-authority/questions-v1.json`
- `scripts/verify.sh`

## Industry references reviewed

Reviewed on 2026-07-31: [Diátaxis](https://diataxis.fr/start-here/) for distinct
practitioner purposes, [Backstage TechDocs](https://backstage.io/docs/features/techdocs/concepts/)
for docs-as-code discovery and ownership, [Vale](https://docs.vale.sh/) for
prose-policy automation, and [Lychee](https://github.com/lycheeverse/lychee) for
link-checking maturity. The mandatory implementation remains offline and
repository-local; network-dependent checks are supplementary.
