#!/usr/bin/env python3

"""Offline contract tests for the desktop self-improvement loop (no app)."""

from __future__ import annotations

import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))

from desktop_self_improvement_loop import grade_iteration  # noqa: E402


def observation(**overrides: dict) -> dict:
    """A clean deepseek turn observation with db/git shape."""
    observation = {
        "db": {
            "turns": [{"status": "succeeded", "error_code": None}],
            "execution": {
                "timing_events": [
                    {"kind": "tool_started", "status": "", "name": "read_file", "step": 1},
                    {"kind": "tool_finished", "status": "succeeded", "name": "read_file", "step": 1},
                    {"kind": "model_finished", "status": "succeeded", "name": "", "step": None},
                    {"kind": "turn_finished", "status": "succeeded", "name": "", "step": None},
                ],
                "approvals": [],
            },
        },
        "git": {"changed": True},
    }
    for key, value in overrides.items():
        observation[key] = value
    return observation


def main() -> int:
    # clean turn passes
    assert grade_iteration({"git_change": True}, observation())["passed"] is True

    # failed tool call without detail is strict (cannot prove it recovered)
    bad = observation()
    bad["db"]["execution"]["timing_events"].append(
        {"kind": "tool_finished", "status": "failed", "name": "terminal", "step": 2}
    )
    grade = grade_iteration({"git_change": True}, bad)
    assert not grade["passed"] and grade["tool_errors"] == 1, "failed tool call must fail"

    # permission error (policy denied) must fail
    denied_call = observation()
    denied_call["db"]["execution"]["timing_events"].append(
        {"kind": "tool_finished", "status": "failed", "name": "terminal", "step": 2,
         "call_id": "call_1", "error_message": "runtime: policy denied: developer_access_scope_or_capability"}
    )
    assert not grade_iteration({"git_change": True}, denied_call)["passed"], "permission error must fail"

    # recoverable outcome (wrong path guess) passes and is reported
    recovered = observation()
    recovered["db"]["execution"]["timing_events"].append(
        {"kind": "tool_finished", "status": "failed", "name": "read_file", "step": 2,
         "call_id": "call_2", "error_message": "tool: read scripts/nope.py: not found: script"}
    )
    grade = grade_iteration({"git_change": True}, recovered)
    assert grade["passed"], "recoverable outcome must pass"
    assert grade["tool_errors_recovered"] == 1

    # unresolved approval (approval_required with no approval row is a
    # stuck pause; the turn cannot succeed, but grade it explicitly)
    stuck = observation()
    stuck["db"]["execution"]["timing_events"].append(
        {"kind": "tool_finished", "status": "approval_required", "name": "terminal", "step": 2}
    )
    stuck["db"]["turns"] = [{"status": "running", "error_code": None}]
    assert not grade_iteration({"git_change": True}, stuck)["passed"], "stuck approval must fail"

    # an APPROVED approval pause is clean (real-person behaviour)
    approved = observation()
    approved["db"]["execution"]["timing_events"].append(
        {"kind": "tool_finished", "status": "approval_required", "name": "terminal", "step": 2}
    )
    approved["db"]["execution"]["approvals"] = [{"status": "approved"}]
    assert grade_iteration({"git_change": True}, approved)["passed"], "approved approval must pass"

    # denied approval
    denied = observation()
    denied["db"]["execution"]["approvals"] = [{"status": "denied"}]
    assert not grade_iteration({"git_change": True}, denied)["passed"], "denied approval must fail"

    # pending approval at turn end
    pending = observation()
    pending["db"]["execution"]["approvals"] = [{"status": "pending"}]
    assert not grade_iteration({"git_change": True}, pending)["passed"], "pending approval must fail"

    # no tool calls
    inert = observation()
    inert["db"]["execution"]["timing_events"] = [
        {"kind": "model_finished", "status": "succeeded", "name": "", "step": None}
    ]
    assert not grade_iteration({"git_change": True}, inert)["passed"], "no tool calls must fail"

    # no repository change
    static = observation()
    static["git"] = {"changed": False}
    assert not grade_iteration({"git_change": True}, static)["passed"], "no git change must fail"

    # failed turn
    failed_turn = observation()
    failed_turn["db"]["turns"] = [{"status": "failed", "error_code": "tool_error"}]
    assert not grade_iteration({"git_change": True}, failed_turn)["passed"], "failed turn must fail"

    print("DESKTOP_SELF_IMPROVEMENT_SELFTEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
