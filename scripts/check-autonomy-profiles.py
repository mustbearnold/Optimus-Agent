#!/usr/bin/env python3
"""Fail-closed gate: every access surface speaks the ADR-0044 vocabulary.

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
  4. React persistence         — apps/optimus-ui/src/state/composerStore.ts
  5. Wry desktop menu values   — apps/optimus-desktop/ui/index.html
  6. Wry desktop persistence   — apps/optimus-desktop/ui/app.js, which owns
                                 that menu after paint and restores it on boot
  7. CLI policy words          — apps/optimus-cli/src/parsers.rs

Rules (each is a way #118 could come back):
  - Every composer offers every canonical profile and invents none.
  - `standard` is first *as rendered*: the item a hurried human picks is the
    recommended default, never break-glass. For the React menu that means the
    tier order times the option order, not the option order alone.
  - `unrestricted_host` is last and sits in the `expert` tier.
  - The spellings that reach `UnrestrictedHost` are exactly the ones named
    here — an allowlist, not a blocklist of words somebody thought of. A
    blocklist is only ever as good as its author's imagination, and 'full' was
    already outside one author's.
  - The two Rust parse tables agree, so the profile a surface gets cannot
    depend on which crate read the string.
  - Every stored default is `standard`, in both composers, and neither restores
    a stored value to break-glass: ADR-0044 §5 keeps unrestricted host out of
    anything durable.
  - Only `unrestricted` names `PolicyMode::Unrestricted` on the CLI.

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
DESKTOP_JS = ROOT / "apps/optimus-desktop/ui/app.js"
CLI_PARSERS = ROOT / "apps/optimus-cli/src/parsers.rs"

# The complete set of words that may reach break-glass, per crate. Anything
# else mapping to `UnrestrictedHost` fails, including words nobody here
# thought of — which is the point. `yolo` is the CLI's own flag and lives in
# the graph crate only.
BREAK_GLASS = frozenset({"unrestricted_host", "unrestricted"})
GRAPH_ONLY = frozenset({"yolo"})


def fail(message: str) -> None:
    print(f"AUTONOMY_PROFILES_FAIL: {message}")
    sys.exit(1)


def read(path: Path) -> str:
    if not path.is_file():
        fail(f"{path.relative_to(ROOT)} is missing")
    return path.read_text(encoding="utf-8")


def strip_comments(source: str) -> str:
    """Remove // line and /* */ block comments.

    A commented-out `tier:` or match arm must not be read as real code, and
    real code must not be hidden behind one.
    """
    return re.sub(r"//[^\n]*", "", re.sub(r"/\*.*?\*/", "", source, flags=re.S))


def braced_contents(source: str, opening: int, surface: str) -> tuple[str, int]:
    """Return the contents and closing offset of one balanced `{ ... }`.

    The bounded source shapes checked here contain strings, so braces inside a
    quoted diagnostic must not end a Rust match or JavaScript object early.
    """
    if opening < 0 or opening >= len(source) or source[opening] != "{":
        fail(f"{surface}: could not find the opening brace")
    depth = 0
    quote: str | None = None
    escaped = False
    for offset in range(opening, len(source)):
        char = source[offset]
        if quote is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in {'"', "'"}:
            quote = char
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1 : offset], offset
    fail(f"{surface}: opening brace has no matching close")


def match_body(source: str, signature: str, surface: str) -> str:
    """Extract the single top-level match body owned by a named function."""
    source = strip_comments(source)
    functions = list(re.finditer(signature, source, re.S))
    if len(functions) != 1:
        fail(f"{surface}: expected one matching function, found {len(functions)}")
    function_open = source.find("{", functions[0].end())
    function_body, _ = braced_contents(source, function_open, surface)
    matches = list(re.finditer(r"\bmatch\b[^{}]*\{", function_body, re.S))
    if len(matches) != 1:
        fail(f"{surface}: expected one top-level match, found {len(matches)}")
    match_open = function_open + 1 + matches[0].end() - 1
    body, _ = braced_contents(source, match_open, surface)
    return body


def logical_arms(body: str, surface: str) -> list[str]:
    """Split a Rust match into complete arms, including wrapped/block arms."""
    arms: list[str] = []
    start = 0
    depth = 0
    quote: str | None = None
    escaped = False
    for offset, char in enumerate(body):
        if quote is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in {'"', "'"}:
            quote = char
        elif char in "({[":
            depth += 1
        elif char in ")}]":
            depth -= 1
            if depth < 0:
                fail(f"{surface}: match arm has an unmatched closing delimiter")
        elif char == "," and depth == 0:
            arm = body[start:offset].strip()
            if arm:
                arms.append(arm)
            start = offset + 1
    if quote is not None or depth != 0:
        fail(f"{surface}: match arm has an unclosed string or delimiter")
    trailing = body[start:].strip()
    if trailing:
        arms.append(trailing)
    if not arms:
        fail(f"{surface}: match has no arms")
    return arms


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
    guard, a fully-qualified variant, a wrapped or-pattern, or a catch-all that
    returns a profile would otherwise be invisible here — and an invisible arm
    is an arm the rules below never get to judge.
    """
    table: dict[str, str] = {}
    body = match_body(
        source,
        r"\bfn\s+parse\(raw:\s*&str\)\s*->\s*Option<Self>",
        f"{crate}: AutonomyProfile::parse",
    )
    arms = logical_arms(body, f"{crate}: AutonomyProfile::parse")
    for arm in arms:
        if arm.count("=>") != 1:
            fail(f"{crate}: cannot classify the match arm {arm!r}")
        pattern, result = arm.split("=>", 1)
        if pattern.strip() == "_":
            if not re.fullmatch(r"\s*None\s*", result):
                fail(
                    f"{crate}: the catch-all arm returns {result.strip()!r} — an "
                    "unrecognized string must be no profile at all (#118)"
                )
            continue
        if re.search(r"\bif\b", pattern):
            fail(
                f"{crate}: the arm {arm!r} carries a match guard this gate "
                "cannot evaluate"
            )
        literals = re.findall(r'"([a-z_\-]+)"', pattern)
        pattern_shape = re.sub(r'"[a-z_\-]+"', "", pattern)
        if not literals or re.sub(r"[|\s]", "", pattern_shape):
            fail(
                f"{crate}: cannot classify the pattern {pattern.strip()!r}; "
                "only literal or-patterns are allowed"
            )
        simple_result = result.strip()
        if simple_result.startswith("{") and simple_result.endswith("}"):
            simple_result = simple_result[1:-1].strip()
        variant = re.fullmatch(
            r"Some\((?:Self|AutonomyProfile)::(\w+)\)", simple_result
        )
        if not variant:
            fail(
                f"{crate}: cannot classify the arm {arm!r} in AutonomyProfile::"
                'parse — rewrite it as `"literal" | "literal" => '
                "Some(Self::Variant)` so this gate can judge it"
            )
        for spelling in literals:
            table[spelling] = variant.group(1)

    if not table:
        fail(f"{crate}: parsed no spellings out of AutonomyProfile::parse")
    if len(arms) < 2:
        fail(
            f"{crate}: AutonomyProfile::parse has {len(arms)} arm(s); that is "
            "not the vocabulary"
        )
    return table


