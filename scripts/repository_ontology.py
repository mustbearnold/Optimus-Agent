#!/usr/bin/env python3
"""Executable component database for Optimus repository and development state."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DATABASE = ROOT / "docs" / "repository-components.json"
GENERATED_WIKI = ROOT / "docs" / "COMPONENTS.md"
BENCHMARK = ROOT / "evals" / "repository-orientation" / "questions-v1.json"

STORAGE_PLANES = {"repository", "development", "installed-runtime"}
CONCERNS = {
    "product-runtime", "product-interface", "development-tooling",
    "evaluation-definition", "knowledge", "evidence", "delivery-control",
    "experiment", "workspace-view",
}
DISTRIBUTIONS = {
    "binary", "bundled", "embedded", "transitive", "repository-only",
    "generated-local", "none",
}
LIFECYCLES = {
    "primary", "supporting", "optional", "rollback-only", "incubating",
    "historical", "generated",
}
REPOSITORY_ROLES = {"required", "generated", "historical", "external"}
RETENTIONS = {"essential", "regenerable", "conditional"}
COVERED_CHILDREN = ("apps", "crates", "evals", "skills")
IGNORED_TOP_LEVEL = {".git"}


class OntologyError(RuntimeError):
    """Repository meaning is missing, contradictory, or stale."""


def canonical_json(payload: Any) -> str:
    return json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def load_database(path: Path = DATABASE) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise OntologyError(f"cannot load component database: {error}") from error
    if payload.get("schema_version") != 1 or not isinstance(payload.get("components"), list):
        raise OntologyError("component database must be schema version 1")
    return payload


def git_files(root: Path = ROOT) -> list[str]:
    marker = root / ".git"
    if marker.is_file():
        line = marker.read_text(encoding="utf-8").strip()
        if not line.startswith("gitdir:"):
            raise OntologyError("worktree Git pointer is malformed")
        git_dir = Path(line.removeprefix("gitdir:").strip())
    elif marker.is_dir():
        git_dir = marker
    else:
        raise OntologyError("cannot resolve repository Git metadata")
    result = subprocess.run(
        [
            "git", f"--git-dir={git_dir}", f"--work-tree={root}", "ls-files", "-z",
            "--cached", "--others", "--exclude-standard",
        ],
        cwd=root,
        env={key: value for key, value in os.environ.items() if not key.startswith("GIT_")},
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise OntologyError(result.stderr.decode(errors="replace").strip())
    return sorted(
        relative for item in result.stdout.split(b"\0") if item
        for relative in (item.decode(),) if (root / relative).exists()
    )


def workspace_packages(root: Path = ROOT) -> dict[str, str]:
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    packages: dict[str, str] = {}
    for member in manifest.get("workspace", {}).get("members", []):
        member_manifest = root / member / "Cargo.toml"
        parsed = tomllib.loads(member_manifest.read_text(encoding="utf-8"))
        name = parsed.get("package", {}).get("name")
        if name:
            packages[Path(member).as_posix()] = str(name)
    return packages


def workspace_default_members(root: Path, members: dict[str, str]) -> set[str]:
    """Member paths a bare `cargo build` compiles.

    Cargo reads an absent `default-members` as every member, so a workspace that
    never narrows the set makes no exclusion claim for a component row to match.
    """
    try:
        manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    except OSError:
        return set(members)
    workspace = manifest.get("workspace", {})
    if "default-members" not in workspace:
        return set(members)
    return {Path(member).as_posix() for member in workspace["default-members"]}


def npm_packages(root: Path = ROOT) -> dict[str, str]:
    packages: dict[str, str] = {}
    for path in (root / "apps").glob("*/package.json"):
        payload = json.loads(path.read_text(encoding="utf-8"))
        if payload.get("name"):
            packages[path.parent.relative_to(root).as_posix()] = str(payload["name"])
    return packages


def validate_database(payload: dict[str, Any], root: Path = ROOT) -> list[dict[str, Any]]:
    components = payload["components"]
    errors: list[str] = []
    by_id: dict[str, dict[str, Any]] = {}
    by_path: dict[str, dict[str, Any]] = {}
    required = {
        "id", "path", "storage_plane", "concern", "distribution", "lifecycle",
        "repository_role", "retention", "summary", "not", "safe_to_delete",
    }
    for component in components:
        component_id = str(component.get("id", ""))
        path = str(component.get("path", ""))
        missing = sorted(required - set(component))
        if missing:
            errors.append(f"{component_id or path}: missing fields {', '.join(missing)}")
        if component_id in by_id:
            errors.append(f"duplicate component id: {component_id}")
        if path in by_path:
            errors.append(f"duplicate component path: {path}")
        by_id[component_id] = component
        by_path[path] = component
        if component.get("storage_plane") not in STORAGE_PLANES:
            errors.append(f"{component_id}: invalid storage_plane")
        if component.get("concern") not in CONCERNS:
            errors.append(f"{component_id}: invalid concern")
        if component.get("distribution") not in DISTRIBUTIONS:
            errors.append(f"{component_id}: invalid distribution")
        if component.get("lifecycle") not in LIFECYCLES:
            errors.append(f"{component_id}: invalid lifecycle")
        if component.get("repository_role") not in REPOSITORY_ROLES:
            errors.append(f"{component_id}: invalid repository_role")
        if component.get("retention") not in RETENTIONS:
            errors.append(f"{component_id}: invalid retention")
        if not isinstance(component.get("not"), list) or not component.get("not"):
            errors.append(f"{component_id}: at least one common misconception is required")
        if not isinstance(component.get("safe_to_delete"), bool):
            errors.append(f"{component_id}: safe_to_delete must be boolean")
        lifecycle_events = component.get("lifecycle_events", [])
        if not isinstance(lifecycle_events, list):
            errors.append(f"{component_id}: lifecycle_events must be a list")
            lifecycle_events = []
        previous_at = ""
        for event in lifecycle_events:
            if not isinstance(event, dict):
                errors.append(f"{component_id}: lifecycle event must be an object")
                continue
            at = str(event.get("at", ""))
            state = event.get("state")
            if state not in LIFECYCLES:
                errors.append(f"{component_id}: lifecycle event has invalid state")
            try:
                dt.date.fromisoformat(at)
            except ValueError:
                errors.append(f"{component_id}: lifecycle event date must be ISO")
            if previous_at and at < previous_at:
                errors.append(f"{component_id}: lifecycle events must be chronological")
            previous_at = at
            if not event.get("reason") or not event.get("evidence"):
                errors.append(f"{component_id}: lifecycle event needs reason and evidence")
        if lifecycle_events and lifecycle_events[-1].get("state") != component.get("lifecycle"):
            errors.append(f"{component_id}: latest lifecycle event must match current lifecycle")
        if path.startswith("workspace://"):
            if component.get("repository_role") != "external":
                errors.append(f"{component_id}: workspace URI must have external role")
        else:
            candidate = (root / path).resolve(strict=False)
            try:
                candidate.relative_to(root.resolve())
            except ValueError:
                errors.append(f"{component_id}: path escapes repository")
            if not candidate.exists() and not component.get("optional_path", False):
                errors.append(f"{component_id}: component path does not exist: {path}")
        for output in component.get("outputs_to", []):
            if str(output) != "workspace://Development" and not str(output).startswith(
                ("workspace://Development/", "runtime://")
            ):
                errors.append(f"{component_id}: generated output must leave Repository: {output}")
        if component.get("lifecycle") in {"rollback-only", "incubating"}:
            for field in ("removal_when", "review_by"):
                if not component.get(field):
                    errors.append(f"{component_id}: {component['lifecycle']} requires {field}")
            try:
                deadline = dt.date.fromisoformat(str(component.get("review_by", "")))
                if deadline < dt.date.today():
                    errors.append(f"{component_id}: lifecycle review expired on {deadline}")
            except ValueError:
                errors.append(f"{component_id}: review_by must be an ISO date")

    for component in components:
        component_id = str(component.get("id", ""))
        parent = component.get("parent")
        if parent and parent not in by_id:
            errors.append(f"{component_id}: unknown parent {parent}")
        for paired in component.get("paired_with", []):
            if paired not in by_id:
                errors.append(f"{component_id}: unknown paired component {paired}")
        seen: set[str] = set()
        cursor = component
        while cursor.get("parent"):
            parent_id = str(cursor["parent"])
            if parent_id in seen or parent_id == component_id:
                errors.append(f"{component_id}: parent cycle")
                break
            seen.add(parent_id)
            cursor = by_id.get(parent_id, {})

    tracked = git_files(root)
    top_level = sorted({path.split("/", 1)[0] for path in tracked if "/" in path} - IGNORED_TOP_LEVEL)
    missing_roots = [path for path in top_level if path not in by_path]
    if missing_roots:
        errors.append("unclassified top-level roots: " + ", ".join(missing_roots))
    for parent in COVERED_CHILDREN:
        children = sorted({
            path.split("/", 2)[1] for path in tracked if path.startswith(parent + "/") and path.count("/") >= 1
        })
        missing = [f"{parent}/{child}" for child in children if f"{parent}/{child}" not in by_path]
        if missing:
            errors.append(f"unclassified {parent} components: " + ", ".join(missing))

    rust_packages = workspace_packages(root)
    default_members = workspace_default_members(root, rust_packages)
    for path, package in rust_packages.items():
        component = by_path.get(path)
        if component is None:
            errors.append(f"manifest package is not classified: {path}")
            continue
        component_id = str(component.get("id", ""))
        if component.get("package") != package:
            errors.append(f"{path}: package claim does not match manifest ({package})")
        declared = component.get("default_member", True)
        if not isinstance(declared, bool):
            errors.append(f"{component_id}: default_member must be boolean")
        elif declared and path not in default_members:
            errors.append(
                f"{component_id}: Cargo.toml drops {path} from default-members "
                "while the row still claims default membership"
            )
        elif not declared and path in default_members:
            errors.append(
                f"{component_id}: row declares default_member false "
                f"while Cargo.toml default-members keeps {path}"
            )
    for component in components:
        if "default_member" in component and str(component.get("path", "")) not in rust_packages:
            errors.append(
                f"{component.get('id')}: default_member belongs to workspace members only"
            )
    for path, package in npm_packages(root).items():
        component = by_path.get(path)
        field = "npm_package" if path in rust_packages else "package"
        if component is None:
            errors.append(f"manifest package is not classified: {path}")
        elif component.get(field) != package:
            errors.append(f"{path}: {field} claim does not match manifest ({package})")

    if errors:
        raise OntologyError("\n".join(errors))
    return sorted(components, key=lambda item: (str(item["path"]), str(item["id"])))


def catalog_payload(payload: dict[str, Any] | None = None) -> dict[str, Any]:
    loaded = payload or load_database()
    components = validate_database(loaded)
    return {"schema_version": 1, "components": components}


def wiki(payload: dict[str, Any]) -> str:
    components = payload["components"]
    lines = [
        "<!-- Generated by scripts/repository_ontology.py; do not edit manually. -->",
        "# Optimus Agent component wiki", "",
        "This is the generated human view of `docs/repository-components.json`.", "",
        "| Path | Purpose | Lifecycle | Distribution | Delete? |",
        "|---|---|---|---|---|",
    ]
    for item in components:
        lines.append(
            f"| `{item['path']}` | {item['summary']} | `{item['lifecycle']}` | "
            f"`{item['distribution']}` | {'yes' if item['safe_to_delete'] else 'no'} |"
        )
    lines.extend(["", "Use `just explain-path <path>` for misconceptions, pairings, and output destinations.", ""])
    return "\n".join(lines)


def generate() -> None:
    payload = catalog_payload()
    GENERATED_WIKI.write_text(wiki(payload), encoding="utf-8")


def check() -> dict[str, int]:
    payload = catalog_payload()
    if not GENERATED_WIKI.is_file() or GENERATED_WIKI.read_text(encoding="utf-8") != wiki(payload):
        raise OntologyError("generated docs/COMPONENTS.md is stale; run just docs-generate")
    result = benchmark(payload)
    return {"components": len(payload["components"]), "questions": result["questions"]}


def workspace_root(root: Path = ROOT) -> Path:
    marker = root / ".git"
    if marker.is_file():
        line = marker.read_text(encoding="utf-8").strip()
        if line.startswith("gitdir:"):
            git_dir = Path(line.removeprefix("gitdir:").strip()).resolve()
            commondir = git_dir / "commondir"
            if commondir.is_file():
                common = (git_dir / commondir.read_text(encoding="utf-8").strip()).resolve()
                if common.name == "git" and common.parent.name == "Development":
                    return common.parent.parent
    return root


def normalize_path(raw: str, root: Path = ROOT) -> str:
    if raw.startswith("workspace://"):
        return raw.rstrip("/")
    candidate = Path(raw).expanduser()
    if not candidate.is_absolute():
        return candidate.as_posix().strip("./") or "."
    resolved = candidate.resolve(strict=False)
    wrapper = workspace_root(root).resolve()
    for view in (root.resolve(), wrapper / "Repository", wrapper / "Source"):
        try:
            return resolved.relative_to(view.resolve(strict=False)).as_posix() or "."
        except ValueError:
            pass
    development = wrapper / "Development"
    try:
        relative = resolved.relative_to(development)
    except ValueError:
        return resolved.as_posix()
    if relative.parts[:1] == ("worktrees",) and len(relative.parts) >= 2:
        return Path(*relative.parts[2:]).as_posix() or "."
    return "workspace://Development" + ("/" + relative.as_posix() if relative.parts else "")


def resolve_component(raw: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
    normalized = normalize_path(raw)
    components = (payload or catalog_payload())["components"]
    matches: list[tuple[int, dict[str, Any]]] = []
    for component in components:
        path = str(component["path"])
        if path == "." or normalized == path or normalized.startswith(path.rstrip("/") + "/"):
            matches.append((len(path), component))
    if not matches:
        raise OntologyError(f"path is not classified: {raw} (normalized as {normalized})")
    return max(matches, key=lambda item: item[0])[1]


def explain(raw: str, *, as_json: bool = False) -> None:
    component = resolve_component(raw)
    if as_json:
        print(canonical_json(component), end="")
        return
    print(f"PATH: {component['path']}")
    print(f"STORAGE: {component['storage_plane']}")
    print(f"CONCERN: {component['concern']}")
    print(f"PURPOSE: {component['summary']}")
    print(f"DISTRIBUTION: {component['distribution']}")
    print(f"LIFECYCLE: {component['lifecycle']}")
    print(f"REQUIRED IN REPOSITORY: {'yes' if component['repository_role'] == 'required' else 'no'}")
    print(f"SAFE TO DELETE: {'yes' if component['safe_to_delete'] else 'no'}")
    if component.get("outputs_to"):
        print("OUTPUTS TO: " + ", ".join(component["outputs_to"]))
    print("NOT: " + "; ".join(component["not"]))
    print(f"AUTHORITY: docs/repository-components.json#{component['id']}")


def orient() -> None:
    payload = catalog_payload()
    wrapper = workspace_root()
    print("OPTIMUS DEVELOPMENT ORIENTATION")
    print("VIEW: assigned development worktree")
    print(f"EDITABLE: {ROOT}")
    print(f"REPOSITORY VIEW: {wrapper / 'Repository'} (clean landed GitHub content; do not edit)")
    print(f"DEVELOPMENT STATE: {wrapper / 'Development'} (worktrees, evidence, caches, records)")
    print("PRODUCT INSTRUCTIONS: OPTIMUS_AGENTS.md")
    print("DEVELOPER INSTRUCTIONS: AGENTS.md")
    print("CURRENT STATUS: docs/current/status.md")
    print(f"COMPONENTS: {len(payload['components'])} governed entries")
    print("PATH HELP: just explain-path <path>")


def benchmark(payload: dict[str, Any] | None = None) -> dict[str, int]:
    catalog = payload or catalog_payload()
    suite = json.loads(BENCHMARK.read_text(encoding="utf-8"))
    failures: list[str] = []
    for case in suite["questions"]:
        try:
            component = resolve_component(case["path"], catalog)
        except OntologyError as error:
            failures.append(f"{case['id']}: {error}")
            continue
        if component["id"] != case["component"]:
            failures.append(f"{case['id']}: expected {case['component']}, got {component['id']}")
        for key, expected in case.get("facts", {}).items():
            if component.get(key) != expected:
                failures.append(f"{case['id']}: {key} expected {expected!r}, got {component.get(key)!r}")
        text = canonical_json(component).casefold()
        for misconception in case.get("forbidden", []):
            if misconception.casefold() in text:
                failures.append(f"{case['id']}: forbidden misconception present: {misconception}")
    if failures:
        raise OntologyError("orientation benchmark failed:\n" + "\n".join(failures))
    return {"questions": len(suite["questions"])}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("check")
    sub.add_parser("generate")
    sub.add_parser("orient")
    explain_parser = sub.add_parser("explain-path")
    explain_parser.add_argument("path")
    explain_parser.add_argument("--json", action="store_true")
    sub.add_parser("benchmark")
    args = parser.parse_args()
    try:
        if args.command == "check":
            result = check()
            print(f"REPOSITORY_ONTOLOGY_OK components={result['components']} questions={result['questions']}")
        elif args.command == "generate":
            generate()
            print("REPOSITORY_ONTOLOGY_GENERATED")
        elif args.command == "orient":
            orient()
        elif args.command == "explain-path":
            explain(args.path, as_json=args.json)
        else:
            result = benchmark()
            print(f"REPOSITORY_ORIENTATION_BENCHMARK_OK questions={result['questions']}")
        return 0
    except OntologyError as error:
        print(f"REPOSITORY_ONTOLOGY_FAILED\n{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
