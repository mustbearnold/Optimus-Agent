#!/usr/bin/env python3
"""Validate the thesis-axis capability ledger and scorecard marker.

Re-keyed per docs/architecture/north-star-2026-07.md (decided in #63): Hermes
is not the yardstick, so rows carry a `thesis_axis` (project-integrity /
one-core-many-faces) instead of a `hermes_reference`, and every row's
`trajectory` is either RUNNABLE — a `cargo:`/`playwright:` reference this
validator resolves to a real target — or `null` and pinned on the shrink-only
UNCLASSIFIED_TRAJECTORIES list below. The six C-criteria carry the red; this
ledger carries the counter (13/50 runnable at re-key, target 50/50).
"""

from __future__ import annotations

import json
import re
import sys
from collections import Counter
from pathlib import Path

from optimus_version import evaluate as evaluate_versioning

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "docs" / "architecture" / "parity-capability-ledger.json"
SCORECARD = ROOT / "docs" / "architecture" / "sota-scorecard.md"
PROGRAM = (
    ROOT
    / "_attic"
    / "plans"
    / "historical"
    / "2026-07-19_161855-hermes-parity-parallel-subagent-program.md"
)
E2E_DIR = ROOT / "apps" / "optimus-desktop" / "e2e"
VALID_STATES = {"missing", "partial", "parity", "win"}
VALID_AXES = {"project-integrity", "one-core-many-faces"}
REQUIRED = {
    "id",
    "capability",
    "thesis_axis",
    "state",
    "evidence",
    "trajectory",
    "owner_ticket",
}

# Rows with no runnable trajectory yet. Pinned 2026-07-28 at the re-key (38
# measured in #63, minus the deleted eval.comparative row); may only shrink —
# do NOT add entries. Giving a row a runnable trajectory deletes its line.
UNCLASSIFIED_TRAJECTORIES = frozenset(
    {
        "artifacts.store-ui",
        "browser.annotations",
        "browser.cdp",
        "browser.http",
        "campaign.subagents",
        "chat.thinking-tools",
        "core.pack-budget",
        "core.tool-loop",
        "cron.lifecycle",
        "desktop.cua",
        "desktop.logs",
        "desktop.native-cua",
        "files.mutate",
        "gateway.discord-slack",
        "gateway.queue",
        "gateway.telegram",
        "gateway.ui",
        "mcp.client",
        "media.voice",
        "memory.ui",
        "migration.hermes",
        "packs.breadth",
        "plugins.signed",
        "profiles.isolation",
        "projects.scope",
        "provider.catalog",
        "provider.failover",
        "release.updater",
        "session.search-hygiene",
        "skills.ui",
        "surface.acp",
        "surface.commands",
        "surface.proxy",
        "surface.tui",
        "terminal.pty",
        "web.search",
    }
)


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def trajectory_resolves(trajectory: str) -> str | None:
    """Return an error string if the runnable reference does not resolve."""
    if trajectory.startswith("cargo:"):
        ref = trajectory[len("cargo:") :]
        crate, _, test = ref.partition("/")
        if not crate or not test:
            return f"cargo trajectory must be cargo:<crate>/<test>, got {trajectory!r}"
        integration = ROOT / "crates" / crate / "tests" / f"{test}.rs"
        if integration.exists():
            return None
        # Inline `mod tests` in a source module is equally runnable
        # (`cargo nextest run -p <crate> -- <test>::`).
        inline = ROOT / "crates" / crate / "src" / f"{test}.rs"
        if inline.exists() and "mod tests" in inline.read_text(encoding="utf-8"):
            return None
        return (
            f"cargo trajectory target missing: {integration.relative_to(ROOT)} "
            f"(and no src/{test}.rs with a tests module)"
        )
    if trajectory.startswith("playwright:"):
        needle = trajectory[len("playwright:") :].split(";")[0].strip()
        if not needle:
            return f"playwright trajectory has an empty needle: {trajectory!r}"
        for spec in sorted(E2E_DIR.glob("*.spec.js")):
            if needle in spec.read_text(encoding="utf-8"):
                return None
        return f"playwright needle not found in {E2E_DIR.relative_to(ROOT)}: {needle!r}"
    return f"trajectory must start with cargo: or playwright:, got {trajectory!r}"


