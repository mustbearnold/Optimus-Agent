# WBS Review — SDD Migration Protocol decomposition

- **Reviewer**: WBS review — Round 1 (independent, ASTRONOMICAL-THINK mode)
- **Date**: 2026-08-06
- **Artifact**: `_attic/plans/wbs.md` (153 lines, commit `a3e4759`)
- **Ground truth**: `/tmp/SDD_MIGRATION_recovered.md` — 278 lines, git hash
  `cdb4f3f2e1ae0a156de7faa7067a13b2c6ec6524`, byte-identical to
  `c988536^:SDD_MIGRATION.md` (the sealed protocol). Verified by
  `git hash-object` comparison.
- **Method**: line-by-line audit of the protocol (all 278 lines) against every
  WBS item and every traceability-matrix row; cross-check of commit messages
  verbatim; check of skill-derived items against the sdd-migration skill.

---

## 1. Coverage completeness (the core claim)

**Result: coverage is substantially complete but NOT perfect. 4 defects found.**

### What verifies clean (credit where due)

- **All 9 invariants → 1.1–1.9**: each maps 1:1 with no paraphrase loss.
  Verified: git safety net (Inv.1 → 1.1), committed-only deletion (Inv.2 → 1.2),
  never-touch list + no secrets in reports (Inv.3 → 1.3), commit separation
  (Inv.4 → 1.4), attic-when-uncertain (Inv.5 → 1.5), build not broken (Inv.6 →
  1.6), no PR/branch ceremony (Inv.7 → 1.7), idempotent resume (Inv.8 → 1.8),
  no code relocation for aesthetics (Inv.9 → 1.9). All complete.
- **Target end-state → 2.1–2.7**: README thin, AGENTS.md, specs/ tree with
  active-only plan/tasks, docs/ three kinds, code untouched + _attic, placement
  law, monorepo rule. All complete.
- **Phase 1 steps 1–6 → 3.1–3.6**: snapshot commit message verbatim, tree walk
  with 8 classes (all listed), toolchain detection, EARLY/MID/LATE stage,
  MIGRATION_REPORT.md contents (inventory, toolchain, stage, phase checklist),
  commit message verbatim.
- **Phase 2 → 4.1–4.8**: all four delete-on-sight groups A (OS/editor cruft,
  build output, logs/tmp/cache/empty files, exact duplicates), stale rule with
  ALL THREE conditions (content captured elsewhere OR dead code; nothing
  references it via grep filename+title; in git history), attic + exact
  ATTIC.md line format (`filename — original path — why atticked — suggested
  fate`), .gitignore fortification, commit message verbatim + build verify.
- **Phase 3 → 5.1–5.7**: capability identification from all 7 listed sources
  (entry points, route tables, CLI, public API, test suites, package structure,
  existing-doc claims), 5–15 target, sub-features = sections, Appendix A
  template, dependency ordering, `[inferred]` tags, three-fate DOC sentencing
  with no-fourth-fate and ADR conversion by original date order, BACKLOG gaps,
  thin README, all stage adaptations (EARLY derive, unknowns → Open Questions,
  empty repo → constitution + 001 skeleton, LATE code-is-spec), commit verbatim.
- **Phase 4 → 6.1–6.4**: constitution from Appendix B adjusted to reality with
  principles-only + aspirations-to-BACKLOG, conventions from Appendix C with
  formatter table, AGENTS.md + CLAUDE.md copy/symlink, commit verbatim.
- **Phase 5 → 7.1–7.5**: formatter adoption with keep-existing-config rule,
  configs-first commit (verbatim), format-everything + markdown normalization +
  broken-link repair, zero-logic commit (verbatim), build/tests + revert-and-
  note behavior-changing formatter.
- **Phase 6 → 8.1–8.10**: all 6 checklist boxes (build/tests, code-area→spec
  mapping, no doc outside the 5 allowed locations, relative links, gitignore
  coverage, ATTIC.md explains everything), report finalization with counts/
  lists/open questions/human-judgment items, protocol deletion, seal commit
  (verbatim), human handoff.
