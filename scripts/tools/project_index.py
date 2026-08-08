#!/usr/bin/env python3
"""Deterministic root folder index for the Optimus repository (index.md).

`index.md` maps every folder in the repository: the full directory tree with
recursive file counts, annotated with the governed component summaries
(`docs/repository-components.json`) and crate/package manifest descriptions.

The map is derived purely from the tracked file tree (gitignored state such as
`target/`, `node_modules/`, `Development/`, and `.engineering-memory/` is
deliberately absent), and the renderer embeds no timestamps and no commit
SHAs, so the output is a pure function of the tree. That determinism is what
makes staleness checkable.

Refresh contract:
  * `just orient` regenerates it at the start of every development turn
    (AGENTS.md mandates orient as step 0 of every turn);
  * `just project-index` regenerates it on demand;
  * the `project-index` gate in `scripts/verify.sh` fails the land gate when
    the committed file drifts from what the generator would emit.

Usage:
  python3 scripts/tools/project_index.py generate [--root PATH]
  python3 scripts/tools/project_index.py check [--root PATH]
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INDEX_NAME = "index.md"

# Notes for root files the component database does not classify. Keep this map
# minimal: anything with a governed summary is read from the database instead.
ROOT_FILE_NOTES = {
    "SOUL.md": "Mandatory persona for every AI agent working on Optimus in this repository.",
    "index.md": "Generated folder index (this file); refreshed every turn via `just orient`.",
    "justfile": "One memorable command per job; gate logic lives in scripts/verify.sh.",
}


class ProjectIndexError(RuntimeError):
    """The folder index is missing, contradictory, or stale."""


def git_files(root: Path) -> list[str]:
    """Tracked files plus untracked-but-not-ignored files that exist on disk.

    Mirrors `repository_ontology.git_files` so the index always describes the
    repository exactly as the other repository-knowledge tools see it.
    """
    marker = root / ".git"
    if marker.is_file():
        line = marker.read_text(encoding="utf-8").strip()
        if not line.startswith("gitdir:"):
            raise ProjectIndexError("worktree Git pointer is malformed")
        git_dir = Path(line.removeprefix("gitdir:").strip())
    elif marker.is_dir():
        git_dir = marker
    else:
        raise ProjectIndexError("cannot resolve repository Git metadata")
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
        raise ProjectIndexError(result.stderr.decode(errors="replace").strip())
    return sorted(
        relative for item in result.stdout.split(b"\0") if item
        for relative in (item.decode(),) if (root / relative).exists()
    )


def component_summaries(root: Path) -> dict[str, str]:
    """path -> governed summary from the component database (lenient load)."""
    database = root / "docs" / "repository-components.json"
    try:
        payload = json.loads(database.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    summaries: dict[str, str] = {}
    for component in payload.get("components", []):
        path = str(component.get("path", ""))
        summary = str(component.get("summary", "")).strip()
        if path and summary:
            summaries[path] = summary
    return summaries


def manifest_descriptions(root: Path) -> dict[str, str]:
    """dir -> description from Cargo and npm manifests (fallback annotations)."""
    descriptions: dict[str, str] = {}
    manifests = sorted(root.glob("crates/*/Cargo.toml")) + sorted(root.glob("apps/*/Cargo.toml"))
    for manifest in manifests:
        try:
            parsed = tomllib.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError):
            continue
        description = parsed.get("package", {}).get("description")
        if description:
            descriptions[manifest.parent.relative_to(root).as_posix()] = str(description)
    for manifest in sorted(root.glob("apps/*/package.json")):
        relative = manifest.parent.relative_to(root).as_posix()
        if relative in descriptions:
            continue
        try:
            payload = json.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        description = payload.get("description")
        if description:
            descriptions[relative] = str(description)
    return descriptions


def annotations(root: Path) -> dict[str, str]:
    notes = manifest_descriptions(root)
    notes.update(component_summaries(root))
    return notes


def collect(
    files: list[str],
) -> tuple[dict[str, list[str]], dict[str, list[str]], list[str]]:
    """Return (dir -> direct subdirs, dir -> direct files, every directory).

    Every file-path prefix is a folder, including leaf directories that hold
    only files; "" is the root.
    """
    subdir_sets: dict[str, set[str]] = {}
    file_lists: dict[str, list[str]] = {}
    all_dirs: set[str] = set()
    for relative in files:
        parts = relative.split("/")
        if len(parts) == 1:
            file_lists.setdefault("", []).append(relative)
            continue
        for depth in range(1, len(parts)):
            all_dirs.add("/".join(parts[:depth]))
            parent = "/".join(parts[: depth - 1])
            subdir_sets.setdefault(parent, set()).add("/".join(parts[:depth]))
        file_lists.setdefault("/".join(parts[:-1]), []).append(relative)
    return (
        {parent: sorted(children) for parent, children in subdir_sets.items()},
        {parent: sorted(children) for parent, children in file_lists.items()},
        sorted(all_dirs),
    )


def recursive_counts(
    subdirs: dict[str, list[str]], file_lists: dict[str, list[str]]
) -> dict[str, int]:
    counts: dict[str, int] = {}

    def count(directory: str) -> int:
        if directory in counts:
            return counts[directory]
        total = len(file_lists.get(directory, []))
        for child in subdirs.get(directory, []):
            total += count(child)
        counts[directory] = total
        return total

    count("")
    return counts


def render(root: Path, extra_files: frozenset[str] = frozenset()) -> tuple[str, int, int]:
    """Pure function of the tree: (index content, folder count, file count)."""
    files = sorted(set(git_files(root)) | set(extra_files))
    subdirs, file_lists, folders = collect(files)
    counts = recursive_counts(subdirs, file_lists)
    notes = annotations(root)

    lines: list[str] = [
        "<!-- Generated by scripts/tools/project_index.py; do not edit manually. -->",
        "<!-- Refresh: `just project-index`. Also refreshed automatically by `just orient` -->",
        "<!-- at the start of every development turn, and enforced at land time by the -->",
        "<!-- `project-index` gate in scripts/verify.sh. -->",
        "",
        "# Optimus Agent — Folder Index",
        "",
        f"Complete map of every folder in this repository: **{len(folders)} folders**,"
        f" **{len(files)} files**. Generated from the tracked file tree, so git-excluded",
        "state (`target/`, `node_modules/`, `Development/`, `.engineering-memory/`, `.optimus/`,",
        "`.hermes/`, `.steploop/`) is deliberately absent.",
        "",
        "## Folder tree",
        "",
        "```text",
        "Optimus Agent/",
    ]

    def annotate(path: str) -> str:
        note = notes.get(path) or ROOT_FILE_NOTES.get(path)
        return f" — {note}" if note else ""

    def render_directory(directory: str, prefix: str) -> None:
        children: list[tuple[str, bool]] = [
            (child, True) for child in subdirs.get(directory, [])
        ] + [(name, False) for name in file_lists.get(directory, [])]
        for position, (child, is_dir) in enumerate(children):
            last = position == len(children) - 1
            connector = "└── " if last else "├── "
            name = child.rsplit("/", 1)[-1]
            if is_dir:
                label = f"{name}/ ({counts[child]} files){annotate(child)}"
            else:
                label = f"{name}{annotate(child)}"
            lines.append(f"{prefix}{connector}{label}")
            if is_dir:
                render_directory(child, prefix + ("    " if last else "│   "))

    render_directory("", "")
    lines.append("```")
    lines.append("")

    lines.extend([
        "## Top-level roots",
        "",
        "| Root | Folders | Files | Purpose |",
        "|---|---|---|---|",
    ])
    for child in subdirs.get("", []):
        name = child.rsplit("/", 1)[-1]
        folder_total = sum(
            1 for folder in folders if folder == child or folder.startswith(child + "/")
        )
        lines.append(f"| `{name}/` | {folder_total} | {counts[child]} | {notes.get(child, '')} |")
    lines.append("")
    lines.extend([
        "Every agent working in this repository must adopt [SOUL.md](SOUL.md) and start",
        "each development turn with `just orient`, which regenerates this index.",
        "",
    ])
    return "\n".join(lines), len(folders), len(files)


def expected_content(root: Path) -> tuple[str, int, int]:
    """What index.md must contain right now.

    When the file does not exist yet, the render must still list it: the
    generator creates it, so the stable output includes it from the start.
    """
    if (root / INDEX_NAME).is_file() or INDEX_NAME in git_files(root):
        return render(root)
    return render(root, extra_files=frozenset({INDEX_NAME}))


def generate(root: Path) -> str:
    content, _, _ = expected_content(root)
    target = root / INDEX_NAME
    if not target.is_file() or target.read_text(encoding="utf-8") != content:
        target.write_text(content, encoding="utf-8")
    return content


def check(root: Path) -> dict[str, int]:
    target = root / INDEX_NAME
    if not target.is_file():
        raise ProjectIndexError("index.md is missing; run: just project-index")
    content, folders, files = expected_content(root)
    if target.read_text(encoding="utf-8") != content:
        raise ProjectIndexError("index.md is stale; run: just project-index")
    return {"folders": folders, "files": files}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("generate", "check"):
        command = sub.add_parser(name)
        command.add_argument("--root", default=str(ROOT))
    args = parser.parse_args()
    root = Path(args.root).resolve()
    try:
        if args.command == "generate":
            generate(root)
            print(f"PROJECT_INDEX_GENERATED {root / INDEX_NAME}")
        else:
            result = check(root)
            print(f"PROJECT_INDEX_OK folders={result['folders']} files={result['files']}")
        return 0
    except ProjectIndexError as error:
        print(f"PROJECT_INDEX_FAILED\n{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
