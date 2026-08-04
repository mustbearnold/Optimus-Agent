---
doc_id: decisions-0054-a-selector-may-only-over-select
doc_type: decision
plane: decision
status: current
authority: record
summary: - Date: 2026-07-29 - Program: program P42
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - scripts/tools/impact_select.py
  - justfile
depends_on:
  - docs/decisions/0049-module-size-is-measured-honestly.md
  - docs/decisions/0052-isolated-durable-engineering-runs.md
  - docs/decisions/0053-a-repository-is-asked-not-assumed.md
validated_by:
  - scripts/tests/test_impact_select.py
---

# ADR-0054: A test selector may only ever over-select

- **Status:** Accepted
- **Date:** 2026-07-29
- **Program:** program P42

## Context

`just verify` runs 38 checks and the full workspace test suite. It is correct,
and on a warm cache the Rust suite alone is ~25s against ~0.7s for a single
leaf crate. That ratio is the whole problem: a gate slow enough to interrupt
the inner loop is a gate people stop running between commits, and the checks
that matter most are the ones run most often.

The obvious fix — run only the tests the patch can break — introduces a failure
mode worse than slowness. A selector that misses a test does not report an
error. It reports **success**, from a run that never executed the thing that
would have failed. The signal and the bug look identical from outside.

That asymmetry is total. Over-selecting costs seconds. Under-selecting costs a
regression that ships with a green check next to it.

## Decision

Focused verification exists (`just dev-check`, `just test-changed`,
`just impact`), and every rule in `scripts/tools/impact_select.py` is written in the
direction that keeps its output a superset of what would actually fail.

**1. Unknown escalates.** A changed path no rule classifies selects the entire
workspace, with the path named in the plan's `unclassified` list. The
alternative — dropping it — produces a selector that silently stops testing
whatever it stops recognising. Nothing about that failure is visible: the tree
grows a new directory, the selector shrugs, and coverage drops without a single
red mark. One unclassified path escalates the whole plan; a patch that touches
a known crate *and* something unrecognised is not half-safe.

**2. The gate cannot shrink itself.** A change to `justfile`, `verify.sh`, any
`check-*.py`, `impact_select.py` itself, the workspace manifest, the lockfile,
`.cargo/**`, or `.github/**` selects everything. Otherwise the cheapest way to
make a patch pass is to edit the thing that decides what passing means. This is
the same floor [ADR-0053](0053-a-repository-is-asked-not-assumed.md) puts under
sensitive paths, applied to the selector rather than to review.

**3. Impact is transitive.** A changed crate selects that crate and every crate
that depends on it, computed by inverting the workspace manifests rather than
by a hand-maintained table. Dev- and build-dependencies count: a crate used
only by another crate's tests still breaks those tests when it changes. A
hand-maintained map would be correct on the day it was written and wrong
thereafter.

**4. Selecting nothing is not passing.** An empty selection reports
`nothing-selected` and, under `--require-selection`, exits non-zero.
"No tests ran" and "the tests passed" are different sentences. This is the same
rule as ADR-0053's *absent is not satisfied*, one layer down.

The selector emits a **plan**, never an action. `--json` for a program,
`--cargo-args` for a shell, plain text for a human — including what it
escalated and why. Deciding and running stay separate so that a caller can
refuse a plan it does not like, and so the selector can be tested without
running a single test.

**`just verify` is unchanged.** Focused verification is for the inner loop.
The pre-push hook still runs everything, so a wrong answer here costs cycles,
never a missed regression at the boundary that matters.

## Alternatives considered

**Coverage-derived selection.** Rejected for now. Per-test line coverage would
give a far tighter selection, but it needs an instrumented build to stay
current, and a stale coverage map under-selects — silently, which is the one
outcome ruled out above. Manifest-derived closure is coarser and cannot go
stale without the manifest changing.

