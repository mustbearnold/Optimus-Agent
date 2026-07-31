#!/usr/bin/env python3
"""Temporal project graph and actionable local-state observations for Optimus."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import repository_ontology


ROOT = Path(__file__).resolve().parents[1]
GRAPH = ROOT / ".engineering-memory" / "temporal-project-graph.json"
SCHEMA_VERSION = 1


class KnowledgeError(RuntimeError):
    """Temporal project knowledge could not be derived without guessing."""


def environment() -> dict[str, str]:
    return {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}


def git(*args: str, root: Path = ROOT) -> str:
    marker = root / ".git"
    if marker.is_file():
        line = marker.read_text(encoding="utf-8").strip()
        if not line.startswith("gitdir:"):
            raise KnowledgeError("worktree Git pointer is malformed")
        git_dir = Path(line.removeprefix("gitdir:").strip())
    elif marker.is_dir():
        git_dir = marker
    else:
        raise KnowledgeError("repository Git metadata is missing")
    result = subprocess.run(
        ["git", f"--git-dir={git_dir}", f"--work-tree={root}", *args],
        cwd=root, env=environment(), text=True, capture_output=True, check=False,
    )
    if result.returncode != 0:
        raise KnowledgeError(result.stderr.strip() or result.stdout.strip())
    return result.stdout


def canonical(payload: Any) -> str:
    return json.dumps(payload, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def parse_history(root: Path = ROOT) -> tuple[list[dict[str, Any]], dict[str, list[dict[str, str]]]]:
    raw = git(
        "log", "--reverse", "--date=iso-strict",
        "--format=@@@%H%x09%P%x09%cI%x09%s", "--name-status", "--no-renames", "HEAD",
        root=root,
    )
    commits: list[dict[str, Any]] = []
    histories: dict[str, list[dict[str, str]]] = {}
    current: dict[str, Any] | None = None
    for line in raw.splitlines():
        if line.startswith("@@@"):
            parts = line[3:].split("\t", 3)
            if len(parts) != 4:
                raise KnowledgeError("Git history record is malformed")
            current = {
                "id": f"commit:{parts[0]}", "sha": parts[0],
                "parents": parts[1].split() if parts[1] else [],
                "committed_at": parts[2], "subject": parts[3], "touch_count": 0,
            }
            commits.append(current)
            continue
        if not line or current is None:
            continue
        fields = line.split("\t")
        if len(fields) < 2:
            continue
        status, path = fields[0], fields[-1]
        event = {
            "commit": current["sha"], "at": current["committed_at"],
            "status": status[0],
        }
        histories.setdefault(path, []).append(event)
        current["touch_count"] += 1
    return commits, histories


def candidate_files(root: Path = ROOT) -> list[str]:
    return sorted(
        path for path in git(
            "ls-files", "-z", "--cached", "--others", "--exclude-standard", root=root
        ).split("\0")
        if path and (root / path).is_file()
    )


def working_states(root: Path = ROOT) -> dict[str, str]:
    raw = git("status", "--porcelain=v1", "-z", "--untracked-files=all", root=root)
    states: dict[str, str] = {}
    for item in raw.split("\0"):
        if len(item) < 4:
            continue
        states[item[3:]] = item[:2]
    return states


def component_for(path: str, catalog: dict[str, Any]) -> dict[str, Any]:
    return repository_ontology.resolve_component(path, catalog)


def build_graph(root: Path = ROOT) -> dict[str, Any]:
    catalog = repository_ontology.catalog_payload()
    commits, histories = parse_history(root)
    current = candidate_files(root)
    current_set = set(current)
    states = working_states(root)
    files: list[dict[str, Any]] = []
    all_paths = sorted(current_set | set(histories))
    identity = hashlib.sha256()
    for relative in all_paths:
        path = root / relative
        exists = relative in current_set
        history = histories.get(relative, [])
        component = component_for(relative, catalog)
        content_sha = hashlib.sha256(path.read_bytes()).hexdigest() if exists else None
        if exists:
            identity.update(relative.encode())
            identity.update(b"\0")
            identity.update(bytes.fromhex(content_sha or ""))
        files.append({
            "id": f"file:{relative}", "path": relative, "exists": exists,
            "bytes": path.stat().st_size if exists else 0,
            "content_sha256": content_sha,
            "component": component["id"],
            "lifecycle": component["lifecycle"],
            "distribution": component["distribution"],
            "working_state": states.get(relative, "clean" if exists else "deleted"),
            "created_at": history[0]["at"] if history else None,
            "last_committed_change_at": history[-1]["at"] if history else None,
            "last_commit": history[-1]["commit"] if history else None,
            "change_count": len(history), "events": history,
        })
    components = []
    for item in catalog["components"]:
        component_files = [node for node in files if node["component"] == item["id"] and node["exists"]]
        last = max(
            (node["last_committed_change_at"] for node in component_files if node["last_committed_change_at"]),
            default=None,
        )
        components.append({
            "id": f"component:{item['id']}", "component_id": item["id"],
            "path": item["path"], "lifecycle": item["lifecycle"],
            "retention": item["retention"], "safe_to_delete": item["safe_to_delete"],
            "review_by": item.get("review_by"), "current_file_count": len(component_files),
            "last_committed_change_at": last,
            "parent": item.get("parent"), "paired_with": item.get("paired_with", []),
            "lifecycle_events": item.get("lifecycle_events", []),
        })
    head = git("rev-parse", "HEAD", root=root).strip()
    return {
        "schema_version": SCHEMA_VERSION,
        "identity": identity.hexdigest(),
        "head": head,
        "scope": {
            "repository_history": "HEAD ancestry",
            "current_files": "tracked plus non-ignored working files",
            "local_state": "separate observation snapshots",
        },
        "counts": {
            "commits": len(commits), "components": len(components),
            "current_files": sum(1 for item in files if item["exists"]),
            "historical_files": sum(1 for item in files if not item["exists"]),
            "file_events": sum(len(item["events"]) for item in files),
        },
        "components": components, "files": files, "commits": commits,
    }


def write_graph(root: Path = ROOT) -> dict[str, Any]:
    graph = build_graph(root)
    GRAPH.parent.mkdir(parents=True, exist_ok=True)
    GRAPH.write_text(canonical(graph), encoding="utf-8")
    return graph


def parse_time(value: str | None) -> dt.datetime | None:
    if not value:
        return None
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def age_days(value: str | None, now: dt.datetime) -> int | None:
    parsed = parse_time(value)
    return max(0, (now - parsed).days) if parsed else None


def disk_usage(path: Path) -> int:
    if not path.exists():
        return 0
    result = subprocess.run(
        ["du", "-sb", "--", str(path)], text=True, capture_output=True, check=False
    )
    if result.returncode != 0:
        raise KnowledgeError(f"cannot measure {path}: {result.stderr.strip()}")
    return int(result.stdout.split()[0])


def human_bytes(size: int) -> str:
    value = float(size)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if value < 1024 or unit == "TiB":
            return f"{value:.1f} {unit}"
        value /= 1024
    return f"{size} B"


def workspace_observation(root: Path = ROOT) -> dict[str, Any]:
    wrapper = repository_ontology.workspace_root(root)
    development = wrapper / "Development"
    catalog = repository_ontology.catalog_payload()
    areas: list[dict[str, Any]] = []
    known: set[str] = set()
    if development.is_dir():
        for path in sorted(development.iterdir(), key=lambda item: item.name.casefold()):
            uri = f"workspace://Development/{path.name}"
            component = component_for(uri, catalog)
            known.add(path.name)
            areas.append({
                "path": uri, "component": component["id"],
                "bytes": disk_usage(path), "safe_to_delete": component["safe_to_delete"],
                "retention": component["retention"],
                "observed_mtime": dt.datetime.fromtimestamp(
                    path.stat().st_mtime, tz=dt.timezone.utc
                ).isoformat(timespec="seconds"),
            })
    generated: list[dict[str, Any]] = []
    for component in catalog["components"]:
        path_text = str(component["path"])
        if (
            component["storage_plane"] != "development"
            or not component["safe_to_delete"]
            or path_text.startswith("workspace://")
        ):
            continue
        path = root / path_text
        if path.exists():
            generated.append({
                "path": path_text, "component": component["id"],
                "bytes": disk_usage(path), "safe_to_delete": True,
                "activity": "active-worktree-cache",
            })

    # These are closed generated-output conventions, not age guesses. Cargo
    # target directories are reconstructible from the named source worktree;
    # the archived root target is equally reconstructible from its preserved
    # snapshot. Nothing else beneath tmp/ or Archive/ inherits that authority.
    inactive_generated: list[dict[str, Any]] = []
    temporary = development / "tmp"
    if temporary.is_dir():
        for path in sorted(temporary.glob("cargo-target-*")):
            if path.is_dir() and ((path / ".rustc_info.json").exists() or (path / "debug").is_dir()):
                inactive_generated.append({
                    "path": f"workspace://Development/tmp/{path.name}",
                    "component": "development-temporary-evidence",
                    "bytes": disk_usage(path), "safe_to_delete": True,
                    "activity": "inactive-generated-cache",
                    "proof": "Cargo target layout under the dedicated temporary area",
                })
    archived_target = development / "Archive" / "stale-root-snapshot" / "target"
    if archived_target.is_dir() and (
        (archived_target / ".rustc_info.json").exists() or (archived_target / "debug").is_dir()
    ):
        inactive_generated.append({
            "path": "workspace://Development/Archive/stale-root-snapshot/target",
            "component": "development-archive", "bytes": disk_usage(archived_target),
            "safe_to_delete": True, "activity": "inactive-generated-cache",
            "proof": "Cargo target layout inside a preserved source snapshot",
        })

    registered = {
        str(Path(line.removeprefix("worktree ")).resolve())
        for line in git("worktree", "list", "--porcelain", root=root).splitlines()
        if line.startswith("worktree ")
    }
    worktrees: list[dict[str, Any]] = []
    worktree_root = development / "worktrees"
    if worktree_root.is_dir():
        for path in sorted(worktree_root.iterdir(), key=lambda item: item.name.casefold()):
            if not path.is_dir():
                continue
            registered_here = str(path.resolve()) in registered
            worktrees.append({
                "path": f"workspace://Development/worktrees/{path.name}",
                "bytes": disk_usage(path), "registered": registered_here,
                "state": "registered" if registered_here else "physical-orphan",
                "remediation": None if registered_here else "just worktree-retirement-plan",
            })
    return {
        "observed_at": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "wrapper": str(wrapper), "areas": areas, "generated_paths": generated,
        "inactive_generated_paths": inactive_generated, "worktrees": worktrees,
        "total_development_bytes": sum(item["bytes"] for item in areas),
    }


def cleanup_report(
    graph: dict[str, Any], observation: dict[str, Any], *, now: dt.datetime | None = None
) -> dict[str, Any]:
    current = now or dt.datetime.now(dt.timezone.utc)
    active = [item for item in observation["generated_paths"] if item["bytes"] > 0]
    recommended = [
        item for item in observation.get("inactive_generated_paths", []) if item["bytes"] > 0
    ]
    safe = list(active) + list(recommended)
    safe.extend(
        item for item in observation["areas"] if item["safe_to_delete"] and item["bytes"] > 0
    )
    review: list[dict[str, Any]] = []
    stable: list[dict[str, Any]] = []
    for component in graph["components"]:
        age = age_days(component["last_committed_change_at"], current)
        deadline = component.get("review_by")
        days_to_review = None
        if deadline:
            review_date = dt.date.fromisoformat(deadline)
            days_to_review = (review_date - current.date()).days
        if days_to_review is not None and days_to_review <= 30:
            review.append({
                "component": component["component_id"], "reason": "lifecycle review due",
                "days_to_review": days_to_review, "last_change_age_days": age,
            })
        elif age is not None and age >= 90 and component["lifecycle"] in {
            "rollback-only", "incubating", "historical",
        }:
            review.append({
                "component": component["component_id"], "reason": "inactive non-primary component",
                "last_change_age_days": age,
            })
        elif age is not None and age >= 90:
            stable.append({"component": component["component_id"], "last_change_age_days": age})
    return {
        "safe_generated_cleanup": sorted(safe, key=lambda item: -item["bytes"]),
        "recommended_cleanup": sorted(recommended, key=lambda item: -item["bytes"]),
        "regenerable_active_caches": sorted(active, key=lambda item: -item["bytes"]),
        "orphaned_worktrees": sorted(
            (item for item in observation.get("worktrees", []) if not item["registered"]),
            key=lambda item: -item["bytes"],
        ),
        "decision_required": sorted(review, key=lambda item: item["component"]),
        "old_but_not_cleanup": sorted(stable, key=lambda item: -item["last_change_age_days"]),
        "rule": "age alone never authorizes deletion",
    }


def current_bundle(root: Path = ROOT) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    graph = build_graph(root)
    observation = workspace_observation(root)
    return graph, observation, cleanup_report(graph, observation)


def print_status(root: Path = ROOT) -> None:
    graph, observation, cleanup = current_bundle(root)
    print("OPTIMUS TEMPORAL PROJECT STATUS")
    print(f"HEAD: {graph['head']}")
    print(
        "HISTORY: "
        f"{graph['counts']['commits']} commits, {graph['counts']['current_files']} current files, "
        f"{graph['counts']['historical_files']} retired paths, {graph['counts']['file_events']} events"
    )
    print(f"COMPONENTS: {graph['counts']['components']}")
    print(f"DEVELOPMENT: {human_bytes(observation['total_development_bytes'])}")
    for area in sorted(observation["areas"], key=lambda item: -item["bytes"]):
        print(f"  {human_bytes(area['bytes']):>10}  {area['path']}  retention={area['retention']}")
    print(f"RECOMMENDED GENERATED CLEANUP: {len(cleanup['recommended_cleanup'])}")
    for item in cleanup["recommended_cleanup"]:
        print(f"  {human_bytes(item['bytes']):>10}  {item['path']}")
    print(f"PHYSICAL ORPHAN WORKTREES: {len(cleanup['orphaned_worktrees'])}")
    for item in cleanup["orphaned_worktrees"]:
        print(f"  {human_bytes(item['bytes']):>10}  {item['path']}")
    print(f"DECISIONS DUE: {len(cleanup['decision_required'])}")
    for item in cleanup["decision_required"]:
        print(f"  {item['component']}: {item['reason']}")
    print("NOTE: old but primary/stable files are never deletion candidates from age alone.")


def print_cleanup(root: Path = ROOT) -> None:
    graph, observation, report = current_bundle(root)
    del graph, observation
    print("RECOMMENDED NOW — INACTIVE GENERATED OUTPUT")
    if not report["recommended_cleanup"]:
        print("  none")
    for item in report["recommended_cleanup"]:
        print(
            f"  {human_bytes(item['bytes']):>10}  {item['path']}  "
            f"({item['proof']})"
        )
    print("PHYSICAL ORPHAN WORKTREES — USE MANAGED RETIREMENT")
    if not report["orphaned_worktrees"]:
        print("  none")
    for item in report["orphaned_worktrees"]:
        print(f"  {human_bytes(item['bytes']):>10}  {item['path']}")
    print("REGENERABLE BUT ACTIVE — RETAIN UNLESS DISK PRESSURE")
    if not report["regenerable_active_caches"]:
        print("  none")
    for item in report["regenerable_active_caches"]:
        print(f"  {human_bytes(item['bytes']):>10}  {item['path']}")
    print("DECISION REQUIRED")
    if not report["decision_required"]:
        print("  none")
    for item in report["decision_required"]:
        print(f"  {item['component']}: {item['reason']}")
    print("Old age without lifecycle or retention evidence is not a cleanup rule.")


def print_history(path: str, root: Path = ROOT) -> None:
    graph = build_graph(root)
    normalized = repository_ontology.normalize_path(path, root)
    nodes = [
        item for item in graph["files"]
        if item["path"] == normalized or item["path"].startswith(normalized.rstrip("/") + "/")
    ]
    if not nodes:
        raise KnowledgeError(f"no current or historical path matches {path}")
    print(f"PATH HISTORY: {normalized}")
    for node in sorted(nodes, key=lambda item: item["path"]):
        print(
            f"{node['path']} exists={str(node['exists']).lower()} component={node['component']} "
            f"changes={node['change_count']} last={node['last_committed_change_at'] or 'uncommitted'}"
        )
        for event in node["events"][-10:]:
            print(f"  {event['at']} {event['status']} {event['commit'][:12]}")


def snapshot(root: Path = ROOT) -> Path:
    graph, observation, cleanup = current_bundle(root)
    wrapper = repository_ontology.workspace_root(root)
    directory = wrapper / "Development" / "land" / "project-knowledge" / "snapshots"
    directory.mkdir(parents=True, exist_ok=True)
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    path = directory / f"{stamp}-{graph['head'][:12]}.json"
    payload = {
        "schema_version": SCHEMA_VERSION, "graph_identity": graph["identity"],
        "head": graph["head"], "counts": graph["counts"],
        "observation": observation, "cleanup": cleanup,
    }
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(canonical(payload))
    return path


def check(root: Path = ROOT) -> dict[str, int]:
    graph = build_graph(root)
    if graph["counts"]["current_files"] != len(candidate_files(root)):
        raise KnowledgeError("temporal graph omitted current repository files")
    if any(not item["component"] for item in graph["files"]):
        raise KnowledgeError("temporal graph contains an unclassified file")
    if not any(not item["exists"] for item in graph["files"]):
        raise KnowledgeError("temporal graph failed to preserve retired paths")
    return graph["counts"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("generate")
    sub.add_parser("check")
    sub.add_parser("status")
    sub.add_parser("cleanup")
    history_parser = sub.add_parser("history")
    history_parser.add_argument("path")
    sub.add_parser("snapshot")
    args = parser.parse_args()
    try:
        if args.command == "generate":
            graph = write_graph()
            print(f"TEMPORAL_PROJECT_GRAPH_GENERATED identity={graph['identity']}")
        elif args.command == "check":
            counts = check()
            print(
                "TEMPORAL_PROJECT_GRAPH_OK "
                f"commits={counts['commits']} files={counts['current_files']} "
                f"historical={counts['historical_files']} events={counts['file_events']}"
            )
        elif args.command == "status":
            print_status()
        elif args.command == "cleanup":
            print_cleanup()
        elif args.command == "history":
            print_history(args.path)
        else:
            print(f"PROJECT_KNOWLEDGE_SNAPSHOT {snapshot()}")
        return 0
    except (KnowledgeError, repository_ontology.OntologyError) as error:
        print(f"TEMPORAL_PROJECT_KNOWLEDGE_FAILED\n{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
