---
doc_id: decisions-0039-files-mutate-effect-taxonomy
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-25 - Program: program P22
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - crates/optimus-graph/src/lib.rs
  - crates/optimus-runtime/src/lib.rs
  - crates/optimus-packs/src/lib.rs
  - crates/optimus-kernel/src/lib.rs
depends_on:
  - docs/decisions/0016-fs-sandbox-allowlist.md
  - docs/decisions/0018-fail-closed-runtime-safety.md
  - docs/decisions/0031-safe-project-work-loop.md
  - docs/decisions/0035-command-capability-envelope.md
  - docs/decisions/0036-domain-modularity-single-catalog.md
  - docs/plans/product-complete-program.md
validated_by:
  - crates/optimus-runtime/tests/path_confinement.rs
  - crates/optimus-runtime/tests/approvals_surface.rs
  - crates/optimus-kernel/tests/domain_modularity.rs
---

# ADR-0039: Files-mutate effect taxonomy (program P22)

- **Status:** Accepted
- **Date:** 2026-07-25
- **Program:** program P22

## Context

Agent host mutation was limited to `WriteFile` / `ProjectWriteFile` (plus
commands). Product-complete requires mkdir/rename/delete/patch under the same
Work Graph + SmartDeny + cap-std plane (no second ambient write path).

## Decision

1. Extend `Effect` with exact variants (and Project* twins bound to
   `workspace_sha256`):
   - `Mkdir` / `ProjectMkdir`
   - `DeletePath` / `ProjectDeletePath`
   - `RenamePath` / `ProjectRenamePath`
   - `PatchFile` / `ProjectPatchFile` (exact single-occurrence string replace)
2. `is_high_risk` includes every host-mutating file op; `AssertFileEquals` stays
   non-high-risk.
3. Skill class `FsWorkspace` covers all file-mutate effects via
   `Effect::requires_fs_workspace_skill`.
4. Tools: `mkdir`, `delete_path`, `rename_path`, `patch_file` in core pack;
   kernel dispatch always uses Project* effects with active workspace hash.
5. Rename: both paths pass `safe_relative_path` (Normal components only, secret
   basenames denied); no cross-root rename.
6. Patch: fail closed if `old_string` empty or match count ≠ 1; apply via atomic
   write replace; never success without receipt.
7. Crash recovery: file-mutate effects are `Interrupted` (replay-safe), not
   ambiguous command outcomes.
8. Specialists: do not silently widen `workspace_writer` tool ceiling in this
   ADR; registered-only (ADR-0033).

## Consequences

- Positive: single mutate plane; SmartDeny exact-action extends naturally.
- Negative: core pack schema tokens grow (still under default budget).
- Residual: campaign `StepKind` still Write/Run only until a follow-up; concurrent
  multi-project mutate lease is settings-honest but may ship in the same program
  wave as isolation enforcement tests.

## Alternatives considered

- Shell-based mutate via `RunCommand` — rejected (weaker receipts, harder
  confinement proofs).
- Ambient `FsRoots::write` IPC — rejected (second plane, SmartDeny bypass risk).

## Risks

- Incomplete match arms when adding variants — mitigated by exhaustive Rust
  matches + closed tool registry (ADR-0036 / program P21).

## Documentation completion addendum (2026-07-31)

## Reasons

The decision makes the invariant in the Decision section explicit and testable. It is preferred because the failure described in Context cannot be managed reliably through prompt convention or caller discipline alone.

## Evaluation evidence

- `crates/optimus-runtime/tests/path_confinement.rs`
- `crates/optimus-runtime/tests/approvals_surface.rs`
- `crates/optimus-kernel/tests/domain_modularity.rs`

## Conditions for reconsideration

Reconsider when the named boundary or threat model changes and a replacement preserves typed enforcement, observability, deterministic failure, and regression coverage.

## Relevant code

- `crates/optimus-graph/src/lib.rs`
- `crates/optimus-runtime/src/lib.rs`
- `crates/optimus-packs/src/lib.rs`
- `crates/optimus-kernel/src/lib.rs`

## Relevant tests

- `crates/optimus-runtime/tests/path_confinement.rs`
- `crates/optimus-runtime/tests/approvals_surface.rs`
- `crates/optimus-kernel/tests/domain_modularity.rs`
