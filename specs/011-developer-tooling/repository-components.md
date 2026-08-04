---
doc_id: repository-components
doc_type: reference
plane: current
status: current
authority: canonical
summary: Canonical development knowledge system for understanding every major Optimus repository component, its lifecycle, distribution, common confusions, outputs, and removal conditions.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: repository-component-authority
owns:
  - docs/repository-components.json
  - scripts/repository_ontology.py
  - evals/repository-orientation/questions-v1.json
covers:
  - Cargo.toml
  - apps/*/Cargo.toml
  - apps/*/package.json
  - crates/*/Cargo.toml
depends_on:
  - AGENTS.md
validated_by:
  - scripts/test_repository_ontology.py
  - scripts/docs_system.py
---

# Repository component authority

This is the front door for answering “what is this?”, “does it ship?”, “is it
development machinery?”, and “can it be removed?” without asking an agent to
reconstruct years of project history.

The single authored semantic database is
[`repository-components.json`](../../docs/repository-components.json). Cargo manifests
remain authoritative for Rust package identity and membership; npm manifests
remain authoritative for JavaScript package identity. The database adds only
the meanings those manifests cannot express: storage location, concern,
distribution, lifecycle, retention, common misconceptions, paired components,
generated-output destination, and removal criteria.

One manifest fact is mirrored rather than described: `default_member` states
whether a workspace member belongs to `[workspace].default-members`. A row that
narrates its own build participation in prose can drift from `Cargo.toml`
silently; a declared boolean cannot. The field is omitted where the row claims
default membership, and set to `false` on the three members a bare `cargo
build` deliberately skips.

## Agent startup

Run:

```text
just orient
just explain-path evals
just explain-path apps/optimus-desktop/ui
just project-status
just path-history spikes/001-leptos-wry-csr
```

The complete generated human view is [`COMPONENTS.md`](../../docs/COMPONENTS.md). Never
edit it manually.

The component database is the semantic present tense. The embedded
[temporal project database](../../specs/009-project-knowledge/project-knowledge.md) joins it to Git event history
and machine-local Development observations so removed paths, lifecycle changes,
and inactive generated caches remain explainable and
queryable.

## Enforcement

The documentation contract fails closed when a new top-level domain, app,
crate, evaluation suite, or developer skill lacks classification. It also
rejects manifest disagreement, default-membership disagreement in either
direction, broken component relationships, generated output aimed into
Repository, expired rollback/incubation reviews, stale generated views, and
failed fresh-agent orientation cases.

Executable source outranks the database when they disagree. Such disagreement
is a red gate to repair, not permission to silently trust either claim.
