# Synthetic User Lab evidence — 2026-07-31, round 2

## Candidate and method

- Base commit: `e07fd0da84d52fb535d2e6c401f64a557df6ee46`
- Candidate binary SHA-256:
  `e003055e9eec4fcfd25e7e5a58bc6c333e3d7a5d76799a4baa8a41111d5187d0`
- Surface: native `optimus` terminal UI in isolated tmux ptys
- Live provider/model: `codex/gpt-5.6-terra`, thinking level `low`
- Access: `review_changes` with the `confined` command envelope
- Cohort/seed: `everyday-autonomy-v1` / `2`
- Run: `local/tmp/synthetic-user-lab/20260731T095827Z-seed-2`

Seed 2 selected the two cohort humans not exercised in the preceding live run.
The exact prompts contained no simulation markers or persona names. Each human
had a distinct Optimus home and workspace. Read-only queries bound the visible
TUI conversations to `sessions.db` and `execution.db`. Temporary credential
copies were removed before grading; no `auth.json` remains in the evidence.

## Results after deterministic regrade

| Human | Shape | Score | Durable outcome | Notable result |
|---|---|---:|---|---|
| Sam, food-pantry volunteer | two-turn CSV budget update | 100 | 2/2 succeeded | Correctly kept a 15% reserve, updated the same CSV with the forgotten $190 crates, and reported the plan was $49.70 under the spendable limit |
| Jo, parent coordinating a day | two-turn schedule correction | 100 | 2/2 succeeded | Preserved the 90-minute invoice block and identified that the moved dentist appointment now conflicts with school pickup |

The cohort mean is 100 after correcting one rubric defect described below.

## Confirmed current behaviour

- Sam's CSV contains the exact revised rows: $1,990.30 planned spend, $2,040.00
  maximum spendable, $409.70 unspent, and $49.70 remaining capacity.
- Optimus used `write_file` twice against the same `committee_budget.csv` path.
  It did not create a duplicate, invoke the terminal, or perform path-discovery
  work. Both confined writes completed in under 11 ms tool time.
- The active `review_changes` profile still required one approval for the first
  harmless workspace write and another approval to update that same file.
  This is correct policy enforcement as implemented, but it confirms the user's
  reported permission-wall friction for ordinary, reversible workspace work.
- Jo used no tools or approvals. It changed only the affected schedule blocks,
  retained all stated constraints, and explicitly identified the remaining
  real-world dependency instead of inventing a feasible timetable.
- All four accepted turns produced one successful terminal outcome. No failed,
  cancelled, or running turn remained.

## Evaluator finding

The original Sam rubric required the final prose to contain `190`. Optimus had
already persisted that line item in the CSV and reported every decision-relevant
updated total, so failing the answer for not repeating one input was grading
verbosity rather than task completion. The rubric now requires the revised
$1,990.30 total plus an explicit over/within-limit conclusion. Stored durable
observations were regraded without another provider call.

## Next experiment

The repeated confined-write approvals are now reproduced by two independent
humans (Mara and Sam). The next autonomy experiment should replay an unchanged
workspace-write scenario under a directly selectable `standard` profile and
compare approval counts, tool receipts, terminal outcomes, and file contents.
The TUI currently exposes the default `review_changes` path and `/yolo`, but no
bounded command for selecting `standard`; unrestricted-host mode is not an
acceptable substitute for this test.