- **SDD loop 1–6 → 9.1–9.6**: all six rules complete and faithful, with
  "installed in constitution.md" placement noted.
- **Appendices A–D → 10.1–10.4**: spec template fields (Status/Owner/Purpose/
  Requirements RFC-2119 + [inferred]/Acceptance GWT/Out of scope/Open
  questions/Links), constitution template (authority order, 6 principles,
  definition of done), conventions template (markdown rules, formatter table
  for all 6 languages, .editorconfig, commit types incl. `sdd`, naming rules,
  zero-padded never-reused numbers), ADR template (Date/Status/Context/
  Decision/Consequences).
- **Commit messages**: every one of the 8 protocol commit messages appears
  verbatim in the WBS (3.1, 3.6, 4.8, 5.7, 6.4, 7.2, 7.4, 8.9).
- **Traceability matrix §11**: every row maps to what it claims; all protocol
  elements appear in exactly one row; item ranges are contiguous and correct.

### BLOCKER 1 — §12 note 2 misattributes the sequencing rule to "protocol invariant 7"

WBS §12 note 2: "Execution order is strictly 1 → 10; each phase's commit
completes before the next begins (**protocol invariant 7**)."

The sequencing rule is NOT invariant 7. Invariant 7 is: "No PRs, no branch
ceremony. Work directly on the current branch. One checkpoint commit per phase:
`sdd(phase-N): <summary>`." The "execute phases in order; finish each phase's
commit before starting the next" rule is the protocol INTRO (line 9 of the
recovered text), which sits outside the numbered invariants.

Why it matters: the document's entire value proposition is 1:1 citation
fidelity. A misattributed protocol reference in the section that establishes
execution order is exactly the class of error this artifact exists to prevent.
It also means the intro's sequencing rule has NO row in the §11 matrix (the
matrix row "Invariants 1–9 | 1.1–1.9" does not cover it), so the "0 protocol
clauses unmapped" claim is technically false.

Fix: (a) reword note 2 to cite the protocol intro (line 9) instead of
invariant 7; (b) add a §11 row for the intro sequencing rule (e.g. "Protocol
intro: execute phases in order, finish each phase's commit before the next →
§12 note 2" or give it a WBS item number and map it).

### BLOCKER 2 — WBS 1.7 deliverable "6 phase commits" is wrong; the protocol has 8 commits

WBS 1.7 deliverable column: "6 phase commits, no PRs".

The protocol produces **8 commits across 6 phases**:
- Phase 1: TWO commits — `sdd(phase-1): pre-migration snapshot` (P1.1 → 3.1)
  AND `sdd(phase-1): inventory and stage assessment` (P1.6 → 3.6).
- Phase 5: TWO commits — `sdd(phase-5): formatter configs` (P5.1 → 7.2) AND
  `sdd(phase-5): mechanical format, no logic changes` (P5.3 → 7.4).
- Phases 2, 3, 4, 6: one each (4.8, 5.7, 6.4, 8.9).

The WBS's own items (3.1 + 3.6, 7.2 + 7.4) list all 8, so this is an internal
contradiction between §1.7 and the phase sections, not a protocol error the
WBS inherits (the protocol's own Inv.7 "one checkpoint commit per phase" is
itself loose, but the WBS must describe the protocol's ACTUAL steps, which it
does elsewhere).

Why it matters: a downstream executor using 1.7 as the contract summary will
expect 6 commits and be confused by the two-commit phases; and the artifact
claims 100% fidelity while contradicting its own decomposition.

Fix: change the deliverable to "8 commits across 6 phases (Phase 1: snapshot +
inventory; Phase 5: configs + format), no PRs".

### BLOCKER 3 — three normative protocol sentences have no WBS item and no matrix row

