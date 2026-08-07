#!/usr/bin/env python3

"""Offline contract tests for the desktop task suite (no app launch)."""

from __future__ import annotations

import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools"))

import json  # noqa: E402

from desktop_task_evidence import term_matches  # noqa: E402
from desktop_task_harness import grade_task  # noqa: E402
from desktop_task_suite import BINDINGS_OFFLINE, TASKS, preflight_skip, validate_tasks  # noqa: E402
from desktop_task_atspi import atspi_available, choose_channel, submit_prompt  # noqa: E402


def offline_observation(**overrides: dict) -> dict:
    """A passing offline-echo observation with DOM/db/console shape."""
    observation = {
        "dom": {
            "transcript": [
                {"role": "user", "text": "Echo token T1-ABCD1234 and stop."},
                {"role": "assistant", "text": "offline echo: Echo token T1-ABCD1234 and stop."},
            ],
            "new_session_empty": True,
            "reopened_transcript": ["Echo token T1-ABCD1234 and stop."],
        },
        "db": {
            "turns": [{"status": "succeeded"}],
            "sessions": [{"messages": [{"role": "user", "content": "Echo token T1-ABCD1234 and stop."}]}],
            "execution": {
                "manifests": 1,
                "durations_ms": [150],
                "tool_calls": 0,
                "approvals": 0,
                "bindings": [{"provider": "offline", "model": "offline-scripted"}],
            },
        },
        "console": {"events": []},
    }
    for key, value in overrides.items():
        observation[key] = value
    return observation


def offline_expect() -> dict:
    return {
        "turns": 1,
        "sessions": 1,
        "manifests": 1,
        "echo": True,
        "prompts": ["Echo token T1-ABCD1234 and stop."],
        "bindings": BINDINGS_OFFLINE,
        "tool_calls": 0,
        "approvals": 0,
        "min_duration_ms": 1,
        "max_exceptions": 0,
    }


def main() -> int:
    # --- task contracts ---------------------------------------------------
    validate_tasks(TASKS)  # raises on any contract violation
    assert [t["difficulty"] for t in TASKS] == ["easy", "medium", "hard", "ultra-hard"]
    bad = json.loads(json.dumps(TASKS[0]))
    try:
        validate_tasks([bad, json.loads(json.dumps(TASKS[0]))])
        raise AssertionError("duplicate task ids must be rejected")
    except ValueError:
        pass
    no_nonce = json.loads(json.dumps(TASKS[0]))
    no_nonce["sessions"] = [["no nonce here"]]
    try:
        validate_tasks([no_nonce])
        raise AssertionError("prompts without {nonce} must be rejected")
    except ValueError:
        pass

    # --- grading ----------------------------------------------------------
    expect = offline_expect()
    assert grade_task(expect, offline_observation())["passed"] is True, "clean observation passes"

    bad_turns = offline_observation(db={
        "turns": [{"status": "running"}],
        "sessions": [{"messages": []}],
        "execution": {"manifests": 1, "durations_ms": [150], "tool_calls": 0, "approvals": 0, "bindings": [{"provider": "offline", "model": "offline-scripted"}]},
    })
    assert not grade_task(expect, bad_turns)["passed"], "unsettled turn must fail"

    no_echo = offline_observation(dom={
        "transcript": [{"role": "user", "text": "Echo token T1-ABCD1234 and stop."}],
    })
    assert not grade_task(expect, no_echo)["passed"], "missing echo must fail"

    leak = offline_observation(db={
        "turns": [{"status": "succeeded"}],
        "sessions": [
            {"messages": [{"role": "user", "content": "first secret"}]},
            {"messages": [{"role": "user", "content": "first secret leaked"}]},
        ],
        "execution": {"manifests": 1, "durations_ms": [150], "tool_calls": 0, "approvals": 0, "bindings": [{"provider": "offline", "model": "offline-scripted"}]},
    })
    expect_isolation = {**expect, "isolation": True, "foreign_prompts": ["first secret"]}
    assert not grade_task(expect_isolation, leak)["passed"], "session leak must fail"

    console_bad = offline_observation(console={"events": [{"method": "Runtime.exceptionThrown"}]})
    assert not grade_task(expect, console_bad)["passed"], "uncaught console exception must fail"

    # --- evidence helpers -------------------------------------------------
    assert term_matches("T1-ABCD1234", "offline echo: Echo token T1-ABCD1234 and stop.")
    assert not term_matches("T9-FFFF", "offline echo: T1-ABCD1234")
    assert term_matches("over|within|fits", "the budget fits"), "alternation"
    assert not term_matches("over|within|fits", "nothing here")

    # --- preflight --------------------------------------------------------
    assert preflight_skip(None) is not None, "missing binary must self-skip"
    assert preflight_skip(TASKS and None) is not None

    print("DESKTOP_TASK_SUITE_SELFTEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
