---
doc_id: spec-016-staleness-prevention
doc_type: reference
plane: work
status: current
authority: canonical
summary: Gate-enforced freshness for the Optimus tree — a citation-lint gate that fails any spec/ADR/runbook file:line claim that does not resolve against the live tree, a docs-lock refresh that converges the full binding closure in one pass (no partial-refresh rotation), a scheduled stale-audit that scans review_by dates, and a one-command docs+EM cascade. Zero staleness as a gate property, not a hygiene habit.
reviewed_on: 2026-08-06
review_by: 2026-11-06
knowledge_type: specification
covers:
  - specs/conventions.md
  - scripts/tools/docs_system.py
  - scripts/verify.sh
  - justfile
  - scripts/gates/check-module-size.py
  - scripts/tests/test_docs_system.py
  - scripts/tests/test_verify_gate_parity.py
depends_on:
  - docs/decisions/0049-module-size-is-measured-honestly.md
  - docs/decisions/0063-documentation-is-a-governed-authority-plane.md
  - docs/decisions/0083-one-wire-protocol-for-all-surfaces.md
---

# Spec-016: Staleness prevention — gate-enforced freshness

## Revision table

| Round | Verdict | Findings | Fixes |
|---|---|---|---|
| 1 | REJECTED (8 blocking / 7 non-blocking) | [R1-B1]–[R1-B8], [R1-N1]–[R1-N7] | Fixed in v2: cascade reordered to generate-before-check and aligned with the existing `em-generate` target (B1, N5); A3 scoped to the binding path, catalog documented as untouched by refresh (B2); R7 rewritten as a plain line budget — scripts/ sits outside the mechanical ratchet, no invented enforcement (B3); A2/A4/A6 gain named executors (B4); pass-2 keyword sourcing defined + moved-no-symbol fixture added (B5); lint set extended to `specs/*.md` and the runbook open question closed (B6); Current-state refresh paragraph labelled "at spec-writing time (pre-R3)" (B7); evidence citations corrected — rotation described as mechanism-entailed, real record filenames, parity claim sourced to its own file (B8); refresh CLI `nargs` contract change named in R3 (N1); "ANY subset" qualified against the expected set (N2); R3 regression list extended with multi-binder convergence + status-flip cases (N3); review_by scan extended to runbooks with missing-review_by semantics (N4); R2 triggers extended to behaviour claims (N6); ADR-0085 scoped to record symbol anchors as an extension, not prior art (N7). |
| 2 | **APPROVED** (no blocking; 1 non-blocking) | [R2-N1] | All 8 round-1 blockers + all 7 non-blocking verifiably closed against the live tree (24 citations re-derived, review_by coverage 111/111, worktree clean). N1 applied at landing: Purpose item 2 tagged "(pre-R3)" so the pre-fix present-state sentence is marker-disciplined. |

## Purpose

Owner directive (2026-08-06): "What is the best practice method for
ensuring zero staleness in AI project folders?" — answered with a
mechanism, not a habit. The spec-015 review waves (rounds 3–7) proved
the failure class live: the Phase-A landing silently displaced ~40
file:line citations in specs/ADRs and no gate noticed, because the
existing gates pin *sets and bindings* (registry parity, dead-binding
validation) but never *citations*. The docs lock then exhibited a
partial-refresh rotation (refreshing a subset re-stales a different
subset via binding-digest propagation), and prose describing the
present state went stale without a date marker.

This spec makes staleness fail loudly and freshness cheap:

1. **Citation lint**: every `path:line` claim in specs, ADRs, and
   runbooks must resolve against the live tree (±2 lines) or the gate
   fails; symbol-anchored citations are supported and drift-immune.
2. **Binding-closure refresh**: `docs_system.py refresh` converges the
   entire lock in one pass (a doc's digest hashes every file it binds,
   so a change re-stales the whole binding closure; at spec-writing
   time (pre-R3) only the named ids are updated, forcing repeated
   refresh cycles).
3. **Scheduled audit**: a `stale-audit` target scans `review_by`
   frontmatter for overdue reviews and re-runs the citation lint +
   binding check + EM check — staleness schedules itself out.