QUOTED = r"""['"]([a-z_]+)['"]"""


def menu_options(source: str) -> list[tuple[str, str]]:
    """(value, tier) for each access option, in the order the menu *renders*.

    The composer renders tier by tier, so `accessOptions` order alone is not
    what a human sees: moving the expert tier to the top of `accessTiers` would
    put break-glass first while leaving `accessOptions` untouched. Both lists
    are read, and the rendered sequence is what the rules judge. Each option is
    matched inside its own `{ … }` so a `tier:` two objects away cannot be
    borrowed to make a wrong one look right.
    """
    source = strip_comments(source)
    if source.count("const accessOptions") != 1 or source.count("const accessTiers") != 1:
        fail("Composer.tsx: accessOptions/accessTiers must each be declared exactly once")
    block = re.search(r"const accessOptions = \[(.*?)\n\] as const;", source, re.S)
    if not block:
        fail("Composer.tsx: could not find accessOptions")

    options: list[tuple[str, str]] = []
    for entry in re.findall(r"\{([^{}]*)\}", block.group(1)):
        value = re.search(rf"\bvalue:\s*{QUOTED}", entry)
        tier = re.search(rf"\btier:\s*{QUOTED}", entry)
        if not value or not tier:
            fail(
                f"Composer.tsx: the access option {entry.strip()!r} does not "
                "state both a value and a tier"
            )
        options.append((value.group(1), tier.group(1)))
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
    blocks = re.findall(r'<select id="access".*?</select>', source, re.S)
    if not blocks:
        fail("optimus-desktop/ui/index.html: could not find the access select")
    if len(blocks) != 1:
        fail(
            f"optimus-desktop/ui/index.html: {len(blocks)} access selects; the "
            "one the send path reads must be the only one"
        )
    values = re.findall(r'<option value="([a-z_]+)"', blocks[0])
    if not values:
        fail("optimus-desktop/ui/index.html: the access select has no options")
    selected = re.findall(r'<option value="([a-z_]+)"[^>]*\bselected\b', blocks[0])
    if selected != ["standard"]:
        fail(
            "optimus-desktop/ui/index.html: the access select pre-selects "
            f"{selected or ['(nothing)']}; standard is the default profile"
        )
    return values


