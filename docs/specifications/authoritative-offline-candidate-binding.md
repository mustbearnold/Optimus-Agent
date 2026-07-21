---
knowledge_type: specification
status: historical
covers:
  - crates/optimus-kernel/src/evaluation.rs
  - crates/optimus-kernel/src/lib.rs
  - crates/optimus-kernel/tests/evaluation_contracts.rs
  - apps/optimus-cli/tests/eval_report.rs
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
  - docs/architecture/system-overview.md
  - docs/maps/observability-and-evaluations.md
---

# Authoritative offline candidate binding

**Date:** 2026-07-20

## Problem and outcome

**Observed fact:** the exact Priority-2 runner always executes the built-in offline
scripted trajectory and integrity suites, but it accepts any structurally valid
`CandidateBinding`. A caller can therefore label an offline report as another
provider/model or supply unrelated contract, tool-catalog, and route-policy hashes.

**Observed fact:** Engineering Memory already owns the canonical LF-normalized
source-tree SHA-256 policy. Evaluation, tool-catalog, and route-policy source files
are compile-time authorities available to the kernel and current-tree authorities
available to the generator.

**Intended outcome:** one public kernel constructor derives the only valid offline
evaluation context from canonical source bytes plus an explicit current source-tree
hash. The exact runner rejects every context mismatch before run state. A new
`python scripts/engineering_memory.py binding` command emits that exact binding for
the current source tree, and its output works directly with `optimus eval report`.

## Scope

- Derive `contract_sha256` from canonical `evaluation.rs` source bytes.
- Derive `tool_catalog_sha256` from canonical `optimus-packs/src/lib.rs` bytes.
- Derive `route_policy_sha256` from canonical `routing.rs` bytes.
- Fix provider/model to `offline` / `offline-scripted` for this executor.
- Accept only an explicit valid source-tree SHA-256 in the public constructor.
- Require the runner binding to equal the derived context before UUID ownership.
- Add Engineering Memory `binding`, using its fresh in-memory repository index and
  the same canonical source-file hashing policy.
- Emit exactly one pretty JSON `CandidateBinding` to stdout on success.
- Exercise generator output through the real CLI report binary.
- Update current authority and generated Engineering Memory.

## Non-scope

- Generating resource measurements or thresholds.
- Live-provider evaluation or provider telemetry.
- Changing report, dataset, metric, comparison, or baseline schemas.
- Accepting or comparing baselines through the CLI.
- Hashing runtime binaries, dependencies, environment variables, credentials, or
  ignored/generated files.
- Introducing a second repository-tree hashing algorithm.

## Authoritative existing behaviour

- Engineering Memory canonicalizes UTF-8 files to LF, preserves binary bytes, sorts
  source records by path, and emits `repository-index.json.tree_sha256`.
- `CandidateBinding` comparison permits only `source_tree_sha256` to differ between
  compatible candidate reports.
- `run_priority2_offline_evaluation` preflights structural binding, measurements,
  and thresholds before creating `evaluation-runs`.
- Exact report execution is offline and deterministic; no provider route occurs.
- `optimus eval report` accepts a bounded binding JSON file without modifying it.

## Contracts and invariants

1. `priority2_offline_candidate_binding(source_tree_sha256)` validates a lowercase
   or uppercase 64-character hexadecimal digest and returns one binding with fixed
   offline provider/model and source-derived context hashes.
2. Source-file hashes use the same UTF-8 newline canonicalization as Engineering
   Memory (`CRLF` and lone `CR` become `LF`) before SHA-256.
3. The contract authority is the complete `evaluation.rs` source; tool authority is
   the complete canonical packs source; route authority is the complete routing
   source. This is deliberately conservative: any authority-file byte change makes
   reports context-incompatible until rebuilt and regenerated.
4. The exact runner derives the expected binding from the supplied source-tree hash
   and requires full equality before creating `evaluation-runs`.
5. Structurally valid but mismatched provider, model, contract, tool, or route values
   return `Err`, execute neither suite, and create no evaluation-run state.
6. The Engineering Memory binding command computes a fresh source tree in memory;
   it does not trust or mutate checked-in generated JSON.
7. Python and Rust use the same canonical source bytes and field names. Generator
   output must deserialize as `CandidateBinding` and be accepted unchanged by the
   compiled exact runner.
8. Binding generation is read-only. It writes no file itself; shell redirection, if
   requested by an operator, owns destination replacement and recovery. The
   destination must be outside the indexed repository so writing the evidence does
   not immediately change the source identity it contains.
9. Existing `generate`, `check`, `validate`, `eval run`, and `eval report` interfaces
   remain compatible.

## Failure, interruption, and recovery

