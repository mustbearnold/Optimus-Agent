---
doc_id: decisions-0049-module-size-is-measured-honestly
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0049: The module-size law is measured honestly, and does not tax splitting, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
knowledge_type: decision
covers:
  - scripts/gates/check-module-size.py
  - docs/architecture/module-size-baseline.json
  - AGENTS.md
depends_on:
  - docs/architecture.md
validated_by:
  - scripts/tests/test_module_size.py
---

# ADR-0049: The module-size law is measured honestly, and does not tax splitting

- **Status:** Proposed
- **Date:** 2026-07-27

## Context

The 800-line module law (AGENTS.md rule 21) is enforced by
`scripts/gates/check-module-size.py` as a ratchet. The law is sound. Its measurement
was not.

"Production lines" was implemented as *everything before the first
`#[cfg(test)]`*. That is a different quantity which happens to agree with the
intent whenever a file keeps its tests at the bottom — and silently
under-reports whenever it does not.

Two failures were observed while splitting `optimus-kernel`, and both were
caused by the gate rather than caught by it:

1. **The number lied.** `crates/optimus-kernel/src/lib.rs` reported 912 lines
   while holding roughly 1200 lines of production code: a `system_prompt_tests`
   module sat two-thirds of the way down and hid the ~280 production lines
   behind it. Splitting that file honestly made the *reported* number jump from
   952 to 1230 — the gate punished the refactor it exists to cause, because the
   refactor removed the test module that was doing the hiding. A metric that
   improves when you move a test module up teaches exactly that move.

2. **Complying with the law could break the law.** Splitting `browser.rs` — over
   the limit, so the split was mandatory — required adding `mod page_extract;`
   to `lib.rs`. `lib.rs` was sitting exactly at its baseline of 912, so it went
   to 913 and the gate failed. The only way through was to compress two
   unrelated re-exports onto one line, which made the code marginally worse for
   no reason but arithmetic. Every split costs its declaring file one line, so
   the ratchet levies a tax on the behaviour it is trying to produce.

## Decision

**Production lines exclude every `#[cfg(test)]` item, wherever it appears** —
not everything after the first one. Each attributed item is skipped by brace
depth, so a test module in the middle of a file no longer conceals the code
after it. `#[cfg(all(test, unix))]` and its relatives count as test items;
`#[cfg(feature = "test")]` does not.

**Production lines exclude bare `mod x;` declarations.** A module declaration is
a registry entry, not logic. A `mod x { … }` with a body is still code and still
counts.

**Braces are counted off source with comments and literal contents blanked out.**
The skip is driven by brace depth, and a `{` inside a string or a doc comment is
not a brace. Rust lifetimes (`&'a str`) are distinguished from char literals,
because scanning `'a` as a literal consumes the rest of the file.

The 800-line limit, the shrink-only ratchet, and the rule against hand-editing
the baseline are all unchanged.

## Alternatives considered

**Leave it alone.** The law still mostly worked, and the mismeasurement was
survivable. Rejected because both failure modes actively push authors the wrong
way: one rewards relocating a test module over splitting a file, the other
penalises splitting. A gate that can be satisfied by making the code worse will
be.

**Keep the truncating metric and grant the ratchet slack** — allow a baselined
file to grow by a few lines. Fixes the second failure and not the first, and
slack in a ratchet is how ratchets stop ratcheting.

**Count all lines, tests included.** Simplest possible metric and impossible to
game. Rejected: it penalises inline coverage, which inverts the rule's purpose,
and Rust convention puts unit tests in the file they test.

**Parse with `syn` or `rust-analyzer` instead of scanning text.** Exact, and it
would remove the blanking machinery. Rejected as disproportionate: the gate must
run in the ~1s `just gates` tier with no Rust toolchain dependency, and the
scanner's failure modes are all covered by `scripts/tests/test_module_size.py`.

## Reasons

- A gate is a claim about the codebase. If the claim is false, the gate is worse
  than nothing, because it is trusted.
- Rule 21 exists to force splits. Any measurement that makes splitting more
  expensive than not splitting is working against the rule it enforces.
- The cost of the honest metric was one file. That is the argument for it: the
  ratchet was not holding back a wave of violations, it was mis-stating a small
  number of them.

## Consequences

- Re-measuring the tree **added nothing to the baseline and grew nothing**.
  Every grandfathered file stayed put or shrank; `optimus-runtime/src/lib.rs`
  fell 1507 → 1409 and `optimus-kernel/src/lib.rs` 912 → 881, both of which are
  the true figures rather than a relaxation.
- One file was revealed: `apps/optimus-tui/src/session.rs` measured 860 under
  the honest metric, having been masked by an indented `#[cfg(test)]` item at
  line 726. It was **split rather than grandfathered** — the approval cluster
  moved to `apps/optimus-tui/src/session/approval.rs`, leaving 714 and 176.
- Splitting a module no longer costs its declaring file anything.
- Files may now be measurably larger than they read, since a mid-file test
  module no longer hides the tail. That is the point.

## Risks

- **The scanner is not a Rust parser.** Nested block comments, raw strings with
  hashes, byte strings and lifetimes are handled explicitly; something exotic
  enough could still desynchronise the brace count. The failure is loud — a
  desynchronised count produces an absurd number, not a plausible one — and
  `scripts/tests/test_module_size.py` pins each case.
- **`mod x;` exemption is a small hole.** A file consisting only of module
  declarations measures near zero. That is correct — it is a registry, not a
  god-module — but it does mean the metric cannot see a crate that is wide
  rather than deep. Rule 21 was never the tool for that.

## Evaluation evidence

- `scripts/tests/test_module_size.py` grew from 3 to 11 `production_lines` cases:
  code after a test module is counted, `mod` declarations are not, a brace in a
  literal does not desynchronise the skip, a lifetime is not a char literal,
  `#[cfg(all(test, …))]` is recognised, `#[cfg(feature = "test")]` is not, a
  block-less `#[cfg(test)] use …;` ends at its statement, and a single-line
  `#[cfg(test)] mod t {}` does not swallow the rest of the file. All pass.
- That last case was a defect in the first cut of this scanner, found by review
  rather than by the tree: an item whose braces open and close on one line never
  raises the depth a later line could see, so the skip never ended and every
  following line went uncounted — the same under-reporting this ADR removes,
  reintroduced by its own fix. Re-measuring after the correction changed no
  file, so nothing in the tree was hitting it yet.
- Re-measuring the tree: 110 files, 14 over the limit, 14 baselined, 0 added,
  0 grown.
- `cargo test -p optimus-tui` after the `session.rs` split: 140 passed, 0 failed.

## Conditions for reconsideration

- The scanner desynchronising on real source, which would mean the cost of a
  real parser is now justified.
- The baseline reaching zero, at which point the ratchet machinery can go and
  the limit becomes a plain assertion.

## Relevant code

- `scripts/gates/check-module-size.py` — `code_only`, `production_lines`, `MEASURE`
- `apps/optimus-tui/src/session/approval.rs` — the split this revealed
- `docs/architecture/module-size-baseline.json`

## Relevant tests

- `scripts/tests/test_module_size.py::ProductionLinesTests` — all ten cases
- `scripts/tests/test_module_size.py::BaselineTests::test_baseline_matches_the_current_tree`