def alias_table(source: str, surface: str) -> list[tuple[str, str]]:
    """(stored word, restored profile) pairs, and the shape they are held in.

    Both composers persist the composer and read it back on boot. ADR-0044 §5
    keeps break-glass out of anything durable, so what a stored value restores
    to is as much a part of this vocabulary as what the menu offers.
    """
    declarations = list(re.finditer(r"\bconst\s+ACCESS_ALIASES\b[^=]*=", source))
    if len(declarations) != 1:
        fail(f"{surface}: expected one ACCESS_ALIASES table, found {len(declarations)}")
    expression_end = source.find(";", declarations[0].end())
    if expression_end < 0:
        fail(f"{surface}: ACCESS_ALIASES has no terminating semicolon")
    expression = source[declarations[0].end() : expression_end]
    # A plain object literal answers for 'constructor' and '__proto__' with
    # something that is not a profile, so a stored value nobody wrote into the
    # table would still restore to a truthy non-default.
    if "Object.create(null)" not in expression:
        fail(
            f"{surface}: ACCESS_ALIASES must be built on a null prototype "
            "(Object.create(null)) so inherited names are unknown values"
        )
    opening = expression.find("{")
    aliases, _ = braced_contents(expression, opening, f"{surface}: ACCESS_ALIASES")
    restores = re.findall(rf"([a-z_]+):\s*{QUOTED}", aliases)
    if not restores:
        fail(f"{surface}: ACCESS_ALIASES maps nothing")
    return restores


def composer_defaults(source: str) -> list[tuple[str, str]]:
    """(export name, access) for each exported ComposerSettings literal.

    Read per object rather than by regexing the file for `access: '…',`: an
    access written last in its literal carries no trailing comma, and a rule
    that needs one is a rule a formatter can switch off.
    """
    literals = re.findall(
        r"export const (\w+): ComposerSettings = \{(.*?)\n\};", source, re.S
    )
    expected = {"offlineComposer", "codexComposer"}
    found = {name for name, _ in literals}
    if found != expected:
        fail(
            f"composerStore.ts: default objects are {sorted(found)}; expected "
            f"exactly {sorted(expected)}"
        )
    defaults: list[tuple[str, str]] = []
    for name, body in literals:
        value = re.search(rf"\baccess:\s*{QUOTED}", body)
        if not value:
            fail(f"composerStore.ts: {name} states no access value")
        defaults.append((name, value.group(1)))
    return defaults


def restore_contract(source: str, surface: str, use_pattern: str) -> None:
    """Prove stored access goes through a Standard-fallback restore filter."""
    if not re.search(use_pattern, source):
        fail(
            f"{surface}: persisted access must pass through restoredAccess(); "
            "assigning it raw makes break-glass survive a restart (ADR-0044 §5)"
        )
    declarations = list(
        re.finditer(r"(?:export\s+)?function\s+restoredAccess\([^)]*\)[^{]*\{", source)
    )
    if len(declarations) != 1:
        fail(f"{surface}: expected one restoredAccess function, found {len(declarations)}")
    opening = declarations[0].end() - 1
    body, _ = braced_contents(source, opening, f"{surface}: restoredAccess")
    if "unrestricted_host" in body:
        fail(f"{surface}: restoredAccess contains break-glass as a durable value")
    if not re.search(r"return\s+['\"]standard['\"]", body) or not re.search(
        r"ACCESS_ALIASES\[[^]]+\]\s*(?:\?\?|\|\|)\s*['\"]standard['\"]", body
    ):
        fail(f"{surface}: restoredAccess must fall back to standard for unknown values")


