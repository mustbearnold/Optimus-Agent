#!/usr/bin/env python3
"""Offline contract tests for the seeded native Synthetic User Lab."""

from __future__ import annotations

import copy
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from synthetic_user_lab import DEFAULT_COHORT, compile_manifest, load_cohort, public_scenario  # noqa: E402
from synthetic_user_lab_eval import evaluate  # noqa: E402


def main() -> int:
    cohort = load_cohort(DEFAULT_COHORT)
    first = compile_manifest(cohort, 42, 3)
    second = compile_manifest(cohort, 42, 3)
    assert first == second, "the same seed must compile the same cohort"
    assert first != compile_manifest(cohort, 43, 3), "different seeds should explore different cohorts"
    assert "private_profile" not in first
    assert all(len(value) == 64 for value in first["private_profile_sha256"].values())

    scenario = cohort["scenarios"][0]
    public = public_scenario(scenario)
    assert "private_profile" not in public, "the evaluator boundary leaked simulator state"
    required = " and ".join(term.split("|")[0] for term in scenario["rubric"]["required_terms"])
    observation = {
        "expected_turns": 2,
        "turns": [
            {"status": "succeeded"},
            {"status": "succeeded"},
        ],
        "sessions": [{"messages": [{"role": "assistant", "content": required}]}],
        "approvals": scenario["rubric"]["max_approvals"],
        "tool_calls": scenario["rubric"]["max_tool_calls"],
        "duration_ms": 50,
        "candidate_bindings": [{
            "provider": "offline", "model": "offline", "autonomy_profile": "review_changes",
            "command_fs_envelope": "confined_no_network",
        }],
    }
    grade = evaluate(public, observation)
    assert grade["score"] == 100 and grade["passed"]

    failed = copy.deepcopy(observation)
    failed["turns"][1]["status"] = "failed"
    failed["approvals"] += 3
    bad = evaluate(public, failed)
    assert not bad["passed"] and bad["score"] < grade["score"]
    assert {row["code"] for row in bad["findings"]} >= {
        "approval_budget_exceeded", "terminal_turn_failures"
    }

    # Earlier conversation context must neither rescue failed cross-session
    # recall nor make a corrected value look stale.
    final_only = copy.deepcopy(observation)
    final_only["sessions"] = [
        {"messages": [{"role": "assistant", "content": "old context 728 cat"}]},
        {"messages": [{"role": "assistant", "content": required}]},
    ]
    assert evaluate(public, final_only)["passed"]

    workspace_only = copy.deepcopy(observation)
    workspace_only["sessions"] = [{"messages": [{"role": "assistant", "content": "done"}]}]
    workspace_only["workspace"] = {
        "files": [{"path": "artifact.txt", "excerpt": required}],
    }
    assert evaluate(public, workspace_only)["passed"]

    leaked = copy.deepcopy(cohort)
    leaked["scenarios"][0]["sessions"][0][0] = "[SIM-1] do the task"
    try:
        load_cohort_from_value(leaked)
    except ValueError:
        pass
    else:
        raise AssertionError("a harness marker in user text must be rejected")
    print("SYNTHETIC_USER_LAB_SELFTEST_OK")
    return 0


def load_cohort_from_value(payload: dict) -> dict:
    import json
    import tempfile

    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "cohort.json"
        path.write_text(json.dumps(payload), encoding="utf-8")
        return load_cohort(path)


if __name__ == "__main__":
    raise SystemExit(main())
