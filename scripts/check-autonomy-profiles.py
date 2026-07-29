#!/usr/bin/env python3
"""Fail-closed gate: the composer's access menu is the ADR-0044 vocabulary.

Issue #118: the menu offered `Full access` first, that string parsed to
`AutonomyProfile::UnrestrictedHost`, and the pairing in `chat.rs` turned
SmartDeny off — so the first item of a three-item menu handed over the host.
Nothing failed, because no gate held the TypeScript menu against the Rust
vocabulary it is a face for. This is that gate.

Compares:
  1. Canonical profile names   — crates/optimus-policy/src/lib.rs (`as_str`)
  2. Accepted spellings        — the `parse` arms in optimus-policy and
                                 optimus-graph, which must agree
  3. React menu values         — apps/optimus-ui/.../Composer.tsx, in the order
                                 the tiers actually render them
  4. Wry desktop menu values   — apps/optimus-desktop/ui/index.html, the other
                                 shipped composer
  5. Stored defaults + aliases — apps/optimus-ui/src/state/composerStore.ts

Rules (each is a way #118 could come back):
  - Every composer offers every canonical profile and invents none.
  - `standard` is first *as rendered*: the item a hurried human picks is the
    recommended default, never break-glass. For the React menu that means the
    tier order times the option order, not the option order alone.
  - `unrestricted_host` is last and sits in the `expert` tier.
  - Only `unrestricted_host` parses to `UnrestrictedHost`. No ordinary-sounding
    word ('full', 'all', 'host', …) may be an alias for it, and no catch-all
    arm may reach it.
  - The two Rust parse tables agree, so the profile a surface gets cannot
    depend on which crate read the string.
  - Every stored default is `standard`, and no restore alias resolves to
    break-glass.

Anything this file cannot classify is a failure, not a pass: an arm the parser
does not understand is an arm nobody is checking.

Exit 0 on success; print the violation and exit 1 otherwise.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

POLICY = ROOT / "crates/optimus-policy/src/lib.rs"
GRAPH = ROOT / "crates/optimus-graph/src/lib.rs"
COMPOSER = ROOT / "apps/optimus-ui/src/components/workbench/Composer.tsx"
STORE = ROOT / "apps/optimus-ui/src/state/composerStore.ts"
DESKTOP = ROOT / "apps/optimus-desktop/ui/index.html"

# Words that read as ordinary product language rather than as break-glass. The
# composer shipped 'full' as the label for unrestricted host; none of these may
# ever parse to it again.
ORDINARY_WORDS = frozenset(
    {"full", "all", "host", "everything", "auto", "yes", "on", "max"}
)


def fail(message: str) -> None:
    print(f"AUTONOMY_PROFILES_FAIL: {message}")
    sys.exit(1)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"{path.relative_to(ROOT)} is missing")
    return path.read_text(encoding="utf-8")


def canonical_profiles(source: str) -> list[str]:
    """Profile names in declaration order, from `as_str`'s match arms."""
    block = re.search(
        r"impl AutonomyProfile \{.*?fn as_str\(self\) -> &'static str \{(.*?)\n    \}",
        source,
        re.S,
    )
    if not block:
        fail("optimus-policy: could not find AutonomyProfile::as_str")
    return re.findall(r'=>\s*"([a-z_]+)"', block.group(1))


