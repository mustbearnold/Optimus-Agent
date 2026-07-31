---
doc_id: evidence-engineering-memory-v2-redesign-report
doc_type: evidence
plane: evidence
status: historical
authority: record
summary: - Date: 2026-07-25 - Worktree: /home/mustbearnold/Projects/worktrees/optimus-engineering-memory-v2 - Branch: redesign/engineering-memory-v2 - Base: main @ c8b7e27
reviewed_on: 2026-07-31
review_by: never
knowledge_type: evidence
covers:
  - scripts/engineering_memory.py
  - scripts/test_engineering_memory.py
  - .engineering-memory/**
depends_on:
  - docs/decisions/0032-engineering-memory-compact-lenses.md
validated_by:
  - scripts/test_engineering_memory.py
---

# Engineering Memory v2 redesign report

- **Date:** 2026-07-25
- **Worktree:** `/home/mustbearnold/Projects/worktrees/optimus-engineering-memory-v2`
- **Branch:** `redesign/engineering-memory-v2`
- **Base:** `main` @ `c8b7e27`

## Goal

Make Engineering Memory as fast and token-efficient as possible without
sacrificing high quality or high accuracy.

## What shipped

### Fact plane (schema v2)

- Compact canonical JSON (sorted keys, no pretty-print)
- `knowledge-staleness.json`: hash/count/pattern only (no `covered_files`)
- `change-impact.json`: `pattern_to_knowledge` only (no expanded
  `source_to_knowledge` / `resolved_tests`)
- `manifest.json`: counts, artifact hashes, serving policy
- Local gitignored `.hash-cache.json` for mtime/size/inode hash acceleration
- `owns` / `watches` frontmatter support (`covers` remains owns-compatible)
- Tool registry operational envelope templates (`storage: templated_v2`)

### Lens plane

Commands:

- `context --budget N`
- `impact --path ...`
- `owner --path ...`
- `tools [--available]`
- `stale`
- `report`
- `stat`
- `validate --quick`
- plus `check` / `generate` / `validate` / `binding`

### Coverage precision

High-churn docs narrowed from crate-wide globs to precise `owns` with advisory
`watches`:

- `docs/architecture/system-overview.md`
- `docs/contracts/high-risk-contracts.md`
- `docs/maps/*` (repository, security, observability, memory)
- historical priority specs and broad plans/lessons

Example: changing `crates/optimus-kernel/src/openai_compat.rs` now hard-impacts
2 docs and watch-notifies 7, instead of stale-forcing architecture/contracts and
nanotask specs.

### Authority updates

- ADR-0032
- Engineering Memory guide, update skill, AGENTS workflow
- Phase plan updated (Phase 6 compact lenses; retrieval becomes Phase 7)
- Decisions index entry for 0032

## Measured results

Compared against main worktree artifacts at redesign start.

| Metric | Before (main) | After (v2 complete) | Delta |
|---|---:|---:|---:|
| Generated JSON bytes | 955,403 | ~311,844 | **-67%** |
| Approx tokens if fully loaded | ~238,850 | ~77,961 | **-67%** |
| `knowledge-staleness.json` | 537,563 B | ~48,509 B | **-91%** |
| `tool-registry.json` | 43,375 B | ~24,550 B | **-43%** |
| `check` latency | ~3.8 s | **0.10 s** | **~38×** |
| `validate --quick` | n/a | **~1.0 s** | new hot path |
| `validate` full | ~5.2 s | **~1.8 s** | **~3×** |
| `context` latency | n/a | **~0.13 s** | new |
| `context` tokens (leaf kernel file) | often huge/ad hoc | **745 / 3000** | budgeted |
| Unit tests | 18 (1 pre-existing fail) | **23 pass** | fixed + added |

### Accuracy gates preserved

- Fail-closed tool/workflow/contract extraction
- Deterministic generation equality
- No ambient Git identity in generated facts
- Semantic supersession guards retained
- Full validate still rebuilds and byte-compares all artifacts
- Source/tests still outrank prose
- No invented specialist agents/tools
- Template expansion required before tool field validation

### Command smoke

```text
ENGINEERING_MEMORY_CURRENT
ENGINEERING_MEMORY_VALID (full and quick)
ENGINEERING_MEMORY_REPORT ... recommendation: no knowledge refresh required
EM_CONTEXT v2 ... used_tokens<=3000
```

## Agent workflow now

```text
python scripts/engineering_memory.py check
python scripts/engineering_memory.py context --budget 3000
# edit owned docs only if needed
python scripts/engineering_memory.py generate
python scripts/engineering_memory.py validate --quick
python scripts/engineering_memory.py report
```

Do **not** load raw `.engineering-memory/*.json` into prompts.

## Residual debt (non-blocking)

1. Narrow remaining app-shell docs that still use broad UI/electron globs.
2. Typed Rust extract helper when catalog parser churn justifies it.
3. Optional non-authoritative retrieval lens only after fixture baselines.

## Non-goals kept

- Not merged into runtime `optimus-memory`
- No LLM-authored authoritative facts
- No embeddings on the correctness path
- No ambient commit self-identity in generated indexes

## Verification commands

```bash
cd /home/mustbearnold/Projects/worktrees/optimus-engineering-memory-v2
python -m unittest scripts.test_engineering_memory -v
python scripts/engineering_memory.py generate
python scripts/engineering_memory.py validate
python scripts/engineering_memory.py validate --quick
python scripts/engineering_memory.py check
python scripts/engineering_memory.py context --budget 3000 --path crates/optimus-kernel/src/openai_compat.rs
python scripts/engineering_memory.py impact --path crates/optimus-kernel/src/openai_compat.rs
python scripts/engineering_memory.py stat
python scripts/engineering_memory.py report
```
