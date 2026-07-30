#!/usr/bin/env python3
"""Fail-closed Optimus/Hermes version and parity gate.

Optimus has its own product SemVer.  A Hermes parity version is a separately
verified claim backed by a frozen Hermes feature inventory, per-feature test
evidence, the high-level parity ledger, and paired performance measurements.

The release check intentionally allows ordinary Optimus development releases,
but refuses a product version that numerically equals the tracked Hermes
version unless every parity gate is satisfied.
"""

from __future__ import annotations

import argparse
import ast
import datetime as dt
import hashlib
import json
import math
import os
import re
import statistics
import subprocess
import sys
import tempfile
import tomllib
from collections import Counter, deque
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_REL = Path("docs/architecture/optimus-version.json")
MANUAL_REL = Path("docs/architecture/hermes-manual-capabilities.json")
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
FEATURE_ID_RE = re.compile(r"^[a-z0-9]+(?:[.:-][a-z0-9]+)*$")
UTC = dt.timezone.utc
GIT_LOCAL_ENV_VARS = (
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
)


class ParityError(RuntimeError):
    """Raised for capture or promotion failures."""


@dataclass
class Evaluation:
    product_version: str = "unknown"
    target_version: str = "unknown"
    claim_status: str = "unknown"
    feature_total: int = 0
    feature_verified: int = 0
    ledger_counts: Counter[str] = field(default_factory=Counter)
    performance_total: int = 0
    performance_passed: int = 0
    errors: list[str] = field(default_factory=list)
    blockers: list[str] = field(default_factory=list)

    @property
    def ready(self) -> bool:
        return not self.errors and not self.blockers


def read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise ParityError(f"cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise ParityError(f"invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ParityError(f"{path} must contain a JSON object")
    return value


def atomic_write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(value, indent=2, sort_keys=False, ensure_ascii=False) + "\n"
    temp = path.with_suffix(path.suffix + ".tmp")
    temp.write_text(payload, encoding="utf-8")
    temp.replace(path)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def command_output(
    command: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    timeout: int = 120,
) -> str:
    proc = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
        check=False,
    )
    if proc.returncode != 0:
        rendered = " ".join(command)
        raise ParityError(f"command failed ({proc.returncode}): {rendered}\n{proc.stdout}")
    return proc.stdout


def git_output(root: Path, *args: str) -> str:
    # The canonical Optimus repository is bare with linked worktrees. Its
    # shared `core.bare=true` otherwise makes an ordinary `git status` fail
    # even when `root` is a real worktree. Hooks also export repository-local
    # Git variables to their children, so discard those before asking about a
    # potentially different repository. Scope the worktree override to this
    # command instead of mutating the process environment.
    env = os.environ.copy()
    for name in GIT_LOCAL_ENV_VARS:
        env.pop(name, None)
    return command_output(
        ["git", f"--work-tree={root.resolve()}", *args],
        cwd=root,
        env=env,
    ).strip()


def workspace_version(root: Path) -> str:
    cargo_path = root / "Cargo.toml"
    try:
        cargo = tomllib.loads(cargo_path.read_text(encoding="utf-8"))
        value = cargo["workspace"]["package"]["version"]
    except (OSError, tomllib.TOMLDecodeError, KeyError, TypeError) as exc:
        raise ParityError(f"cannot read workspace package version from {cargo_path}: {exc}") from exc
    if not isinstance(value, str) or not SEMVER_RE.fullmatch(value):
        raise ParityError(f"workspace package version is not SemVer: {value!r}")
    return value


def parse_timestamp(value: Any) -> dt.datetime | None:
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip().replace("Z", "+00:00")
    try:
        parsed = dt.datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=UTC)
    return parsed.astimezone(UTC)


def is_fresh(value: Any, max_age_days: int, now: dt.datetime) -> bool:
    parsed = parse_timestamp(value)
    if parsed is None or parsed > now + dt.timedelta(minutes=5):
        return False
    return now - parsed <= dt.timedelta(days=max_age_days)


def percentile(values: list[float], quantile: float) -> float:
    """Return a linearly interpolated percentile for a non-empty sample."""
    if not values:
        raise ValueError("percentile requires at least one value")
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    rank = (len(ordered) - 1) * quantile
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return ordered[lower]
    fraction = rank - lower
    return ordered[lower] * (1.0 - fraction) + ordered[upper] * fraction


def valid_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def source_fingerprint(source: Path) -> str:
    candidates: list[Path] = []
    fixed = [
        source / "toolsets.py",
        source / "hermes_cli" / "commands.py",
        source / "hermes_cli" / "provider_catalog.py",
    ]
    candidates.extend(path for path in fixed if path.is_file())
    for base in (source / "tools", source / "plugins" / "platforms"):
        if base.is_dir():
            candidates.extend(path for path in base.rglob("*.py") if path.is_file())
    digest = hashlib.sha256()
    for path in sorted(set(candidates)):
        relative = path.relative_to(source).as_posix().encode("utf-8")
        digest.update(len(relative).to_bytes(4, "big"))
        digest.update(relative)
        data = path.read_bytes()
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")


def option_id(value: str) -> str:
    prefix = "long" if value.startswith("--") else "short"
    return f"{prefix}-{slug(value)}"


def feature(
    feature_id: str,
    kind: str,
    name: str,
    source: str,
    *,
    detail: str | None = None,
) -> dict[str, Any]:
    row: dict[str, Any] = {
        "id": feature_id,
        "kind": kind,
        "name": name,
        "source": source,
    }
    if detail:
        row["detail"] = detail
    return row


