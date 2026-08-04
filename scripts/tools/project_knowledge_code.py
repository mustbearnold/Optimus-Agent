#!/usr/bin/env python3
"""Derive interval-valid code structure for the temporal project graph.

Dependency edges between package manifests carry explicit validity intervals
in both ancestry order and UTC event time: the commit that introduced a
dependency opens the interval and the commit that removed it closes the
interval, so historical dependency questions are answered by interval
containment instead of current-state guessing. Symbols are a current-tree
projection only; the graph stores no historical symbol claim it cannot prove
from retained evidence.
"""

from __future__ import annotations

import json
import re
import tomllib
from pathlib import Path
from typing import Any, Callable


class CodeGraphError(RuntimeError):
    """Code structure could not be derived without guessing."""


MANIFEST_NAMES = {"Cargo.toml": "cargo", "package.json": "npm"}

CARGO_DEPENDENCY_TABLES = (
    ("dependencies", "normal"),
    ("dev-dependencies", "dev"),
    ("build-dependencies", "build"),
)

NPM_DEPENDENCY_TABLES = (
    ("dependencies", "normal"),
    ("devDependencies", "dev"),
)

RUST_ITEM = re.compile(
    r"^(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:(?:default|async|unsafe|extern\s+\"[^\"]*\")\s+)*"
    r"(?:const\s+(?=(?:async\s+|unsafe\s+|extern\s+\"[^\"]*\"\s+)*fn))?"
    r"(?:(?:async|unsafe|extern\s+\"[^\"]*\")\s+)*"
    r"(fn|struct|enum|trait|type|mod|const|static|union)\s+"
    r"([A-Za-z_][A-Za-z0-9_]*)"
)
RUST_MACRO = re.compile(r"^macro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)")
RUST_IMPL = re.compile(
    r"^impl(?:<[^>]*>)?\s+"
    r"(?:[A-Za-z_][A-Za-z0-9_:<>,'&\s]*?\bfor\s+)?"
    r"&?(?:dyn\s+)?([A-Za-z_][A-Za-z0-9_]*)"
)
TS_EXPORT = re.compile(
    r"^export\s+(?:default\s+)?(?:abstract\s+)?(?:async\s+)?"
    r"(function|class|const|let|var|interface|type|enum)\s+"
    r"([A-Za-z_$][A-Za-z0-9_$]*)"
)

RUST_SUFFIXES = {".rs"}
SCRIPT_SUFFIXES = {".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"}


def parse_cargo_manifest(text: str) -> dict[str, Any] | None:
    try:
        payload = tomllib.loads(text)
    except tomllib.TOMLDecodeError:
        return None
    name = payload.get("package", {}).get("name")
    dependencies: list[tuple[str, str, str]] = []
    for table, dep_kind in CARGO_DEPENDENCY_TABLES:
        for dependency, requirement in sorted(payload.get(table, {}).items()):
            dependencies.append((dependency, dep_kind, _cargo_requirement(requirement)))
    for dependency, requirement in sorted(
        payload.get("workspace", {}).get("dependencies", {}).items()
    ):
        dependencies.append((dependency, "workspace", _cargo_requirement(requirement)))
    return {"name": name, "dependencies": dependencies}


def _cargo_requirement(requirement: Any) -> str:
    if isinstance(requirement, str):
        return requirement
    if isinstance(requirement, dict):
        if "version" in requirement:
            return str(requirement["version"])
        if "path" in requirement:
            return f"path:{requirement['path']}"
        if requirement.get("workspace"):
            return "workspace"
    return ""


def parse_npm_manifest(text: str) -> dict[str, Any] | None:
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        return None
    if not isinstance(payload, dict):
        return None
    name = payload.get("name")
    dependencies: list[tuple[str, str, str]] = []
    for table, dep_kind in NPM_DEPENDENCY_TABLES:
        entries = payload.get(table, {})
        if not isinstance(entries, dict):
            continue
        for dependency, requirement in sorted(entries.items()):
            dependencies.append((dependency, dep_kind, str(requirement)))
    return {"name": name, "dependencies": dependencies}


def parse_manifest(kind: str, text: str) -> dict[str, Any] | None:
    if kind == "cargo":
        return parse_cargo_manifest(text)
    return parse_npm_manifest(text)


def _fallback_name(manifest_path: str) -> str:
    parent = Path(manifest_path).parent.name
    return parent or "workspace-root"