def parse_table(source: str, crate: str) -> dict[str, str]:
    """Every accepted spelling mapped to the variant it produces.

    Every arm in the block must be one this function understands. A match
    guard, a fully-qualified variant, or a catch-all that returns a profile
    would otherwise be invisible here — and an invisible arm is an arm the
    rules below never get to judge.
    """
    block = re.search(
        r"fn parse\(raw: &str\) -> Option<Self> \{(.*?)\n    \}", source, re.S
    )
    if not block:
        fail(f"{crate}: could not find AutonomyProfile::parse")
    body = re.sub(r"//[^\n]*", "", block.group(1))

    table: dict[str, str] = {}
    understood = 0
    for line in body.splitlines():
        arm = line.strip()
        if "=>" not in arm:
            continue
        understood += 1
        pattern, result = arm.split("=>", 1)
        if pattern.strip() == "_":
            if "None" not in result:
                fail(
                    f"{crate}: the catch-all arm returns {result.strip()!r} — an "
                    "unrecognized string must be no profile at all (#118)"
                )
            continue
        literals = re.findall(r'"([a-z_\-]+)"', pattern)
        variant = re.search(r"Some\((?:Self|AutonomyProfile)::(\w+)\)", result)
        # A continuation line (`=> {` with the variant below it) is understood
        # only if the next non-empty line names one.
        if not variant and result.strip().endswith("{"):
            tail = body[body.index(line) + len(line) :]
            variant = re.search(r"Some\((?:Self|AutonomyProfile)::(\w+)\)", tail)
        if not literals or not variant:
            fail(
                f"{crate}: cannot classify the arm {arm!r} in AutonomyProfile::"
                "parse — rewrite it as `\"literal\" | \"literal\" => "
                "Some(Self::Variant)` so this gate can judge it"
            )
        if re.search(r"\bif\b", pattern):
            fail(f"{crate}: the arm {arm!r} carries a match guard this gate cannot evaluate")
        for spelling in literals:
            table[spelling] = variant.group(1)

    if not table:
        fail(f"{crate}: parsed no spellings out of AutonomyProfile::parse")
    if understood < 2:
        fail(f"{crate}: AutonomyProfile::parse has {understood} arm(s); that is not the vocabulary")
    return table


QUOTED = r"""['"]([a-z_]+)['"]"""


def menu_options(source: str) -> list[tuple[str, str]]:
    """(value, tier) for each access option, in the order the menu *renders*.

    The composer renders tier by tier, so `accessOptions` order alone is not
    what a human sees: moving the expert tier to the top of `accessTiers` would
    put break-glass first while leaving `accessOptions` untouched. Both lists
    are read, and the rendered sequence is what the rules judge.
    """
    block = re.search(r"const accessOptions = \[(.*?)\n\] as const;", source, re.S)
    if not block:
        fail("Composer.tsx: could not find accessOptions")
    if source.count("const accessOptions") != 1 or source.count("const accessTiers") != 1:
        fail("Composer.tsx: accessOptions/accessTiers must each be declared exactly once")
    options = re.findall(
        rf"value:\s*{QUOTED}.*?tier:\s*{QUOTED}", block.group(1), re.S
    )
    if not options:
        fail("Composer.tsx: accessOptions has no value/tier pairs")

    tier_block = re.search(r"const accessTiers = \[(.*?)\n\] as const;", source, re.S)
    if not tier_block:
        fail("Composer.tsx: could not find accessTiers")
    tier_order = re.findall(rf"tier:\s*{QUOTED}", tier_block.group(1))
    if not tier_order:
        fail("Composer.tsx: accessTiers names no tiers")
    unknown = sorted({tier for _, tier in options} - set(tier_order))
    if unknown:
        fail(f"Composer.tsx: options sit in tiers that never render: {unknown}")

    return [
        (value, tier)
        for rendered_tier in tier_order
        for value, tier in options
        if tier == rendered_tier
    ]


def desktop_options(source: str) -> list[str]:
    """Option values of the Wry composer's access select, in render order."""
    block = re.search(r'<select id="access".*?</select>', source, re.S)
    if not block:
        fail("optimus-desktop/ui/index.html: could not find the access select")
    values = re.findall(r'<option value="([a-z_]+)"', block.group(0))
    if not values:
        fail("optimus-desktop/ui/index.html: the access select has no options")
    selected = re.findall(r'<option value="([a-z_]+)"[^>]*\bselected\b', block.group(0))
    if selected != ["standard"]:
        fail(
            "optimus-desktop/ui/index.html: the access select pre-selects "
            f"{selected or ['(nothing)']}; standard is the default profile"
        )
    return values


