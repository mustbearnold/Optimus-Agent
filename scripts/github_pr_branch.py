#!/usr/bin/env python3
"""Keep local branch names aligned with GitHub PR numbers: pr/<N>-<slug>.

Remote PR head stays on a stable name (usually the original push branch). Locally
we rename to pr/<N>-… and track the remote head — GitHub closes PRs if the head
branch is renamed/deleted, so we never rename the remote head by default.

Commands:
  open    Push, create PR, rename **local** branch to pr/<N>-<slug>
  adopt   Rename **local** branch to pr/<N>-<slug> for an existing open PR
  check   Exit 0 if local branch is pr/<N>-… for the open PR

Examples:
  python3 scripts/github_pr_branch.py open --title "✨ feat(cli): …" --slug feat-cli-foo \\
      --label "✨ type:feat" --label "💻 area:cli" --label "▪️ size:S"

  python3 scripts/github_pr_branch.py adopt
  python3 scripts/github_pr_branch.py check
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _subprocess_env() -> dict[str, str]:
    """Force plain gh/git output so JSON parsing works under agent TTY wrappers."""
    env = os.environ.copy()
    env.setdefault("NO_COLOR", "1")
    env.setdefault("CLICOLOR", "0")
    env["GH_FORCE_TTY"] = "0"
    return env


def run(
    cmd: list[str],
    *,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    r = subprocess.run(
        cmd,
        check=False,
        capture_output=True,
        text=True,
        env=_subprocess_env(),
    )
    if check and r.returncode != 0:
        msg = (r.stderr or r.stdout or "").strip() or f"exit {r.returncode}"
        raise SystemExit(f"$ {' '.join(cmd)}\n{msg}")
    return r


def git(*args: str, check: bool = True) -> str:
    return run(["git", *args], check=check).stdout.strip()


def current_branch() -> str:
    return git("branch", "--show-current")


def slugify(text: str) -> str:
    text = text.lower().strip()
    text = re.sub(r"^pr/\d+-", "", text)
    text = re.sub(r"^(wip|feat|fix|docs|refactor|test|chore|ci|agent)/", "", text)
    text = re.sub(
        r"^[\U0001F300-\U0001FAFF\u2600-\u27BF\u2300-\u23FF]+\s*",
        "",
        text,
    )
    text = re.sub(
        r"^(feat|fix|docs|refactor|test|chore|ci|perf|build|style|revert|architecture)"
        r"(\([^)]*\))?:\s*",
        "",
        text,
    )
    text = re.sub(r"[^a-z0-9]+", "-", text)
    text = re.sub(r"-+", "-", text).strip("-")
    return text[:60] or "work"


def default_slug_from_branch(branch: str) -> str:
    if re.match(r"^pr/\d+-", branch):
        return re.sub(r"^pr/\d+-", "", branch)
    if "/" in branch:
        return slugify(branch.split("/", 1)[1])
    return slugify(branch)


def _strip_ansi(text: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", text)


def pr_view(number: int | None = None) -> dict:
    cmd = ["gh", "pr", "view", "--json", "number,url,headRefName,title,state"]
    if number is not None:
        cmd.insert(3, str(number))
    r = run(cmd)
    raw = _strip_ansi(r.stdout).strip()
    if not raw:
        raise SystemExit(f"$ {' '.join(cmd)}\nempty gh output")
    try:
        return json.loads(raw)
    except json.JSONDecodeError as e:
        raise SystemExit(f"$ {' '.join(cmd)}\ninvalid JSON: {e}\n{raw[:200]!r}") from e


def pr_for_branch(branch: str) -> dict | None:
    r = run(
        [
            "gh",
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "number,url,headRefName,title,state",
        ],
        check=False,
    )
    if r.returncode != 0 or not r.stdout.strip():
        return None
    raw = _strip_ansi(r.stdout).strip()
    try:
        items = json.loads(raw)
    except json.JSONDecodeError:
        return None
    return items[0] if items else None


def resolve_pr(number: int | None) -> dict:
    if number is not None:
        return pr_view(number)
    branch = current_branch()
    # If local is pr/N-…, ask gh for PR by number first
    m = re.match(r"^pr/(\d+)-", branch)
    if m:
        try:
            return pr_view(int(m.group(1)))
        except SystemExit:
            pass
    # Remote tracking branch name (may differ from local pr/N name)
    upstream = git("rev-parse", "--abbrev-ref", "@{upstream}", check=False)
    if upstream.startswith("origin/"):
        remote_branch = upstream[len("origin/") :]
        pr = pr_for_branch(remote_branch)
        if pr:
            return pr
    pr = pr_for_branch(branch)
    if pr:
        return pr
    return pr_view()  # current checkout association


def adopt_local(number: int, slug: str, remote_head: str) -> str:
    """Rename local branch to pr/<N>-<slug> and track origin/<remote_head>."""
    new_local = f"pr/{number}-{slug}"
    old_local = current_branch()

    # Ensure remote head exists locally as a ref
    run(["git", "fetch", "origin", remote_head])

    if old_local != new_local:
        existing = git("branch", "--list", new_local)
        if existing:
            old_tip = git("rev-parse", old_local)
            target_tip = git("rev-parse", new_local)
            if old_tip != target_tip:
                raise SystemExit(
                    f"refusing to replace local {new_local!r}: tip differs from "
                    f"{old_local!r}. Rename or delete {new_local} manually, then retry."
                )
            run(["git", "branch", "-D", new_local], check=False)
        run(["git", "branch", "-m", new_local])

    run(["git", "branch", f"--set-upstream-to=origin/{remote_head}", new_local])
    print(f"local {old_local} → {new_local}  (tracks origin/{remote_head}, PR #{number})")
    return new_local


def cmd_adopt(args: argparse.Namespace) -> int:
    pr = resolve_pr(args.number)
    number = int(pr["number"])
    remote_head = pr["headRefName"]
    old = current_branch()
    slug = slugify(args.slug or default_slug_from_branch(old) or pr.get("title") or "work")
    adopt_local(number, slug, remote_head)
    print(pr.get("url", f"PR #{number}"))
    return 0


REQUIRED_LABEL_NAMESPACES = ("type", "area")


def known_labels() -> list[str]:
    """Canonical label names from .github/labels.yml.

    Parsed with a line scan rather than a YAML dependency: this script runs on a
    bare checkout, and the file's shape is fixed by sync-github-labels.py.
    """
    path = ROOT / ".github" / "labels.yml"
    if not path.is_file():
        return []
    names: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        m = re.match(r'\s*-\s*name:\s*"(.+)"\s*$', line)
        if m:
            names.append(m.group(1))
    return names


def namespace_of(label: str) -> str | None:
    """`✨ type:feat` → `type`. Labels are emoji + space + namespace:value."""
    m = re.match(r"^\S+\s+([a-z]+):", label)
    return m.group(1) if m else None


def validate_labels(labels: list[str]) -> None:
    """Fail closed when a PR would open without the labels metrics depend on.

    Labelling used to be optional here, so it depended on whoever opened the PR
    remembering. It stopped happening for 9 consecutive PRs on 2026-07-28 and
    nothing noticed, because nothing was checking. Program P40–P46 keys routing
    and per-PR metrics off `type:` and `area:`, so an unlabelled PR is missing
    data, not a style lapse.
    """
    catalog = known_labels()
    unknown = [label for label in labels if catalog and label not in catalog]
    if unknown:
        raise SystemExit(
            "unknown label(s): "
            + ", ".join(repr(label) for label in unknown)
            + "\nvalid names live in .github/labels.yml"
        )
    present = {namespace_of(label) for label in labels}
    missing = [ns for ns in REQUIRED_LABEL_NAMESPACES if ns not in present]
    if missing:
        options = {ns: [n for n in catalog if namespace_of(n) == ns] for ns in missing}
        detail = "\n".join(
            f"  {ns}: " + (", ".join(repr(n) for n in names) or "(none in labels.yml)")
            for ns, names in options.items()
        )
        raise SystemExit(
            "a PR must carry at least one label per namespace: "
            + ", ".join(missing)
            + f"\npass --label for each. Choices:\n{detail}"
        )


def missing_namespaces(labels: list[str]) -> list[str]:
    """Required namespaces absent from an already-created issue or PR."""
    present = {namespace_of(label) for label in labels}
    return [ns for ns in REQUIRED_LABEL_NAMESPACES if ns not in present]


def cmd_audit_labels(args: argparse.Namespace) -> int:
    """Report issues and PRs missing required labels. Read-only.

    `open` fails closed from now on, but that only protects what this script
    creates. Anything opened by hand — every issue, and any PR created with
    `gh pr create` directly — bypasses it. This makes the gap visible instead
    of waiting for someone to notice months of unlabelled history.
    """
    gaps: list[str] = []
    for kind in ("issue", "pr"):
        listing = run(
            [
                "gh", kind, "list",
                "--state", args.state,
                "--limit", str(args.limit),
                "--json", "number,title,labels",
            ]
        )
        for item in json.loads(_strip_ansi(listing.stdout) or "[]"):
            labels = [label["name"] for label in item.get("labels") or []]
            missing = missing_namespaces(labels)
            if missing:
                marker = "#" if kind == "issue" else "PR #"
                gaps.append(
                    f"  {marker}{item['number']} missing {', '.join(missing)}"
                    f"  — {item['title'][:60]}"
                )

    if not gaps:
        print(f"all {args.state} issues and PRs carry type: and area: labels")
        return 0
    print(f"{len(gaps)} unlabelled item(s):")
    print("\n".join(gaps))
    return 1


def cmd_open(args: argparse.Namespace) -> int:
    old = current_branch()
    if not old:
        raise SystemExit("detached HEAD; checkout a branch first")

    # Refuse before the push, so a rejected open leaves nothing behind.
    validate_labels(args.label or [])

    # Keep remote head stable under current name (prefer wip/…)
    remote_head = old
    run(["git", "push", "-u", "origin", f"HEAD:{remote_head}"])

    create = ["gh", "pr", "create", "--title", args.title, "--head", remote_head]
    if args.draft:
        create.append("--draft")
    if args.body_file:
        create.extend(["--body-file", args.body_file])
    else:
        create.extend(["--body", args.body or ""])
    for label in args.label or []:
        create.extend(["--label", label])
    if args.base:
        create.extend(["--base", args.base])

    r = run(create)
    url = r.stdout.strip().splitlines()[-1]
    m = re.search(r"/pull/(\d+)\s*$", url) or re.search(r"/pull/(\d+)", url)
    if not m:
        raise SystemExit(f"could not parse PR number from: {url!r}")
    number = int(m.group(1))
    slug = slugify(args.slug or default_slug_from_branch(old) or args.title)
    adopt_local(number, slug, remote_head)
    print(url)
    print(f"local branch: pr/{number}-{slug}  remote head: {remote_head}")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    branch = current_branch()
    try:
        pr = resolve_pr(args.number)
    except SystemExit as e:
        print(str(e), file=sys.stderr)
        return 1
    number = int(pr["number"])
    state = str(pr.get("state") or "").upper()
    if state and state != "OPEN":
        print(
            f"MISMATCH: PR #{number} state={state!r} (expected OPEN)",
            file=sys.stderr,
        )
        return 1
    expected_prefix = f"pr/{number}-"
    if not branch.startswith(expected_prefix):
        print(
            f"MISMATCH: local branch={branch!r} PR=#{number} "
            f"expected local name {expected_prefix}<slug>",
            file=sys.stderr,
        )
        print("fix: python3 scripts/github_pr_branch.py adopt", file=sys.stderr)
        return 1
    upstream = git("rev-parse", "--abbrev-ref", "@{upstream}", check=False)
    remote_head = pr["headRefName"]
    expected_upstream = f"origin/{remote_head}"
    if not upstream or upstream == "@{upstream}":
        print(
            f"MISMATCH: local {branch!r} has no upstream; "
            f"expected {expected_upstream}",
            file=sys.stderr,
        )
        print("fix: python3 scripts/github_pr_branch.py adopt", file=sys.stderr)
        return 1
    if upstream != expected_upstream:
        print(
            f"MISMATCH: upstream={upstream!r} expected {expected_upstream!r} "
            f"(remote PR head must stay {remote_head!r}; do not rename remote)",
            file=sys.stderr,
        )
        print("fix: python3 scripts/github_pr_branch.py adopt", file=sys.stderr)
        return 1
    print(f"OK local {branch} ↔ PR #{number} (remote head {remote_head}, upstream {upstream})")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_open = sub.add_parser("open", help="Create PR; rename local branch to pr/<N>-<slug>")
    p_open.add_argument("--title", required=True)
    p_open.add_argument("--body", default="")
    p_open.add_argument("--body-file")
    p_open.add_argument("--slug")
    p_open.add_argument("--label", action="append", default=[])
    p_open.add_argument("--base", default="main")
    p_open.add_argument("--draft", action="store_true")
    p_open.set_defaults(func=cmd_open)

    p_adopt = sub.add_parser("adopt", help="Rename local branch to pr/<N>-<slug>")
    p_adopt.add_argument("--number", type=int)
    p_adopt.add_argument("--slug")
    p_adopt.set_defaults(func=cmd_adopt)

    p_audit = sub.add_parser("audit-labels", help="Report issues/PRs missing type: or area:")
    p_audit.add_argument("--state", default="all", choices=["open", "closed", "all"])
    p_audit.add_argument("--limit", type=int, default=200)
    p_audit.set_defaults(func=cmd_audit_labels)

    p_check = sub.add_parser("check", help="Verify local branch matches open PR number")
    p_check.add_argument("--number", type=int)
    p_check.set_defaults(func=cmd_check)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