**A hand-maintained source-to-test table.** Rejected. The existing
`.engineering-memory/source-to-test-map.json` already says what such a map is
worth: its own `limitations` field records that package-default mappings
establish candidate impact, not coverage. A table that must be edited whenever
the tree changes will be wrong exactly when someone is moving fast.

**Let the selector run the tests.** Rejected. A component that both decides
scope and executes it cannot be tested for its decisions without side effects,
and its output cannot be reviewed before it acts.

**Make `just verify` use the selector.** Rejected. The pre-push gate is the
last honest checkpoint before code leaves the machine. Speeding it up with a
heuristic trades the one place where completeness is worth its cost.

## Reasons

- The two error directions are not comparable: seconds versus a shipped
  regression behind a green check.
- Rules derived from manifests cannot drift out of date without the manifests
  changing, and a manifest change escalates anyway.
- Emitting a plan rather than an action makes every rule testable offline, in
  milliseconds, with no build.

## Consequences

- Focused verification is fast where it is narrow and honest where it is not.
  On this branch — which edits the justfile and `verify.sh` — `just impact`
  correctly reports `EVERYTHING`, and says which six files caused it.
- New top-level directories escalate until classified. This is deliberate
  friction and the correct default; the fix is one entry in `PATH_SUITES`.
- `INERT_PATHS` is a promise that no gate reads those files. Every entry is a
  liability, so the list is kept short.
- `.md` anywhere reaches the gates, because markdown frontmatter feeds the
  engineering-memory knowledge graph.

## Risks

- **`INERT_PATHS` drift.** If a gate starts reading something listed there, the
  selector under-selects — the one failure mode this ADR exists to prevent.
  Mitigated only by keeping the list short and reviewing additions as
  security-adjacent.
- **Suite mapping is by prefix.** `PATH_SUITES` maps directories to JS/e2e
  suites by hand, because cargo does not describe them. A new UI surface
  outside the mapped prefixes escalates (safe) until someone maps it.
- **Warm-cache timings.** The 0.7s figure assumes a warm target directory. A
  cold build dominates either path and the ratio does not hold.

## Evaluation evidence

`scripts/tests/test_impact_select.py` — 27 tests. The ones carrying the decision:

- `test_a_path_no_rule_recognises_selects_everything`
- `test_the_selector_cannot_shrink_itself`
- `test_one_unclassified_path_escalates_the_whole_plan`
- `test_an_empty_patch_selects_nothing_and_says_so` /
  `test_require_selection_fails_on_an_empty_plan`
- `test_impact_is_transitive_not_one_hop` — `optimus-store` reaches
  `optimus-cli`, which no single manifest states
- `test_a_leaf_crate_does_not_drag_in_the_workspace` — the counterweight; a
  selector that always escalates is `just test` with extra steps
- `test_every_seeded_regression_is_selected` — the P42 exit gate's ten cases
- `test_every_workspace_package_selects_itself` — the exhaustive form: no crate
  may be invisible to the selector

Measured on this repository, warm cache: `cargo test -p optimus-engineering
--all-targets` **0.67s** against `cargo test --workspace --all-targets`
**24.9s**. (That package was archived by
[ADR-0073](0073-an-unreachable-vertical-is-archived-not-carried.md) on
2026-08-01; the ratio is what the measurement records, and any leaf crate
reproduces it. This decision is unaffected — the selector lives in
`scripts/tools/impact_select.py`, not in the archived crate.)

## Conditions for reconsideration

- A coverage map exists that is regenerated by the gate itself, so it cannot go
  stale between runs. Then rule 1's escalation could narrow.
- Escalation becomes the common case in practice, at which point the rules are
  too coarse to be earning their complexity.
- `just verify` becomes fast enough that a separate focused tier is not worth
  maintaining.

## Relevant code

- `scripts/tools/impact_select.py` — the four rules, the workspace closure, the plan.
- `justfile` — `impact`, `dev-check`, `test-changed`.

## Relevant tests

- `scripts/tests/test_impact_select.py`
