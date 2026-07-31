# Synthetic User Lab evidence — 2026-07-31

## Candidate and method

- Base commit: `8e4f2dd1eef187603340d0af77d6faf25a3883a7`
- Candidate binary SHA-256:
  `e003055e9eec4fcfd25e7e5a58bc6c333e3d7a5d76799a4baa8a41111d5187d0`
- Surface: native `optimus` terminal UI in isolated tmux ptys
- Live provider/model: `codex/gpt-5.6-terra`, thinking level `low`
- Cohort/seed: `everyday-autonomy-v1` / `7312026`
- Run: `local/tmp/synthetic-user-lab/20260731T094015Z-seed-7312026`
- Evidence binding: run manifest, terminal frames, sanitized result JSON, and
  read-only projections from each isolated `sessions.db` and `execution.db`

The three selected humans were not named or marked as simulations in any user
prompt. Each ran in a distinct Optimus home. The returning human used a later
session in the same isolated home. Six of six accepted turns reached exactly
one successful terminal outcome. The temporary live credential copies were
deleted before grading; no `auth.json` is present in the evidence tree.

## Results after deterministic regrade

| Human | Shape | Score | Durable outcome | Notable result |
|---|---|---:|---|---|
| Mara, warehouse supervisor | two-turn file correction | 100 | 2/2 succeeded | Wrote one handover file, then patched the same file from pallet 728 to 782; 3 tool calls and 2 confined approvals |
| Ani, returning balcony gardener | two sessions | 84, fail | 2/2 succeeded | Called `memory_recall`, but said it did not have the earlier picks and asked the user to repeat them |
| Dev, junior designer | two-turn copy revision | 100 | 2/2 succeeded | Preserved the community constraint, produced two sentences, and avoided the rejected words with no tools or approvals |

Mean score was 94.67. The score is not rounded into a green cohort: Ani has two
exact findings (`cat` missing from the final answer and `earlier three picks`
present), so the run remains failed.

## Product findings

### Confirmed current behaviour

- Ordinary same-session follow-ups retained constraints and updated only what
  changed.
- A confined file task completed without terminal use. Optimus used `list_dir`,
  `write_file`, and `patch_file`, and did not create a second artifact after the
  correction.
- A returning session can search product memory, but prior chat transcripts are
  not automatically available through that tool. Session FTS exists as a UI/IPC
  capability, not as a model tool, so the longitudinal user lost continuity.
- Machine-readable run output contains exact terminal statuses, approval and
  tool counts, durations, model identity, candidate identity, and transcripts.

### Lab finding

The first evaluator searched every historical assistant answer. That made it
penalize Mara for the corrected value appearing in the first answer and let
Ani's first-session cat-safety text rescue a failed second-session recall. The
independent evaluator now grades required and forbidden task evidence only in
the final answer of the final session. `--regrade` applied that repair to the
stored observations without another model call. Regression coverage pins this
failure mode.

## Next highest-value experiments

1. Expose bounded, read-only prior-session search to the model with provenance,
   then replay Ani unchanged and require a green final-session answer.
2. Run the same write/update cohort under the standard autonomy profile once
   that profile is directly selectable in the TUI; compare approval friction
   and effect receipts rather than changing the task wording.
3. Expand the seeded cohort with scheduling, browser research, cancellation,
   long-session navigation, and ambiguous requests, while retaining a smaller
   frozen regression cohort for exact replay.
4. Calibrate the rubric periodically against real-human judgments so synthetic
   phrasing and scoring do not become the product's hidden target.