- Invalid source identity, unreadable source, Cargo metadata failure, or source/hash
  mismatch returns non-zero and no binding JSON on stdout.
- Kernel mismatch fails before evaluation run ownership, so no rollback is needed.
- Interruption during generation leaves no repository state. Interruption after a
  caller redirects stdout is outside Optimus and may leave a partial destination.
- Concurrent generators are read-only and deterministic for equal source bytes.
- A source change during generation can produce mixed evidence unless bounded. The
  generator therefore builds all identities in one process and recomputes the three
  authority file records from the same repository snapshot traversal; callers must
  rerun after concurrent edits. No writer lease is introduced for this read-only
  developer command.

## Interface and compatibility

```text
python scripts/engineering_memory.py binding > ../optimus-binding.json
optimus --home PATH eval report --binding ../optimus-binding.json --measurements measurements.json
```

Add public `priority2_offline_candidate_binding(source_tree_sha256)` returning
`Result<CandidateBinding>`. No serialized or database schema changes.

## Acceptance criteria

- The constructor returns fixed offline identities and deterministic canonical
  source hashes for a valid source-tree digest.
- Invalid source-tree hashes fail.
- Changing provider, model, contract, tool, or route on an otherwise valid binding
  fails before `evaluation-runs` exists.
- The Python command emits only valid pretty JSON on stdout and is byte-deterministic
  for unchanged source.
- Generated `source_tree_sha256` equals a fresh Engineering Memory repository index;
  the other three hashes equal canonical current authority-file hashes.
- Real generator output is accepted unchanged by `optimus eval report`, which
  returns a passing ten-case report carrying the exact generated binding.
- Existing report, CLI failure, and legacy trajectory behavior remain green.
- Focused, canonical, Engineering Memory, scope, and detached-tree gates pass.

## Execution plan and ledger

### Slice 1 — compiled offline context enforcement

- **Outcome:** exact reports cannot be mislabeled or bound to unrelated context.
- **Dependencies:** existing candidate validation and preflight ordering.
- **RED:** require the public derived constructor and assert each context mutation
  fails without `evaluation-runs`; compilation first fails because it does not exist.
- **GREEN:** canonicalize embedded source text, derive four fixed context fields, and
  compare the supplied binding before run ownership.
- **Refactor:** share digest/newline helpers only inside evaluation authority.
- **Verification:** selected constructor/mutation test, complete evaluation contracts,
  and strict kernel Clippy.
- **Complete when:** only the exact derived offline context reaches execution.
- **Observed evidence:** RED failed because the public constructor was absent.
  GREEN derived canonical evaluation/tool/routing source hashes, fixed the offline
  identity, rejected an invalid source hash, and rejected five context mutations
  before run ownership; evaluation contracts passed 19/19 with strict kernel Clippy.

### Slice 2 — current-tree binding command

- **Outcome:** operators can create the required binding without manual hashes.
- **Dependencies:** Slice 1 and Engineering Memory source-record policy.
- **RED:** command/integration tests fail because `binding` is not a recognized script
  command and existing fixture bindings are synthetic.
- **GREEN:** build a fresh source tree, hash the three authority files canonically,
  emit exact JSON, and feed it to the compiled report command in integration tests.
- **Refactor:** centralize binding construction in one Python function used by tests
  and CLI dispatch.
- **Verification:** Engineering Memory unit tests, CLI binary integration, generator
  determinism/failure behavior, and strict CLI Clippy.
- **Complete when:** script-to-report execution preserves one exact binding.
- **Observed evidence:** RED failed because the current-tree binding builder was
  absent. GREEN emitted deterministic JSON from one canonical source traversal,
  kept failure stdout empty, and replaced synthetic CLI fixtures with the real
  generator output; Engineering Memory passed 17/17 and CLI binary acceptance 4/4.

## Final verification

- CLI binary integration, evaluation, integrity, and trace contracts.
- Engineering Memory tests, generation, strict validation, and currentness.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- `cargo test --workspace --all-features`.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`.
- Exact diff/path review and detached staged-tree focused verification.

## Prohibited actions

- Do not accept arbitrary provider/model identities for the offline executor.
- Do not trust stale generated hashes, invent another tree policy, fabricate resource
  measurements, or automatically accept a baseline.
- Do not print source contents or sensitive paths in binding output.
- Do not redirect generated binding evidence into the indexed source tree.
- Do not manually edit generated Engineering Memory JSON.
- Do not create a branch or pull request, install dependencies, deploy, release,
  publish, access credentials, or modify unrelated paths.

## Assumptions and unresolved work

- **Reasonable inference:** source-file hashes are conservative but honest context
  identities until those contracts gain independently versioned canonical manifests.
- **Unresolved:** provenance-bound resource measurement and baseline CLI operations
  remain separate future milestones.