4. **One-command cascade**: `just docs-cascade` runs the full
   refresh→generate→check→EM→PK sequence in the pinned order, so the
   order can never be gotten wrong.

## Current state (Confirmed behaviour)

- The docs verification lock (`docs/verification-lock.json`) records,
  per governed document, a `binding_sha256`: the content hash of EVERY
  file the doc's `owns`/`covers`/`depends_on`/`validated_by` patterns
  resolve (`scripts/tools/docs_system.py:459-483`), and
  `docs_system.py check` fails when the lock differs from a fresh
  computation (`validate_lock`, `scripts/tools/docs_system.py:511-521`).
- `refresh` writes lock entries ONLY for the named ids
  (`scripts/tools/docs_system.py:574-592`, loop :587-591); retiring a
  superseded doc requires naming it (`retire` set :583-589). A
  document whose digest changed transitively (because a file it binds
  changed) therefore stays stale until it is named in a later refresh
  — the partial-refresh rotation is mechanism-entailed. *(at
  spec-writing time, pre-R3; R3 replaces this with one-pass
  convergence — the round-1 review's "observed during v16–v18"
  phrasing overstated the evidence, the mechanism is the fact)*
- There is no gate that verifies file:line citations in prose. The
  spec-015 rounds 3–7 performed this audit by hand (a rebuilt /tmp
  script each round); the round-7 audit covered 192 citations.
- `review_by` frontmatter exists on specs and ADRs (e.g.
  `specs/015-surface-protocol/spec.md:9`) but nothing enforces it; a
  review can silently miss its own deadline.
- Gate tiers: `scripts/verify.sh` `tier_gates()` (:251) and
  `tier_all()` (:501) each spawn the "gates" section then the "gate
  self-tests" section (:278/:536). `just docs-check` (:214),
  `docs-generate` (:218), `docs-refresh` (:222), `em-check` (:238) and
  `em-generate` (:242-246, EM generate :243 → PK generate :244 →
  validate :245) exist individually, but no single target runs the
  pinned cascade.
- The module-size law (AGENTS.md rule 21) is enforced by
  `scripts/gates/check-module-size.py` as a shrink-only ratchet with
  SCAN_ROOTS = ("crates", "apps") (:60) — `scripts/` is never
  measured (ADR-0049:28-29), so `docs_system.py`'s growth is governed
  by this spec's R7 line budget instead.
- A `just docs-cascade` / `just stale-audit` target does not exist yet
  (Phase A1).

## Requirements

### R1. Citation lint gate

- A `check-citation-drift.py` gate (`scripts/gates/`) MUST lint every
  `path:line` citation in a pinned document set: all `specs/*.md`,
  all `specs/*/spec.md`, all `docs/decisions/*.md`, all
  `docs/runbooks/*.md` (flat globs). The set matches the Purpose
  promise "every spec/ADR/runbook file:line claim".
- A citation resolves when: (1) the file exists, and (2) the cited
  line is within ±2 of a real anchor. Anchor resolution is two-pass:
  pass 1 is the exact line; pass 2, for a cite that misses pass 1,
  locates the nearest anchor by keyword context — the keyword is, in
  order: (a) the backticked identifier in the claim sentence, (b) the
  nearest identifier token immediately before the cite, (c) fail. A
  cite whose target moved 1–2 lines with no symbol still exits 0
  (pass-2 non-vacuity, pinned by the moved-no-symbol fixture).
- Symbol-anchored citations (`file:line` + a named `fn`/`class`/`def`
  in the claim) MUST resolve to that symbol's live location when the
  keyword matches a definition in the file; this is an EXTENSION
  beyond the round-7 line-exact convention (round7.md:5-60) and is
  recorded as such in ADR-0085 — never claimed as prior art.
- A lint failure MUST fail the gate (exit 1) with the offending
  citation listed; the gate MUST NOT fix or rewrite anything.
- The gate MUST be wired into BOTH `tier_gates` and `tier_all` via
  `verify.sh` (spawn sites :251/:278/:501/:536) and gain a self-test
  in the "gate self-tests" sections, exactly like
  `test_verify_gate_parity.py` (which compares spawn-name sets between
  the tiers and stays generic — no gate names hardcoded).

