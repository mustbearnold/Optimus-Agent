---
doc_id: evidence-adaptive-synthetic-user-lab-2026-07-31
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: - Persona simulator: Ollama 0.32.5, qwen3:8b, 16,384-token context. - GPU observation during generation: RTX 5070, 100% GPU residency, about 7.4 GiB model memory. Plan calls took 2.6–3.3 seconds and warm follow-up calls took 0.6–1.5...
reviewed_on: 2026-07-31
review_by: never
---

# Adaptive Synthetic User Lab — 2026-07-31

**Status:** Confirmed current behaviour for the named runs only

## Binding

- Persona simulator: Ollama 0.32.5, `qwen3:8b`, 16,384-token context.
- GPU observation during generation: RTX 5070, 100% GPU residency, about
  7.4 GiB model memory. Plan calls took 2.6–3.3 seconds and warm follow-up
  calls took 0.6–1.5 seconds.
- Optimus target: real native TUI, Codex OAuth, resolved model
  `gpt-5.6-terra`, `low` thinking, `review_changes`, confined workspace.
- Private persona plans remained outside Optimus messages. Copied OAuth state
  was deleted before evidence extraction.

## Runs and interpretation

### Quick task, seed 7312028

Optimus produced a useful one-hour-per-day small-business website plan in one
successful turn. The driver then refused its own evidence because the local
human declared satisfaction before a generated two-turn minimum. This was a
harness defect, not an Optimus failure. Quick tasks now permit honest one-turn
completion.

### Project journey, seed 7312029

Optimus created a usable `rota.csv` and `README.md` in one successful turn.
The bounded workspace inspection confirmed both artifacts and their content.
The original evaluator reported 100, but its permissive approval budget hid two
approval prompts for two harmless confined file writes. The adaptive project
budget is now one approval, so future runs expose that friction.

### Project journey, seed 7312031

The scenario completed three successful Optimus turns and created a substantive
beginner Python pack: `study-guide.md`, `practice-quiz.md`, and
`answer-key.md`. Durable execution recorded five tool calls and three approval
prompts. The run scored 66 under the tightened rubric.

Two distinct findings came from the failure:

1. **Confirmed Optimus product friction:** three user approvals were required
   for harmless writes already confined to the requested isolated workspace.
2. **Confirmed harness defect:** the local simulator inverted roles in two
   follow-ups, speaking as if it had created/reviewed Optimus's files. Product
   conclusions must not be drawn from those follow-up answers.

The adapter now rejects common role inversions and retries locally before any
additional OAuth turn. Multi-turn scenario classes constrain `done=false`
through structured output until their declared minimum, while quick tasks may
finish naturally. Required artifact evidence may be satisfied by bounded
workspace facts rather than demanding that the final conversational reply
repeat every filename.

## Product direction

`project_journey` is one scenario class in a domain-neutral harness. These runs
do not redefine Optimus as a project builder. The same simulator boundary also
samples quick tasks, research, recovery, revision, and longitudinal use; new
classes can be added without changing the Optimus target or evaluator roles.
