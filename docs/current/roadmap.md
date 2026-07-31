---
doc_id: current-roadmap
doc_type: reference
plane: current
status: current
authority: canonical
summary: Make Optimus Agent broadly capable, trustworthy, efficient and pleasant enough that people give it real outcomes and it simply gets them done. No single domain, provider, interface, benchmark, project journey or development workflow is...
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: plan
owns:
  - docs/current/roadmap.md
watches:
  - docs/current/status.md
  - docs/evidence/**
  - evals/**
  - crates/**
  - apps/**
covers:
  - docs/current/roadmap.md
depends_on:
  - docs/current/status.md
  - docs/architecture/north-star-2026-07.md
validated_by:
  - scripts/docs_system.py
  - scripts/test_docs_system.py
---

# Current Optimus Agent roadmap

## North star

Make Optimus Agent broadly capable, trustworthy, efficient and pleasant enough
that people give it real outcomes and it simply gets them done. No single
domain, provider, interface, benchmark, project journey or development workflow
is allowed to become the product's identity.

## How work is prioritized

The roadmap is a rolling evidence queue, not a permanent sequence of invented
phase numbers. Choose the smallest complete change that improves a measured
user outcome across one or more capability axes:

1. **Outcome completion** — the requested result exists and is correct.
2. **Low-friction autonomy** — routine confined work proceeds without a
   permission wall while consequential effects retain explicit boundaries.
3. **Breadth and composition** — tools, specialists, workflows and providers
   compose across domains without domain-specific product distortion.
4. **Continuity** — conversations, projects, memory and recovery preserve the
   right context over time without leaking between scopes.
5. **Reliability and observability** — cancellation, retry, replay, provenance
   and one terminal outcome remain inspectable.
6. **Human experience** — TUI, desktop and CLI remain clear, fast and useful to
   non-experts as well as developers.
7. **Learning velocity** — neutral artificial humans, real users and independent
   evaluation convert failures into reproducible improvements.

## Current priority evidence

At this review point, the highest-confidence cross-domain needs are:

- reduce unnecessary approvals for harmless, explicitly requested confined
  changes;
- strengthen multi-turn and longitudinal continuity;
- expand adaptive neutral-human testing without letting its scenarios steer the
  product;
- mature specialist routing and bounded collaboration from registered verticals
  into useful general orchestration;
- keep the documentation and Engineering Memory truth layers smaller and more
  semantically reliable than the history they preserve.

Named programs under `docs/plans/` are historical or supporting implementation
records unless this document explicitly promotes one. They cannot silently
replace this roadmap by calling themselves “primary”.

## Exit measure

There is no honest “finished” claim based only on completed phases. Progress is
measured by a versioned capability matrix and diverse end-to-end journeys whose
results retain exact candidate, provider, model, permissions, artifacts,
terminal outcomes, friction and evaluator provenance.
