#!/usr/bin/env python3
"""Desktop task-suite runner: easy → ultra-hard contracts on the installed app.

Synthetic tasks in four difficulty tiers, every one fully deterministic on
the offline echo provider (host answers ``offline echo: <message>`` with
zero tool calls and zero approvals):

  easy          one prompt, one turn: echo lands in the native DOM, the turn
                settles completed, execution.db records one manifest bound to
                offline/offline-scripted with a real duration.
  medium        two sequential turns in one session: both echoes land in
                order, two manifests with durations.
  hard          two sessions: a fresh session is created through the rail,
                starts empty, takes its own turn, and its transcript never
                leaks the first session's prompts.
  ultra-hard    the full loop: two sessions plus reopening the first session
                from the rail and requiring its exact transcript to restore
                while the durable stores stay consistent (turns == manifests
                == 3, timings present, no uncaught console exceptions).

Each task runs against its own fresh app instance and isolated ``--home``,
so no run can contaminate the next. Evidence lands under
``local/tmp/desktop-task-harness/<stamp>/<task-id>/``.

Exit 0 with ``DESKTOP_TASK_SUITE_OK`` when every task passed; exit 1 with
``DESKTOP_TASK_SUITE_FAIL`` and the failing contracts otherwise. Self-skips
(missing binary, missing ``websockets``, no display and no Xvfb) print
``DESKTOP_TASK_SUITE_SKIP: <reason>`` and exit 0 — the established pattern
for optional-device gates (spec-014 R13).
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import shutil
import sys
import time
from pathlib import Path

from desktop_task_harness import (
    grade_task,
    resolve_binary,
    run_task,
)

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = ROOT / "Development" / "tmp" / "desktop-task-harness"

BINDINGS_OFFLINE = [["offline", "offline-scripted"]]

# Each template receives its own unique {nonce} (embedded inside the first 48
# characters so the session auto-title carries it — spec-014 R8).
TASKS: list[dict] = [
    {
        "id": "easy-echo",
        "difficulty": "easy",
        "description": "One prompt, one turn: echo lands in the native DOM and the "
        "durable stores bind offline/offline-scripted with a real duration.",
        "sessions": [["Echo token {nonce} and stop."]],
        "expect": {
            "turns": 1,
            "sessions": 1,
            "manifests": 1,
            "echo": True,
            "bindings": BINDINGS_OFFLINE,
            "tool_calls": 0,
            "approvals": 0,
            "min_timing_events": 2,
            "max_exceptions": 0,
        },
    },
    {
        "id": "medium-conversation",
        "difficulty": "medium",
        "description": "Two sequential turns in one session: both echoes land in "
        "order and two manifests carry durations.",
        "sessions": [["Echo token {nonce} and stop.", "Echo token {nonce} and stop."]],
        "expect": {
            "turns": 2,
            "sessions": 1,
            "manifests": 2,
            "echo": True,
            "bindings": BINDINGS_OFFLINE,
            "tool_calls": 0,
            "approvals": 0,
            "min_timing_events": 4,
            "max_exceptions": 0,
        },
    },
    {
        "id": "hard-session-isolation",
        "difficulty": "hard",
        "description": "Two sessions: a fresh thread starts empty, takes its own "
        "turn, and never leaks the first session's prompts.",
        "sessions": [
            ["Echo token {nonce} and stop.", "Echo token {nonce} and stop."],
            ["Echo token {nonce} and stop."],
        ],
        "expect": {
            "turns": 3,
            "sessions": 2,
            "manifests": 3,
            "echo": True,
            "new_session_empty": True,
            "isolation": True,
            "bindings": BINDINGS_OFFLINE,
            "tool_calls": 0,
            "approvals": 0,
            "min_timing_events": 6,
            "max_exceptions": 0,
        },
    },
    {
        "id": "ultra-hard-full-loop",
        "difficulty": "ultra-hard",
        "description": "The full lifecycle: two sessions plus reopening the first "
        "session from the rail with its exact transcript restored, stores "
        "consistent (turns == manifests == 3), timings present, no uncaught "
        "console exceptions.",
        "sessions": [
            ["Echo token {nonce} and stop.", "Echo token {nonce} and stop."],
            ["Echo token {nonce} and stop."],
        ],
        "reopen_first_session": True,
        "expect": {
            "turns": 3,
            "sessions": 2,
            "manifests": 3,
            "echo": True,
            "new_session_empty": True,
            "isolation": True,
            "reopen": True,
            "bindings": BINDINGS_OFFLINE,
            "tool_calls": 0,
            "approvals": 0,
            "min_timing_events": 6,
            "max_exceptions": 0,
        },
    },
]


def validate_tasks(tasks: list[dict]) -> None:
    seen: set[str] = set()
    for task in tasks:
        task_id = task.get("id", "")
        if not task_id or task_id in seen:
            raise ValueError("task ids must be non-empty and unique")
        seen.add(task_id)
        if task.get("difficulty") not in {"easy", "medium", "hard", "ultra-hard"}:
            raise ValueError(f"{task_id}: unknown difficulty")
        if not task.get("sessions") or not all(task["sessions"]):
            raise ValueError(f"{task_id}: sessions must contain prompts")
        for template in (p for session in task["sessions"] for p in session):
            if "{nonce}" not in template:
                raise ValueError(f"{task_id}: every prompt template must carry {{nonce}}")
        expect = task.get("expect", {})
        required = {"turns", "sessions", "manifests", "echo", "bindings", "tool_calls", "approvals"}
        missing = required - set(expect)
        if missing:
            raise ValueError(f"{task_id}: expect missing {sorted(missing)}")
        if expect["bindings"] != BINDINGS_OFFLINE:
            raise ValueError(f"{task_id}: bindings must pin offline/offline-scripted")


def preflight_skip(binary: Path | None) -> str | None:
    """Return a skip reason when the environment cannot run the suite at all."""
    if binary is None:
        return (
            "no installed or repo candidate binary "
            "(run scripts/rebuild-install-relaunch.sh or cargo build -p optimus-agent)"
        )
    try:
        import websockets  # noqa: F401
    except ImportError:
        return "python websockets module missing"
    if not os.environ.get("DISPLAY") and not shutil.which("Xvfb"):
        return "no DISPLAY and Xvfb is not installed"
    return None


async def run_suite(
    tasks: list[dict],
    binary: Path,
    output_dir: Path,
    timeout: float,
    enforce_ocr: bool = True,
    ocr_model: str = "qwen3.5:9b",
    ocr_endpoint: str = "http://127.0.0.1:11434",
) -> dict:
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_dir = output_dir / stamp
    run_dir.mkdir(parents=True, exist_ok=True)
    (run_dir / "contracts.json").write_text(
        json.dumps(tasks, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    results = []
    for task in tasks:
        task_dir = run_dir / task["id"]
        task_dir.mkdir(parents=True, exist_ok=True)
        started = time.monotonic()
        try:
            evidence = await run_task(
                binary, task, task_dir, timeout=timeout,
                enforce_ocr=enforce_ocr, ocr_model=ocr_model, ocr_endpoint=ocr_endpoint,
            )
            grade = evidence["grade"]
            results.append({
                "task_id": task["id"],
                "difficulty": task["difficulty"],
                "passed": grade["passed"],
                "failures": grade["failures"],
                "duration_s": round(time.monotonic() - started, 1),
                "console_events": grade["console_event_count"],
                "console_exceptions": grade["console_exceptions"],
            })
        except Exception as error:  # noqa: BLE001 — report every task, keep going
            if os.environ.get("DESKTOP_TASK_DEBUG"):
                import traceback

                traceback.print_exc()
            results.append({
                "task_id": task["id"],
                "difficulty": task["difficulty"],
                "passed": False,
                "failures": [f"harness error: {error}"],
                "duration_s": round(time.monotonic() - started, 1),
            })
        print(
            f"  {task['id']}: {'ok' if results[-1]['passed'] else 'FAIL'} "
            f"({results[-1]['duration_s']}s)"
        )
        for failure in results[-1]["failures"]:
            print(f"    {failure}")
    report = {
        "version": 1,
        "run_dir": str(run_dir),
        "passed": all(row["passed"] for row in results),
        "tasks": results,
    }
    (run_dir / "report.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--all", action="store_true", help="run the full easy→ultra-hard suite")
    parser.add_argument("--task", choices=["easy", "medium", "hard", "ultra-hard"])
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--timeout", type=float, default=240)
    parser.add_argument("--plan", action="store_true", help="validate contracts only")
    parser.add_argument("--no-ocr", action="store_true",
                        help="disable the enforced qwen OCR gate (not for acceptance runs)")
    parser.add_argument("--ocr-model", default="qwen3.5:9b")
    parser.add_argument("--ocr-endpoint", default="http://127.0.0.1:11434")
    args = parser.parse_args()

    validate_tasks(TASKS)
    selected = TASKS if args.all else [t for t in TASKS if t["difficulty"] == args.task]
    if not selected:
        print("use --all or --task easy|medium|hard|ultra-hard", file=sys.stderr)
        return 2

    binary = resolve_binary(args.binary)
    if args.plan:
        print(json.dumps([{"id": t["id"], "difficulty": t["difficulty"]} for t in selected], indent=2))
        return 0

    reason = preflight_skip(binary)
    if reason:
        print(f"DESKTOP_TASK_SUITE_SKIP: {reason}")
        return 0
    assert binary is not None  # preflight_skip guarantees a runnable candidate

    report = asyncio.run(run_suite(
        selected, binary, args.output, args.timeout,
        enforce_ocr=not args.no_ocr, ocr_model=args.ocr_model, ocr_endpoint=args.ocr_endpoint,
    ))
    if report["passed"]:
        print(
            f"DESKTOP_TASK_SUITE_OK tasks={len(report['tasks'])} "
            f"evidence={report['run_dir']}"
        )
        return 0
    print(f"DESKTOP_TASK_SUITE_FAIL evidence={report['run_dir']}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
