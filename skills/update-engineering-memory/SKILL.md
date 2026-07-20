---
name: update-engineering-memory
description: Use after substantial Optimus code, contract, tool, workflow, model, memory, security, observability, evaluation, or architecture changes to detect affected repository knowledge, update only evidence-backed documents, regenerate deterministic indexes, and validate freshness.
---

# Update Optimus Engineering Memory

Maintain repository development knowledge without conflating it with runtime
memory or turning documentation into speculative architecture.

## Hard rules

- Source and executable tests outrank documentation.
- Run `check` before `generate`; generation refreshes the recorded baseline.
- Never edit `.engineering-memory/*.json` manually.
- Preserve old ADRs. Add a superseding ADR instead of rewriting history.
- Label important claims as confirmed, inferred, planned, or unresolved.
- Do not register a specialist agent, workflow, tool, model, prompt, GPU path, or
  integration that is not present in source.
- Do not mark a gap resolved because code compiles.
- Use generated source-tree hashes in Git checkouts, worktrees, and archives;
  never embed or invent a commit SHA as generated self-identity.
- Keep tiny changes cheap: update only impacted knowledge.

## Procedure

### 1. Establish repository identity

From the repository root:

```text
python scripts/engineering_memory.py check
```

A nonzero result is expected when covered source changed. Save the listed
`CHANGED`, `STALE`, and `IMPACT` paths before doing anything else.

Inspect Git status/diff when available, but use the repository index and direct
file comparison as deterministic generated identity in every environment.

### 2. Trace ownership and evidence

For every changed behavior:

1. Identify the owning application/crate and canonical type.
2. Read impacted documents from `.engineering-memory/change-impact.json`.
3. Read relevant ADRs and high-risk contracts.
4. Inspect focused tests and evaluation cases.
5. Determine whether behavior is current, inferred, planned, or unresolved.

Do not infer safety, cancellation, idempotency, replay, or permissions from a
name.

### 3. Update curated knowledge

Update only affected Markdown, ADRs, or this skill. Keep frontmatter patterns
accurate:

```yaml
---
knowledge_type: architecture
status: current
covers:
  - crates/owner/src/**
depends_on:
  - docs/decisions/NNNN-decision.md
validated_by:
  - crates/owner/tests/**
last_verified_commit: <historical source commit or null>
---
```

`last_verified_commit` is curated historical provenance, not the SHA of the
commit containing the document. A commit cannot embed its own identity; keep
remote/commit receipts in external delivery evidence. Record reusable lessons
only when they can change future implementation or validation behavior.

### 4. Reconcile deterministic extractors

If canonical source shape changed, update `scripts/engineering_memory.py` and
its tests. The parser must fail closed on unknown shapes.

Examples:

- Cargo packages/dependencies come from `cargo metadata`.
- Tool identity/schema/pack ownership comes from
  `optimus-packs::ToolDesc` and `builtin_catalog`.
- Specialist-agent count stays zero until a real typed agent definition exists.
- Workflow registry entries must point to implemented source and terminal states.

Do not “fix” extraction by hard-coding an output that no longer reconciles with
source.

### 5. Verify behavior

Run focused product tests first, then the relevant integration/evaluation
surface. For Engineering Memory itself run:

```text
python -m unittest scripts/test_engineering_memory.py -v
```

Fix parser, reference, or frontmatter failures before generation.

### 6. Regenerate and validate

```text
python scripts/engineering_memory.py generate
python scripts/engineering_memory.py validate
python scripts/engineering_memory.py check
```

Expected final markers:

```text
ENGINEERING_MEMORY_VALID
ENGINEERING_MEMORY_CURRENT
```

Warnings are known gaps and must be reported. Use `validate --strict` when a
change is intended to close every warning in its scope; do not weaken strict
checks to approve a poor change.

### 7. Report

Report:

- changed code/knowledge surfaces;
- generated registry/package/tool/workflow counts;
- focused and integration/evaluation results;
- remaining warnings and stale knowledge;
- security, cancellation, replay, approval, and CPU-fallback implications;
- exact tree hash from `.engineering-memory/repository-index.json`.

## Pitfalls

- **Generate first:** destroys the useful old-vs-current staleness comparison.
- **Manual JSON repair:** will be overwritten and hides extractor drift.
- **Planned-as-current prose:** creates false capabilities for later agents.
- **Broad `covers` globs:** make unrelated tiny changes expensive and obscure
  ownership.
- **Missing tests disguised as docs:** a contract documents behavior; it does not
  enforce it.
- **Ambient archive conversion:** use `git -c core.autocrlf=false archive` for
  staged-tree verification so exported bytes match Git blobs on every machine.
- **Empty agent registry “fixed” with aspirational entries:** absence is the
  correct current fact.
- **GPU dependency without fallback evidence:** record it as a blocking gap.
- **Historical ADR cleanup:** never renumber or rewrite accepted history merely
  to remove a warning; add an index or superseding ADR deliberately.
