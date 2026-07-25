# Optimus versioning and Hermes parity policy

**Status:** active, fail-closed  
**Optimus product version:** `0.1.0`  
**Tracked Hermes target:** `0.19.0` at upstream revision `8967e73e`  
**Verified Hermes parity version:** none

## Why there are two versions

Optimus has an independent product version and a separate Hermes parity version.
They answer different questions:

- **Optimus product version** is normal SemVer from `Cargo.toml`.
- **Hermes target version** is the exact Hermes release currently being audited.
- **Hermes parity version** is `null` until every gate in this document passes for one immutable Optimus revision.

A normal Optimus development release may use any honest independent version. If
its three-part numeric SemVer core equals the tracked Hermes number, the release
check refuses it unless the Hermes parity claim is verified. Prerelease or build
suffixes do not disguise that collision. This prevents an accidental or
marketing-only numerical match.

Example while work is incomplete:

```text
Optimus Agent 0.1.0
Hermes target: 0.19.0
Hermes parity: unverified
Frozen Hermes feature contracts: 2063
```

## Non-negotiable parity invariant

Optimus may claim `Hermes parity: X.Y.Z` only when the exact candidate:

1. implements or strictly exceeds **every** feature contract frozen from Hermes
   `X.Y.Z`;
2. has executable, revision-bound evidence for each feature contract;
3. has no `missing` or `partial` row in the human parity rollup;
4. matches or beats Hermes success rate and deterministic quality score;
5. matches or beats Hermes p50 and p95 wall latency and time-to-first-token;
6. matches or beats Hermes cost per successful task and peak resident memory;
7. passes the required comparison scenarios on the same machine, model,
   provider, permissions, and paired randomized task order;
8. uses fresh evidence from a clean, immutable Optimus revision; and
9. has a completed audit against the official Hermes documentation.

There are **no feature waivers**. An equivalent Optimus design is allowed, but
it must prove the same user-visible outcome and edge behavior. A missing Hermes
feature cannot be traded for an unrelated Optimus advantage.

## Sources of truth

| File | Purpose |
|---|---|
| `docs/architecture/optimus-version.json` | Version target, claim, release rules, and benchmark thresholds |
| `docs/architecture/hermes-baselines/hermes-0.19.0.json` | Frozen machine inventory for Hermes 0.19.0 |
| `docs/architecture/hermes-manual-capabilities.json` | Non-CLI product capabilities curated from official docs/source |
| `docs/architecture/hermes-feature-evidence.json` | Per-feature Optimus evidence bound to a commit |
| `docs/architecture/hermes-performance-evidence.json` | Raw paired benchmark samples and protocol provenance |
| `docs/architecture/parity-capability-ledger.json` | Human-readable capability rollup and ownership |
| `scripts/optimus_version.py` | Capture, validation, status, release, and promotion gate |
| `scripts/check-parity-ledger.py` | Rollup validation plus version-system integrity check |

Executable evidence outranks prose. Architecture documents are not parity
proof unless a claim also names a passing trajectory and an existing evidence
artifact.

## Frozen Hermes inventory

The v0.19.0 baseline contains **2,063 distinct contracts** and has SHA-256:

```text
cafbcf313b4fbd7885b4df9b888a2539885d8d62ec55e6df1cf88dc0e66cf725
```

It inventories:

- recursively discovered CLI commands and options;
- slash commands, aliases, and subcommands;
- toolsets and statically registered tools;
- provider catalog entries;
- bundled messaging platforms; and
- non-CLI capabilities from the official product surface.

The source capture is tied to official commit `8967e73e`, not to the locally
modified Hermes checkout. Normalized ID collisions are retained as independent,
deterministically suffixed contracts; capture never drops one silently. MCP
server tool names are intentionally dynamic and unbounded, so the frozen
contract covers MCP client/server behavior rather than arbitrary third-party
runtime names.

The machine capture has zero warnings. The separate official-documentation
inventory audit remains `pending`, so parity is blocked even if someone were to
populate evidence prematurely.

## Per-feature evidence contract

`hermes-feature-evidence.json` maps each frozen feature ID to a claim. A passing
claim has this shape:

```json
{
  "cli.command.example": {
    "status": "verified",
    "evidence": ["path/to/current/test-or-report"],
    "trajectory": "cargo:package/test-name",
    "verified_at": "2026-07-23T12:00:00Z",
    "optimus_revision": "40-character-git-commit-sha"
  }
}
```

Rules:

- Every baseline ID must be present and `verified`.
- Evidence paths must exist.
- A named executable trajectory is mandatory.
- Evidence older than 30 days does not pass.
- All feature claims must refer to the same clean Optimus revision.
- Unknown IDs are schema errors, not ignored extensions.

