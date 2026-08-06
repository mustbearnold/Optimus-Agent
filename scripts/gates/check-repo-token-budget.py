#!/usr/bin/env python3
"""Enforce token budgets on the agent-facing surfaces of the repository.

Approved plan: `Development/tmp/plan-token-efficiency-draft.md` (D-5,
single-agent owner gate, 3 rounds, APPROVED 2026-08-07). Agents consume
tokens every session from a small hot path (AGENTS.md, the profile skill,
orient/em-context output) and from on-demand cold docs. Without a gate the
savings rot within weeks, so this is a **ratchet**, mirroring the
module-size pattern (ADR-0049):

  * a surface whose measured size exceeds its `budget_bytes` fails the gate
  * budgets live in `docs/architecture/token-budget-baseline.json`
  * `--update` re-baselines to the measured size — allowed only after a
    deliberate, committed growth wave (D-5 protocol); it never runs in CI
  * the profile-skill surface is optional: when no Hermes profile skill
    exists under $HOME, it is skipped (the repo gate never depends on
    profile state)

Size is measured in **bytes** (chars). Estimated tokens ≈ bytes ÷ 4 for
prose (documented conversion factor; the ratchet measures chars and
reports the estimate — DeepSeek/cl100k tokenizers differ <15% on English
prose).

Exit 0 when clean; exit 1 with findings.

  python3 scripts/gates/check-repo-token-budget.py          # gate
  python3 scripts/gates/check-repo-token-budget.py --report # table, no gating
  python3 scripts/gates/check-repo-token-budget.py --update # re-baseline
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASELINE = ROOT / "docs" / "architecture" / "token-budget-baseline.json"

# Surface definitions: key -> how to measure it.
#   - "file": the root-relative path's size
#   - "dir_md": sum of *.md sizes under the root-relative directory
#   - "profile_skill": largest matching SKILL.md under $HOME/.hermes/profiles;
#     optional (skipped when nothing matches)
SURFACES: dict[str, dict] = {
    "AGENTS.md": {"kind": "file", "rel": "AGENTS.md", "optional": False},
    "CONTEXT.md": {"kind": "file", "rel": "CONTEXT.md", "optional": False},
    "docs/architecture.md": {"kind": "file", "rel": "docs/architecture.md", "optional": False},
    "skills/ (in-repo)": {"kind": "dir_md", "rel": "skills", "optional": False},
    "skill:optimus-agent-development/SKILL.md": {"kind": "profile_skill", "optional": True},
}


def profile_skill_paths(home: Path) -> list[Path]:
    """Every profile-skill SKILL.md under `home/.hermes/profiles`."""
    return sorted(
        (home / ".hermes" / "profiles").glob(
            "*/skills/software-development/optimus-agent-development/SKILL.md"
        )
    )


def measure(root: Path = ROOT, home: Path | None = None) -> dict[str, int]:
    """Measured byte sizes for every surface; missing optional surfaces absent."""
    home = home or Path.home()
    sizes: dict[str, int] = {}
    for key, spec in SURFACES.items():
        kind = spec["kind"]
        if kind == "file":
            path = root / spec["rel"]
            if path.is_file():
                sizes[key] = path.stat().st_size
        elif kind == "dir_md":
            base = root / spec["rel"]
            if base.is_dir():
                sizes[key] = sum(
                    path.stat().st_size
                    for path in base.rglob("*.md")
                    if path.is_file()
                )
        elif kind == "profile_skill":
            candidates = profile_skill_paths(home)
            if candidates:
                sizes[key] = max(path.stat().st_size for path in candidates)
    return sizes


def load_baseline(root: Path = ROOT) -> dict[str, dict]:
    if not BASELINE.exists() and root == ROOT:
        raise SystemExit(
            f"{BASELINE}: baseline missing; run --update after a deliberate "
            "growth wave or land the approved token-efficiency plan"
        )
    if root != ROOT:
        alt = root / "docs" / "architecture" / "token-budget-baseline.json"
        if not alt.exists():
            return {}
        payload = json.loads(alt.read_text(encoding="utf-8"))
        return payload["surfaces"]
    return json.loads(BASELINE.read_text(encoding="utf-8"))["surfaces"]


def write_baseline(root: Path, sizes: dict[str, int]) -> None:
    alt = root / "docs" / "architecture" / "token-budget-baseline.json"
    current = load_baseline(root) if alt.exists() else {}
    surfaces: dict[str, dict] = {}
    for key in SURFACES:
        measured = sizes.get(key)
        if measured is None:
            # Optional surface absent — carry its budget forward so a later
            # reappearance is still policed.
            if key in current:
                surfaces[key] = current[key]
            continue
        surfaces[key] = {
            "budget_bytes": measured,
            "target_bytes": current.get(key, {}).get("target_bytes", measured),
            "note": current.get(key, {}).get("note", ""),
        }
    alt.write_text(
        json.dumps(
            {
                "comment": (
                    "Token-budget ratchet for agent-facing surfaces (approved "
                    "plan Development/tmp/plan-token-efficiency-draft.md, D-5). "
                    "budget_bytes is a ceiling: surfaces may never exceed it. "
                    "target_bytes is the approved-plan destination, reached "
                    "when the corresponding workstream lands. Re-baseline "
                    "(--update) only after a deliberate, committed growth wave."
                ),
                "measure": "bytes (chars); est. tokens ~= bytes / 4",
                "surfaces": surfaces,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def findings(root: Path = ROOT, home: Path | None = None) -> list[str]:
    home = home or Path.home()
    sizes = measure(root, home)
    baseline = load_baseline(root)
    problems: list[str] = []

    for key, spec in SURFACES.items():
        budget = baseline.get(key, {}).get("budget_bytes")
        if budget is None:
            problems.append(f"{key}: no budget in token-budget-baseline.json")
            continue
        measured = sizes.get(key)
        if measured is None:
            if not spec["optional"]:
                problems.append(f"{key}: surface missing (required)")
            continue
        if measured > budget:
            target = baseline.get(key, {}).get("target_bytes")
            hint = f" (target {target} B)" if target and target < measured else ""
            problems.append(
                f"{key}: {measured} B exceeds budget {budget} B{hint}. "
                "Shrink it, or re-baseline only after a deliberate, committed "
                "growth wave (`--update`)."
            )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", action="store_true", help="print sizes, no gating")
    parser.add_argument("--update", action="store_true", help="re-baseline to measured sizes")
    args = parser.parse_args()

    sizes = measure()

    if args.report:
        print(f"{'size':>10}  surface")
        for key, size in sorted(sizes.items(), key=lambda kv: -kv[1]):
            print(f"{size:>10}  {key}")
        return 0

    if args.update:
        write_baseline(ROOT, sizes)
        print(f"token-budget baseline updated: {len(sizes)} surfaces pinned")
        return 0

    problems = findings()
    if problems:
        print("token-budget check FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(
        "token-budget ok surfaces="
        f"{len(sizes)} budgets={len(load_baseline(ROOT))}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
