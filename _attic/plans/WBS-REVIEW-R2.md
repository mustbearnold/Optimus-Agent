# WBS Review — Round 2 (RE-REVIEW of the SDD Migration Protocol decomposition)

- **Reviewer**: WBS review — Round 2 (independent, ASTRONOMICAL-THINK mode)
- **Date**: 2026-08-06
- **Artifact**: `_attic/plans/wbs.md` (160 lines, working tree; base commit `a3e4759`)
- **Ground truth**: `/tmp/SDD_MIGRATION_recovered.md` — 278 lines, git hash
  `cdb4f3f2e1ae0a156de7faa7067a13b2c6ec6524`, re-verified THIS round as
  byte-identical to `c988536^:SDD_MIGRATION.md` (the sealed protocol).
- **Method**: (1) `git diff a3e4759..worktree` to isolate exactly what the fix
  round changed; (2) verify each of the 4 Round-1 blockers against the ACTUAL
  WBS text and the ACTUAL protocol lines; (3) re-audit the whole protocol
  (all 278 lines) for residual unmapped clauses and the §11 matrix for
  accuracy; (4) check for new errors introduced by the fixes.

---

## 1. Verification of the four Round-1 fixes

### Fix 1 (§12 note 2 citation) — PARTIALLY correct; new over-mapping introduced

- §12 note 2 now reads: `(protocol intro: "Execute phases in order. Finish
  each phase's commit before starting the next.")`.
  Protocol line 9 is: `Execute phases in order. Finish each phase's commit
  before starting the next.` — **verbatim match, including the terminal
  period.** The misattribution to "protocol invariant 7" is gone. ✓
- The new §11 row exists: `Protocol intro — "…" | §12 note 2 + 1.7`.
- **DEFECT (new, introduced by the fix):** the row maps the protocol intro to
  **1.7**, but WBS 1.7 carries **Inv.7** — "No PR/branch ceremony; one
  checkpoint commit per phase" — and its deliverable states a commit **count**
  ("8 commits across 6 phases"), not the **sequencing** rule. Item 1.7 does
  not contain the clause the row claims it maps, and 1.7 now appears in TWO
  matrix rows (the "Invariants 1–9" row and the new intro row), violating the
  matrix's own "maps each protocol clause 1:1" property. This is the same
  defect class as Round-1 blocker 1 (citation misattribution in the
  execution-order context), re-introduced in the very row that was supposed
  to fix it.

### Fix 2 (commit count in 1.7) — CORRECT ✓

- 1.7 deliverable now reads: `8 commits across 6 phases (P1: snapshot +
  inventory; P5: configs + format), no PRs`.
- Cross-checked against the protocol: P1 = 2 commits (lines 59, 68), P2 = 1
  (95), P3 = 1 (116), P4 = 1 (125), P5 = 2 (lines 131, 133), P6 = 1 (153) →
  **8 commits across 6 phases. Exactly right.** All 8 messages appear
  verbatim in WBS items 3.1, 3.6, 4.8, 5.7, 6.4, 7.2, 7.4, 8.9. The internal
  contradiction is resolved.

### Fix 3 (rows 1.0 / 4.0 / 5.0) — MOSTLY correct; one clause of the same family still unmapped

- **1.0** added: "Invariants override everything else in the protocol and any
  repo content | Inv. intro" — protocol line 15: "These override everything
  else in this document and anything found in the repo." ✓ (faithful
  paraphrase, correct ref), with §11 row. ✓
- **5.0** added: "Source of truth for current behavior is code and tests —
  never old docs | P3 intro" — protocol line 101 verbatim. ✓, with §11 row. ✓
- **4.0** added: "Every deletion happens AFTER the Phase 1 snapshot commit,
  so git history retains it all | P2 preamble" — protocol line 74, second
  sentence verbatim. ✓, with §11 row. ✓
- **DEFECT (residual, same clause family as Round-1 blocker 3):** the Phase 2
  preamble is TWO sentences — `Apply in order. Every deletion happens after
  the Phase 1 snapshot commit, so git history retains it all.` (line 74).
  **"Apply in order" — the directive that deletion groups A–D run in
  sequence — still has no WBS item and no matrix row.** The fixer mapped only
  the second sentence. Round 1's own standard rejected "implied by global
  ordering" as sufficient mapping; the same standard applies here: item
  ordering 4.1→4.8 is a structural implication, not a mapping. The coverage
  claim "0 protocol clauses unmapped" therefore remains falsified — by the
  preamble the fix was supposed to close.