def parse_help_subcommands(text: str) -> list[str]:
    """Extract argparse subcommands from its positional-arguments section."""
    lines = text.splitlines()
    active = False
    names: list[str] = []
    for line in lines:
        stripped = line.strip().lower()
        if stripped in {"positional arguments:", "positional arguments"}:
            active = True
            continue
        if active and line and not line[0].isspace() and stripped.endswith(":"):
            break
        if not active:
            continue
        match = re.match(r"^ {4}([a-z0-9][a-z0-9_-]*)(?:\s+\([^)]+\))?\s{2,}\S", line)
        if match:
            names.append(match.group(1))
    return sorted(set(names))


def parse_help_options(text: str) -> list[str]:
    options: set[str] = set()
    for line in text.splitlines():
        if not line.lstrip().startswith("-"):
            continue
        left = re.split(r"\s{2,}", line.strip(), maxsplit=1)[0]
        options.update(re.findall(r"(?<!\w)--[a-z0-9][a-z0-9-]*|(?<!\w)-[A-Za-z0-9](?!\w)", left))
    return sorted(options)


def capture_cli_features(hermes_command: list[str], env: dict[str, str]) -> tuple[list[dict[str, Any]], list[str]]:
    rows: list[dict[str, Any]] = []
    warnings: list[str] = []
    root_help = command_output([*hermes_command, "--help"], env=env)
    rows.extend(
        feature(f"cli.option.root.{option_id(option)}", "cli-option", option, "hermes --help")
        for option in parse_help_options(root_help)
    )
    queue: deque[tuple[str, ...]] = deque((name,) for name in parse_help_subcommands(root_help))
    seen: set[tuple[str, ...]] = set()
    while queue:
        path = queue.popleft()
        if path in seen:
            continue
        seen.add(path)
        if len(seen) > 512:
            raise ParityError("Hermes CLI inventory exceeded the 512-command safety limit")
        label = " ".join(path)
        source = f"hermes {label} --help"
        try:
            help_text = command_output([*hermes_command, *path, "--help"], env=env, timeout=30)
        except ParityError as exc:
            warnings.append(f"could not inventory CLI path {label!r}: {exc}")
            continue
        path_id = ".".join(slug(part) for part in path)
        rows.append(feature(f"cli.command.{path_id}", "cli-command", label, source))
        for option in parse_help_options(help_text):
            rows.append(
                feature(
                    f"cli.option.{path_id}.{option_id(option)}",
                    "cli-option",
                    f"{label} {option}",
                    source,
                )
            )
        if len(path) < 4:
            for child in parse_help_subcommands(help_text):
                queue.append((*path, child))
    return rows, warnings


def find_source_python(source: Path, override: str | None = None) -> Path:
    if override:
        # Keep the venv launcher path intact. Path.resolve() follows its symlink
        # to the base interpreter and loses the virtualenv's site-packages.
        candidate = Path(os.path.abspath(Path(override).expanduser()))
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
        raise ParityError(f"Hermes Python is not executable: {candidate}")
    for candidate in (source / "venv" / "bin" / "python", source / ".venv" / "bin" / "python"):
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
    raise ParityError(f"Hermes virtualenv Python not found below {source}")


def capture_runtime_registries(
    source: Path, env: dict[str, str], python_override: str | None = None
) -> dict[str, Any]:
    python = find_source_python(source, python_override)
    code = r'''
import json
from hermes_cli.commands import COMMAND_REGISTRY
from toolsets import TOOLSETS
from hermes_cli.provider_catalog import provider_catalog

def names(value):
    if value is None:
        return []
    if isinstance(value, dict):
        return [str(item) for item in value.keys()]
    result = []
    for item in value:
        result.append(str(getattr(item, "name", item)))
    return result

commands = []
for command in COMMAND_REGISTRY:
    commands.append({
        "name": str(command.name),
        "aliases": [str(item) for item in getattr(command, "aliases", [])],
        "subcommands": names(getattr(command, "subcommands", None)),
    })
print(json.dumps({
    "slash_commands": commands,
    "toolsets": sorted(str(item) for item in TOOLSETS.keys()),
    "providers": sorted(str(item.slug) for item in provider_catalog()),
}, sort_keys=True))
'''
    output = command_output([str(python), "-c", code], cwd=source, env=env)
    for line in reversed(output.splitlines()):
        if line.lstrip().startswith("{"):
            try:
                value = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict):
                return value
    raise ParityError("Hermes registry capture did not emit a JSON object")


def capture_tool_features(source: Path) -> tuple[list[dict[str, Any]], list[str]]:
    rows: list[dict[str, Any]] = []
    warnings: list[str] = []
    tools_dir = source / "tools"
    for path in sorted(tools_dir.glob("*.py")):
        try:
            tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        except (OSError, SyntaxError) as exc:
            warnings.append(f"could not parse {path.relative_to(source)}: {exc}")
            continue
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Attribute):
                continue
            if node.func.attr != "register":
                continue
            if not isinstance(node.func.value, ast.Name) or node.func.value.id != "registry":
                continue
            keywords = {item.arg: item.value for item in node.keywords if item.arg}
            name_node = keywords.get("name")
            toolset_node = keywords.get("toolset")
            if not isinstance(name_node, ast.Constant) or not isinstance(name_node.value, str):
                # MCP tools are deliberately named at runtime by external
                # servers. Their finite product feature is covered by the
                # manual MCP client/server capabilities; arbitrary remote tool
                # names cannot be part of a version-frozen Hermes inventory.
                if path.name == "mcp_tool.py":
                    continue
                warnings.append(
                    f"dynamic registry.register name at {path.relative_to(source)}:{getattr(node, 'lineno', '?')}"
                )
                continue
            toolset = toolset_node.value if isinstance(toolset_node, ast.Constant) and isinstance(toolset_node.value, str) else None
            rows.append(
                feature(
                    f"tool.{slug(name_node.value)}",
                    "tool",
                    name_node.value,
                    f"{path.relative_to(source).as_posix()}:{getattr(node, 'lineno', '?')}",
                    detail=f"toolset={toolset}" if toolset else None,
                )
            )
    return rows, warnings


