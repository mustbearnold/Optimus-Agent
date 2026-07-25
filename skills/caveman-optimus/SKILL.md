---
name: caveman-optimus
description: >
  Use for Optimus S+++ program phases (P10–P19), architecture marks, fail-closed
  gates, verification docs, Engineering Memory refresh, and GitHub delivery
  (wip → PR → local pr/N). Also for “continue”, next phase, claim hygiene, and
  plane-safe naming. Chat compact; repo artifacts full English; prove with hold suite.
version: 0.1.0
license: MIT
---

# Caveman Optimus

Repo-local copy of the Optimus S+++ delivery skill. Full doctrine:

- User agents path: `~/.agents/skills/caveman-optimus/SKILL.md` (same content intent)
- Laws: root `AGENTS.md`, `docs/contributing/artifact-naming.md`,
  `docs/contributing/github-conventions.md`

## Quick loop

1. Read `docs/plans/s-plus-plus-plus-program.md` **Immediate next action** + phase microtasks.
2. Branch `wip/<phase-slug>` from updated `main`.
3. Implement only microtasks; keep planes separate (`P##` ≠ `PR #N` ≠ `ADR`).
4. Hold suite for owned gates; EM `generate` + `validate --quick` when docs/code knowledge moved.
5. Move mark only with verification file + marks + program “done”.
6. On ship: emoji commit → `python3 scripts/github_pr_branch.py open …` → local `pr/<N>-…`.

## Default hold (docs / release / architecture)

```bash
python3 scripts/check-parity-ledger.py
python3 scripts/optimus_version.py release-check
python3 scripts/check-architecture-marks.py
python3 scripts/engineering_memory.py check
python3 scripts/engineering_memory.py generate
python3 scripts/engineering_memory.py validate --quick
python3 scripts/engineering_memory.py report
```

## Chat handoff shape

```text
P## → mark S+++ (or residual).
Hold green: <list>.
PR #N · remote wip/… · local pr/N-… · next P##.
Non-claims: …
```

## Hard bans

- Greenwash S+++ without exit evidence
- Rename remote PR head to `pr/N-…` (closes PR)
- Hand-edit `.engineering-memory/*.json`
- Caveman grammar in commits/PR/docs
- Equating architecture Release S+++ with Hermes `gate` PASS