### R2. Snapshot discipline

- A lint companion (same gate, `--snapshot` mode or a second script
  `check-review-by.py`) MUST flag prose that claims present-state
  behaviour about the tree when the file carries no
  `reviewed_on`/`review_by` marker or a marker older than the trigger
  window. Trigger words include "today", "currently", "the gate is",
  and present-tense behaviour claims about tool mechanics ("refresh
  writes", "check fails", "the lock records").
- A section describing pre-fix behaviour MUST carry an explicit
  "(at spec-writing time, pre-<revision>)" label; labelled sections
  are exempt from the trigger.
- `review_by` frontmatter MUST exist on every pinned doc (specs, ADRs,
  runbooks). A doc missing it is a finding: report-only in the tier
  gate, fail in `stale-audit`.

### R3. One-pass binding-closure refresh

- `docs_system.py refresh` MUST converge the FULL binding closure in
  one pass: refreshing any doc re-digests every doc that binds a
  changed file (transitive closure), so `check` passes after ONE
  refresh for any subset of docs still in the expected set — a doc
  that left the expected set still fails with `extra` until explicitly
  retired.
- The refresh CLI contract changes: `doc_id` nargs `"+"` → `"*"`
  (`scripts/tools/docs_system.py:726`) so a no-arg refresh (full
  recompute) is expressible. A no-arg refresh MUST converge the whole
  lock.
- The catalog (`docs/catalog.json`) is NOT touched by refresh — the
  catalog embeds each doc's `content_sha256` (catalog_payload,
  `scripts/tools/docs_system.py:524-536`, digest loop :528-532), so a
  DOC-content change still requires `docs-generate` (the cascade
  covers it, R4); refresh only propagates binding digests.
- Regression tests MUST cover: (a) a changed file re-stales every
  binder and ONE refresh re-converges them all (multi-binder
  convergence); (b) a doc whose status flipped
  (current→planned→retired) is left untouched until named; (c) a
  deleted-file / deleted-symbol / moved-symbol fixture (R1's
  executor, `scripts/tests/test_citation_drift.py`).

### R4. One-command cascade

- A `just docs-cascade` target MUST run, in order: `docs-refresh`
  over the full changed closure (R3), `docs-generate`, `docs-check`,
  `engineering_memory.py generate`, `project_knowledge.py generate`,
  `engineering_memory.py check`. Generate MUST precede check
  (`check_generated` fails on a stale catalog,
  `scripts/tools/docs_system.py:668-677`, :675). The PK↔EM order
  matches the existing `em-generate` target
  (`justfile:242-246`) and both orders converge: only
  docs-before-EM is load-bearing (EM staleness compares whole-tree
  SHAs, spec-015 A6), PK writes temporal sqlite and EM writes json
  maps — neither reads the other's output.
- A6's end state MUST be green after ONE `just docs-cascade` on the
  spec-edit scenario: lock current, catalog fresh, EM current, PK
  current, both checks green.

### R5. Scheduled stale audit

- A `just stale-audit` target MUST scan every pinned doc's
  `review_by` (same set as R1) and fail when any review is overdue;
  it MUST re-run the citation lint (R1), the binding check, and
  `em-check`. It MUST exit 0 with a per-doc listing when everything
  is fresh. Runbooks are IN the scan (6/6 carry review_by today).
- A synthetic-overdue fixture (a `review_by` in the past)
  (`scripts/tests/test_stale_audit.py`) MUST fail the audit; a fresh
  doc must pass. A doc missing `review_by` fails the audit (R2).

### R6. Wiring and parity

- Both new gates join `verify.sh` tier_gates AND tier_all (spawn
  sites :251/:278/:501/:536), each with its self-test, and
  `test_verify_gate_parity.py` MUST stay green (it compares the
  spawn-name SETS between tiers; no gate names hardcoded — the new
  names appear in both, so parity holds by construction).

### R7. Line budget for the tool layer