def capture_platform_features(source: Path) -> list[dict[str, Any]]:
    base = source / "plugins" / "platforms"
    rows: list[dict[str, Any]] = []
    if not base.is_dir():
        return rows
    for directory in sorted(path for path in base.iterdir() if path.is_dir()):
        if any(directory.glob("*adapter.py")):
            rows.append(
                feature(
                    f"platform.{slug(directory.name)}",
                    "platform",
                    directory.name,
                    directory.relative_to(source).as_posix(),
                )
            )
    return rows


def deduplicate_features(rows: Iterable[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[str]]:
    by_id: dict[str, dict[str, Any]] = {}
    warnings: list[str] = []
    for row in rows:
        feature_id = str(row.get("id", ""))
        previous = by_id.get(feature_id)
        if previous is not None:
            if previous == row:
                continue
            # Never discard two contracts that normalize to the same readable
            # ID. Retain the later one under a deterministic variant suffix so
            # both require evidence at parity time.
            encoded = json.dumps(row, sort_keys=True, separators=(",", ":")).encode("utf-8")
            variant_id = f"{feature_id}.variant-{hashlib.sha256(encoded).hexdigest()[:10]}"
            variant = dict(row)
            variant["id"] = variant_id
            if variant_id in by_id and by_id[variant_id] != variant:
                warnings.append(f"unresolvable feature id collision: {feature_id}")
                continue
            by_id[variant_id] = variant
            continue
        by_id[feature_id] = row
    return sorted(by_id.values(), key=lambda item: (item["kind"], item["id"])), warnings


def capture_baseline(args: argparse.Namespace, root: Path) -> int:
    manifest_path = root / MANIFEST_REL
    manifest = read_json(manifest_path)
    target = manifest.get("hermes_target", {})
    expected_version = str(target.get("version", ""))
    expected_revision = str(target.get("upstream_revision", ""))
    source = Path(args.hermes_source).expanduser().resolve()
    if not source.is_dir():
        raise ParityError(f"Hermes source directory does not exist: {source}")

    with tempfile.TemporaryDirectory(prefix="optimus-hermes-inventory-") as temp_home:
        env = os.environ.copy()
        env["HERMES_HOME"] = temp_home
        if args.hermes_python:
            python = find_source_python(source, args.hermes_python)
            env["PYTHONPATH"] = str(source)
            env["PYTHONNOUSERSITE"] = "1"
            hermes_command = [
                str(python),
                "-c",
                "import sys; from hermes_cli.main import main; sys.argv[0]='hermes'; raise SystemExit(main())",
            ]
        else:
            hermes_command = [args.hermes_bin]
        version_output = command_output([*hermes_command, "--version"], env=env).strip()
        match = re.search(r"Hermes Agent v([^\s]+)", version_output)
        actual_version = match.group(1) if match else ""
        if actual_version != expected_version:
            raise ParityError(
                f"Hermes binary is {actual_version or 'unknown'}, expected tracked target {expected_version}"
            )
        cli_rows, cli_warnings = capture_cli_features(hermes_command, env)
        runtime = capture_runtime_registries(source, env, args.hermes_python)

    rows: list[dict[str, Any]] = list(cli_rows)
    for command in runtime.get("slash_commands", []):
        name = str(command.get("name", ""))
        if not name:
            continue
        rows.append(feature(f"slash.command.{slug(name)}", "slash-command", f"/{name}", "hermes_cli/commands.py"))
        for alias in command.get("aliases", []):
            rows.append(
                feature(
                    f"slash.alias.{slug(alias)}",
                    "slash-alias",
                    f"/{alias}",
                    f"alias of /{name}",
                )
            )
        for subcommand in command.get("subcommands", []):
            rows.append(
                feature(
                    f"slash.subcommand.{slug(name)}.{slug(str(subcommand))}",
                    "slash-subcommand",
                    f"/{name} {subcommand}",
                    "hermes_cli/commands.py",
                )
            )
    rows.extend(
        feature(f"toolset.{slug(name)}", "toolset", str(name), "toolsets.py")
        for name in runtime.get("toolsets", [])
    )
    rows.extend(
        feature(f"provider.{slug(name)}", "provider", str(name), "hermes_cli/provider_catalog.py")
        for name in runtime.get("providers", [])
    )
    tool_rows, tool_warnings = capture_tool_features(source)
    rows.extend(tool_rows)
    rows.extend(capture_platform_features(source))

    manual = read_json(root / MANUAL_REL)
    for row in manual.get("features", []):
        manual_id = str(row.get("id", ""))
        rows.append(
            feature(
                f"manual.{manual_id}",
                "manual",
                str(row.get("name", manual_id)),
                str(row.get("reference", "official documentation")),
            )
        )

    rows, duplicate_warnings = deduplicate_features(rows)
    warnings = [*cli_warnings, *tool_warnings, *duplicate_warnings]
    revision = git_output(source, "rev-parse", "HEAD")
    dirty = bool(git_output(source, "status", "--porcelain"))
    if expected_revision and not revision.startswith(expected_revision):
        warnings.append(f"source revision {revision} does not start with expected {expected_revision}")
    if dirty:
        warnings.append("Hermes source tree was dirty during capture")
    counts = Counter(str(row["kind"]) for row in rows)
    baseline = {
        "schema_version": 1,
        "hermes_version": expected_version,
        "upstream_revision": expected_revision,
        "captured_at": dt.datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "capture_method": "scripts/optimus_version.py capture-hermes",
        "source": {
            "path": str(source),
            "revision": revision,
            "dirty": dirty,
            "registry_fingerprint_sha256": source_fingerprint(source),
            "version_output": version_output,
        },
        "inventory_summary": dict(sorted(counts.items())),
        "capture_warnings": warnings,
        "features": rows,
    }
    baseline_rel = Path(str(manifest.get("baseline", {}).get("path", "")))
    if not baseline_rel.as_posix() or baseline_rel.is_absolute():
        raise ParityError("manifest baseline.path must be a relative path")
    baseline_path = root / baseline_rel
    atomic_write_json(baseline_path, baseline)
    baseline_hash = sha256_file(baseline_path)

    manifest["baseline"]["sha256"] = baseline_hash
    manifest["baseline"]["feature_count"] = len(rows)
    atomic_write_json(manifest_path, manifest)
    for key in ("feature_claims_path", "performance_path"):
        evidence_path = root / Path(str(manifest["evidence"][key]))
        evidence = read_json(evidence_path)
        evidence["hermes_target_version"] = expected_version
        evidence["baseline_sha256"] = baseline_hash
        atomic_write_json(evidence_path, evidence)

    print(
        f"captured Hermes {expected_version} baseline: features={len(rows)} "
        f"sha256={baseline_hash} warnings={len(warnings)}"
    )
    for warning in warnings:
        print(f"WARNING: {warning}", file=sys.stderr)
    return 0


def evaluate_performance(
    report: dict[str, Any],
    manifest: dict[str, Any],
    baseline_hash: str,
    now: dt.datetime,
    result: Evaluation,
) -> None:
    target_version = str(manifest["hermes_target"]["version"])
    rules = manifest.get("performance_rules", {})
    if report.get("schema_version") != 1:
        result.errors.append("performance evidence schema_version must be 1")
    if report.get("hermes_target_version") != target_version:
        result.errors.append("performance evidence targets a different Hermes version")
    if report.get("baseline_sha256") != baseline_hash:
        result.errors.append("performance evidence baseline hash does not match the frozen inventory")
    if report.get("hermes_revision") != manifest["hermes_target"].get("upstream_revision"):
        result.errors.append("performance evidence Hermes revision does not match the target")

    max_age = int(rules.get("evidence_max_age_days", 30))
    if not is_fresh(report.get("generated_at"), max_age, now):
        result.blockers.append(f"performance evidence is absent, future-dated, or older than {max_age} days")

    protocol = report.get("comparison_protocol")
    if not isinstance(protocol, dict):
        result.errors.append("performance comparison_protocol must be an object")
        protocol = {}
    protocol_requirements = {
        "same_machine": "require_same_machine",
        "same_model": "require_same_model",
        "same_provider": "require_same_provider",
        "same_tool_permissions": "require_same_tool_permissions",
        "paired_order_randomized": "require_paired_order_randomization",
    }
    failed_protocol = [
        field_name
        for field_name, rule_name in protocol_requirements.items()
        if rules.get(rule_name, True) and protocol.get(field_name) is not True
    ]
    if failed_protocol:
        result.blockers.append("benchmark protocol not proven: " + ", ".join(failed_protocol))
    required_protocol_text = ("machine_fingerprint", "model", "provider")
    missing_protocol_text = [
        key for key in required_protocol_text if not isinstance(protocol.get(key), str) or not protocol[key].strip()
    ]
    required_protocol_hashes = (
        "dataset_sha256",
        "grader_sha256",
        "benchmark_harness_sha256",
        "hermes_binary_sha256",
        "optimus_binary_sha256",
    )
    missing_protocol_hashes = [
        key
        for key in required_protocol_hashes
        if not isinstance(protocol.get(key), str)
        or re.fullmatch(r"[0-9a-f]{64}", protocol[key]) is None
    ]
    if missing_protocol_text or missing_protocol_hashes:
        result.blockers.append(
            "benchmark provenance not proven: "
            + ", ".join([*missing_protocol_text, *missing_protocol_hashes])
        )
    optimus_revision = report.get("optimus_revision")
    if not isinstance(optimus_revision, str) or re.fullmatch(r"[0-9a-f]{40}", optimus_revision) is None:
        result.blockers.append("performance evidence is not bound to a full Optimus revision")

    scenarios = report.get("scenarios")
    if not isinstance(scenarios, list):
        result.errors.append("performance scenarios must be a list")
        scenarios = []
    by_id: dict[str, dict[str, Any]] = {}
    for row in scenarios:
        if not isinstance(row, dict) or not isinstance(row.get("id"), str):
            result.errors.append("each performance scenario must be an object with a string id")
            continue
        if row["id"] in by_id:
            result.errors.append(f"duplicate performance scenario: {row['id']}")
            continue
        by_id[row["id"]] = row

    required = rules.get("required_scenarios", [])
    result.performance_total = len(required) if isinstance(required, list) else 0
    minimum_samples = int(rules.get("minimum_paired_samples_per_scenario", 30))
    minimum_seeds = int(rules.get("minimum_distinct_seeds_per_scenario", 3))
    scenario_failures: list[str] = []
    for requirement in required if isinstance(required, list) else []:
        scenario_id = str(requirement.get("id", ""))
        metrics = requirement.get("metrics", [])
        scenario = by_id.get(scenario_id)
        if scenario is None:
            scenario_failures.append(f"{scenario_id}: missing")
            continue
        runs = scenario.get("runs")
        if not isinstance(runs, list) or len(runs) < minimum_samples:
            count = len(runs) if isinstance(runs, list) else 0
            scenario_failures.append(f"{scenario_id}: {count}/{minimum_samples} paired samples")
            continue
        seeds = {str(run.get("seed")) for run in runs if isinstance(run, dict) and run.get("seed") is not None}
        if len(seeds) < minimum_seeds:
            scenario_failures.append(f"{scenario_id}: {len(seeds)}/{minimum_seeds} distinct seeds")
            continue

        malformed = False
        execution_orders: set[str] = set()
        sample_identities: set[tuple[str, str]] = set()
        for index, run in enumerate(runs):
            if not isinstance(run, dict):
                result.errors.append(f"{scenario_id}.runs[{index}] must be an object")
                malformed = True
                continue
            case_id = run.get("case_id")
            execution_order = run.get("execution_order")
            if not isinstance(case_id, str) or not case_id.strip():
                result.errors.append(f"{scenario_id}.runs[{index}].case_id must be a non-empty string")
                malformed = True
            if execution_order not in {"hermes-first", "optimus-first"}:
                result.errors.append(
                    f"{scenario_id}.runs[{index}].execution_order must be hermes-first or optimus-first"
                )
                malformed = True
            else:
                execution_orders.add(execution_order)
            if isinstance(case_id, str) and case_id.strip() and run.get("seed") is not None:
                identity = (case_id, str(run["seed"]))
                if identity in sample_identities:
                    result.errors.append(f"{scenario_id}: duplicate case/seed pair {identity}")
                    malformed = True
                sample_identities.add(identity)
            for agent in ("hermes", "optimus"):
                sample = run.get(agent)
                if not isinstance(sample, dict):
                    result.errors.append(f"{scenario_id}.runs[{index}].{agent} must be an object")
                    malformed = True
                    continue
                if not isinstance(sample.get("success"), bool):
                    result.errors.append(f"{scenario_id}.runs[{index}].{agent}.success must be boolean")
                    malformed = True
                quality = sample.get("quality_score")
                quality_invalid = not valid_number(quality)
                if not quality_invalid:
                    assert isinstance(quality, (int, float)) and not isinstance(quality, bool)
                    quality_invalid = not 0.0 <= float(quality) <= 1.0
                if quality_invalid:
                    result.errors.append(
                        f"{scenario_id}.runs[{index}].{agent}.quality_score must be in [0,1]"
                    )
                    malformed = True
                for metric in metrics if isinstance(metrics, list) else []:
                    value = sample.get(metric)
                    value_invalid = not valid_number(value)
                    if not value_invalid:
                        assert isinstance(value, (int, float)) and not isinstance(value, bool)
                        value_invalid = float(value) < 0.0
                    if value_invalid:
                        result.errors.append(
                            f"{scenario_id}.runs[{index}].{agent}.{metric} must be non-negative"
                        )
                        malformed = True
        if execution_orders != {"hermes-first", "optimus-first"}:
            result.errors.append(f"{scenario_id}: paired runs must contain both execution orders")
            malformed = True
        if malformed:
            scenario_failures.append(f"{scenario_id}: malformed samples")
            continue

        hermes_success = statistics.fmean(1.0 if run["hermes"]["success"] else 0.0 for run in runs)
        optimus_success = statistics.fmean(1.0 if run["optimus"]["success"] else 0.0 for run in runs)
        minimum_success_delta = float(rules.get("minimum_success_rate_delta", 0.0))
        if optimus_success - hermes_success < minimum_success_delta - 1e-12:
            scenario_failures.append(
                f"{scenario_id}: success delta {optimus_success - hermes_success:.4f} < {minimum_success_delta:.4f}"
            )
            continue
        hermes_quality = statistics.fmean(float(run["hermes"]["quality_score"]) for run in runs)
        optimus_quality = statistics.fmean(float(run["optimus"]["quality_score"]) for run in runs)
        minimum_quality_delta = float(rules.get("minimum_quality_score_delta", 0.0))
        if optimus_quality - hermes_quality < minimum_quality_delta - 1e-12:
            scenario_failures.append(
                f"{scenario_id}: quality delta {optimus_quality - hermes_quality:.4f} < {minimum_quality_delta:.4f}"
            )
            continue

        metric_failure: str | None = None
        for metric in metrics if isinstance(metrics, list) else []:
            hermes_values = [float(run["hermes"][metric]) for run in runs]
            optimus_values = [float(run["optimus"][metric]) for run in runs]
            if metric == "cost_usd":
                hermes_successes = sum(1 for run in runs if run["hermes"]["success"])
                optimus_successes = sum(1 for run in runs if run["optimus"]["success"])
                if not hermes_successes or not optimus_successes:
                    metric_failure = f"{scenario_id}: cost per success undefined"
                    break
                hermes_metric = sum(hermes_values) / hermes_successes
                optimus_metric = sum(optimus_values) / optimus_successes
                threshold = float(rules.get("maximum_cost_per_success_ratio", 1.0))
                label = "cost/success"
            else:
                threshold_key = {
                    "wall_ms": "maximum_latency_ratio",
                    "ttft_ms": "maximum_ttft_ratio",
                    "rss_mb": "maximum_memory_ratio",
                }.get(str(metric))
                if threshold_key is None:
                    result.errors.append(f"unsupported parity metric in policy: {metric}")
                    metric_failure = f"{scenario_id}: unsupported metric {metric}"
                    break
                threshold = float(rules.get(threshold_key, 1.0))
                for percentile_name, quantile in (("p50", 0.50), ("p95", 0.95)):
                    hermes_metric = percentile(hermes_values, quantile)
                    optimus_metric = percentile(optimus_values, quantile)
                    if hermes_metric <= 0.0:
                        metric_failure = f"{scenario_id}: Hermes {metric} {percentile_name} is not positive"
                        break
                    ratio = optimus_metric / hermes_metric
                    if ratio > threshold + 1e-12:
                        metric_failure = (
                            f"{scenario_id}: {metric} {percentile_name} ratio {ratio:.4f} > {threshold:.4f}"
                        )
                        break
                if metric_failure:
                    break
                continue
            if hermes_metric <= 0.0:
                metric_failure = f"{scenario_id}: Hermes {label} is not positive"
                break
            ratio = optimus_metric / hermes_metric
            if ratio > threshold + 1e-12:
                metric_failure = f"{scenario_id}: {label} ratio {ratio:.4f} > {threshold:.4f}"
                break
        if metric_failure:
            scenario_failures.append(metric_failure)
            continue
        result.performance_passed += 1

    if scenario_failures:
        preview = "; ".join(scenario_failures[:8])
        suffix = f"; +{len(scenario_failures) - 8} more" if len(scenario_failures) > 8 else ""
        result.blockers.append(f"performance scenarios not at parity ({len(scenario_failures)}): {preview}{suffix}")


def evaluate(root: Path = ROOT, *, now: dt.datetime | None = None) -> Evaluation:
    result = Evaluation()
    now = now or dt.datetime.now(UTC)
    try:
        manifest = read_json(root / MANIFEST_REL)
        result.product_version = workspace_version(root)
    except ParityError as exc:
        result.errors.append(str(exc))
        return result

    if manifest.get("schema_version") != 1:
        result.errors.append("optimus-version schema_version must be 1")
    target = manifest.get("hermes_target")
    if not isinstance(target, dict):
        result.errors.append("hermes_target must be an object")
        return result
    result.target_version = str(target.get("version", ""))
    if not SEMVER_RE.fullmatch(result.target_version):
        result.errors.append(f"Hermes target is not SemVer: {result.target_version!r}")

    baseline_meta = manifest.get("baseline")
    if not isinstance(baseline_meta, dict):
        result.errors.append("baseline must be an object")
        return result
    baseline_rel = Path(str(baseline_meta.get("path", "")))
    if baseline_rel.is_absolute() or not baseline_rel.as_posix():
        result.errors.append("baseline.path must be a relative path")
        return result
    baseline_path = root / baseline_rel
    try:
        baseline = read_json(baseline_path)
        baseline_hash = sha256_file(baseline_path)
    except ParityError as exc:
        result.errors.append(str(exc))
        return result
    if baseline_meta.get("sha256") != baseline_hash:
        result.errors.append("frozen Hermes baseline SHA-256 does not match optimus-version.json")
    if baseline.get("schema_version") != 1:
        result.errors.append("Hermes baseline schema_version must be 1")
    if baseline.get("hermes_version") != result.target_version:
        result.errors.append("Hermes baseline version does not match the tracked target")
    if baseline.get("upstream_revision") != target.get("upstream_revision"):
        result.errors.append("Hermes baseline revision does not match the tracked target")
    features = baseline.get("features")
    if not isinstance(features, list):
        result.errors.append("Hermes baseline features must be a list")
        features = []
    result.feature_total = len(features)
    if baseline_meta.get("feature_count") != len(features):
        result.errors.append("baseline.feature_count does not match the frozen inventory")

    feature_ids: list[str] = []
    kinds: Counter[str] = Counter()
    for index, row in enumerate(features):
        if not isinstance(row, dict):
            result.errors.append(f"baseline.features[{index}] must be an object")
            continue
        feature_id = row.get("id")
        kind = row.get("kind")
        if not isinstance(feature_id, str) or not FEATURE_ID_RE.fullmatch(feature_id):
            result.errors.append(f"baseline.features[{index}] has invalid id {feature_id!r}")
            continue
        if not isinstance(kind, str) or not kind:
            result.errors.append(f"{feature_id}: kind is required")
            continue
        feature_ids.append(feature_id)
        kinds[kind] += 1
    duplicates = sorted(item for item, count in Counter(feature_ids).items() if count > 1)
    if duplicates:
        result.errors.append(f"duplicate baseline feature ids: {duplicates[:10]}")
    required_kinds = manifest.get("release_rules", {}).get("required_inventory_kinds", [])
    missing_kinds = sorted(str(item) for item in required_kinds if kinds[str(item)] == 0)
    if missing_kinds:
        result.errors.append(f"baseline is missing required inventory kinds: {missing_kinds}")
    if baseline.get("capture_warnings"):
        result.blockers.append(
            f"Hermes inventory capture has {len(baseline['capture_warnings'])} unresolved warning(s)"
        )
    audit = baseline_meta.get("inventory_audit")
    if not isinstance(audit, dict) or audit.get("status") != "complete":
        result.blockers.append("official-docs inventory audit is not complete")
    elif not audit.get("reviewer") or not parse_timestamp(audit.get("reviewed_at")):
        result.errors.append("completed inventory audit requires reviewer and reviewed_at")

    evidence_meta = manifest.get("evidence")
    if not isinstance(evidence_meta, dict):
        result.errors.append("evidence must be an object")
        return result
    try:
        feature_evidence = read_json(root / Path(str(evidence_meta["feature_claims_path"])))
        performance = read_json(root / Path(str(evidence_meta["performance_path"])))
        ledger = read_json(root / Path(str(evidence_meta["rollup_ledger_path"])))
    except (ParityError, KeyError) as exc:
        result.errors.append(str(exc))
        return result

    if feature_evidence.get("schema_version") != 1:
        result.errors.append("feature evidence schema_version must be 1")
    if feature_evidence.get("hermes_target_version") != result.target_version:
        result.errors.append("feature evidence targets a different Hermes version")
    if feature_evidence.get("baseline_sha256") != baseline_hash:
        result.errors.append("feature evidence baseline hash does not match the frozen inventory")
    claims = feature_evidence.get("claims")
    if not isinstance(claims, dict):
        result.errors.append("feature evidence claims must be an object")
        claims = {}
    unknown_claims = sorted(set(claims) - set(feature_ids))
    if unknown_claims:
        result.errors.append(f"feature evidence contains unknown ids: {unknown_claims[:10]}")

    max_age = int(manifest.get("release_rules", {}).get("evidence_max_age_days", 30))
    unverified: list[str] = []
    claim_revisions: set[str] = set()
    for feature_id in feature_ids:
        claim = claims.get(feature_id)
        if not isinstance(claim, dict) or claim.get("status") != "verified":
            unverified.append(feature_id)
            continue
        evidence_paths = claim.get("evidence")
        if not isinstance(evidence_paths, list) or not evidence_paths or any(
            not isinstance(item, str) or not item.strip() for item in evidence_paths
        ):
            result.errors.append(f"{feature_id}: verified claim requires evidence paths")
            continue
        missing_paths = [item for item in evidence_paths if not (root / item).exists()]
        if missing_paths:
            result.errors.append(f"{feature_id}: missing evidence paths {missing_paths}")
            continue
        if not isinstance(claim.get("trajectory"), str) or not claim["trajectory"].strip():
            result.errors.append(f"{feature_id}: verified claim requires a trajectory")
            continue
        revision = claim.get("optimus_revision")
        if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
            result.errors.append(f"{feature_id}: verified claim requires a full Optimus commit SHA")
            continue
        if not is_fresh(claim.get("verified_at"), max_age, now):
            unverified.append(feature_id)
            continue
        claim_revisions.add(revision)
        result.feature_verified += 1
    if unverified:
        preview = ", ".join(unverified[:8])
        suffix = f", +{len(unverified) - 8} more" if len(unverified) > 8 else ""
        result.blockers.append(
            f"baseline feature evidence incomplete: {len(unverified)}/{len(feature_ids)} unverified ({preview}{suffix})"
        )
    if len(claim_revisions) > 1:
        result.errors.append("verified feature evidence spans multiple Optimus revisions")

    capabilities = ledger.get("capabilities")
    if not isinstance(capabilities, list):
        result.errors.append("rollup parity ledger capabilities must be a list")
        capabilities = []
    for row in capabilities:
        if isinstance(row, dict) and isinstance(row.get("state"), str):
            result.ledger_counts[row["state"]] += 1
    not_rollup_parity = [
        str(row.get("id", "unknown"))
        for row in capabilities
        if not isinstance(row, dict) or row.get("state") not in {"parity", "win"}
    ]
    if not_rollup_parity:
        preview = ", ".join(not_rollup_parity[:8])
        suffix = f", +{len(not_rollup_parity) - 8} more" if len(not_rollup_parity) > 8 else ""
        result.blockers.append(
            f"rollup ledger not complete: {len(not_rollup_parity)}/{len(capabilities)} below parity ({preview}{suffix})"
        )

    evaluate_performance(performance, manifest, baseline_hash, now, result)

    try:
        current_revision = git_output(root, "rev-parse", "HEAD")
        dirty = bool(git_output(root, "status", "--porcelain"))
    except ParityError as exc:
        result.errors.append(str(exc))
        current_revision = ""
        dirty = True
    if dirty:
        result.blockers.append("Optimus worktree is dirty; parity promotion requires an immutable clean revision")
    if claim_revisions and current_revision not in claim_revisions:
        result.blockers.append("feature evidence is not bound to the current Optimus revision")
    performance_revision = performance.get("optimus_revision")
    if isinstance(performance_revision, str) and performance_revision and performance_revision != current_revision:
        result.blockers.append("performance evidence is not bound to the current Optimus revision")

    claim = manifest.get("parity_claim")
    if not isinstance(claim, dict):
        result.errors.append("parity_claim must be an object")
        claim = {}
    result.claim_status = str(claim.get("status", ""))
    if result.claim_status not in {"unverified", "verified"}:
        result.errors.append("parity_claim.status must be unverified or verified")
    if result.claim_status == "unverified":
        if any(claim.get(field_name) is not None for field_name in ("hermes_version", "verified_at", "optimus_revision", "reviewer")):
            result.errors.append("unverified parity claim must not contain claim metadata")
    else:
        if claim.get("hermes_version") != result.target_version:
            result.errors.append("verified parity claim version does not match the tracked target")
        if claim.get("optimus_revision") != current_revision:
            result.errors.append("verified parity claim is not bound to the current Optimus revision")
        if not claim.get("reviewer") or not parse_timestamp(claim.get("verified_at")):
            result.errors.append("verified parity claim requires reviewer and verified_at")

    return result


def print_status(result: Evaluation, *, as_json: bool = False) -> None:
    payload = {
        "product_version": result.product_version,
        "hermes_target_version": result.target_version,
        "hermes_parity_version": result.target_version if result.claim_status == "verified" and result.ready else None,
        "claim_status": result.claim_status,
        "features": {"verified": result.feature_verified, "total": result.feature_total},
        "rollup_ledger": dict(sorted(result.ledger_counts.items())),
        "performance_scenarios": {
            "passed": result.performance_passed,
            "total": result.performance_total,
        },
        "ready_for_parity": result.ready,
        "errors": result.errors,
        "blockers": result.blockers,
    }
    if as_json:
        print(json.dumps(payload, indent=2, sort_keys=True))
        return
    parity = payload["hermes_parity_version"] or "unverified"
    print(f"Optimus Agent: {result.product_version}")
    print(f"Hermes target: {result.target_version}")
    print(f"Hermes parity: {parity}")
    print(f"Feature evidence: {result.feature_verified}/{result.feature_total}")
    print(f"Performance scenarios: {result.performance_passed}/{result.performance_total}")
    if result.ledger_counts:
        print("Rollup ledger: " + " ".join(f"{key}={value}" for key, value in sorted(result.ledger_counts.items())))
    print(f"Parity gate: {'PASS' if result.ready else 'BLOCKED'}")
    for error in result.errors:
        print(f"ERROR: {error}")
    for blocker in result.blockers:
        print(f"BLOCKER: {blocker}")


def numeric_semver_core(version: str) -> tuple[int, int, int] | None:
    core = version.split("+", 1)[0].split("-", 1)[0]
    parts = core.split(".")
    if len(parts) != 3 or any(not part.isdigit() for part in parts):
        return None
    return tuple(int(part) for part in parts)  # type: ignore[return-value]


def release_errors(result: Evaluation) -> list[str]:
    errors = list(result.errors)
    product_core = numeric_semver_core(result.product_version)
    target_core = numeric_semver_core(result.target_version)
    collision = product_core is not None and product_core == target_core
    claim_active = result.claim_status == "verified"
    if collision or claim_active:
        if result.blockers:
            reason = "numeric Hermes version match" if collision else "verified Hermes parity claim"
            errors.append(f"{reason} is forbidden while parity gates are blocked")
        if result.claim_status != "verified":
            errors.append(
                f"Optimus {result.product_version} numerically matches Hermes {result.target_version} without a verified parity claim"
            )
    return errors


def promote(root: Path, reviewer: str) -> int:
    result = evaluate(root)
    if result.errors or result.blockers:
        print_status(result)
        raise ParityError("cannot promote: parity gate is not ready")
    manifest_path = root / MANIFEST_REL
    manifest = read_json(manifest_path)
    revision = git_output(root, "rev-parse", "HEAD")
    manifest["parity_claim"] = {
        "status": "verified",
        "hermes_version": result.target_version,
        "verified_at": dt.datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "optimus_revision": revision,
        "reviewer": reviewer,
    }
    atomic_write_json(manifest_path, manifest)
    print(f"promoted Hermes parity version {result.target_version} at Optimus revision {revision}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    status = subparsers.add_parser("status", help="show Optimus and Hermes parity versions")
    status.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    subparsers.add_parser("validate", help="validate versioning files without requiring parity")
    subparsers.add_parser("gate", help="require complete Hermes parity")
    subparsers.add_parser("release-check", help="block false parity claims and numeric version collisions")
    capture = subparsers.add_parser("capture-hermes", help="freeze a machine inventory of the tracked Hermes target")
    capture.add_argument("--hermes-bin", default="hermes")
    capture.add_argument(
        "--hermes-python",
        help="Python from a Hermes venv; with --hermes-source this executes that exact source tree",
    )
    capture.add_argument("--hermes-source", default=str(Path.home() / ".hermes" / "hermes-agent"))
    promotion = subparsers.add_parser("promote", help="record a verified parity claim after every gate passes")
    promotion.add_argument("--reviewer", required=True)
    return parser


def main(argv: list[str] | None = None, *, root: Path = ROOT) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "capture-hermes":
            return capture_baseline(args, root)
        if args.command == "promote":
            return promote(root, args.reviewer)
        result = evaluate(root)
        if args.command == "status":
            print_status(result, as_json=args.json)
            return 0 if not result.errors else 1
        if args.command == "validate":
            print_status(result)
            return 0 if not result.errors else 1
        if args.command == "gate":
            print_status(result)
            return 0 if result.ready else 1
        if args.command == "release-check":
            failures = release_errors(result)
            print_status(result)
            if failures:
                for failure in failures:
                    print(f"RELEASE BLOCKED: {failure}", file=sys.stderr)
                return 1
            print(
                f"release-check PASS: Optimus {result.product_version} is an independent development version; "
                f"Hermes {result.target_version} parity remains {result.claim_status}"
            )
            return 0
    except ParityError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    raise AssertionError(f"unhandled command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
