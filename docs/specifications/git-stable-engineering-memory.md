---
doc_id: specifications-git-stable-engineering-memory
doc_type: history
plane: history
status: historical
authority: historical
summary: - Date: 2026-07-20 - Mode: Standard, deterministic authority repair - Owner: Engineering Memory generator, validator, and current architecture prose
reviewed_on: 2026-07-31
review_by: never
knowledge_type: specification
covers:
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
  - docs/engineering-memory/README.md
  - docs/architecture/system-overview.md
  - docs/maps/repository-and-ownership.md
  - skills/update-engineering-memory/**
depends_on:
  - docs/decisions/0017-engineering-memory-separation.md
  - docs/contracts/high-risk-contracts.md
validated_by:
  - scripts/test_engineering_memory.py
last_verified_commit: 7bb604883362d2e9ccbf0a950422e6c6d7c5b081
---

# Git-stable Engineering Memory authority

- **Date:** 2026-07-20
- **Mode:** Standard, deterministic authority repair
- **Owner:** Engineering Memory generator, validator, and current architecture prose

## Problem and observable outcome

A clean commit at `7bb6048` passes `engineering_memory.py check` but fails strict
`validate` because `repository-index.json` embeds the pre-commit `HEAD`. Any commit
therefore invalidates generated authority even when every indexed source byte is
unchanged. Current Engineering Memory prose also still claims that the repository
has no Git checkout and resurrects fixed wildcard-CORS and missing-lease debt.

After this milestone, generated indexes validate identically in a Git checkout and
a source archive with the same indexed bytes. Current prose matches executable
source for desktop origin policy and campaign/cron/gateway ownership.

## Repository truth

### Confirmed current behaviour

- `repository-index.json` excludes `.git` and `.engineering-memory` from its
  content-addressed `tree_sha256`, but separately embeds ambient `HEAD`.
- Strict validation rebuilds the index from the live environment and compares the
  complete object, so the embedded commit causes post-commit drift.
- No repository consumer reads the generated `git.head` field.
- Frontmatter `last_verified_commit` values are curated provenance and already
  refer to multiple historical commits; they are not live self-identities.
- Desktop HTTP requires a bearer, exact loopback origin, and CSRF on mutations.
- Campaign, cron, and gateway stores implement owner/generation/token/deadline
  claims or leases with stale-owner fencing and focused tests.

### Reasonable inference

The indexed SHA-256 tree is the only deterministic identity that can be embedded
in the same candidate it identifies. Git commit identity must remain external
verification evidence or historical provenance because a commit cannot embed its
own SHA.

### Unresolved assumption

None that changes this milestone. Commit signatures, release provenance, and
remote hosting identity remain delivery concerns, not generated repository bytes.

## Scope

- Remove ambient Git metadata from deterministic generated indexes and use one
  `sha256_tree` verification basis in checkouts, worktrees, and archives.
- Add semantic regressions that reject ambient commit identity and the directly
  contradicted current claims.
- Correct the Engineering Memory guide, update skill, repository ownership map,
  and current architecture debt/observability list, including the stale
  stream-delivery cancellation claim fixed by the preceding milestone.
- Regenerate all `.engineering-memory` outputs through the generator only.
- Commit and push the exact verified candidate to `origin/main` after final gates.

## Non-scope

- Evaluation-driven routing or default-change evaluation gates.
- Updating every historical `last_verified_commit` marker.
- Renumbering ADRs, rewriting historical decisions, or resolving unrelated debt.
- Changing product runtime, persistence schemas, permissions, dependencies, or
  external service behavior.
- Branches, pull requests, deployment, release, publication, or credential access.

## Contracts and invariants

1. Generated output must be a pure function of indexed repository bytes and
   canonical source extraction, not `.git`, branch, `HEAD`, worktree layout, or
   remote state.
   Root entries and domain-presence flags are derived from indexed file records,
   never ambient/untracked directories.
2. UTF-8 repository text records canonicalize CRLF/lone-CR to LF before hashing,
   byte counts, and aggregate identity; undecodable/binary bytes remain exact.
   `repository-index.json.tree_sha256` is the exact aggregate identity of those
   sorted canonical records.
3. Generated staleness entries use `sha256_tree` in every environment.
4. A Git checkout and a source archive containing identical indexed bytes produce
   identical generated maps.
5. `last_verified_commit` remains optional historical provenance in curated
   frontmatter; it is not interpreted as the generated candidate's self-SHA.
6. Strict validation continues to reject any source-derived or generated byte
   drift; this change removes only impossible ambient identity coupling.
7. Current architecture prose must not claim absent Git, wildcard desktop CORS,
   absent campaign leases, absent cron/gateway claims, or consumerless stream
   work after delivery loss.
8. Historical ADRs and specifications remain unchanged unless required as a
   dependency reference.
9. Generated JSON is never edited manually.

## Failure and recovery behaviour

- Malformed or missing generated JSON continues to fail validation.
- A changed indexed source tree continues to fail `check` until curated authority
  is reconciled and generation runs.
- Interrupted generation may leave partial files; rerunning deterministic
  generation restores all maps, after which strict validation must pass.
- Commit or push failure leaves a local commit/candidate; inspect destination
  identity before any retry.

## Execution ledger

### Slice 1 — Deterministic identity

- **Outcome:** generated maps do not change solely because ambient Git metadata
  changes or is absent.
- **Dependencies:** existing tree-hash index and generator tests.
- **RED:** semantic test requires no generated Git object and requires
  `sha256_tree` for repository and staleness verification bases.
- **GREEN:** remove ambient Git extraction and emit stable verification bases.
- **Refactor:** delete the obsolete Git helper; add no compatibility layer because
  there are no consumers.
- **Verify:** focused Engineering Memory unit test and in-memory determinism test.
- **Complete when:** the intended tests execute and pass with no Git-derived field.

### Slice 2 — Current authority reconciliation

- **Outcome:** current guides and debt summaries no longer contradict source.
- **Dependencies:** Slice 1 identity contract and executable lease/origin evidence.
- **RED:** semantic test rejects the exact obsolete no-Git, wildcard-CORS,
  campaign-lease, cron/gateway-claim, and stream-loss claims.
- **GREEN:** minimally update current guide/map/skill/architecture prose.
- **Refactor:** consolidate identity wording around one tree-hash contract.
- **Verify:** focused semantic tests plus strict Engineering Memory validation.
- **Complete when:** obsolete claims are absent and replacements cite current
  bounded behavior without overstating external exactly-once guarantees.

### Final verification and delivery

- Run Engineering Memory tests, generation, strict validation, and currentness.
- Run canonical workspace format, strict Clippy, tests, and rustdoc on final bytes.
- Verify generated indexes from a config-neutral
  `git -c core.autocrlf=false archive` of the staged tree so exported bytes equal
  staged blobs rather than machine checkout policy.
- Inspect exact diff, status, forbidden paths, and indexed tree identity.
- Commit once on `main`, push once to `origin/main`, then independently read back
  local, tracking, and remote identities.

## Acceptance criteria

1. The pre-change strict drift is reproduced and the focused RED tests fail only
   for the missing contract.
2. Generated maps contain no ambient Git commit/branch/worktree identity.
3. Checkout generation and staged-tree archive validation are byte-consistent.
4. Directly contradicted current claims are guarded by semantic tests and removed.
5. Engineering Memory tests, strict validation, currentness, and canonical Rust
   gates pass on final bytes.
6. The pushed `origin/main` identity exactly matches the verified local commit.

## Prohibited actions

Do not create/switch branches, open pull requests, deploy, release, publish,
install dependencies, access credentials, rewrite history, edit generated JSON
manually, or modify unrelated/protected paths. The user authorizes only the final
commit and push to the existing `origin/main` after verification.
