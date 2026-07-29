#!/usr/bin/env python3
"""Which tests can this patch actually break? (program P42)

`just test` runs everything. That is correct and slow, and a slow gate is one
people skip. This selects the subset a patch can plausibly break — but the
whole value depends on the subset being a *superset* of what would fail, and
every rule here is written in the direction that keeps it one.

Four rules, each one biased toward running too much:

1. **Unknown escalates.** A changed path this script cannot classify selects
   the full suite. The alternative — dropping it — is a selector that silently
   stops testing whatever it stops recognising, which is exactly how a
   selector rots without anyone noticing.

2. **The gate cannot shrink itself.** A change to `justfile`, `verify.sh`,
   any `check-*.py`, this file, the workspace manifest, the lockfile, or
   `.github/**` selects everything. Otherwise the cheapest way to make a patch
   pass is to edit the thing that decides what passing means.

3. **Impact is transitive.** Touching a crate selects that crate *and every
   crate that depends on it*, computed from the manifests rather than guessed.
   A change to `optimus-policy` breaks `optimus-kernel`'s tests, and the
   selector has to know that without being told each time.

4. **Selecting nothing is not passing.** An empty selection is reported as
   `nothing-selected` and exits non-zero under `--require-selection`. "No
   tests ran" and "the tests passed" are different sentences, and a gate that
   confuses them is worse than no gate.

Output is a plan, not an action: the caller runs it. `--json` emits the plan
for a program, plain output explains it to a human, including what it
escalated and why.
"""

from __future__ import annotations

import argparse
import fnmatch
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# A change to any of these invalidates the selection itself, so the selection
# is "everything". Rule 2: the gate cannot shrink itself.
ESCALATING_PATHS = (
    "justfile",
    "Justfile",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rust-toolchain",
    "package.json",
    "package-lock.json",
    ".github/**",
    ".cargo/**",
    "scripts/verify.sh",
    "scripts/check-*.py",
    "scripts/impact_select.py",
    "scripts/engineering_memory.py",
    "scripts/verify_skip_report.py",
)

# Suites that are not cargo packages. Selected by path, never by inference.
SUITE_UI = "optimus-ui vitest"
SUITE_ELECTRON = "optimus-electron"
SUITE_ELECTRON_E2E = "electron e2e"
SUITE_PLAYWRIGHT = "playwright"
SUITE_TUI_E2E = "tui e2e"
SUITE_GATES = "gates"

# Path prefix -> suites, for the parts of the tree cargo does not describe.
PATH_SUITES: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("apps/optimus-ui/", (SUITE_UI, SUITE_PLAYWRIGHT, SUITE_ELECTRON_E2E)),
    ("apps/optimus-desktop/", (SUITE_ELECTRON, SUITE_ELECTRON_E2E, SUITE_PLAYWRIGHT)),
    ("docs/", (SUITE_GATES,)),
    ("scripts/", (SUITE_GATES,)),
    # The knowledge graph's own generated state: the engineering-memory gate
    # reads every one of these and fails on drift.
    (".engineering-memory/", (SUITE_GATES,)),
)

# Suffixes the gates read wherever they appear. Markdown feeds the
# engineering-memory frontmatter graph, so a doc edit is a gate concern even
# when it is nowhere near `docs/`.
SUFFIX_SUITES: tuple[tuple[str, tuple[str, ...]], ...] = ((".md", (SUITE_GATES,)),)

# Paths that change nothing a test can observe. Kept deliberately short: every
# entry is a promise that no gate reads the file, and a wrong promise here is
# a missed regression, not a slow build.
INERT_PATHS = (
    ".gitignore",
    ".gitattributes",
    "LICENSE",
    "LICENSE-*",
    "**/*.png",
    "**/*.jpg",
    "**/*.svg",
    "**/*.ico",
    "local/**",
)


