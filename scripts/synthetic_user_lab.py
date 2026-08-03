#!/usr/bin/env python3
"""Drive seeded, blind artificial humans through the real Optimus TUI.

Every selected persona gets an isolated Optimus home. Private persona state is
used only by the deterministic simulator; Optimus receives ordinary user text
with no test marker or persona label. After the native process exits, this tool
opens its SQLite stores read-only and hands a sanitized observation plus public
rubric to the independent grader in synthetic_user_lab_eval.py.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import shutil
import sqlite3
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from synthetic_user_lab_eval import evaluate

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_COHORT = ROOT / "evals" / "synthetic-user-lab" / "cohort-v1.json"
TMUX = shutil.which("tmux") or str(ROOT / "local" / "tools" / "tmux-root" / "usr" / "bin" / "tmux")


def load_cohort(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("version") != 1 or not payload.get("scenarios"):
        raise ValueError("cohort must be version 1 with at least one scenario")
    ids: set[str] = set()
    for scenario in payload["scenarios"]:
        scenario_id = scenario.get("id", "")
        if not scenario_id or scenario_id in ids:
            raise ValueError("scenario ids must be non-empty and unique")
        ids.add(scenario_id)
        if scenario.get("kind") not in {"fresh", "returning"}:
            raise ValueError(f"{scenario_id}: kind must be fresh or returning")
        if scenario.get("approval_policy") not in {"deny", "approve_confined"}:
            raise ValueError(f"{scenario_id}: unsafe approval policy")
        sessions = scenario.get("sessions", [])
        if not sessions or any(not turns for turns in sessions):
            raise ValueError(f"{scenario_id}: sessions must contain turns")
        if scenario["kind"] == "fresh" and len(sessions) != 1:
            raise ValueError(f"{scenario_id}: fresh personas must have one session")
        for prompt in (prompt for session in sessions for prompt in session):
            if not isinstance(prompt, str) or not prompt.strip():
                raise ValueError(f"{scenario_id}: prompts must be non-empty strings")
            lowered = prompt.casefold()
            if "[sim" in lowered or "synthetic user" in lowered or scenario_id.casefold() in lowered:
                raise ValueError(f"{scenario_id}: prompt leaks the test harness")
        rubric = scenario.get("rubric", {})
        required = {"max_approvals", "max_tool_calls", "required_terms", "forbidden_terms"}
        if set(rubric) != required:
            raise ValueError(f"{scenario_id}: rubric fields must be exact")
    return payload


def choose_scenarios(cohort: dict[str, Any], seed: int, count: int) -> list[dict[str, Any]]:
    scenarios = list(cohort["scenarios"])
    if count < 1 or count > len(scenarios):
        raise ValueError(f"count must be between 1 and {len(scenarios)}")
    random.Random(seed).shuffle(scenarios)
    return scenarios[:count]


def public_scenario(scenario: dict[str, Any]) -> dict[str, Any]:
    return {"id": scenario["id"], "kind": scenario["kind"], "rubric": scenario["rubric"]}


def stored_run_path(path: Path) -> str:
    """Keep report paths portable for both repo-local and external evidence."""
    resolved = path.resolve()
    try:
        return str(resolved.relative_to(ROOT))
    except ValueError:
        return str(resolved)


def compile_manifest(cohort: dict[str, Any], seed: int, count: int) -> dict[str, Any]:
    selected = choose_scenarios(cohort, seed, count)
    return {
        "version": 1,
        "cohort_id": cohort["cohort_id"],
        "seed": seed,
        "scenario_ids": [scenario["id"] for scenario in selected],
        "private_profile_sha256": {
            scenario["id"]: hashlib.sha256(
                json.dumps(scenario["private_profile"], sort_keys=True).encode()
            ).hexdigest()
            for scenario in selected
        },
    }


def tmux(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([TMUX, *args], capture_output=True, text=True, check=False)


def capture(session: str) -> str:
    return tmux("capture-pane", "-t", session, "-p", "-S", "-400").stdout


def wait_for_ready(session: str, timeout: float = 20) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if "ready" in capture(session):
            return
        time.sleep(0.25)
    raise RuntimeError(f"TUI did not reach ready\n{capture(session)[-3000:]}")


def ro_connect(path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def turn_rows(home: Path) -> list[dict[str, Any]]:
    path = home / "sessions.db"
    if not path.exists():
        return []
    with ro_connect(path) as conn:
        rows = conn.execute(
            "SELECT id,session_id,status,error_code,created_at,updated_at "
            "FROM session_turns ORDER BY created_at,id"
        ).fetchall()
    return [
        {"id": row[0], "session_id": row[1], "status": row[2], "error_code": row[3], "created_at": row[4], "updated_at": row[5]}
        for row in rows
    ]


def wait_for_turn(home: Path, expected: int, session: str, approval_policy: str, timeout: float) -> None:
    deadline = time.time() + timeout
    decision_sent_for: set[tuple[str, str]] = set()
    while time.time() < deadline:
        rows = turn_rows(home)
        if len(rows) >= expected and rows[expected - 1]["status"] != "running":
            wait_for_ready(session)
            return
        frame = capture(session)
        running_id = next((row["id"] for row in reversed(rows) if row["status"] == "running"), None)
        approval_key = (
            running_id,
            hashlib.sha256(frame.encode()).hexdigest(),
        ) if "Approval required" in frame and running_id else None
        if approval_key and approval_key not in decision_sent_for:
            if approval_policy == "deny":
                tmux("send-keys", "-t", session, "Down", "Enter")
            else:
                tmux("send-keys", "-t", session, "Enter")
            decision_sent_for.add(approval_key)
        time.sleep(0.35)
    raise RuntimeError(f"turn {expected} did not settle in {timeout:.0f}s\n{capture(session)[-3000:]}")


def send_text(session: str, text: str) -> None:
    tmux("set-buffer", "-b", f"lab-{os.getpid()}", text)
    pasted = tmux("paste-buffer", "-b", f"lab-{os.getpid()}", "-t", session, "-p", "-d")
    if pasted.returncode != 0:
        raise RuntimeError(f"tmux paste failed: {pasted.stderr}")
    tmux("send-keys", "-t", session, "Enter")


def extract_observation(home: Path, expected_turns: int) -> dict[str, Any]:
    sessions: list[dict[str, Any]] = []
    with ro_connect(home / "sessions.db") as conn:
        for session_id, title, messages_json in conn.execute(
            "SELECT id,title,messages_json FROM sessions ORDER BY created_at,id"
        ):
            messages = json.loads(messages_json)
            sessions.append({
                "id": session_id,
                "title": title,
                "messages": [
                    {"role": row.get("role", ""), "content": row.get("content", "")}
                    for row in messages
                    if row.get("role") != "system"
                ],
            })
    approvals = tool_calls = duration_ms = 0
    candidate_bindings: list[dict[str, str]] = []
    execution = home / "execution.db"
    if execution.exists():
        with ro_connect(execution) as conn:
            approvals = conn.execute("SELECT count(*) FROM execution_chat_approvals").fetchone()[0]
            tool_calls = conn.execute("SELECT count(*) FROM execution_tool_calls").fetchone()[0]
            duration_ms = conn.execute("SELECT COALESCE(sum(duration_ms),0) FROM execution_manifests").fetchone()[0]
            rows = conn.execute(
                "SELECT DISTINCT provider,model,autonomy_profile,command_fs_envelope "
                "FROM execution_manifests ORDER BY provider,model,autonomy_profile,command_fs_envelope"
            ).fetchall()
            candidate_bindings = [
                {"provider": row[0], "model": row[1], "autonomy_profile": row[2], "command_fs_envelope": row[3]}
                for row in rows
            ]
    return {
        "expected_turns": expected_turns,
        "sessions": sessions,
        "turns": turn_rows(home),
        "approvals": approvals,
        "tool_calls": tool_calls,
        "duration_ms": duration_ms,
        "candidate_bindings": candidate_bindings,
    }


def run_scenario(
    scenario: dict[str, Any], binary: Path, provider: str, model: str | None,
    thinking: str, run_dir: Path, auth_source: Path | None, timeout: float,
) -> tuple[dict[str, Any], dict[str, Any]]:
    home = run_dir / scenario["id"] / "optimus-home"
    frames = run_dir / scenario["id"] / "frames"
    home.mkdir(parents=True)
    frames.mkdir(parents=True)
    copied_auth = home / "auth.json"
    if provider != "offline":
        if auth_source is None or not auth_source.is_file():
            raise RuntimeError("a live provider requires --auth-source pointing to auth.json")
        shutil.copyfile(auth_source, copied_auth)
        copied_auth.chmod(0o600)
    session = f"optimus-lab-{os.getpid()}-{hashlib.sha256(scenario['id'].encode()).hexdigest()[:8]}"
    tmux("kill-session", "-t", session)
    expected = 0
    try:
        launched = tmux(
            "new-session", "-d", "-s", session, "-x", "120", "-y", "38",
            f"'{binary}' --home '{home}'",
        )
        if launched.returncode != 0:
            raise RuntimeError(f"tmux could not launch TUI: {launched.stderr}")
        wait_for_ready(session)
        if provider != "offline":
            send_text(session, f"/provider {provider}")
            wait_for_ready(session)
        if model:
            send_text(session, f"/model {model}")
            wait_for_ready(session)
        send_text(session, f"/thinking {thinking}")
        wait_for_ready(session)
        for session_index, prompts in enumerate(scenario["sessions"]):
            if session_index:
                send_text(session, "/new")
                wait_for_ready(session)
            for prompt in prompts:
                expected += 1
                send_text(session, prompt)
                wait_for_turn(home, expected, session, scenario["approval_policy"], timeout)
                (frames / f"turn-{expected:02d}.txt").write_text(capture(session), encoding="utf-8")
    finally:
        tmux("send-keys", "-t", session, "C-c")
        time.sleep(0.3)
        tmux("kill-session", "-t", session)
        if copied_auth.exists():
            copied_auth.unlink()
    observation = extract_observation(home, expected)
    grade = evaluate(public_scenario(scenario), observation)
    return observation, grade


def regrade(run_dir: Path, cohort: dict[str, Any]) -> dict[str, Any]:
    """Recompute scores from saved observations without launching a model."""
    report_path = run_dir / "report.json"
    report = json.loads(report_path.read_text(encoding="utf-8"))
    scenarios = {row["id"]: row for row in cohort["scenarios"]}
    for result in report["results"]:
        scenario_id = result["scenario_id"]
        result["grade"] = evaluate(public_scenario(scenarios[scenario_id]), result["observation"])
        (run_dir / scenario_id / "result.json").write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    report["passed"] = all(row["grade"]["passed"] for row in report["results"])
    report["mean_score"] = round(
        sum(row["grade"]["score"] for row in report["results"]) / len(report["results"]), 2
    )
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cohort", type=Path, default=DEFAULT_COHORT)
    parser.add_argument("--seed", type=int, default=7312026)
    parser.add_argument("--count", type=int, default=3)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "debug" / "optimus")
    parser.add_argument("--provider", default="offline")
    parser.add_argument("--model")
    parser.add_argument("--thinking", default="low")
    parser.add_argument("--auth-source", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "local" / "tmp" / "synthetic-user-lab")
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--plan", action="store_true", help="validate and print the seeded blind manifest only")
    parser.add_argument("--regrade", type=Path, help="regrade a saved run without launching Optimus")
    args = parser.parse_args()

    cohort = load_cohort(args.cohort)
    if args.regrade:
        report = regrade(args.regrade.resolve(), cohort)
        for result in report["results"]:
            grade = result["grade"]
            print(f"{result['scenario_id']}: score={grade['score']} passed={grade['passed']}")
        print(f"SYNTHETIC_USER_LAB_REGRADED mean_score={report['mean_score']}")
        return 0 if report["passed"] else 1
    manifest = compile_manifest(cohort, args.seed, args.count)
    if args.plan:
        print(json.dumps(manifest, indent=2, sort_keys=True))
        return 0
    if not Path(TMUX).is_file():
        raise SystemExit("tmux is required to drive the native TUI")
    if not args.binary.is_file():
        raise SystemExit(f"candidate binary does not exist: {args.binary}")

    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.output / f"{stamp}-seed-{args.seed}"
    run_dir.mkdir(parents=True, exist_ok=False)
    manifest.update({
        "created_at": datetime.now(timezone.utc).isoformat(),
        "candidate_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        "provider": args.provider,
        "model": args.model or "provider-default",
        "thinking": args.thinking,
    })
    (run_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    selected = {row["id"]: row for row in cohort["scenarios"]}
    results = []
    for scenario_id in manifest["scenario_ids"]:
        observation, grade = run_scenario(
            selected[scenario_id], args.binary, args.provider, args.model, args.thinking,
            run_dir, args.auth_source, args.timeout,
        )
        result = {"scenario_id": scenario_id, "observation": observation, "grade": grade}
        results.append(result)
        (run_dir / scenario_id / "result.json").write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print(f"{scenario_id}: score={grade['score']} passed={grade['passed']}")
    report = {
        "version": 1,
        "run_dir": stored_run_path(run_dir),
        "passed": all(row["grade"]["passed"] for row in results),
        "mean_score": round(sum(row["grade"]["score"] for row in results) / len(results), 2),
        "results": results,
    }
    manifest["resolved_candidate_bindings"] = sorted(
        {
            json.dumps(binding, sort_keys=True)
            for result in results
            for binding in result["observation"]["candidate_bindings"]
        }
    )
    manifest["resolved_candidate_bindings"] = [
        json.loads(binding) for binding in manifest["resolved_candidate_bindings"]
    ]
    (run_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (run_dir / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"SYNTHETIC_USER_LAB_OK={report['passed']} mean_score={report['mean_score']} evidence={run_dir}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