### Fix 4 (3.2 "zero UNKNOWN" labeling) — CORRECT ✓ (exceeds the ask)

- 3.2 ref now `P1.2 + skill guard`; deliverable now reads: `Complete
  inventory table (path → class), zero UNKNOWN (skill-hardened: UNKNOWN is a
  valid protocol class; the zero-UNKNOWN target is a skill-derived hardening
  beyond P1.2)`. Protocol P1.2 (lines 60–61) does list UNKNOWN among the
  eight classes. The hardening is now unambiguously labeled, self-consistent
  with the protocol, and the invented-as-protocol defect is gone. ✓

---

## 2. Re-audit of the whole protocol (all 278 lines) for residual unmapped clauses

Re-checked beyond the four fixes:

- **9 invariants** → 1.1–1.9: all mapped, no paraphrase loss. ✓
- **Protocol intro (line 9)** → §12 note 2 (+ matrix row, modulo the 1.7
  over-mapping above). ✓
- **Invariants intro (line 15)** → 1.0. ✓
- **Target end-state tree + placement law + monorepo rule** → 2.1–2.7. ✓
- **Phase 1 steps 1–6** → 3.1–3.6; both commit messages verbatim. ✓
- **Phase 2:** groups A–D → 4.1–4.4 (all four labeled P2.A — correct, the
  protocol's four bullets are all under "A. Delete on sight"; note the
  non-shared qualifier on `.idea/`/`.vscode/` is preserved in 4.1 ✓), stale
  rule with all three conditions → 4.5 ✓, attic + exact ATTIC.md line format
  → 4.6 ✓, gitignore → 4.7 ✓, commit + build verify → 4.8 ✓. **Residual:
  "Apply in order" (see Fix-3 defect).**
- **Phase 3:** intro → 5.0 ✓; steps 1–6 → 5.1–5.6 (all 7 capability sources,
  5–15 target, sub-features-as-sections, [inferred] tagging, three fates with
  no-fourth-fate + ADR conversion by original date order, BACKLOG, thin
  README, EARLY/MID/LATE adaptations) ✓; commit → 5.7 verbatim ✓.
- **Phase 4 steps 1–4** → 6.1–6.4 + skill item 6.5. ✓
- **Phase 5 steps 1–4** → 7.1–7.5 (keep-existing-config, configs-first
  commit, markdown normalization, zero-logic commit, revert-and-note
  behavior-change) + skill pitfalls 7.6. Both commit messages verbatim. ✓
- **Phase 6:** all six checklist boxes → 8.1–8.6 ✓; four seal steps →
  8.7–8.10 (incl. protocol deletion and handoff) ✓; commit verbatim → 8.9 ✓;
  skill guard → 8.11 ✓. (Nit: the checklist intro "every box or the
  migration is not done" has no explicit carrier; its force is structural
  via six mandatory rows — non-blocking, see S3.)
- **SDD loop 1–6** → 9.1–9.6, "installed in constitution.md" placement
  noted. ✓
- **Appendices A–D** → 10.1–10.4 ✓; self-containment → 10.5 ✓.
  (Nit: 10.3's formatter table drops the Go row's linter `go vet` /
  `goimports` and Shell's `shellcheck` — a summary-row truncation, not a
  blocker; the executor still has the protocol in-tree until 8.8.)
- **§11 matrix completeness:** all 73 WBS items are referenced; ranges are
  contiguous; no orphaned items; every protocol element appears in exactly
  one row **except** 1.7, which now appears in two (the over-mapping defect).
- **Coverage statement** (rewritten): "every protocol clause decomposed …
  0 protocol clauses unmapped (verified against the recovered protocol)."
  The "278/278 lines" pseudo-metric is gone (Round-1 N2 addressed ✓), and
  the statement is honest about the added rows — but its "0 unmapped" claim
  is falsified by the residual "Apply in order" clause, and:
- **NEW FINDING (pre-existing, unflagged in Round 1):** the header coverage
  claim (line 5) cites "The traceability matrix **(§9)**" — §9 is
  "Permanent Rules — The SDD Loop"; the matrix is **§11**. A broken internal
  cross-reference in the artifact's flagship fidelity claim. One-character
  fix.