def main() -> int:
    policy_source = read(POLICY)
    profiles = canonical_profiles(policy_source)
    if "standard" not in profiles or "unrestricted_host" not in profiles:
        fail(f"optimus-policy: unexpected profile vocabulary {profiles}")

    policy_table = parse_table(policy_source, "optimus-policy")
    graph_table = parse_table(read(GRAPH), "optimus-graph")

    # The CLI's own break-glass word lives in the graph table only; every other
    # spelling must mean the same thing in both crates.
    shared = set(policy_table) | (set(graph_table) - {"yolo"})
    drifted = sorted(
        word
        for word in shared
        if policy_table.get(word) != graph_table.get(word)
    )
    if drifted:
        fail(
            "optimus-policy and optimus-graph disagree on "
            + ", ".join(f"{w!r} ({policy_table.get(w)} vs {graph_table.get(w)})" for w in drifted)
        )

    for table, crate in ((policy_table, "optimus-policy"), (graph_table, "optimus-graph")):
        for word, variant in sorted(table.items()):
            if variant == "UnrestrictedHost" and word in ORDINARY_WORDS:
                fail(
                    f"{crate}: {word!r} parses to UnrestrictedHost — an "
                    "ordinary-sounding word must not be break-glass (#118)"
                )

    options = menu_options(read(COMPOSER))
    values = [value for value, _ in options]
    if values != sorted(values, key=profiles.index) or set(values) != set(profiles):
        fail(
            f"Composer.tsx offers {values}, which is not the profile "
            f"vocabulary in declaration order {profiles}"
        )
    if values[0] != "standard":
        fail(f"Composer.tsx offers {values[0]!r} first; standard must come first")
    if values[-1] != "unrestricted_host":
        fail(f"Composer.tsx offers {values[-1]!r} last; unrestricted_host must come last")

    tiers = dict(options)
    if tiers["unrestricted_host"] != "expert":
        fail(
            f"Composer.tsx puts unrestricted_host in the {tiers['unrestricted_host']!r} "
            "tier; break-glass belongs under Expert"
        )
    for value in values:
        if value not in policy_table:
            fail(f"Composer.tsx offers {value!r}, which optimus-policy cannot parse")

    desktop = desktop_options(read(DESKTOP))
    if desktop != profiles:
        fail(
            f"optimus-desktop/ui/index.html offers {desktop}, not the profile "
            f"vocabulary in declaration order {profiles}"
        )

    store = read(STORE)
    defaults = re.findall(rf"access:\s*{QUOTED},", store)
    if not defaults:
        fail("composerStore.ts: found no default access value")
    wrong = [value for value in defaults if value != "standard"]
    if wrong:
        fail(f"composerStore.ts defaults to {wrong}; standard is the default profile")

    # What a stored value restores to. Break-glass is not durable (ADR-0044 §5),
    # so no alias may resolve to it — a legacy word that restored straight to
    # unrestricted host would reopen #118 through persistence instead of
    # through the menu.
    aliases = re.search(r"ACCESS_ALIASES[^=]*=[^{]*\{(.*?)\n\}\)?;", store, re.S)
    if not aliases:
        fail("composerStore.ts: could not find ACCESS_ALIASES")
    # A plain object literal answers for 'constructor' and '__proto__' with
    # something that is not a profile, so a stored value nobody wrote into the
    # table would still restore to a truthy non-default.
    if "Object.create(null)" not in store:
        fail(
            "composerStore.ts: ACCESS_ALIASES must be built on a null prototype "
            "(Object.create(null)) so inherited names are unknown values"
        )
    restores = re.findall(rf"([a-z_]+):\s*{QUOTED}", aliases.group(1))
    if not restores:
        fail("composerStore.ts: ACCESS_ALIASES maps nothing")
    for stored, restored in restores:
        if restored == "unrestricted_host":
            fail(
                f"composerStore.ts restores {stored!r} to break-glass; ADR-0044 "
                "§5 keeps unrestricted host out of anything durable"
            )
        if restored not in profiles:
            fail(f"composerStore.ts restores {stored!r} to {restored!r}, which is not a profile")

    print(
        f"AUTONOMY_PROFILES_OK profiles={len(profiles)} react={len(values)} "
        f"desktop={len(desktop)} first={values[0]} last={values[-1]} "
        f"spellings={len(policy_table)} aliases={len(restores)}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