def cli_policy_words(source: str) -> list[str]:
    """Spellings that name `PolicyMode::Unrestricted` on the command line."""
    words: list[str] = []
    body = match_body(
        source,
        r"\bfn\s+parse_policy_mode\(policy:\s*&str\)",
        "optimus-cli: parse_policy_mode",
    )
    for arm in logical_arms(body, "optimus-cli: parse_policy_mode"):
        if arm.count("=>") != 1:
            fail(f"optimus-cli: cannot classify the policy arm {arm!r}")
        pattern, result = arm.split("=>", 1)
        literals = re.findall(r'"([a-z_\-]+)"', pattern)
        variants = re.findall(r"Ok\(PolicyMode::(\w+)\)", result)
        is_error = bool(re.search(r"\bErr\(", result))
        if len(variants) + int(is_error) != 1:
            fail(f"optimus-cli: cannot classify the policy arm {arm!r}")
        if variants and not literals:
            fail(f"optimus-cli: a non-literal arm selects PolicyMode::{variants[0]}")
        if variants:
            pattern_shape = re.sub(r'"[a-z_\-]+"', "", pattern)
            if re.sub(r"[|\s]", "", pattern_shape):
                fail(
                    f"optimus-cli: cannot classify the policy pattern "
                    f"{pattern.strip()!r}; only literal or-patterns may select a mode"
                )
        if variants == ["Unrestricted"]:
            words.extend(literals)
    return sorted(words)


def main() -> int:
    policy_source = read(POLICY)
    profiles = canonical_profiles(policy_source)
    if "standard" not in profiles or "unrestricted_host" not in profiles:
        fail(f"optimus-policy: unexpected profile vocabulary {profiles}")

    policy_table = parse_table(policy_source, "optimus-policy")
    graph_table = parse_table(read(GRAPH), "optimus-graph")

    # The CLI's own break-glass word lives in the graph table only; every other
    # spelling must mean the same thing in both crates. The exemption runs one
    # way: optimus-policy gaining `yolo` is drift too.
    shared = set(policy_table) | (set(graph_table) - GRAPH_ONLY)
    drifted = sorted(
        word for word in shared if policy_table.get(word) != graph_table.get(word)
    )
    if drifted:
        fail(
            "optimus-policy and optimus-graph disagree on "
            + ", ".join(
                f"{w!r} ({policy_table.get(w)} vs {graph_table.get(w)})" for w in drifted
            )
        )

    # An allowlist, not a blocklist: whatever reaches break-glass has to be one
    # of these words, including words this file's author never imagined (#118).
    for table, crate, allowed in (
        (policy_table, "optimus-policy", BREAK_GLASS),
        (graph_table, "optimus-graph", BREAK_GLASS | GRAPH_ONLY),
    ):
        reaching = {w for w, v in table.items() if v == "UnrestrictedHost"}
        if reaching != allowed:
            fail(
                f"{crate}: {sorted(reaching)} parse to UnrestrictedHost; break-"
                f"glass answers to exactly {sorted(allowed)} and nothing else (#118)"
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
    defaults = composer_defaults(store)
    wrong = [f"{name}={value}" for name, value in defaults if value != "standard"]
    if wrong:
        fail(f"composerStore.ts defaults to {wrong}; standard is the default profile")

    # What a stored value restores to, on both composers. Break-glass is not
    # durable (ADR-0044 §5), so no alias may resolve to it — a legacy word that
    # restored straight to unrestricted host would reopen #118 through
    # persistence instead of through the menu.
    desktop_js = read(DESKTOP_JS)
    restore_contract(
        store,
        "composerStore.ts",
        r"\baccess:\s*restoredAccess\(",
    )
    restore_contract(
        desktop_js,
        "optimus-desktop/ui/app.js",
        r"\$\('access'\)\.value\s*=\s*restoredAccess\(",
    )
    restores: list[tuple[str, str]] = []
    for source, surface in ((store, "composerStore.ts"), (desktop_js, "app.js")):
        for stored, restored in alias_table(source, surface):
            restores.append((stored, restored))
            if restored == "unrestricted_host":
                fail(
                    f"{surface} restores {stored!r} to break-glass; ADR-0044 "
                    "§5 keeps unrestricted host out of anything durable"
                )
            if restored not in profiles:
                fail(
                    f"{surface} restores {stored!r} to {restored!r}, which is "
                    "not a profile"
                )

    cli_words = cli_policy_words(read(CLI_PARSERS))
    if cli_words != ["unrestricted"]:
        fail(
            f"optimus-cli/src/parsers.rs lets {cli_words} name "
            "PolicyMode::Unrestricted; only 'unrestricted' may (#118)"
        )

    print(
        f"AUTONOMY_PROFILES_OK profiles={len(profiles)} react={len(values)} "
        f"desktop={len(desktop)} first={values[0]} last={values[-1]} "
        f"spellings={len(policy_table)} defaults={len(defaults)} "
        f"aliases={len(restores)}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
