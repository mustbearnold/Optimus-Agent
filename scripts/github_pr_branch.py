#!/usr/bin/env python3
"""Align local/remote branch names with GitHub PR numbers: pr/<N>-<slug>.

Commands:
  open    Push current branch, create a PR, rename to pr/<N>-<slug>
  adopt   Rename current branch to pr/<N>-<slug> for an already-open PR
  check   Exit 0 if current branch matches its open PR number

Examples:
  python3 scripts/github_pr_branch.py open --title "✨ feat(cli): …" --slug feat-cli-foo \\
      --label "✨ type:feat" --label "💻 area:cli" --label "▪️ size:S"

  python3 scripts/github_pr_branch.py adopt
  python3 scripts/github_pr_branch.py adopt --slug p12-command-fs-envelope
  python3 scripts/github_pr_branch.py check
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


def run(
    cmd: list[str],
    *,
    check: bool = True,
    capture: bool = True,
) -> subprocess.CompletedProcess[str]:
    r = subprocess.run(
        cmd,
        check=False,
        capture_output=capture,
        text=True,
    )
    if check and r.returncode != 0:
        msg = (r.stderr or r.stdout or "").strip() or f"exit {r.returncode}"
        raise SystemExit(f"$ {' '.join(cmd)}\n{msg}")
    return r


def git(*args: str, check: bool = True) -> str:
    r = run(["git", *args], check=check)
    return (r.stdout or "").strip()


def gh_json(args: list[str]) -> object:
    r = run(["gh", *args, "--json", "number,url,headRefName,title"])
    return json.loads(r.stdout)


def current_branch() -> str:
    return git("branch", "--show-current")


def slugify(text: str) -> str:
    text = text.lower().strip()
    text = re.sub(r"^pr/\d+-", "", text)
    text = re.sub(r"^wip/", "", text)
    # strip conventional emoji + type prefixes from titles for slug use
    text = re.sub(
        r"^[\U0001F300-\U0001FAFF\u2600-\u27BF\u2300-\u23FF\u2000-\u206F]+\s*",
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
    if branch.startswith("wip/"):
        return branch[4:]
    return slugify(branch)


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
            "number,url,headRefName,title",
        ],
        check=False,
    )
    if r.returncode != 0 or not r.stdout.strip():
        return None
    try:
        items = json.loads(r.stdout)
    except json.JSONDecodeError:
        return None
    if not items:
        return None
    return items[0]


def pr_for_current_or_number(number: int | None) -> dict:
    if number is not None:
        r = run(
            [
                "gh",
                "pr",
                "view",
                str(number),
                "--json",
                "number,url,headRefName,title",
            ]
        )
        return json.loads(r.stdout)
    branch = current_branch()
    pr = pr_for_branch(branch)
    if pr:
        return pr
    # fallback: PR associated with current checkout
    r = run(
        ["gh", "pr", "view", "--json", "number,url,headRefName,title"],
        check=False,
    )
    if r.returncode != 0:
        raise SystemExit(
            f"no open PR for branch {branch!r}; pass --number or open a PR first"
        )
    return json.loads(r.stdout)


def rename_branch_to_pr(number: int, slug: str, old_branch: str) -> str:
    new_branch = f"pr/{number}-{slug}"
    if old_branch == new_branch:
        print(f"already on {new_branch}")
        return new_branch

    # Prefer GitHub branch rename API so open PRs retarget head automatically.
    repo = run(
        ["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]
    ).stdout.strip()
    if old_branch:
        r = run(
            [
                "gh",
                "api",
                "--method",
                "POST",
                f"repos/{repo}/branches/{old_branch}/rename",
                "-f",
                f"new_name={new_branch}",
            ],
            check=False,
        )
        if r.returncode == 0:
            run(["git", "fetch", "origin", new_branch], check=False)
            run(["git", "branch", "-m", new_branch], check=False)
            # track remote
            run(["git", "branch", f"--set-upstream-to=origin/{new_branch}", new_branch], check=False)
            # checkout if rename left us wrong
            run(["git", "checkout", new_branch], check=False)
            print(f"branch {old_branch} → {new_branch} (github rename)")
            return new_branch
        # fall through to local push strategy
        sys.stderr.write(r.stderr or r.stdout or "github rename failed; falling back\n")

    run(["git", "branch", "-m", new_branch])
    run(["git", "push", "-u", "origin", new_branch])
    if old_branch and old_branch != new_branch:
        run(["git", "push", "origin", "--delete", old_branch], check=False)
    print(f"branch {old_branch} → {new_branch}")
    return new_branch


def cmd_adopt(args: argparse.Namespace) -> int:
    old = current_branch()
    pr = pr_for_current_or_number(args.number)
    number = int(pr["number"])
    slug = args.slug or default_slug_from_branch(old) or slugify(pr.get("title") or "work")
    slug = slugify(slug)
    rename_branch_to_pr(number, slug, old)
    print(f"PR #{number} {pr.get('url', '')}")
    return 0


def cmd_open(args: argparse.Namespace) -> int:
    old = current_branch()
    if not old:
        raise SystemExit("detached HEAD; checkout a branch first")

    # ensure pushed
    run(["git", "push", "-u", "origin", "HEAD"], check=False)

    create = [
        "gh",
        "pr",
        "create",
        "--title",
        args.title,
    ]
    if args.draft:
        create.append("--draft")
    if args.body_file:
        create.extend(["--body-file", args.body_file])
    elif args.body:
        create.extend(["--body", args.body])
    else:
        create.extend(["--body", ""])
    for label in args.label or []:
        create.extend(["--label", label])
    if args.base:
        create.extend(["--base", args.base])

    r = run(create)
    url = r.stdout.strip().splitlines()[-1]
    m = re.search(r"/pull/(\d+)", url)
    if not m:
        raise SystemExit(f"could not parse PR number from: {url}")
    number = int(m.group(1))
    slug = args.slug or default_slug_from_branch(old) or slugify(args.title)
    slug = slugify(slug)
    rename_branch_to_pr(number, slug, old)
    print(url)
    print(f"canonical branch: pr/{number}-{slug}")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    branch = current_branch()
    try:
        pr = pr_for_current_or_number(args.number)
    except SystemExit as e:
        print(str(e), file=sys.stderr)
        return 1
    number = int(pr["number"])
    expected_prefix = f"pr/{number}-"
    if not branch.startswith(expected_prefix):
        print(
            f"MISMATCH: branch={branch!r} pr=#{number} expected prefix {expected_prefix!r}",
            file=sys.stderr,
        )
        print("fix: python3 scripts/github_pr_branch.py adopt", file=sys.stderr)
        return 1
    print(f"OK {branch} ↔ PR #{number}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_open = sub.add_parser("open", help="Create PR then rename branch to pr/<N>-<slug>")
    p_open.add_argument("--title", required=True)
    p_open.add_argument("--body", default="")
    p_open.add_argument("--body-file")
    p_open.add_argument("--slug", help="Override slug portion of pr/<N>-<slug>")
    p_open.add_argument("--label", action="append", default=[])
    p_open.add_argument("--base", default="main")
    p_open.add_argument("--draft", action="store_true")
    p_open.set_defaults(func=cmd_open)

    p_adopt = sub.add_parser("adopt", help="Rename current branch to pr/<N>-<slug>")
    p_adopt.add_argument("--number", type=int, help="PR number (default: current branch PR)")
    p_adopt.add_argument("--slug", help="Override slug")
    p_adopt.set_defaults(func=cmd_adopt)

    p_check = sub.add_parser("check", help="Verify branch name matches open PR number")
    p_check.add_argument("--number", type=int)
    p_check.set_defaults(func=cmd_check)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as e:
        sys.stderr.write(e.stderr or str(e))
        raise SystemExit(e.returncode)
