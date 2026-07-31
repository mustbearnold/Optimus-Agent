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
forbidden task evidence in the final answer of the final session (25), approval
friction (15), tool efficiency (10), and terminal integrity (10). A vertical
bar in a required term denotes acceptable wording alternatives. A pass requires
at least 80 and no exact finding. The machine-readable findings are the
regression inputs; the aggregate score is a trend signal, not a substitute for
investigating the transcript. `--regrade RUN_DIR` deterministically applies a
new rubric or evaluator to stored observations without another model call.

Future exploration may replace the deterministic simulator or add a model
judge, but neither may share context with Optimus or overwrite the objective
SQLite-derived dimensions. Periodic real-human calibration remains necessary
to detect simulator habits that stop resembling real users.
