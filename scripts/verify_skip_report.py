#!/usr/bin/env python3
"""Render the gates that a local `verify.sh` invocation did not run.

`verify.sh` allows skips locally on purpose: a contributor without tmux, or a
fresh clone with no installed JS dependencies, can still inspect
the available suite. Managed land forbids them (`OPTIMUS_VERIFY_FORBID_SKIPS=1`).

What was not intended is the hook printing `clean` afterwards. A push that ran
28 of 31 gates is not clean, it is partly verified, and the difference is
exactly what strict verification is designed to prevent. This turns the skip
list into a sentence that says so.

Kept separate from the runner because a bash heredoc is not testable and this is.
"""

from __future__ import annotations

import sys


def parse(raw: str) -> list[tuple[str, str]]:
    """Read `name<TAB>reason` lines, in order, without repeats.

    A gate can reach `skip` from more than one branch across tiers, and naming
    it twice would read as two separate holes.
    """
    seen: set[str] = set()
    skipped: list[tuple[str, str]] = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        name, _, reason = line.partition('\t')
        name = name.strip()
        if not name or name in seen:
            continue
        seen.add(name)
        skipped.append((name, reason.strip()))
    return skipped


def render(skipped: list[tuple[str, str]]) -> str:
    """The block the hook prints in place of `clean`. Empty when nothing skipped."""
    if not skipped:
        return ''
    count = len(skipped)
    gates = 'gate' if count == 1 else 'gates'
    width = max(len(name) for name, _ in skipped)
    lines = [
        '',
        f'[verify] {count} {gates} did not run here — managed land will refuse:',
        '',
    ]
    lines += [f'    {name:<{width}}  {reason}' for name, reason in skipped]
    lines += [
        '',
        '  Each line names the missing prerequisite. To make a skip fail locally,',
        '  the same strict mode used by managed land:',
        '',
        '    OPTIMUS_VERIFY_FORBID_SKIPS=1 just verify',
        '',
    ]
    return '\n'.join(lines)


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print('usage: verify_skip_report.py <report-file>', file=sys.stderr)
        return 2
    try:
        with open(argv[1], encoding='utf-8') as handle:
            raw = handle.read()
    except FileNotFoundError:
        # No report means no skips: verify.sh only writes the file when it has
        # something to say. Silence is the correct output.
        return 0
    block = render(parse(raw))
    if block:
        print(block)
    return 0


if __name__ == '__main__':
    raise SystemExit(main(sys.argv))
