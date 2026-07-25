#!/usr/bin/env python3
"""Sync GitHub labels from .github/labels.yml (create/update; optional prune).

Usage:
  python3 scripts/sync-github-labels.py              # create/update only
  python3 scripts/sync-github-labels.py --prune      # also delete labels not in YAML
  python3 scripts/sync-github-labels.py --dry-run

Requires: gh auth login, repo write access.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
import urllib.parse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LABELS_FILE = ROOT / ".github" / "labels.yml"


def run(cmd: list[str], input_data: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, input=input_data, capture_output=True, text=True)


def repo_slug() -> str:
    r = run(["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"])
    if r.returncode != 0:
        raise SystemExit(f"gh repo view failed: {r.stderr}")
    return r.stdout.strip()


def parse_labels_yaml(path: Path) -> list[dict[str, str]]:
    """Minimal YAML subset parser for our labels file (no PyYAML dependency)."""
    text = path.read_text(encoding="utf-8")
    labels: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].rstrip()
        if not line.strip():
            continue
        m = re.match(r"^\s*-\s*name:\s*(.+)$", line)
        if m:
            if current:
                labels.append(current)
            name = m.group(1).strip().strip("\"'")
            current = {"name": name}
            continue
        if current is None:
            continue
        m = re.match(r"^\s*color:\s*[\"']?([0-9a-fA-F]{6})[\"']?\s*$", line)
        if m:
            current["color"] = m.group(1).lower()
            continue
        m = re.match(r"^\s*description:\s*(.+)$", line)
        if m:
            desc = m.group(1).strip()
            if (desc.startswith('"') and desc.endswith('"')) or (
                desc.startswith("'") and desc.endswith("'")
            ):
                desc = desc[1:-1]
            current["description"] = desc
            continue
    if current:
        labels.append(current)
    for row in labels:
        if not all(k in row for k in ("name", "color", "description")):
            raise SystemExit(f"incomplete label entry: {row}")
    return labels


def list_remote_labels(repo: str) -> set[str]:
    r = run(
        [
            "gh",
            "api",
            "--paginate",
            f"repos/{repo}/labels",
            "--jq",
            ".[].name",
        ]
    )
    if r.returncode != 0:
        raise SystemExit(f"list labels failed: {r.stderr}")
    return {line.strip() for line in r.stdout.splitlines() if line.strip()}


def upsert(repo: str, label: dict[str, str], exists: bool, dry_run: bool) -> str:
    body = json.dumps(
        {
            "name": label["name"],
            "color": label["color"],
            "description": label["description"],
        }
    )
    if dry_run:
        return "would-update" if exists else "would-create"
    if exists:
        encoded = urllib.parse.quote(label["name"], safe="")
        r = run(
            [
                "gh",
                "api",
                "--method",
                "PATCH",
                f"repos/{repo}/labels/{encoded}",
                "--input",
                "-",
            ],
            input_data=body,
        )
        if r.returncode != 0:
            raise SystemExit(f"PATCH {label['name']} failed: {r.stderr}")
        return "updated"
    r = run(
        [
            "gh",
            "api",
            "--method",
            "POST",
            f"repos/{repo}/labels",
            "--input",
            "-",
        ],
        input_data=body,
    )
    if r.returncode != 0:
        raise SystemExit(f"POST {label['name']} failed: {r.stderr}")
    return "created"


def delete_label(repo: str, name: str, dry_run: bool) -> None:
    if dry_run:
        print(f"  would-delete {name}")
        return
    encoded = urllib.parse.quote(name, safe="")
    r = run(["gh", "api", "--method", "DELETE", f"repos/{repo}/labels/{encoded}"])
    if r.returncode != 0:
        raise SystemExit(f"DELETE {name} failed: {r.stderr}")
    print(f"  deleted {name}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--prune",
        action="store_true",
        help="Delete remote labels not present in .github/labels.yml",
    )
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    if not LABELS_FILE.is_file():
        raise SystemExit(f"missing {LABELS_FILE}")

    desired = parse_labels_yaml(LABELS_FILE)
    desired_names = {row["name"] for row in desired}
    repo = repo_slug()
    remote = list_remote_labels(repo)

    print(f"repo={repo} desired={len(desired)} remote={len(remote)} dry_run={args.dry_run}")
    counts = {"created": 0, "updated": 0, "would-create": 0, "would-update": 0}
    for label in desired:
        action = upsert(repo, label, label["name"] in remote, args.dry_run)
        counts[action] = counts.get(action, 0) + 1
        print(f"  {action:12} {label['name']}")
        if not args.dry_run:
            time.sleep(0.03)

    if args.prune:
        extras = sorted(remote - desired_names)
        print(f"prune candidates={len(extras)}")
        for name in extras:
            delete_label(repo, name, args.dry_run)

    print("summary", counts)
    return 0


if __name__ == "__main__":
    sys.exit(main())
