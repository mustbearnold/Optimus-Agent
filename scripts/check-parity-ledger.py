#!/usr/bin/env python3
"""Validate the evidence-backed Hermes parity ledger and scorecard marker."""

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
    / ".hermes"
    / "plans"
    / "2026-07-19_161855-hermes-parity-parallel-subagent-program.md"
)
VALID_STATES = {"missing", "partial", "parity", "win"}
REQUIRED = {
    "id",
    "capability",
    "hermes_reference",
    "state",
    "evidence",
    "trajectory",
    "owner_ticket",
}


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


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
        if state in {"parity", "win"}:
            if not evidence:
                fail(errors, f"{prefix}: {state} requires evidence")
            if not row["trajectory"]:
                fail(errors, f"{prefix}: {state} requires a trajectory")
        elif row["trajectory"] is not None:
            fail(errors, f"{prefix}: {state} must not claim a passing trajectory")
        if not str(row["owner_ticket"]).strip():
            fail(errors, f"{prefix}: owner_ticket is required")
        else:
            owners = {owner.strip() for owner in str(row["owner_ticket"]).split(",")}
            unknown = sorted(owner for owner in owners if owner != "HOLD" and owner not in valid_tickets)
            if unknown:
                fail(errors, f"{prefix}: unknown owner tickets {unknown}")

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

    print(
        "parity-ledger ok "
        f"capabilities={len(capabilities)} "
        + " ".join(f"{state}={counts[state]}" for state in sorted(VALID_STATES))
        + f" hermes_target={versioning.target_version} feature_contracts={versioning.feature_total}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
