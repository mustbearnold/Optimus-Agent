---
doc_id: decisions-0067-managed-cleanup-fingerprints-symlinks-without-following
doc_type: decision
plane: decision
status: current
authority: record
summary: Managed generated-output cleanup fingerprints symlinks by their own metadata and target string and deletes them as entries without following, replacing the blanket symlink refusal that made recommended candidates uncleanable.
reviewed_on: 2026-08-01
review_by: 2026-11-01
knowledge_type: decision
covers:
  - scripts/managed_project_cleanup.py
  - scripts/test_managed_project_cleanup.py
depends_on:
  - docs/decisions/0064-temporal-project-knowledge-is-derived-provenance.md
  - docs/decisions/0066-temporal-project-knowledge-is-a-code-aware-interval-graph.md
  - docs/project-knowledge.md
validated_by:
  - scripts/test_managed_project_cleanup.py
  - scripts/verify.sh
---

# ADR-0067: Managed cleanup fingerprints symlinks without following

- **Status:** Accepted
- **Date:** 2026-08-01

## Context

ADR-0064 made destructive generated-output cleanup an exact-plan operation and
its fixtures proved, among other refusals, blanket symlink rejection: any
symlink anywhere inside a candidate refused the entire plan. Operational use
after ADR-0066 landed exposed a deadlock in that rule. The recommender
(`just cleanup-candidates`, surfaced by `just project-status`) structurally
proves archived `node_modules` trees and browser caches inactive and nominates
them, but every real `node_modules` contains `.bin/*` symlinks, so
`just project-cleanup-plan` refused — and because one poisoned candidate
aborts planning, the refusal also blocked hundreds of megabytes of
symlink-free candidates. The managed path could recommend cleanup it could
never perform, inviting exactly the unmanaged manual deletion the tooling
exists to prevent.

The refusal guarded against deleting through a link into content outside the
planned tree. That risk attaches to *following* symlinks, not to removing the
link entry itself: `os.walk(followlinks=False)` never descends through links
and `shutil.rmtree` unlinks symlink entries without traversing their targets.

## Decision

- `fingerprint()` records a symlink as its own exact entry — relative path, a
  `link` marker, `lstat` mode and mtime, and the literal `os.readlink` target
  string — without reading or statting the target, so dangling links are
  fingerprintable and a retargeted or replaced link changes the plan digest
  and refuses execution.
- Symlink entries contribute no `file_bytes`; a link's target size is not the
  candidate's payload.
- Execution continues to delete with `shutil.rmtree`, which removes symlink
  entries without following them; regression fixtures prove that file and
  directory targets outside the candidate survive execution byte-for-byte.
- A candidate whose root is itself a symlink remains refused.

## Consequences

- Recommended candidates containing symlinks (archived `node_modules`,
  Playwright caches) become cleanable through the managed exact-plan path.
- The "symlink rejection" bullet in ADR-0064's evaluation evidence is
  superseded by this decision; ADR-0064 is preserved unrewritten per the
  documentary-debt rules.
- The exactness contract is strictly widened, never weakened: symlinks gain
  their own fingerprint identity instead of being unrepresentable.

## Evaluation evidence

- `scripts/test_managed_project_cleanup.py` proves fingerprints change when a
  link is retargeted, dangling links fingerprint without error, symlink
  targets outside the candidate survive execution, and a symlink candidate
  root still refuses.
- The previously refused live plan (archive snapshot `node_modules` under
  `Development/Archive/stale-root-snapshot/`) plans and executes with
  receipts under `Development/land/project-cleanups/`.

## Reconsider when

Reconsider if cleanup ever needs to operate on filesystems where directory
entries can alias content in ways `lstat` + target-string identity cannot
capture (bind mounts, reflinked trees), or if a candidate convention emerges
whose safety genuinely depends on target content rather than link identity.
