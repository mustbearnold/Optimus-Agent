#!/usr/bin/env python3
"""Create and provision assigned worktrees that are ready to land from.

Two gaps kept costing land attempts (observed 2026-08-01): `git worktree add`
against the bare store never writes the per-worktree `config.worktree`, so
plain git in the new checkout fails with "must be run in a work tree"; and a
fresh checkout has no `node_modules`, so the land gate refuses on forbidden
UI-suite skips. `new` creates a worktree with both handled; `ready` repairs
and reports an existing one.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

APP_DIRS = ("apps/optimus-ui", "apps/optimus-electron", "apps/optimus-desktop")
NAME_PATTERN = re.compile(r"^[a-z0-9][a-z0-9-]{1,63}$")


class Refusal(RuntimeError):
    """Provisioning cannot prove the requested state safely."""


def run(common: Path, cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", f"--git-dir={common}", *args],
        cwd=cwd,
        env={key: value for key, value in os.environ.items() if not key.startswith("GIT_")},
        text=True,
        capture_output=True,
        check=False,
    )


def discover(caller: Path) -> tuple[Path, Path, Path]:
    caller = caller.resolve()
    marker = caller / ".git"
    if not marker.is_file():
        raise Refusal("run from an assigned linked worktree")
    git_dir = Path(marker.read_text().strip().removeprefix("gitdir: ").strip()).resolve()
    common = (git_dir / (git_dir / "commondir").read_text().strip()).resolve()
    if common.name != "git" or common.parent.name != "Development":
        raise Refusal("workspace is not the managed Repository/Development layout")
    root = common.parent.parent.resolve()
    try:
        caller.relative_to((root / "Development" / "worktrees").resolve())
    except ValueError as error:
        raise Refusal("caller is outside the assigned-worktree root") from error
    return root, common, caller


def metadata_dir(checkout: Path) -> Path:
    marker = checkout / ".git"
    if not marker.is_file():
        raise Refusal(f"not a linked worktree: {checkout}")
    return Path(marker.read_text().strip().removeprefix("gitdir: ").strip()).resolve()


def write_worktree_config(checkout: Path) -> bool:
    """Ensure the per-worktree config interactive git needs. True if written.

    The bare store sets `core.bare = true` with `extensions.worktreeConfig`,
    and `git worktree add` does not create the per-worktree override — every
    checkout needs `bare = false` written by hand or plain `git status`
    refuses while the managed tooling (which passes --work-tree explicitly)
    keeps working, which is exactly how the gap hides.
    """
    config = metadata_dir(checkout) / "config.worktree"
    wanted = f"[core]\n\tbare = false\n\tworktree = {checkout}\n"
    if config.is_file() and "bare = false" in config.read_text(encoding="utf-8"):
        return False
    config.write_text(wanted, encoding="utf-8")
    return True


def ensure_node_modules(root_checkout: Path, rows: list[tuple[str, str]]) -> bool:
    ok = True
    for app in APP_DIRS:
        app_dir = root_checkout / app
        if not (app_dir / "package.json").is_file():
            rows.append((app, "skip (no package.json)"))
            continue
        if (app_dir / "node_modules").is_dir():
            rows.append((app, "ok"))
            continue
        installed = subprocess.run(
            ["npm", "ci", "--no-audit", "--no-fund"],
            cwd=app_dir,
            text=True,
            capture_output=True,
            check=False,
        )
        if installed.returncode == 0 and (app_dir / "node_modules").is_dir():
            rows.append((app, "installed"))
        else:
            detail = (installed.stderr or installed.stdout).strip().splitlines()
            rows.append((app, f"FAILED ({detail[-1] if detail else 'npm ci'})"))
            ok = False
    return ok


def system_rows(root: Path) -> list[tuple[str, str]]:
    rows: list[tuple[str, str]] = []
    rows.append(("tmux", "ok" if shutil.which("tmux") else "missing (pacman -S tmux; land refuses skips)"))
    rows.append(("clippy", "ok" if shutil.which("cargo-clippy") else "missing (rustup component add clippy)"))
    display = os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY") or shutil.which("xvfb-run")
    rows.append(("display", "ok" if display else "missing (no display and no xvfb-run)"))
    browsers = [
        root / "Development" / "tools" / "playwright-browsers",
        Path.home() / ".cache" / "ms-playwright",
    ]
    found = any(path.is_dir() and any(path.glob("chromium*")) for path in browsers)
    rows.append(("chromium", "ok" if found else "missing (npx playwright install chromium)"))
    return rows


def report(title: str, rows: list[tuple[str, str]]) -> bool:
    print(f"WORKTREE PROVISION — {title}")
    ok = True
    for name, status in rows:
        print(f"  {name:<24}{status}")
        if status.startswith(("FAILED", "missing")):
            ok = False
    print(f"  {'result':<24}{'ready' if ok else 'NOT READY'}")
    return ok


def ready(caller: Path, include_system: bool = True) -> bool:
    root, _, checkout = discover(caller)
    rows: list[tuple[str, str]] = []
    rows.append(("config.worktree", "repaired" if write_worktree_config(checkout) else "ok"))
    deps_ok = ensure_node_modules(checkout, rows)
    if include_system:
        rows.extend(system_rows(root))
    table_ok = report(checkout.name, rows)
    return deps_ok and table_ok


def new(caller: Path, name: str, include_system: bool = True) -> bool:
    if not NAME_PATTERN.fullmatch(name):
        raise Refusal("worktree name must be a lowercase slug (a-z, 0-9, hyphen)")
    root, common, _ = discover(caller)
    target = (root / "Development" / "worktrees" / name).resolve()
    if target.exists():
        raise Refusal(f"worktree path already exists: {target}")
    branch = f"claude/{name}"
    if run(common, root, "rev-parse", "--verify", "--quiet", f"refs/heads/{branch}").returncode == 0:
        raise Refusal(f"branch already exists: {branch}")
    added = run(common, root, "worktree", "add", str(target), "-b", branch, "main")
    if added.returncode != 0:
        raise Refusal(f"git worktree add failed: {added.stderr.strip()}")
    write_worktree_config(target)
    status = subprocess.run(
        ["git", "status", "--short"],
        cwd=target,
        text=True,
        capture_output=True,
        check=False,
    )
    if status.returncode != 0:
        raise Refusal(f"new worktree refuses plain git: {status.stderr.strip()}")
    return ready(target, include_system=include_system)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    new_parser = sub.add_parser("new")
    new_parser.add_argument("name")
    sub.add_parser("ready")
    args = parser.parse_args()
    try:
        if args.command == "new":
            complete = new(Path.cwd(), args.name)
        else:
            complete = ready(Path.cwd())
        return 0 if complete else 1
    except (OSError, Refusal) as error:
        print(f"managed worktree provisioning refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