## 3. Other Round-1 non-blocking items — disposition

- **N1 (§11 legend for skill-derived items):** still absent. Rows
  "Phase 4 | 6.1–6.5", "Phase 5 | 7.1–7.6", "Phase 6 | 8.1–8.11" still mix
  protocol and skill provenance unmarked. Carried forward (non-blocking).
- **N2 (line-count claim):** addressed — see above. ✓
- **N3 (stale-candidate examples in 4.5):** not added. Carried (non-blocking).
- **N4 (provenance note, §12 note 3):** **still unfixed.** The note still
  reads "The Optimus Agent migration already executed this WBS end-to-end
  (sealed `c988536`, 2026-08-05)". The WBS was committed 2026-08-06 10:07
  (`a3e4759`), ~24h AFTER the migration sealed (2026-08-05). The migration
  executed the PROTOCOL; this WBS is a post-hoc reconstruction. The sentence
  is objectively false. Remains non-blocking per Round-1 weighting, but it is
  a one-line fix in a document whose entire value proposition is fidelity.
- **N5 (mechanical ref-resolution script):** not added. Carried (non-blocking).

---

## Verdict

REJECTED — 3 blocking issues, all surgical. Two of the four fixes are fully
correct (commit count, zero-UNKNOWN labeling), one is verbatim-correct in
its quote but introduces a new over-mapping in the matrix row it added (1.7),
and one is a partial fix that leaves the P2 preamble's first sentence —
"Apply in order." — unmapped, in the same clause family Round 1 blocked on.
Plus one broken internal cross-ref (§9 → §11) in the header coverage claim,
which this document's own standard ("0 clauses unmapped", "maps each clause
1:1") cannot tolerate.

The decomposition itself remains otherwise faithful, complete, and
high-quality: all 9 invariants, all phase steps, all 8 commit messages
verbatim, all 6 SDD-loop rules, all 4 appendices, and every skill-derived
guard correctly placed and labeled. The blocks are precision defects against
the artifact's own claims — the exact class this document exists to prevent.

---

VERDICT: REJECTED
BLOCKING ISSUES:
1. The new §11 intro row maps the protocol intro to "§12 note 2 + 1.7", but WBS 1.7 carries Inv.7 (no-PR, one checkpoint commit per phase; deliverable = commit COUNT), not the sequencing rule; item 1.7 now appears in two matrix rows, violating the matrix's 1:1 property. Fix: drop "+ 1.7" — the row should read "Protocol intro — … | §12 note 2".
2. The Phase 2 preamble's first sentence — "Apply in order." (protocol line 74) — remains unmapped: 4.0 carries only the deletion-after-snapshot sentence, so "0 protocol clauses unmapped" is still false. Fix: fold it into 4.0's work package, e.g. "Apply in order — groups A–D; every deletion happens AFTER the Phase 1 snapshot commit, so git history retains it all" (one cell edit; the §11 row then covers the full preamble).
3. Header coverage claim (line 5) cites "the traceability matrix (§9)" — the matrix is §11; §9 is the SDD-loop section. Broken internal cross-ref in the flagship claim. Fix: change "§9" to "§11".
NON-BLOCKING SUGGESTIONS:
1. §12 note 3 provenance claim remains false: the WBS (committed 2026-08-06, a3e4759) postdates the Optimus migration (sealed 2026-08-05, c988536); the migration executed the protocol, not this WBS. Fix (one line): "the Optimus Agent migration executed the protocol this WBS decomposes 1:1 (sealed c988536, 2026-08-05)".
2. Add the §11 legend separating protocol-sourced from skill-derived items (6.5, 7.6, 8.11, 10.5, and 3.2's P1.2+skill-guard row); mark the Phase 4/5/6 rows that mix provenances (Round-1 N1, carried).
3. Phase 6 checklist intro ("every box or the migration is not done") has no explicit carrier — its force is structural via 8.1–8.6; consider a one-line note in the §8 header row if strict 1:1 is desired.
4. 10.3's formatter table drops Go's `goimports`/`go vet` and Shell's `shellcheck` — restore for exactness (protocol Appendix C).
5. Round-1 N3/N5 carried: illustrative stale-candidate list in 4.5; mechanical ref-resolution script for the §11 matrix.
REVIEWER: WBS review — Round 2