class Plan:
    """What to run, and — just as importantly — why."""

    def __init__(self) -> None:
        self.packages: set[str] = set()
        self.suites: set[str] = set()
        self.escalated: bool = False
        self.reasons: list[str] = []
        self.unclassified: list[str] = []
        self.considered: list[str] = []

    def escalate(self, reason: str) -> None:
        if not self.escalated:
            self.escalated = True
        if reason not in self.reasons:
            self.reasons.append(reason)

    def is_empty(self) -> bool:
        return not self.escalated and not self.packages and not self.suites

    def as_dict(self) -> dict[str, object]:
        return {
            "escalated": self.escalated,
            "packages": sorted(self.packages),
            "suites": sorted(self.suites),
            "reasons": list(self.reasons),
            "unclassified": list(self.unclassified),
            "considered_file_count": len(self.considered),
            "status": self.status(),
        }

    def status(self) -> str:
        if self.escalated:
            return "escalated"
        if self.is_empty():
            return "nothing-selected"
        return "selected"


def matches_any(path: str, patterns: tuple[str, ...]) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


# --- workspace graph ---------------------------------------------------------


def workspace_members(root: Path = ROOT) -> dict[str, Path]:
    """Package name -> directory, read from the workspace manifest."""
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    members: dict[str, Path] = {}
    for member in manifest.get("workspace", {}).get("members", []):
        crate_manifest = root / member / "Cargo.toml"
        if not crate_manifest.is_file():
            continue
        parsed = tomllib.loads(crate_manifest.read_text(encoding="utf-8"))
        name = parsed.get("package", {}).get("name")
        if name:
            members[name] = root / member
    return members


def direct_dependencies(crate_dir: Path, known: set[str]) -> set[str]:
    """Workspace crates this crate depends on, in any dependency table.

    Dev- and build-dependencies count: a change to a crate used only by
    another crate's tests still breaks those tests, and a selector that
    ignored that would miss exactly the failures it exists to catch.
    """
    parsed = tomllib.loads((crate_dir / "Cargo.toml").read_text(encoding="utf-8"))
    found: set[str] = set()
    tables = ("dependencies", "dev-dependencies", "build-dependencies")
    for table in tables:
        for name in parsed.get(table, {}):
            if name in known:
                found.add(name)
    for target in parsed.get("target", {}).values():
        for table in tables:
            for name in target.get(table, {}):
                if name in known:
                    found.add(name)
    return found


def reverse_dependents(members: dict[str, Path]) -> dict[str, set[str]]:
    """crate -> crates that depend on it, directly."""
    known = set(members)
    reverse: dict[str, set[str]] = {name: set() for name in members}
    for name, directory in members.items():
        for dependency in direct_dependencies(directory, known):
            reverse[dependency].add(name)
    return reverse


def dependent_closure(seeds: set[str], reverse: dict[str, set[str]]) -> set[str]:
    """Every crate reachable from `seeds` by "is depended on by"."""
    seen: set[str] = set()
    queue = list(seeds)
    while queue:
        current = queue.pop()
        if current in seen:
            continue
        seen.add(current)
        queue.extend(reverse.get(current, ()))
    return seen


# --- classification ----------------------------------------------------------


def package_for_path(path: str, members: dict[str, Path]) -> str | None:
    """The workspace crate a repository-relative path belongs to."""
    best: tuple[int, str] | None = None
    for name, directory in members.items():
        prefix = f"{directory.relative_to(ROOT).as_posix()}/"
        if path.startswith(prefix) and (best is None or len(prefix) > best[0]):
            best = (len(prefix), name)
    return best[1] if best else None


def suites_for_path(path: str) -> tuple[str, ...]:
    for prefix, suites in PATH_SUITES:
        if path.startswith(prefix):
            return suites
    for suffix, suites in SUFFIX_SUITES:
        if path.endswith(suffix):
            return suites
    return ()