def derive(
    run_git: Callable[..., str],
    commits: list[dict[str, Any]],
    histories: dict[str, list[dict[str, str]]],
    current: list[str],
    root: Path,
) -> dict[str, list[dict[str, Any]]]:
    positions = {commit["sha"]: index for index, commit in enumerate(commits)}
    times = {commit["sha"]: commit["committed_at"] for commit in commits}
    current_set = set(current)
    manifest_paths = sorted(
        path
        for path in set(histories) | current_set
        if Path(path).name in MANIFEST_NAMES
    )
    names: dict[str, str] = {}
    dependency_states: dict[str, list[dict[str, Any]]] = {}
    for manifest_path in manifest_paths:
        kind = MANIFEST_NAMES[Path(manifest_path).name]
        opened: dict[tuple[str, str], dict[str, Any]] = {}
        closed: list[dict[str, Any]] = []
        for event in histories.get(manifest_path, []):
            position = positions[event["commit"]]
            occurred = times[event["commit"]]
            if event["status"] == "D":
                parsed = None
            else:
                shown = run_git("show", f"{event['commit']}:{manifest_path}")
                parsed = parse_manifest(kind, shown)
                if parsed is None and event["status"] != "A":
                    # An unparseable intermediate state neither opens nor
                    # closes intervals; the previous proven state carries.
                    continue
            if parsed is not None and parsed.get("name"):
                names[manifest_path] = str(parsed["name"])
            desired = (
                {
                    (dependency, dep_kind): requirement
                    for dependency, dep_kind, requirement in parsed["dependencies"]
                }
                if parsed is not None and event["status"] != "D"
                else {}
            )
            for key in sorted(set(opened) - set(desired)):
                interval = opened.pop(key)
                interval["valid_to_position"] = position
                interval["valid_to_time"] = occurred
                closed.append(interval)
            for key in sorted(set(desired) - set(opened)):
                dependency, dep_kind = key
                opened[key] = {
                    "manifest_path": manifest_path,
                    "dependency": dependency,
                    "dep_kind": dep_kind,
                    "requirement": desired[key],
                    "valid_from_position": position,
                    "valid_from_time": occurred,
                    "valid_to_position": None,
                    "valid_to_time": None,
                }
        dependency_states[manifest_path] = closed + [
            opened[key] for key in sorted(opened)
        ]
        names.setdefault(manifest_path, _fallback_name(manifest_path))
    packages: list[dict[str, Any]] = []
    identifiers: dict[str, str] = {}
    for manifest_path in manifest_paths:
        kind = MANIFEST_NAMES[Path(manifest_path).name]
        identifier = f"{kind}:{names[manifest_path]}"
        if identifier in identifiers.values():
            identifier = f"{kind}:{names[manifest_path]}@{manifest_path}"
        identifiers[manifest_path] = identifier
        packages.append({
            "package_id": identifier,
            "name": names[manifest_path],
            "kind": kind,
            "manifest_path": manifest_path,
            "origin": "internal",
            "exists_now": manifest_path in current_set,
        })
    dependencies: list[dict[str, Any]] = []
    internal_by_name: dict[tuple[str, str], str] = {}
    for package in packages:
        internal_by_name.setdefault(
            (package["kind"], package["name"]), package["package_id"]
        )
    external: dict[str, dict[str, Any]] = {}
    for manifest_path in manifest_paths:
        for interval in dependency_states.get(manifest_path, []):
            kind = MANIFEST_NAMES[Path(manifest_path).name]
            target = internal_by_name.get((kind, interval["dependency"]))
            if target is None:
                target = f"{kind}:{interval['dependency']}"
                external.setdefault(target, {
                    "package_id": target,
                    "name": interval["dependency"],
                    "kind": kind,
                    "manifest_path": None,
                    "origin": "external",
                    "exists_now": None,
                })
            dependencies.append({
                **interval,
                "package_id": identifiers[manifest_path],
                "dependency_package_id": target,
            })
    packages.extend(external[key] for key in sorted(external))
    dependencies.sort(
        key=lambda item: (
            item["manifest_path"], item["dependency"], item["dep_kind"],
            item["valid_from_position"],
        )
    )
    return {
        "packages": packages,
        "package_dependencies": dependencies,
        "code_symbols": derive_symbols(current, root),
    }


def extract_rust_symbols(text: str) -> list[tuple[str, str, int]]:
    symbols: list[tuple[str, str, int]] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        item = RUST_ITEM.match(line)
        if item:
            symbols.append((item.group(2), item.group(1), line_number))
            continue
        macro = RUST_MACRO.match(line)
        if macro:
            symbols.append((macro.group(1), "macro", line_number))
            continue
        implementation = RUST_IMPL.match(line)
        if implementation:
            symbols.append((implementation.group(1), "impl", line_number))
    return symbols


def extract_script_symbols(text: str) -> list[tuple[str, str, int]]:
    symbols: list[tuple[str, str, int]] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        exported = TS_EXPORT.match(line)
        if exported:
            symbols.append((exported.group(2), f"export-{exported.group(1)}", line_number))
    return symbols


def derive_symbols(current: list[str], root: Path) -> list[dict[str, Any]]:
    symbols: list[dict[str, Any]] = []
    for relative in sorted(current):
        suffix = Path(relative).suffix
        if suffix in RUST_SUFFIXES:
            extractor = extract_rust_symbols
        elif suffix in SCRIPT_SUFFIXES:
            extractor = extract_script_symbols
        else:
            continue
        text = (root / relative).read_text(encoding="utf-8", errors="replace")
        seen: set[tuple[str, str, int]] = set()
        for name, kind, line in extractor(text):
            key = (name, kind, line)
            if key in seen:
                continue
            seen.add(key)
            symbols.append({
                "path": relative, "name": name, "kind": kind, "line": line,
            })
    return symbols