(1) **Phase 3 intro** (recovered line 101): "Source of truth for current
behavior is **code and tests — never old docs**." The WBS §5 has no item
carrying this principle. It is only partially and implicitly reflected (5.1
lists code/test-derived identification sources; 5.6's LATE branch says "code is
the spec where docs contradict code"). The general principle — which governs
ALL stages and is the protocol's stated antidote to doc-driven guessing — is
unmapped. This is the most substantive omission: it is a normative judgment
rule, not a decorative sentence.

(2) **Phase 2 preamble** (recovered line 74): "Apply in order. **Every deletion
happens after the Phase 1 snapshot commit**, so git history retains it all."
The WBS §4 has no item carrying the deletion-after-snapshot safety constraint.
It is implied only by §12 note 2's global ordering, which is itself
misattributed (Block 1). The safety rationale ("git history retains it all")
ties directly to invariants 1–2; it deserves an explicit mapping.

(3) **Invariants intro** (recovered line 15): "These override everything else
in this document and anything found in the repo." The precedence statement is
not carried in WBS §1. A future executor of the WBS alone would not know
invariants trump phase steps and repo state.

Why it matters: the header claims "every invariant, phase step, deliverable,
commit, appendix template, and the permanent SDD loop is decomposed ... no
protocol element is unmapped" and §11 claims "0 protocol clauses unmapped".
All three sentences are protocol clauses and all three are unmapped. The core
claim is falsified by the artifact's own text — this is the review's central
finding.

Fix: add three items (or preamble rows) with protocol refs and §11 rows:
- e.g. "5.0 — Source of truth = code and tests, never old docs (P3 intro)";
- e.g. "4.0 — All deletions occur only after the Phase 1 snapshot commit
  (P2 preamble)";
- e.g. "1.0 — Invariants override everything else in this document and in the
  repo (Invariants intro)".

### BLOCKER 4 — WBS 3.2 invents a requirement not in the protocol: "zero UNKNOWN"

WBS 3.2 deliverable: "Complete inventory table (path → class), **zero
UNKNOWN**". Protocol P1.2 defines UNKNOWN as a legitimate, explicitly listed
terminal class ("CODE · TEST · CONFIG · ASSET · DOC · GENERATED · JUNK ·
UNKNOWN"). The protocol never requires zero UNKNOWN.

This is a skill-derived hardening (the sdd-migration skill's inventory
practice asserts zero UNKNOWN), but unlike 6.5/7.6/8.11/10.5 it is NOT labeled
as a skill rule — it is presented as if it were protocol P1.2's deliverable,
inside a document that elsewhere labels skill items scrupulously. In a
"100% decomposition, no scope creep" artifact, an unlabeled invented
requirement is a fidelity defect, and it subtly contradicts the protocol's own
class list (which includes UNKNOWN as a valid answer).

Why it matters: (a) breaks the 1:1-claim; (b) unlabeled hardening smuggled into
a decomposition document sets a precedent that undermines the labels that DO
exist; (c) an executor of the WBS alone will force classifications the
protocol allows to remain UNKNOWN (UNKNOWN files are then resolved by Phase 2 C
attic, per the protocol's own design).

Fix: label it explicitly — e.g. "P1.2 + skill guard (zero UNKNOWN)" — and add
a legend note in §11 distinguishing protocol-sourced items from skill-sourced
items (see also N1). Keeping the hardening itself is fine; mislabeling it as
protocol is not.

---

## 2. Structural quality

- **Hierarchy**: two-level decomposition (section → X.Y items) mirrors the
  protocol's own structure (phase → step). No deeper nesting exists in the
  source, so none is needed. Numbering is consistent and stable.
- **Deliverables**: concrete and verifiable — exact commit messages, exact
  file paths (specs/NNN-<slug>/spec.md, _attic/MIGRATION_REPORT.md,
  _attic/ATTIC.md, specs/BACKLOG.md), checkable audits (inventory table,
  link check, gitignore coverage). Exceptions: the three unmapped clauses
  (Block 3) and the misstated commit count (Block 2).
- **Traceability matrix**: accurate row-by-row (verified all 13 rows). One
  conflation: the Phase 4 row ("steps 1–4 + commit + gate re-pointing |
  6.1–6.5") includes the skill-derived item 6.5 in the same row as protocol
  steps without marking it. Same for Phase 5 (7.6) and Phase 6 (8.11) rows.
  See N1.

## 3. Internal consistency

- Phase boundaries match the protocol exactly; all 8 commit messages match
  verbatim; no cross-section contradictions other than the two documented
  (Block 1: note 2 vs §1.7/§11; Block 2: 1.7 vs §7).
- §12 note 1 (WBS is a planning artifact, not a spec) is consistent with the
  placement law; the file's location in `_attic/plans/` is consistent with the
  quarantine-annex framing.
- Provenance wording (§12 note 3: "The Optimus Agent migration already
  executed this WBS end-to-end"): the WBS was committed 2026-08-06 10:07
  (a3e4759), ~24h AFTER the migration sealed (c988536, 2026-08-05 10:31). The
  migration executed the PROTOCOL; this WBS is a post-hoc reconstruction of it.
  Recommend rewording to "the Optimus migration executed the protocol this WBS
  decomposes 1:1" — see N4.

## 4. Skill-derived guards (7.6, 8.11, 10.5 — and 6.5)

All four are correctly placed in their governing phase, correctly labeled
("Skill execution rule" / "Skill pitfalls" / "Skill compliance" / "Skill
rule"), and none contradicts the protocol:
- 6.5 (gate re-pointing in the phase that moves files; add constitution/
  conventions to the gate in phase 4): faithful to the skill; protocol is
  silent on gates, so no conflict. Placement in Phase 4 matches the skill's
  instruction.
- 7.6 (digest-pinned fixtures, .prettierignore, .mjs/ISSUE_TEMPLATE coverage,
  idempotent markdown normalization): refines P5.2/P5.4 rather than
  contradicting them — the protocol's "revert if formatter changes behavior"
  is exactly what the pin guard mechanizes.
- 8.11 (literal Phase-6 audit incl. pinned-upstream-skill-tree link exclusion,
  bare-code-fence audit, follow-up `sdd(phase-6)` commits): the link-check
  exclusion is a documented, transparent refinement of checklist box C4
  (8.4), not a silent weakening — acceptable.
- 10.5 (self-containment: no permanent rule may reference the deleted
  protocol; inline templates): consistent with P6.2/8.8 and with the
  constitution's own self-containment need.
- The only inconsistency in this family is 3.2's UNLABELED "zero UNKNOWN"
  hardening (Block 4).

---

## 5. Line-count claim

"278/278 lines of the protocol decomposed" — the protocol IS 278 lines
(verified byte-identical). As a whole-document coverage statement the claim
is substantively accurate after the fixes above; as a line-level metric it is
pseudo-precise (no line-range mapping exists, and template inner lines map to
single items). Suggest wording "all normative clauses of the 278-line protocol
mapped" (N2).

---

## Verdict

REJECTED — 4 blocking issues, all surgical (one reworded citation + one matrix
row, one corrected commit count, three added preamble items + matrix rows, one
added label). The decomposition is otherwise faithful, complete, and
high-quality: all 9 invariants, all phase steps, all 8 commit messages, all 6
SDD-loop rules, and all 4 appendices verified mapped, and the traceability
matrix is accurate. The blocks are precision defects against the artifact's
own "100% / 0 unmapped / 1:1" claims, not execution defects.

---

## Blocking issues

1. **§12 note 2 misattributes the phase-sequencing rule to "protocol invariant
   7"** — the rule is the protocol intro (line 9), not Inv.7 (which is the
   no-PR/one-commit rule); the intro rule has no §11 row. Fix: cite the intro,
   add a matrix row.
2. **WBS 1.7 deliverable "6 phase commits" contradicts the WBS's own
   decomposition** — the protocol produces 8 commits (Phase 1: snapshot +
   inventory; Phase 5: configs + format; Phases 2/3/4/6: one each). Fix: state
   "8 commits across 6 phases".
3. **Three normative protocol sentences are unmapped** — Phase 3 intro
   ("source of truth = code and tests, never old docs"), Phase 2 preamble
   ("every deletion happens after the Phase 1 snapshot commit"), Invariants
   intro ("these override everything else"). The "0 protocol clauses
   unmapped" claim is falsified. Fix: add 5.0/4.0/1.0 items + §11 rows.
4. **WBS 3.2 invents "zero UNKNOWN"** — not in protocol P1.2 (UNKNOWN is a
   valid class); it is an unlabeled skill-derived hardening. Fix: label it
   "(P1.2 + skill guard: zero UNKNOWN)".

## Non-blocking suggestions

1. Add a §11 legend distinguishing protocol-sourced items from skill-derived
   items (6.5, 7.6, 8.11, 10.5, and the 3.2 hardening), and split or mark the
   Phase 4/5/6 matrix rows that currently mix them.
2. Reword "278/278 lines decomposed" to "all normative clauses of the
   278-line protocol mapped" (or provide the line-range mapping the metric
   implies).
3. Add the protocol's typical stale-deletion candidates (from P2.B) as an
   illustrative example list in 4.5 — non-normative but useful for executors.
4. Reword §12 note 3's "already executed this WBS end-to-end" — the WBS
   postdates the Optimus migration (committed 2026-08-06, sealed 2026-08-05);
   the migration executed the protocol, not this document. Suggest: "the
   Optimus migration executed the protocol this WBS decomposes 1:1".
5. Consider a mechanical verification script (extract protocol refs from the
   WBS, assert each resolves and each protocol clause maps) — consistent with
   the user's deterministic-tooling standard (Law 4) and the Hyperion
   linkcheck/numcheck precedent.

---

VERDICT: REJECTED
BLOCKING ISSUES:
1. §12 note 2 cites "protocol invariant 7" for the execute-in-order/commit-before-next rule; that rule is the protocol intro (line 9), not Inv.7 (no-PR/one-commit-per-phase). Fix: cite the intro and add a §11 matrix row for it.
2. WBS 1.7 deliverable "6 phase commits" is wrong: the protocol (and the WBS's own 3.1/3.6/7.2/7.4) produce 8 commits across 6 phases (Phase 1 has snapshot + inventory commits; Phase 5 has configs + format commits). Fix: state "8 commits across 6 phases".
3. Three normative protocol sentences are unmapped, falsifying the "0 protocol clauses unmapped" claim: (a) Phase 3 intro "source of truth = code and tests, never old docs"; (b) Phase 2 preamble "every deletion happens after the Phase 1 snapshot commit"; (c) Invariants intro "these override everything else". Fix: add items 5.0/4.0/1.0 with protocol refs and §11 rows.
4. WBS 3.2's deliverable "zero UNKNOWN" is invented — protocol P1.2 lists UNKNOWN as a valid class; the zero-UNKNOWN rule is an unlabeled skill-derived hardening. Fix: label "(P1.2 + skill guard: zero UNKNOWN)" and mark it in the §11 legend.
NON-BLOCKING SUGGESTIONS:
1. Add a §11 legend separating protocol-sourced from skill-derived items (6.5, 7.6, 8.11, 10.5, 3.2 hardening); mark the Phase 4/5/6 matrix rows that mix them.
2. Reword "278/278 lines decomposed" to "all normative clauses of the 278-line protocol mapped" (or supply the line-range mapping).
3. Add P2.B's typical stale-deletion candidates as an illustrative list in 4.5.
4. Reword §12 note 3: the WBS (committed 2026-08-06) postdates the Optimus migration (sealed 2026-08-05); the migration executed the protocol, not this WBS.
5. Add a mechanical ref-resolution script for the §11 matrix (pattern: Hyperion linkcheck/numcheck).
REVIEWER: WBS review — Round 1