- `scripts/tools/docs_system.py` is 775 lines at spec-writing time.
  The R3 fix MUST keep it under 800 lines. If the fix needs more than
  ~20 lines, the lock trio (binding_digest / expected_lock /
  validate_lock) MUST be extracted into a `docs_lock.py` module to
  keep the budget. This is a PLAIN line budget: scripts/ sits outside
  the mechanical ratchet (`check-module-size.py` SCAN_ROOTS,
  `scripts/gates/check-module-size.py:60`; ADR-0049:28-29), so no
  baseline entry exists or is needed.

## Acceptance criteria

- [ ] A1. Given a spec/ADR/runbook containing a `path:line` cite whose
  nearest match is 5 lines off, when the citation-lint gate runs,
  then it exits 1 listing the cite; given a cite 1–2 lines off, it
  exits 0 (pass-2). Executor: `scripts/tests/test_citation_drift.py`
  drift fixtures.
- [ ] A2. Given a deleted file, a deleted symbol, and a moved symbol
  each cited in a pinned doc, when the lint runs, then each fails
  with the citation named; a moved-no-symbol cite (1–2 lines) passes.
  Executor: `scripts/tests/test_citation_drift.py` deleted-file /
  moved-symbol / moved-no-symbol fixtures.
- [ ] A3. Given a source-bound doc whose BOUND FILE's content changed,
  when `docs_system.py refresh <that-id>` runs, then `check` passes
  immediately — including every doc that binds it (no second
  refresh); a no-arg refresh also converges. Executor:
  `scripts/tests/test_docs_system.py` binding-closure regressions.
- [ ] A4. Given a pinned doc with `review_by` in the past, when
  `just stale-audit` runs, then it exits 1 naming the doc; with all
  reviews fresh it exits 0. Executor: `scripts/tests/test_stale_audit.py`.
- [ ] A5. Given both tiers, when `verify.sh` runs, then
  `test_verify_gate_parity.py` stays green with the new gate names in
  both tier spawn-sets. Executor: the existing parity test.
- [ ] A6. Given a spec edit that changed a doc's content, when `just
  docs-cascade` runs once, then lock, catalog, project knowledge, and
  EM are all fresh and both checks green. Executor:
  `scripts/tests/test_docs_cascade.py` cascade integration fixture.

## Implementation phases

Phase A (the mechanism lands as a gate property):

- A1. Land `check-citation-drift.py` + `check-review-by.py` (or one
  gate with `--snapshot`), their self-tests, the R3 refresh
  convergence in `docs_system.py` (nargs `"*"`, closure loop), the
  `docs-cascade` and `stale-audit` justfile targets, and the
  fixture tests named in A1–A6 — one coherent landing wave, gates
  green at every commit (each new gate + self-test joins both tiers
  per R6 before the next commit; the tier-parity test stays green).
- A2. The `covers` bindings of this spec extend at the Phase-A
  implementation commit to the new files (check-citation-drift.py,
  check-review-by.py, test_citation_drift.py, test_stale_audit.py,
  test_docs_cascade.py), NEVER before (ADR-0062 dead-binding
  precedent, `scripts/tools/docs_system.py:680-699`); the docs +
  EM cascade runs in the A1 commit in repo order (AGENTS.md steps
  8–9).

## Open questions

- Gate tolerance: should the citation lint allow a per-doc override
  list (for genuinely historical citations), or fail hard on every
  miss? Default: fail hard; an override list is a Phase-B decision.
- Should `stale-audit` run in CI (a scheduled cron firing
  `just stale-audit` on the repo), or only as a manual/landing gate?
  Phase-B decision.

## Links

- Precedent: the spec-015 citation audits — records in
  `Development/tmp/spec015-review-single-soul.md` (round 3) and
  `Development/tmp/spec015-review-round{4..7}.md` (rounds 4–7); the
  round-7 audit covered 192 citations and the line-exact convention
  is documented in `round7.md:5-60`.
- `test_verify_gate_parity.py` exists and is gate-name-generic
  (verified by its own source).
- `docs_system.py check` self-tests at `verify.sh:278/:536` cover
  the docs lock + catalog today (test_docs_system.py).
