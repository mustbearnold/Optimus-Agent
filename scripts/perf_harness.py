#!/usr/bin/env python3
"""Optimus performance baseline harness (B-STRAT-02).

Measures the eight `performance_rules` scenarios from
docs/architecture/optimus-version.json against the current tree's `optimus`
binary, using only the repository's deterministic offline provider paths
(ScriptedModel turns, seeded vertical workflows, offline cron). Results are
informational until ADR-0069 is accepted: `compare` exits non-zero only with
--enforce on the same machine fingerprint.

  run       [--scenario X] [--samples N] [--seeds N] [--full]   measure now
  baseline  [same flags]           write docs/architecture/perf-baseline.json
  compare   [--input run.json] [--enforce]   ratio report vs committed baseline
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "docs" / "architecture" / "optimus-version.json"
BASELINE = ROOT / "docs" / "architecture" / "perf-baseline.json"
SCHEMA = "optimus-perf/1"
BASE_SEED = 7312026
LONG_SESSION_TURNS = 8
# Offline ScriptedModel completes before optimus-cli prints; no stream exists.
TTFT_REASON = (
    "no streaming seam: the offline ScriptedModel path returns complete "
    "responses and optimus-cli prints only after the turn resolves"
)
COST_REASON = (
    "offline scripted provider is credential-free and unbilled; "
    "cost-per-success requires a live-provider run"
)
GATING_NOTE = (
    "informational: ADR-0069 is Proposed; these numbers gate nothing "
    "until it is accepted"
)


class HarnessError(RuntimeError):
    pass


def load_rules() -> dict:
    return json.loads(MANIFEST.read_text(encoding="utf-8"))["performance_rules"]


def required_scenario_ids() -> list[str]:
    return [row["id"] for row in load_rules()["required_scenarios"]]


# --- machine identity ---------------------------------------------------------


def machine_parts() -> dict:
    cpu_model = "unknown"
    for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
        if line.lower().startswith("model name"):
            cpu_model = line.split(":", 1)[1].strip()
            break
    mem_total_kb = 0
    for line in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines():
        if line.startswith("MemTotal:"):
            mem_total_kb = int(line.split()[1])
            break
    return {
        "hostname": socket.gethostname(),
        "cpu_model": cpu_model,
        "cores": os.cpu_count() or 0,
        "mem_total_kb": mem_total_kb,
    }


def fingerprint_from_parts(parts: dict) -> str:
    text = "|".join(
        str(parts[k]) for k in ("hostname", "cpu_model", "cores", "mem_total_kb")
    )
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


def machine_fingerprint() -> str:
    return fingerprint_from_parts(machine_parts())


def optimus_revision() -> str:
    # Read-only provenance; a checkout without git history still measures.
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True
        )
    except OSError:
        return "unknown"
    return result.stdout.strip() if result.returncode == 0 else "unknown"


# --- process measurement ------------------------------------------------------


def measure_process(argv: list[str], cwd: Path) -> dict:
    """Wall clock plus kernel-accounted peak RSS via wait4 ru_maxrss (KiB)."""
    out_path = cwd / "stdout.txt"
    err_path = cwd / "stderr.txt"
    start = time.perf_counter()
    with open(out_path, "wb") as out, open(err_path, "wb") as err:
        proc = subprocess.Popen(argv, stdout=out, stderr=err, cwd=cwd)
        _, status, rusage = os.wait4(proc.pid, 0)
        wall_ms = (time.perf_counter() - start) * 1000.0
        # The child is already reaped; Popen must not wait for it again.
        proc.returncode = os.waitstatus_to_exitcode(status)
    return {
        "wall_ms": round(wall_ms, 3),
        "peak_rss_kb": int(rusage.ru_maxrss),
        "returncode": proc.returncode,
        "stdout": out_path.read_text(encoding="utf-8", errors="replace"),
        "stderr": err_path.read_text(encoding="utf-8", errors="replace"),
    }


def checked(measured: dict, marker: str, scenario: str) -> dict:
    if measured["returncode"] != 0 or marker not in measured["stdout"]:
        raise HarnessError(
            f"{scenario}: child failed rc={measured['returncode']} "
            f"marker={marker!r} stderr={measured['stderr'][-400:]!r}"
        )
    return measured


def run_untimed(argv: list[str], cwd: Path, scenario: str) -> str:
    result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True)
    if result.returncode != 0:
        raise HarnessError(
            f"{scenario}: setup failed rc={result.returncode} "
            f"stderr={result.stderr[-400:]!r}"
        )
    return result.stdout


# --- scenarios ----------------------------------------------------------------
# Every measured scenario drives the binary's real offline paths; nothing here
# fabricates a metric. Scenarios without a deterministic offline path skip.


def sample_row(wall_ms: float, peak_rss_kb: int | None, seed: int) -> dict:
    return {
        "wall_ms": wall_ms,
        "ttft_ms": None,
        "peak_rss_kb": peak_rss_kb,
        "seed": seed,
    }


def scenario_cold_start(binary: str, home: Path, seed: int) -> dict:
    m = checked(measure_process([binary, "--home", str(home), "version"], home), "Optimus", "cold-start")
    return sample_row(m["wall_ms"], m["peak_rss_kb"], seed)


def scenario_single_turn(binary: str, home: Path, seed: int) -> dict:
    msg = f"perf single-turn seed={seed}"
    m = checked(
        measure_process([binary, "--home", str(home), "chat-offline", msg], home),
        "offline echo",
        "single-turn",
    )
    return sample_row(m["wall_ms"], m["peak_rss_kb"], seed)


def scenario_multi_tool_turn(binary: str, home: Path, seed: int) -> dict:
    # write_file + read_file dispatch through the seeded write_then_read DAG.
    argv = [
        binary, "--home", str(home), "vertical", "write-then-read",
        "--path", f"notes/perf-{seed}.txt",
        "--contents", f"deterministic seed={seed}",
        "--auto-grant",
    ]
    m = checked(measure_process(argv, home), "status=Succeeded", "multi-tool-turn")
    return sample_row(m["wall_ms"], m["peak_rss_kb"], seed)


SESSION_RE = re.compile(r"session ([0-9a-f-]{36})")


def create_session(binary: str, home: Path, seed: int, scenario: str) -> str:
    out = run_untimed(
        [binary, "--home", str(home), "chat-offline", f"warm seed={seed}"],
        home,
        scenario,
    )
    match = SESSION_RE.search(out)
    if not match:
        raise HarnessError(f"{scenario}: no session id in {out!r}")
    return match.group(1)


def scenario_session_resume(binary: str, home: Path, seed: int) -> dict:
    session = create_session(binary, home, seed, "session-resume")
    argv = [
        binary, "--home", str(home), "chat-offline",
        "--session", session, f"resume seed={seed}",
    ]
    m = checked(measure_process(argv, home), "offline echo", "session-resume")
    return sample_row(m["wall_ms"], m["peak_rss_kb"], seed)


def scenario_long_session(binary: str, home: Path, seed: int) -> dict:
    session = create_session(binary, home, seed, "long-session")
    wall = 0.0
    peak = 0
    for turn in range(LONG_SESSION_TURNS):
        argv = [
            binary, "--home", str(home), "chat-offline",
            "--session", session, f"long turn={turn} seed={seed}",
        ]
        m = checked(measure_process(argv, home), "offline echo", "long-session")
        wall += m["wall_ms"]
        peak = max(peak, m["peak_rss_kb"])
    return sample_row(round(wall, 3), peak, seed)


NEXT_RUN_RE = re.compile(r"next_run_unix=(\d+)")


def scenario_scheduled_job(binary: str, homes: list[tuple[Path, int]]) -> list[dict]:
    """Batch-shaped: cron jobs must age past next_run_unix before tick claims them."""
    due = 0
    for home, seed in homes:
        out = run_untimed(
            [
                binary, "--home", str(home), "cron", "add", f"perf-{seed}",
                "--every", "5", f"tick seed={seed}", "--provider", "offline",
            ],
            home,
            "scheduled-job",
        )
        match = NEXT_RUN_RE.search(out)
        if not match:
            raise HarnessError(f"scheduled-job: no next_run_unix in {out!r}")
        due = max(due, int(match.group(1)))
    while time.time() < due + 1:
        time.sleep(0.2)
    rows = []
    for home, seed in homes:
        m = checked(
            measure_process([binary, "--home", str(home), "cron", "tick"], home),
            '"status":"ok',
            "scheduled-job",
        )
        rows.append(sample_row(m["wall_ms"], m["peak_rss_kb"], seed))
    return rows


def scenario_delegated_task(binary: str, home: Path, seed: int) -> dict:
    # Registered workspace_writer handoff: run_write_file_handoff workflow.
    argv = [
        binary, "--home", str(home), "vertical", "write-file",
        "--path", f"notes/delegated-{seed}.txt",
        "--contents", f"handoff seed={seed}",
        "--auto-grant",
    ]
    m = checked(measure_process(argv, home), "terminal=Succeeded", "delegated-task")
    return sample_row(m["wall_ms"], m["peak_rss_kb"], seed)


PER_SAMPLE_SCENARIOS = {
    "cold-start": scenario_cold_start,
    "single-turn": scenario_single_turn,
    "multi-tool-turn": scenario_multi_tool_turn,
    "session-resume": scenario_session_resume,
    "long-session": scenario_long_session,
    "delegated-task": scenario_delegated_task,
}

MEASURES = {
    "cold-start": "process start to `version` output on a fresh home",
    "single-turn": "one offline scripted chat turn end-to-end (chat-offline)",
    "multi-tool-turn": "write_file + read_file dispatches via vertical write-then-read",
    "session-resume": "reopen durable store and run one resumed turn (setup turn untimed)",
    "long-session": f"{LONG_SESSION_TURNS} resumed turns in one session (creation untimed; wall summed, RSS max)",
    "scheduled-job": "cron tick claiming one due offline job (add + due-wait untimed)",
    "browser-task": "not measured",
    "delegated-task": "registered workspace_writer handoff (vertical write-file)",
}

SKIP_REASONS = {
    "browser-task": (
        "requires live: the browser effector is SSRF-safe against private "
        "targets and only reaches the public web; no deterministic offline "
        "target exists in-repo, and a fabricated local number would not "
        "measure this scenario"
    ),
}


def seed_list(seeds: int) -> list[int]:
    return [BASE_SEED + i for i in range(max(1, seeds))]


def run_one_scenario(scenario_id: str, binary: str | None, samples: int, seeds: int) -> dict:
    entry = {
        "id": scenario_id,
        "requires": "live" if scenario_id in SKIP_REASONS else "offline",
        "measures": MEASURES[scenario_id],
        "cost_usd": None,
        "cost_reason": COST_REASON,
        "ttft_reason": TTFT_REASON,
        "rss_reason": None,
    }
    if scenario_id in SKIP_REASONS:
        entry.update(status="skipped", skip_reason=SKIP_REASONS[scenario_id], samples=[])
        return entry
    if binary is None:
        raise HarnessError(f"{scenario_id}: no optimus binary available")
    seeds_cycle = seed_list(seeds)
    tmp_root = Path(tempfile.mkdtemp(prefix=f"optimus-perf-{scenario_id}-"))
    try:
        plan = [
            (tmp_root / f"sample-{i}", seeds_cycle[i % len(seeds_cycle)])
            for i in range(max(1, samples))
        ]
        for home, _ in plan:
            home.mkdir(parents=True)
        if scenario_id == "scheduled-job":
            rows = scenario_scheduled_job(binary, plan)
        else:
            fn = PER_SAMPLE_SCENARIOS[scenario_id]
            rows = [fn(binary, home, seed) for home, seed in plan]
        entry.update(status="measured", samples=rows, aggregates=aggregate(rows))
        return entry
    except HarnessError as error:
        entry.update(status="error", samples=[], error=str(error))
        return entry
    finally:
        shutil.rmtree(tmp_root, ignore_errors=True)


# --- aggregation and result assembly ------------------------------------------


def percentile(values: list[float], q: float) -> float:
    ordered = sorted(values)
    if not ordered:
        raise ValueError("percentile of empty list")
    if len(ordered) == 1:
        return float(ordered[0])
    rank = (len(ordered) - 1) * q
    low = int(rank)
    high = min(low + 1, len(ordered) - 1)
    return round(ordered[low] + (ordered[high] - ordered[low]) * (rank - low), 3)


def aggregate(rows: list[dict]) -> dict:
    walls = [r["wall_ms"] for r in rows]
    rss = [r["peak_rss_kb"] for r in rows if r["peak_rss_kb"] is not None]
    out = {"wall_ms": {"p50": percentile(walls, 0.5), "p95": percentile(walls, 0.95)}}
    if rss:
        out["peak_rss_kb"] = {"p50": percentile(rss, 0.5), "p95": percentile(rss, 0.95)}
    return out


def resolve_binary(explicit: str | None) -> tuple[str | None, dict]:
    candidates = (
        [(explicit, "external")]
        if explicit
        else [
            (str(ROOT / "target" / "release" / "optimus"), "release"),
            (str(ROOT / "target" / "debug" / "optimus"), "debug"),
        ]
    )
    for path, profile in candidates:
        p = Path(path)
        if p.is_file() and os.access(p, os.X_OK):
            info = {
                "path": str(p),
                "profile": profile,
                "modified_at": datetime.fromtimestamp(
                    p.stat().st_mtime, tz=timezone.utc
                ).isoformat(timespec="seconds"),
                "note": (
                    "harness reuses the existing artifact and never rebuilds; "
                    "confirm it matches optimus_revision before trusting deltas"
                ),
            }
            if profile == "debug":
                info["note"] += "; debug profile — build --release for representative numbers"
            return str(p), info
    return None, {
        "path": None,
        "profile": None,
        "note": "no built optimus binary; run `cargo build --release -p optimus-cli`",
    }


def build_result(scenarios: list[dict], samples: int, seeds: int, binary_info: dict) -> dict:
    rules = load_rules()
    compliant = samples >= rules["minimum_paired_samples_per_scenario"] and seeds >= rules[
        "minimum_distinct_seeds_per_scenario"
    ]
    parts = machine_parts()
    return {
        "schema": SCHEMA,
        "started_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "optimus_revision": optimus_revision(),
        "machine_fingerprint": fingerprint_from_parts(parts),
        "machine": parts,
        "binary": binary_info,
        "protocol": {
            "samples_per_scenario": samples,
            "distinct_seeds": seeds,
            "protocol_compliant": compliant,
            "note": GATING_NOTE
            if compliant
            else f"quick mode ({samples} samples/{seeds} seeds) is below the "
            f"protocol minimum ({rules['minimum_paired_samples_per_scenario']}/"
            f"{rules['minimum_distinct_seeds_per_scenario']}); {GATING_NOTE}",
        },
        "scenarios": scenarios,
    }


def validate_result(result: dict) -> list[str]:
    problems = []
    for key in ("schema", "started_at", "optimus_revision", "machine_fingerprint", "machine", "binary", "protocol", "scenarios"):
        if key not in result:
            problems.append(f"missing top-level key: {key}")
    if result.get("schema") != SCHEMA:
        problems.append(f"schema must be {SCHEMA}")
    for scenario in result.get("scenarios", []):
        sid = scenario.get("id", "?")
        status = scenario.get("status")
        if status not in {"measured", "skipped", "error"}:
            problems.append(f"{sid}: invalid status {status!r}")
        if status == "measured":
            if not scenario.get("samples"):
                problems.append(f"{sid}: measured but no samples")
            if "aggregates" not in scenario:
                problems.append(f"{sid}: measured but no aggregates")
            for row in scenario.get("samples", []):
                if not isinstance(row.get("wall_ms"), (int, float)) or row["wall_ms"] <= 0:
                    problems.append(f"{sid}: sample without positive wall_ms")
                if "seed" not in row:
                    problems.append(f"{sid}: sample without seed")
                if row.get("ttft_ms") is None and not scenario.get("ttft_reason"):
                    problems.append(f"{sid}: null ttft_ms without ttft_reason")
                if row.get("peak_rss_kb") is None and not scenario.get("rss_reason"):
                    problems.append(f"{sid}: null peak_rss_kb without rss_reason")
        if status == "skipped":
            if scenario.get("samples"):
                problems.append(f"{sid}: skipped scenarios must carry no samples")
            if not scenario.get("skip_reason"):
                problems.append(f"{sid}: skipped without skip_reason")
    return problems


# --- compare ------------------------------------------------------------------

RATIO_THRESHOLD_KEYS = {
    "wall_ms": "maximum_latency_ratio",
    "peak_rss_kb": "maximum_memory_ratio",
    "ttft_ms": "maximum_ttft_ratio",
}


def compare_results(current: dict, baseline: dict, rules: dict, enforce: bool) -> tuple[list[str], int]:
    lines = []
    same_machine = current.get("machine_fingerprint") == baseline.get("machine_fingerprint")
    enforced = enforce and same_machine
    if enforce and not same_machine:
        lines.append(
            "cross-machine comparison (fingerprints differ): informational only; "
            "the protocol requires the same machine to enforce"
        )
    baseline_by_id = {s["id"]: s for s in baseline.get("scenarios", [])}
    regressions = []
    for scenario in current.get("scenarios", []):
        sid = scenario["id"]
        base = baseline_by_id.get(sid)
        if base is None:
            lines.append(f"{sid}: not in baseline — informational")
            continue
        if scenario.get("status") != "measured" or base.get("status") != "measured":
            lines.append(
                f"{sid}: not comparable (current={scenario.get('status')}, "
                f"baseline={base.get('status')})"
            )
            continue
        for metric, threshold_key in RATIO_THRESHOLD_KEYS.items():
            cur_agg = scenario.get("aggregates", {}).get(metric)
            base_agg = base.get("aggregates", {}).get(metric)
            if not cur_agg or not base_agg or not base_agg.get("p50"):
                continue
            ratio = round(cur_agg["p50"] / base_agg["p50"], 4)
            threshold = rules[threshold_key]
            regressed = ratio > threshold
            verdict = "REGRESSION" if regressed else "ok"
            lines.append(
                f"{sid} {metric}: p50 {base_agg['p50']} -> {cur_agg['p50']} "
                f"ratio={ratio} threshold={threshold} {verdict}"
            )
            if regressed:
                regressions.append(f"{sid}.{metric}")
    if regressions:
        lines.append(f"regressions: {', '.join(regressions)}")
    lines.append(
        "enforced regression gate: "
        + ("FAILED" if (enforced and regressions) else "not triggered")
    )
    lines.append(GATING_NOTE)
    return lines, 1 if (enforced and regressions) else 0


# --- CLI ----------------------------------------------------------------------


def perform_run(args) -> dict:
    rules = load_rules()
    samples = rules["minimum_paired_samples_per_scenario"] if args.full else args.samples
    seeds = rules["minimum_distinct_seeds_per_scenario"] if args.full else args.seeds
    binary, binary_info = resolve_binary(args.binary)
    wanted = required_scenario_ids() if args.scenario == "all" else [args.scenario]
    unknown = set(wanted) - set(required_scenario_ids())
    if unknown:
        raise HarnessError(f"unknown scenario(s): {sorted(unknown)}")
    scenarios = []
    for sid in wanted:
        print(f"[perf] {sid} ...", file=sys.stderr)
        scenarios.append(run_one_scenario(sid, binary, samples, seeds))
    return build_result(scenarios, samples, seeds, binary_info)


def print_summary(result: dict) -> None:
    print(f"optimus perf run @ {result['started_at']}  rev={result['optimus_revision'][:12]}")
    print(f"machine={result['machine_fingerprint']}  binary={result['binary'].get('path')} ({result['binary'].get('profile')})")
    print(result["protocol"]["note"])
    for s in result["scenarios"]:
        if s["status"] == "measured":
            agg = s["aggregates"]
            rss = agg.get("peak_rss_kb", {})
            print(
                f"  {s['id']:<16} n={len(s['samples'])}  wall_ms p50={agg['wall_ms']['p50']} "
                f"p95={agg['wall_ms']['p95']}  peak_rss_kb p50={rss.get('p50')}  ttft=null"
            )
        elif s["status"] == "skipped":
            print(f"  {s['id']:<16} SKIPPED ({s['requires']}): {s['skip_reason'][:80]}...")
        else:
            print(f"  {s['id']:<16} ERROR: {s.get('error', '')[:120]}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    for name in ("run", "baseline", "compare"):
        p = sub.add_parser(name)
        p.add_argument("--scenario", default="all")
        p.add_argument("--samples", type=int, default=5)
        p.add_argument("--seeds", type=int, default=2)
        p.add_argument("--full", action="store_true")
        p.add_argument("--binary")
        if name == "run":
            p.add_argument("--output", type=Path)
            p.add_argument("--json", action="store_true")
        if name == "baseline":
            p.add_argument("--out", type=Path, default=BASELINE)
        if name == "compare":
            p.add_argument("--input", type=Path)
            p.add_argument("--baseline-path", type=Path, default=BASELINE)
            p.add_argument("--enforce", action="store_true")
    args = parser.parse_args()

    try:
        if args.mode == "compare":
            if args.input:
                current = json.loads(args.input.read_text(encoding="utf-8"))
            else:
                current = perform_run(args)
            if not args.baseline_path.is_file():
                print(f"no committed baseline at {args.baseline_path}; run `baseline` first")
                return 0 if not args.enforce else 2
            baseline = json.loads(args.baseline_path.read_text(encoding="utf-8"))
            lines, code = compare_results(current, baseline, load_rules(), args.enforce)
            print("\n".join(lines))
            return code

        result = perform_run(args)
        problems = validate_result(result)
        if problems:
            print("INVALID RESULT:\n" + "\n".join(problems), file=sys.stderr)
            return 2
        if any(s["status"] == "error" for s in result["scenarios"]):
            print_summary(result)
            return 2
        if args.mode == "baseline":
            args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            print(f"baseline overwritten: {args.out}")
            print_summary(result)
            return 0
        if args.json:
            print(json.dumps(result, indent=2, sort_keys=True))
        else:
            print_summary(result)
        if args.output:
            args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            print(f"wrote {args.output}")
        return 0
    except HarnessError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