def build_plan(
    changed: list[str],
    members: dict[str, Path] | None = None,
    reverse: dict[str, set[str]] | None = None,
) -> Plan:
    if members is None:
        members = workspace_members()
    if reverse is None:
        reverse = reverse_dependents(members)

    plan = Plan()
    plan.considered = list(changed)
    seeds: set[str] = set()

    for path in changed:
        if matches_any(path, INERT_PATHS):
            continue
        if matches_any(path, ESCALATING_PATHS):
            plan.escalate(f"{path} decides what verification means")
            continue
        package = package_for_path(path, members)
        if package is not None:
            seeds.add(package)
            continue
        suites = suites_for_path(path)
        if suites:
            plan.suites.update(suites)
            continue
        # Rule 1. Nothing here recognised the path, so nothing here can claim
        # it is safe to skip.
        plan.unclassified.append(path)
        plan.escalate(f"{path} is not classified by any rule")

    if seeds:
        plan.packages.update(dependent_closure(seeds, reverse))
        # A crate change can surface through the desktop shell, which is not a
        # cargo package and so never appears in the closure.
        if any(name.startswith("optimus-") for name in plan.packages):
            plan.suites.add(SUITE_GATES)
    if "optimus-tui" in plan.packages:
        plan.suites.add(SUITE_TUI_E2E)
    if "optimus-desktop" in plan.packages:
        plan.suites.update((SUITE_ELECTRON, SUITE_ELECTRON_E2E))

    return plan


# --- git ---------------------------------------------------------------------


def git(args: list[str], root: Path = ROOT) -> str:
    result = subprocess.run(
        ["git", *args], cwd=root, capture_output=True, text=True, check=False
    )
    return result.stdout if result.returncode == 0 else ""


def changed_files(base: str | None, root: Path = ROOT) -> list[str]:
    """Paths changed against `base`, plus anything uncommitted.

    Uncommitted work is included because the inner loop is where this is
    useful; a selector that only saw committed changes would report
    `nothing-selected` for the edit you are actually making.
    """
    paths: set[str] = set()
    if base:
        merge_base = git(["merge-base", base, "HEAD"], root).strip() or base
        for line in git(["diff", "--name-only", merge_base, "HEAD"], root).splitlines():
            if line.strip():
                paths.add(line.strip())
    for line in git(["status", "--porcelain"], root).splitlines():
        entry = line[3:].strip() if len(line) > 3 else ""
        if " -> " in entry:  # rename: both sides matter
            before, after = entry.split(" -> ", 1)
            paths.update((before.strip(), after.strip()))
        elif entry:
            paths.add(entry)
    return sorted(paths)


def default_base(root: Path = ROOT) -> str | None:
    head = git(["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"], root).strip()
    if head:
        return head.removeprefix("refs/remotes/")
    for candidate in ("origin/main", "main"):
        if git(["rev-parse", "--verify", "--quiet", candidate], root).strip():
            return candidate
    return None


# --- reporting ---------------------------------------------------------------


def render(plan: Plan) -> str:
    lines: list[str] = []
    if plan.escalated:
        lines.append("selection: EVERYTHING")
        for reason in plan.reasons:
            lines.append(f"  because {reason}")
        return "\n".join(lines)
    if plan.is_empty():
        lines.append("selection: NOTHING")
        lines.append("  no changed file maps to a test; this is not a pass")
        return "\n".join(lines)
    lines.append(f"selection: {len(plan.packages)} package(s), {len(plan.suites)} suite(s)")
    for name in sorted(plan.packages):
        lines.append(f"  package  {name}")
    for name in sorted(plan.suites):
        lines.append(f"  suite    {name}")
    return "\n".join(lines)


def cargo_arguments(plan: Plan) -> list[str]:
    """`cargo test` selector arguments for this plan."""
    if plan.escalated:
        return ["--workspace"]
    args: list[str] = []
    for name in sorted(plan.packages):
        args.extend(["-p", name])
    return args


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", default=None, help="compare against this ref")
    parser.add_argument("--json", action="store_true", help="emit the plan as JSON")
    parser.add_argument(
        "--cargo-args",
        action="store_true",
        help="emit only the cargo package selector arguments",
    )
    parser.add_argument(
        "--require-selection",
        action="store_true",
        help="exit non-zero when nothing is selected",
    )
    parser.add_argument("--paths", nargs="*", default=None, help="classify these paths")
    args = parser.parse_args(argv)

    if args.paths is not None:
        changed = sorted(args.paths)
    else:
        changed = changed_files(args.base or default_base())

    plan = build_plan(changed)

    if args.cargo_args:
        print(" ".join(cargo_arguments(plan)))
    elif args.json:
        print(json.dumps(plan.as_dict(), indent=2, sort_keys=True))
    else:
        print(render(plan))

    if args.require_selection and plan.is_empty():
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