`missing`, `partial`, `not-applicable`, `waived`, and prose-only evidence never
pass the parity gate.

## Comparative performance contract

The performance report stores raw paired samples. It does not accept manually
entered aggregate claims. Every required scenario needs at least 30 paired
samples across at least three distinct seeds:

1. cold start;
2. single-turn response;
3. multi-tool turn;
4. long session;
5. session resume;
6. scheduled job;
7. browser task; and
8. delegated task.

Each sample contains `hermes` and `optimus` records with `success`, a
reproducible `quality_score`, and the metrics required by that scenario.
The gate recomputes all statistics.

Hard thresholds:

| Axis | Requirement |
|---|---|
| Success rate | Optimus ≥ Hermes |
| Deterministic quality | Optimus ≥ Hermes |
| Wall time p50 and p95 | Optimus / Hermes ≤ 1.0 |
| TTFT p50 and p95 | Optimus / Hermes ≤ 1.0 |
| Cost per successful task | Optimus / Hermes ≤ 1.0 |
| Peak RSS p50 and p95 | Optimus / Hermes ≤ 1.0 |

The report must also affirm same machine, same model, same provider, same tool
permissions, and randomized paired order. It must hash the dataset, deterministic
grader, benchmark harness, Hermes binary, and Optimus binary, identify the
machine/provider/model, and record each sample's case ID, seed, and execution
order. Both `hermes-first` and `optimus-first` samples are required. Evidence is
valid for 30 days and must target the exact Hermes baseline and Optimus commit.

## Commands

```bash
# Human and machine-readable status
python3 scripts/optimus_version.py status
python3 scripts/optimus_version.py status --json

# Structural integrity; incomplete parity is reported but is not an error
python3 scripts/optimus_version.py validate

# Strict full-parity gate; expected to fail until all work is complete
python3 scripts/optimus_version.py gate

# Release preflight. Development versions pass; false matching claims fail.
python3 scripts/optimus_version.py release-check

# Existing rollup plus version-system integrity
python3 scripts/check-parity-ledger.py

# Architecture S+++ claim hygiene (not Hermes product parity)
python3 scripts/check-architecture-marks.py

# Record parity only after all blockers are gone
python3 scripts/optimus_version.py promote --reviewer "reviewer identity"

# Built CLI status
optimus version
optimus version --json
```

Both `scripts/rebuild-install-relaunch.sh` and
`scripts/rebuild-install-relaunch.ps1` run `release-check` before build/binary
selection, then run it again and revalidate both selected binary versions
immediately before stopping or replacing an installed application. Their
`VERSION.txt` and `install-meta.json` record the target, parity value, claim
status, and frozen feature count.

## Capturing a clean Hermes baseline

Never capture from a dirty or locally patched Hermes tree. Use an exact detached
worktree and the installed Hermes virtualenv only as the dependency runtime:

```bash
source_repo="$HOME/.hermes/hermes-agent"
clean_source="$(mktemp -d /tmp/optimus-hermes-0.19.0-XXXXXX)"
git -C "$source_repo" worktree add --detach "$clean_source" 8967e73e
python3 scripts/optimus_version.py capture-hermes \
  --hermes-source "$clean_source" \
  --hermes-python "$source_repo/venv/bin/python"
git -C "$source_repo" worktree remove --force "$clean_source"
```

Capture updates the baseline hash in the version manifest and both evidence
files. Existing evidence is therefore invalidated whenever the baseline bytes
change.

## When Hermes publishes a new version

1. Update `hermes_target` to the new exact version, release date, and upstream
   revision.
2. Reset `parity_claim` to `unverified` with null metadata.
3. Capture a clean baseline from that exact revision.
4. Re-audit the official docs and mark the inventory audit complete only after
   resolving every discrepancy.
5. Add evidence for every new or changed feature contract.
6. Re-run all paired comparison scenarios on one immutable Optimus revision.
7. Run `validate`, the repository test suites, `gate`, and `release-check`.
8. Use `promote` only after the gate has no error or blocker.

A previously verified older Hermes parity version may remain historical, but it
must not be presented as parity with the newly tracked release.

## Current honest status

Optimus `0.1.0` tracks Hermes `0.19.0`, but parity is **unverified**:

- feature contracts verified: `0 / 2063` under the new strict per-feature schema;
- rollup rows below parity: `37 / 51`;
- required performance scenarios passing: `0 / 8`;
- official-documentation inventory audit: pending.

This is intentional. The version system exists to prevent the number from
advancing ahead of the product and evidence.