def main() -> int:
    errors: list[str] = []
    try:
        ledger = json.loads(LEDGER.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"parity ledger unreadable: {exc}", file=sys.stderr)
        return 1

    if ledger.get("schema_version") != 1:
        fail(errors, "schema_version must be 1")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", str(ledger.get("updated_at", ""))):
        fail(errors, "updated_at must be YYYY-MM-DD")
    if set(ledger.get("states", [])) != VALID_STATES:
        fail(errors, f"states must equal {sorted(VALID_STATES)}")

    capabilities = ledger.get("capabilities")
    if not isinstance(capabilities, list) or not capabilities:
        fail(errors, "capabilities must be a non-empty list")
        capabilities = []

    ids = [row.get("id") for row in capabilities if isinstance(row, dict)]
    duplicates = sorted(key for key, count in Counter(ids).items() if count > 1)
    if duplicates:
        fail(errors, f"duplicate capability ids: {duplicates}")

    try:
        program = PROGRAM.read_text(encoding="utf-8")
        valid_tickets = set(re.findall(r"(?m)^\| (PF-\d+|[A-EP]-\d+) \|", program))
    except OSError as exc:
        fail(errors, f"program plan unreadable: {exc}")
        valid_tickets = set()

    runnable = 0
    for index, row in enumerate(capabilities):
        prefix = f"capabilities[{index}]"
        if not isinstance(row, dict):
            fail(errors, f"{prefix} must be an object")
            continue
        missing_fields = sorted(REQUIRED - row.keys())
        if missing_fields:
            fail(errors, f"{prefix} missing fields: {missing_fields}")
            continue
        capability_id = row["id"]
        prefix = str(capability_id)
        if not re.fullmatch(r"[a-z0-9]+(?:[.-][a-z0-9]+)*", str(capability_id)):
            fail(errors, f"{prefix}: invalid id")
        if row["thesis_axis"] not in VALID_AXES:
            fail(errors, f"{prefix}: thesis_axis must be one of {sorted(VALID_AXES)}")
        state = row["state"]
        if state not in VALID_STATES:
            fail(errors, f"{prefix}: invalid state {state!r}")
        evidence = row["evidence"]
        if not isinstance(evidence, list) or any(not isinstance(item, str) for item in evidence):
            fail(errors, f"{prefix}: evidence must be a string list")
            evidence = []
        for evidence_path in evidence:
            if not (ROOT / evidence_path).exists():
                fail(errors, f"{prefix}: missing evidence path {evidence_path}")
        if state in {"parity", "win"} and not evidence:
            fail(errors, f"{prefix}: {state} requires evidence")
        trajectory = row["trajectory"]
        if trajectory is None:
            if capability_id not in UNCLASSIFIED_TRAJECTORIES:
                fail(
                    errors,
                    f"{prefix}: no runnable trajectory and not pinned — new rows "
                    "must carry a runnable trajectory at birth",
                )
        else:
            if not isinstance(trajectory, str):
                fail(errors, f"{prefix}: trajectory must be a string or null")
            else:
                problem = trajectory_resolves(trajectory)
                if problem:
                    fail(errors, f"{prefix}: {problem}")
                else:
                    runnable += 1
            if capability_id in UNCLASSIFIED_TRAJECTORIES:
                fail(
                    errors,
                    f"stale pin: {prefix} has a runnable trajectory but is still "
                    "on UNCLASSIFIED_TRAJECTORIES — shrink it",
                )
        if not str(row["owner_ticket"]).strip():
            fail(errors, f"{prefix}: owner_ticket is required")
        else:
            owners = {owner.strip() for owner in str(row["owner_ticket"]).split(",")}
            unknown = sorted(owner for owner in owners if owner != "HOLD" and owner not in valid_tickets)
            if unknown:
                fail(errors, f"{prefix}: unknown owner tickets {unknown}")

    for pinned in sorted(UNCLASSIFIED_TRAJECTORIES - set(ids)):
        fail(
            errors,
            f"stale pin: {pinned} is not in the ledger — shrink UNCLASSIFIED_TRAJECTORIES",
        )

    counts = Counter(
        row["state"]
        for row in capabilities
        if isinstance(row, dict) and row.get("state") in VALID_STATES
    )

    try:
        scorecard = SCORECARD.read_text(encoding="utf-8")
    except OSError as exc:
        fail(errors, f"scorecard unreadable: {exc}")
        scorecard = ""
    expected_marker = f"Updated: {ledger.get('updated_at')} · {ledger.get('scorecard_marker')}"
    if expected_marker not in scorecard:
        fail(errors, f"stale scorecard marker; expected {expected_marker!r}")
    for state in sorted(VALID_STATES):
        expected_row = f"| **{state}** | {counts[state]} |"
        if expected_row not in scorecard:
            fail(errors, f"stale scorecard count; expected row prefix {expected_row!r}")
    expected_total = f"| **total** | {len(capabilities)} |"
    if expected_total not in scorecard:
        fail(errors, f"stale scorecard total; expected row prefix {expected_total!r}")

    versioning = evaluate_versioning(ROOT)
    for error in versioning.errors:
        fail(errors, f"versioning: {error}")

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    total = len(capabilities)
    print(
        "parity-ledger ok "
        f"capabilities={total} "
        + " ".join(f"{state}={counts[state]}" for state in sorted(VALID_STATES))
        + f" trajectories={runnable}/{total} runnable (target {total}/{total})"
        f" unclassified={len(UNCLASSIFIED_TRAJECTORIES)} (shrink-only)"
        + f" hermes_target={versioning.target_version} feature_contracts={versioning.feature_total}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
