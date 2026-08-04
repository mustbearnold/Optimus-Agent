---
doc_id: contributing-project-hygiene
doc_type: how-to
plane: current
status: current
authority: supporting
summary: just clean-report inventories rebuildable output in the assigned worktree and reports larger shared-root or sibling-worktree candidates without touching them. just clean deletes only a closed, tested allowlist inside the assigned linked...
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: process
covers:
  - scripts/tools/project_hygiene.py
  - justfile
validated_by:
  - scripts/tests/test_project_hygiene.py
---

# Project hygiene

`just clean-report` inventories rebuildable output in the assigned worktree and
reports larger shared-root or sibling-worktree candidates without touching
them. `just clean` deletes only a closed, tested allowlist inside the assigned
linked worktree.

The cleaner refuses the bare repository root, symlink escapes, non-ignored
paths, tracked files, and unknown artifact classes. It never deletes `.git`,
`.optimus`, `.codex`, `local/`, evidence, reference snapshots, shared tools, or
another worktree. The ignored `.engineering-memory/` cache is rebuildable and
is cleaned as a unit; curated Engineering Memory authority lives in source and
`docs/`, not that directory.

Run `just clean-report` at the end of build-heavy work. Material reported as
`REPORT_ONLY` needs a separately scoped cleanup after its owning worktree and
evidence have been classified.
