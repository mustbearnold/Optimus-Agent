# Synthetic User Lab

**Status:** Confirmed current behaviour (version 1)

The Synthetic User Lab turns ad-hoc role-play into repeatable product evidence.
It drives the real native terminal UI, not a private kernel shortcut, and binds
each result to the candidate binary plus Optimus's durable SQLite records.

## Separation of roles

1. The **simulator** owns a private persona profile and a seeded sequence of
   natural, incomplete user messages. Persona names, test markers, hidden goals,
   and scoring instructions are never sent to Optimus.
2. **Optimus** runs in a separate home for each persona. Multi-turn sessions use
   the same durable session; returning personas use a later `/new` session in
   the same isolated home.
3. The deterministic **evaluator** receives only the public rubric and a
   sanitized projection of `sessions.db` and `execution.db`. It cannot see the
   private persona profile and therefore cannot reward the story that generated
   the test.

The cohort source is `evals/synthetic-user-lab/cohort-v1.json`. Selection is
seeded, so failures replay exactly while a different seed rotates the humans.
Fresh personas and longitudinal returners are separate scenario kinds.

## Evidence and safety contract

- `scripts/synthetic_user_lab.py` drives the candidate in a tmux pty.
- One persona equals one isolated Optimus home and workspace.
- Live credentials may be copied from an explicit `--auth-source` only for the
  duration of the native process. The copy is deleted before results are read.
- Approval handling is declared per scenario: deny, or approve only actions
  already confined to that persona's isolated home. The lab never enables
  unrestricted-host mode.
- The run manifest records seed, exact candidate SHA-256, requested provider,
  model and thinking level, selected scenario ids, hashes of private profiles,
  and the distinct provider/model/access bindings resolved by durable execution
  manifests after the cohort completes.
- Frames, sanitized transcripts, terminal turn outcomes, tool counts, approval
  counts, duration, findings, and score live under `local/tmp/synthetic-user-lab/`.
  Credentials and raw database exports are not report artifacts.

## Modes

Validate and inspect a deterministic cohort without launching Optimus:

```text
python3 scripts/synthetic_user_lab.py --plan --seed 7312026 --count 3
```

Run the native offline regression path:

```text
python3 scripts/synthetic_user_lab.py --provider offline --count 3
```

Run a bounded live cohort against an explicit candidate and credential source:

```text
python3 scripts/synthetic_user_lab.py --provider codex --thinking low \
  --auth-source ~/.local/share/optimus/auth.json --count 3
```

Offline mode proves the harness, isolation, PTY, persistence, and evaluator
contracts. Live mode measures product behaviour. A product-quality claim must
name which mode ran; a green offline echo is never presented as evidence that a
real model completed the user's task.

## Scoring

The version-1 score is an integer out of 100: turn completion (40), required and
forbidden task evidence in the final answer and any bounded adaptive-workspace
observation (25), approval
friction (15), tool efficiency (10), and terminal integrity (10). A vertical
bar in a required term denotes acceptable wording alternatives. A pass requires
at least 80 and no exact finding. The machine-readable findings are the
regression inputs; the aggregate score is a trend signal, not a substitute for
investigating the transcript. `--regrade RUN_DIR` deterministically applies a
new rubric or evaluator to stored observations without another model call.

## Adaptive local humans

`scripts/adaptive_user_lab.py` adds exploration without replacing the frozen
regression cohort. A seeded Ollama model owns the private human and chooses each
next message after seeing Optimus's last answer plus a bounded inspection of its
isolated workspace. Optimus still runs through the real TUI on the independently
selected target provider; the local model is never substituted for Optimus.

The adaptive seam is deliberately domain-neutral. Its scenario classes include
quick tasks, multi-turn revisions, research, recovery, longitudinal users, and
real project journeys. These are sampling dimensions, not a product roadmap:
the harness can add future classes without changing the simulator/Optimus/
evaluator boundary.

```text
python3 scripts/adaptive_user_lab.py \
  --scenario-class project_journey \
  --simulator-model qwen3:8b \
  --auth-source ~/.local/share/optimus/auth.json
```

Private profiles and simulator state remain outside the text sent to Optimus.
The public manifest stores only the profile hash; the ignored evidence tree
keeps the private plan for replay diagnosis. Ollama call timing/token counts,
native frames, target provider/model bindings, workspace facts, transcript,
and SQLite outcomes are recorded separately.

Scenario-class semantics are enforced structurally. Quick tasks may finish in
one turn, while revision, recovery, longitudinal, and project journeys require
the simulator to produce a user-role follow-up until their declared minimum.
The adapter rejects common role inversions (for example, a simulated user
claiming it created Optimus's files) and retries locally without spending a
target-model turn.

A local model is never allowed to grade its own interaction. Periodic real-human
and independent frontier-model calibration remains necessary to detect simulator
habits that stop resembling real users, and neither may overwrite the objective
SQLite-derived dimensions.
