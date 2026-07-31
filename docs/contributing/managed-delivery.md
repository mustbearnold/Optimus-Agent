---
knowledge_type: process
status: current
owns:
  - scripts/managed_delivery.py
  - scripts/test_managed_delivery.py
  - scripts/managed_branch_retirement.py
  - scripts/test_managed_branch_retirement.py
  - docs/contributing/managed-delivery.md
watches:
  - justfile
  - scripts/verify.sh
  - AGENTS.md
validated_by:
  - scripts/test_managed_delivery.py
  - scripts/test_managed_branch_retirement.py
last_verified_commit: null
---

# Managed delivery

This is a developer control-plane workflow. It does not change Optimus Agent
runtime permissions, approval policy, prompts, or product behaviour.

## Public commands

Coding agents use exactly:

```text
just checkpoint <label>
just undo <label>
just land <task-id> --model <model> --effort <level>
just branch-retirement-plan '<superseded-json>'
just retire-branches <plan-sha256> '<superseded-json>'
```

They do not run raw history-changing Git. `scripts/managed_delivery.py` is the
single trusted implementation behind these commands.

`label` and `task-id` are lowercase bounded slugs. A model is recorded exactly
as supplied by the producing coding agent. Effort is one of `none`, `minimal`,
`low`, `medium`, `high`, `xhigh`, `max`, or `ultra`. Repository state cannot
independently prove which hosted model produced a patch, so the receipt labels
this provenance as caller-attested rather than verified.

## Checkpoints

A checkpoint snapshots tracked files, deletions, and non-ignored untracked
files through an alternate Git index. It creates a private
`refs/optimus/checkpoints/<worktree-id>/<label>` commit without changing HEAD,
the task branch, the real index, or working files.

Ignored build output, credentials, and runtime homes are intentionally excluded.
Reusing a label for the same tree is idempotent. Reusing it for different
progress refuses rather than silently moving recovery history.

## Undo

`undo` restores only the invoking assigned worktree. Before restoring, it creates
an automatic `before-undo-*` safety checkpoint. HEAD and the task branch do not
move, and `undo` never changes remote main or reverses a landed task.

The restore makes current non-ignored files Git-known, then updates to the
checkpoint tree. That permits exact removal of files created after the
checkpoint without a broad filesystem clean. Ignored files remain outside the
checkpoint contract.

## Land

Land is conservative:

1. Resolve the linked worktree from its `.git` pointer and reject the bare root,
   main checkout, detached HEAD, unmerged index, or an in-progress Git operation.
2. Read remote `refs/heads/main` and require it to be an ancestor of the task
   branch. If main advanced independently, refuse; land never merges or rebases.
3. Snapshot the candidate with an alternate index and create an automatic
   pre-land checkpoint.
4. Run the complete `scripts/verify.sh all` contract with skips forbidden.
   This is the command behind `just verify`. A later gate cache may safely
   narrow reruns, but the initial implementation always chooses full evidence.
5. Require the candidate tree and branch HEAD to remain unchanged during
   verification.
6. Generate one machine-authored commit with `git commit-tree`, parented directly
   to the remote-main SHA. Local checkpoint or task-branch commits are therefore
   not copied into delivery history.
7. Push that exact commit non-force to remote `refs/heads/main`, then read it
   back. Local `refs/heads/main` is never moved because it may be checked out in
   another linked worktree.
8. Write an immutable task receipt and move only the invoking task branch to the
   landed commit.

The generated message contains the task id, affected seams, conservative
diff-derived symbols, full-suite fixture result, gate status, producing model,
and reasoning effort. Ambiguous hunks use `path::<module-scope>` rather than an
invented function name.

## Records and refusal

Shared, ignored control state lives under `local/land/`:

- `checkpoints/` — private checkpoint receipts;
- `tasks/<task-id>/` — immutable attempts and the terminal land receipt;
- `evidence/` — full verification output;
- `locks/` — per-worktree and final-land serialization;
- `tmp/` — alternate indexes.

Writes use temporary files, `fsync`, and atomic rename. A red gate writes its
evidence and refusal attempt but does not move the task branch or remote main. A
push rejection retains the private candidate commit for diagnosis and retry.
A landed task id is immutable; repeating the same provenance is idempotent,
while different model or effort values refuse.

## Remote branch retirement

Branch deletion is deliberately separate from landing. The plan command reads
every remote head and classifies each non-main tip as either an ancestor already
contained in `main` or an explicitly superseded tip with a recorded reason. Any
unresolved tip refuses the plan.

The plan's canonical JSON has a SHA-256 digest. Execution recalculates the
complete remote state and refuses if that digest changed. It then deletes every
reviewed branch in one atomic push, with an exact-SHA lease per ref. `main` is
structurally excluded, so a red push removes no branches and concurrent branch
movement cannot be silently discarded.

A successful operation writes an immutable receipt under
`local/land/branch-retirements/`. Agents do not replace this command with raw
`git push --delete`, non-atomic loops, or `gh`.

If remote main changes while gates run, land refuses before committing or
pushing. A race after the final check is still protected by Git's non-force
fast-forward rule. Reversing an already landed change requires a new task and
new verified inverse patch; `undo` never rewrites published history.

## Trust boundary and prerequisites

The helper, repository instructions, and agent command deny-lists prevent
accidental bypass. They are not a security boundary against a malicious process
running as the same operating-system user with the same Git credentials.
Absolute remote enforcement would require server policy or credentials exposed
only to the land helper.

The `just` executable must be installed for the public commands to exist. If it
is unavailable, agents report that tooling limitation; they do not substitute
direct Python or raw Git.
