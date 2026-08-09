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
  - Every stored default is `standard`, in every React preset, and neither
    composer restores a stored value to break-glass: ADR-0044 §5 keeps
    unrestricted host out of anything durable.
  - The Wry composer renders Full project under Advanced and Unrestricted host
    under a warning-treated Expert heading; its legacy `smart_deny` value
    restores to Review changes, while legacy `full` restores to Standard.
  - Only `unrestricted` names `PolicyMode::Unrestricted` on the CLI.

Anything this file cannot classify is a failure, not a pass: an arm the parser
does not understand is an arm nobody is checking.

Exit 0 on success; print the violation and exit 1 otherwise.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

POLICY = ROOT / "crates/optimus-policy/src/lib.rs"
GRAPH = ROOT / "crates/optimus-graph/src/lib.rs"
COMPOSER = ROOT / "apps/optimus-ui/src/components/workbench/Composer.tsx"
STORE = ROOT / "apps/optimus-ui/src/state/composerStore.ts"
DESKTOP = ROOT / "apps/optimus-desktop/ui/index.html"
DESKTOP_JS = ROOT / "apps/optimus-desktop/ui/app.js"
DESKTOP_STYLE = ROOT / "apps/optimus-desktop/ui/style.css"
CLI_PARSERS = ROOT / "apps/optimus-cli/src/parsers.rs"

# The complete set of words that may reach break-glass, per crate. Anything
# else mapping to `UnrestrictedHost` fails, including words nobody here
# thought of — which is the point. `yolo` is the CLI's own flag and lives in
# the graph crate only.
BREAK_GLASS = frozenset({"unrestricted_host", "unrestricted"})
GRAPH_ONLY = frozenset({"yolo"})
EXPECTED_ALIASES = {
    "standard": "standard",
    "review_changes": "review_changes",
    "smart_deny": "review_changes",
    "ask": "review_changes",
    "read_only": "read_only",
    "read": "read_only",
    "full_project": "full_project",
    "developer_full_access": "developer_full_access",
}
EXPECTED_TIERS = {
    "standard": "primary",
    "review_changes": "primary",
    "read_only": "primary",
    "full_project": "advanced",
    "developer_full_access": "developer",
    "unrestricted_host": "expert",
}


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


def strip_html_comments(source: str) -> str:
    """Remove balanced HTML comments so dead menu markup cannot satisfy a gate."""
    output: list[str] = []
    offset = 0
    while offset < len(source):
        opening = source.find("<!--", offset)
        closing = source.find("-->", offset)
        if closing >= 0 and (opening < 0 or closing < opening):
            fail("optimus-desktop/ui/index.html: unmatched HTML comment close")
        if opening < 0:
            output.append(source[offset:])
            break
        output.append(source[offset:opening])
        closing = source.find("-->", opening + 4)
        if closing < 0:
            fail("optimus-desktop/ui/index.html: unclosed HTML comment")
        comment = source[opening : closing + 3]
        output.append("\n" * comment.count("\n"))
        offset = closing + 3
    return "".join(output)


def braced_contents(
    source: str, opening: int, surface: str, rust: bool = False
) -> tuple[str, int]:
    """Return the contents and closing offset of one balanced `{ ... }`.

    The bounded source shapes checked here contain strings, so braces inside a
    quoted diagnostic must not end a Rust match or JavaScript object early.
    With `rust=True`, a lone `'` is a lifetime, not a quote, and a char
    literal (`'x'`, `'\n'`) is skipped as a unit — otherwise
    `&'static str` desynchronises the scanner for the rest of the impl.
    """
    if opening < 0 or opening >= len(source) or source[opening] != "{":
        fail(f"{surface}: could not find the opening brace")
    depth = 0
    quote: str | None = None
    escaped = False
    offset = opening
    limit = len(source)
    while offset < limit:
        char = source[offset]
        if quote is not None:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            offset += 1
            continue
        if rust and char == "'":
            if offset + 2 < limit and source[offset + 2] == "'":
                offset += 4 if source[offset + 1] == "\\" else 3
            else:
                offset += 1
            continue
        if char in {'"', "'", "`"}:
            quote = char
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1 : offset], offset
        offset += 1
    fail(f"{surface}: opening brace has no matching close")


def logical_statements(body: str, surface: str) -> list[str]:
    """Split a JavaScript block at top-level semicolons.

    A renderer contract must inspect statements that execute in the selected
    branch, not matching text hidden inside a nested function or conditional.
    Strings and nested callbacks may contain semicolons without ending their
    owning statement.
    """
    statements: list[str] = []
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
        if char in {'"', "'", "`"}:
            quote = char
        elif char in "({[":
            depth += 1
        elif char in ")}]":
            depth -= 1
            if depth < 0:
                fail(f"{surface}: statement has an unmatched closing delimiter")
        elif char == ";" and depth == 0:
            statement = body[start : offset + 1].strip()
            if statement:
                statements.append(statement)
            start = offset + 1
    if quote is not None or depth != 0:
        fail(f"{surface}: statement has an unclosed string or delimiter")
    trailing = body[start:].strip()
    if trailing:
        statements.append(trailing)
    if not statements:
        fail(f"{surface}: block has no statements")
    return statements


def match_body(source: str, signature: str, surface: str) -> str:
    """Extract the single top-level match body owned by a named function."""
    source = strip_comments(source)
    functions = list(re.finditer(signature, source, re.S))
    if len(functions) != 1:
        fail(f"{surface}: expected one matching function, found {len(functions)}")
    function_open = source.find("{", functions[0].end())
    function_body, _ = braced_contents(source, function_open, surface, rust=True)
    matches = list(re.finditer(r"\bmatch\b[^{}]*\{", function_body, re.S))
    if len(matches) != 1:
        fail(f"{surface}: expected one top-level match, found {len(matches)}")
    match_open = function_open + 1 + matches[0].end() - 1
    body, _ = braced_contents(source, match_open, surface, rust=True)
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
            # Rust permits omitting the comma between a block arm and the
            # following arm. Rustfmt removes an optional trailing comma here,
            # so the closing block is the reliable boundary.
            if char == "}" and depth == 0:
                arm = body[start : offset + 1].strip()
                if arm:
                    arms.append(arm)
                start = offset + 1
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

    The search is scoped to the AutonomyProfile impl: other types in the same
    crate (e.g. CommandFsEnvelope) legitimately define the same
    `fn parse(raw: &str) -> Option<Self>` shape.
    """
    table: dict[str, str] = {}
    stripped = strip_comments(source)
    impl = re.search(r"impl\s+AutonomyProfile\s*\{", stripped)
    if not impl:
        fail(f"{crate}: could not find impl AutonomyProfile")
    impl_body, _ = braced_contents(
        stripped,
        stripped.find("{", impl.start()),
        f"{crate}: impl AutonomyProfile",
        rust=True,
    )
    body = match_body(
        impl_body,
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


def menu_options(source: str) -> list[tuple[str, str, str]]:
    """(value, tier, hint) for each access option in rendered order.

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

    options: list[tuple[str, str, str]] = []
    for entry in re.findall(r"\{([^{}]*)\}", block.group(1)):
        value = re.search(rf"\bvalue:\s*{QUOTED}", entry)
        tier = re.search(rf"\btier:\s*{QUOTED}", entry)
        hint = re.search(r"\bhint:\s*(['\"])(.*?)\1", entry, re.S)
        if not value or not tier or not hint or not hint.group(2).strip():
            fail(
                f"Composer.tsx: the access option {entry.strip()!r} does not "
                "state a value, tier, and human-readable hint"
            )
        options.append((value.group(1), tier.group(1), hint.group(2).strip()))
    if not options:
        fail("Composer.tsx: accessOptions has no value/tier pairs")

    tier_block = re.search(r"const accessTiers = \[(.*?)\n\] as const;", source, re.S)
    if not tier_block:
        fail("Composer.tsx: could not find accessTiers")
    tier_order = re.findall(rf"tier:\s*{QUOTED}", tier_block.group(1))
    if not tier_order:
        fail("Composer.tsx: accessTiers names no tiers")
    unknown = sorted({tier for _, tier, _ in options} - set(tier_order))
    if unknown:
        fail(f"Composer.tsx: options sit in tiers that never render: {unknown}")

    return [
        (value, tier, hint)
        for rendered_tier in tier_order
        for value, tier, hint in options
        if tier == rendered_tier
    ]


def desktop_options(source: str) -> list[tuple[str, str, bool, str]]:
    """(value, tier, warning, hint) for the Wry access select in render order."""
    source = strip_html_comments(source)
    blocks = re.findall(r'<select id="access".*?</select>', source, re.S)
    if not blocks:
        fail("optimus-desktop/ui/index.html: could not find the access select")
    if len(blocks) != 1:
        fail(
            f"optimus-desktop/ui/index.html: {len(blocks)} access selects; the "
            "one the send path reads must be the only one"
        )
    entries: list[tuple[str, str, bool, str]] = []
    selected: list[str] = []
    for attributes in re.findall(r"<option\b([^>]*)>", blocks[0]):
        value = re.search(r'\bvalue="([a-z_]+)"', attributes)
        tier = re.search(r'\bdata-tier="([a-z_]+)"', attributes)
        hint = re.search(r'\bdata-hint="([^"]+)"', attributes)
        if not value or not tier or not hint or not hint.group(1).strip():
            fail(
                "optimus-desktop/ui/index.html: every access option must state "
                "a canonical value, visible tier, and explanatory hint"
            )
        entries.append(
            (
                value.group(1),
                tier.group(1),
                'data-warning="true"' in attributes,
                hint.group(1).strip(),
            )
        )
        if re.search(r"(?:^|\s)selected(?:\s|=|$)", attributes):
            selected.append(value.group(1))
    if not entries:
        fail("optimus-desktop/ui/index.html: the access select has no options")
    if selected != ["standard"]:
        fail(
            "optimus-desktop/ui/index.html: the access select pre-selects "
            f"{selected or ['(nothing)']}; standard is the default profile"
        )
    return entries


def desktop_render_contract(source: str, style: str) -> None:
    """Hold tier, hint, and warning rendering in executed access statements."""
    source = strip_comments(source)
    style = strip_comments(style)
    functions = list(re.finditer(r"\bfunction\s+buildCddHtml\(kind\)\s*\{", source))
    if len(functions) != 1:
        fail(
            "optimus-desktop/ui/app.js: expected one buildCddHtml(kind) "
            f"function, found {len(functions)}"
        )
    function_body, _ = braced_contents(
        source,
        functions[0].end() - 1,
        "optimus-desktop/ui/app.js: buildCddHtml",
    )
    access_branches = list(
        re.finditer(
            r"\bif\s*\(\s*kind\s*===\s*(['\"])access\1\s*\)\s*\{",
            function_body,
        )
    )
    if len(access_branches) != 1:
        fail(
            "optimus-desktop/ui/app.js: expected one live access-render branch, "
            f"found {len(access_branches)}"
        )
    access_body, _ = braced_contents(
        function_body,
        access_branches[0].end() - 1,
        "optimus-desktop/ui/app.js: access render branch",
    )
    statements = logical_statements(
        access_body, "optimus-desktop/ui/app.js: access render branch"
    )

    groups = [
        statement
        for statement in statements
        if re.fullmatch(r"const\s+groups\s*=\s*\[\]\s*;", statement)
    ]
    collectors = [
        statement
        for statement in statements
        if re.match(
            r"Array\.from\(\$\((['\"])access\1\)\.options\)"
            r"\.forEach\(\(o\)\s*=>\s*\{",
            statement,
        )
    ]
    renders = [
        statement
        for statement in statements
        if re.match(r"return\s+groups\.map\(\(group\)\s*=>\s*\{", statement)
    ]
    if len(statements) != 3 or len(groups) != 1 or len(collectors) != 1 or len(renders) != 1:
        fail(
            "optimus-desktop/ui/app.js: access rendering must execute one "
            "groups declaration, one access-option collector, and one grouped "
            "return; nested or unrelated matching code does not count"
        )

    collector_open = collectors[0].find("{")
    collector_body, _ = braced_contents(
        collectors[0],
        collector_open,
        "optimus-desktop/ui/app.js: access option collector",
    )
    required_collector = (
        "o.dataset.tier",
        "groups.push(group)",
        "group.options.push(o)",
    )
    missing_collector = [
        token for token in required_collector if token not in collector_body
    ]
    if missing_collector:
        fail(
            "optimus-desktop/ui/app.js: live access option collection omits "
            f"{missing_collector}"
        )

    render_open = renders[0].find("{")
    render_body, _ = braced_contents(
        renders[0], render_open, "optimus-desktop/ui/app.js: grouped access return"
    )
    render_statements = logical_statements(
        render_body, "optimus-desktop/ui/app.js: grouped access return"
    )
    headings = [
        statement
        for statement in render_statements
        if re.match(r"const\s+heading\s*=", statement)
    ]
    option_maps = [
        statement
        for statement in render_statements
        if re.match(
            r"const\s+options\s*=\s*group\.options\.map\(\(o\)\s*=>\s*\{",
            statement,
        )
    ]
    group_returns = [
        statement
        for statement in render_statements
        if re.match(r"return\s+", statement)
    ]
    if len(headings) != 1 or len(option_maps) != 1 or len(group_returns) != 1:
        fail(
            "optimus-desktop/ui/app.js: grouped access return must directly "
            "build one heading, one option map, and one tier wrapper"
        )
    required_heading = ('class="cdd-sec"', 'data-tier="${esc(group.tier)}"')
    if any(token not in headings[0] for token in required_heading):
        fail("optimus-desktop/ui/app.js: access tier heading is not rendered live")
    required_group_return = (
        'class="cdd-access-tier"',
        'role="group"',
        'aria-label="${esc(tierLabel)}"',
        "${heading}",
        "${options}",
    )
    missing_group_return = [
        token for token in required_group_return if token not in group_returns[0]
    ]
    if missing_group_return:
        fail(
            "optimus-desktop/ui/app.js: live tier wrapper omits "
            f"{missing_group_return}"
        )

    option_open = option_maps[0].find("{")
    option_body, _ = braced_contents(
        option_maps[0], option_open, "optimus-desktop/ui/app.js: access option map"
    )
    option_statements = logical_statements(
        option_body, "optimus-desktop/ui/app.js: access option map"
    )
    required_option_statements = {
        "warning": ("o.dataset.warning", "'true'"),
        "hint": ("o.dataset.hint",),
        "active": ("o.value === $('access').value",),
        "warningClass": ("warning", "access-warning"),
        "risk": ("warning", "access-risk"),
        "label": ("o.textContent", "hint"),
    }
    for name, tokens in required_option_statements.items():
        declarations = [
            statement
            for statement in option_statements
            if re.match(rf"const\s+{name}\s*=", statement)
        ]
        missing = (
            list(tokens)
            if len(declarations) != 1
            else [token for token in tokens if token not in declarations[0]]
        )
        if missing:
            fail(
                "optimus-desktop/ui/app.js: live access option rendering must "
                f"derive {name} from {missing}"
            )
    option_returns = [
        statement for statement in option_statements if re.match(r"return\s+", statement)
    ]
    if len(option_returns) != 1:
        fail("optimus-desktop/ui/app.js: access option map must return one live option")
    required_option_return = (
        'role="option"',
        'aria-label="${esc(label)}"',
        'aria-selected="${active',
        "warningClass",
        "${risk}",
        'data-tier="${esc(group.tier)}"',
        'data-v="${esc(o.value)}"',
        'class="access-copy"',
        'class="access-hint"',
    )
    missing_option_return = [
        token for token in required_option_return if token not in option_returns[0]
    ]
    if missing_option_return:
        fail(
            "optimus-desktop/ui/app.js: live access option return omits "
            f"{missing_option_return}"
        )

    required_style = (
        "#cddPortal .cdd-access-tier",
        '#cddPortal .cdd-sec[data-tier="expert"]',
        "#cddPortal button.access-warning",
        "#cddPortal .access-copy",
        "#cddPortal .access-hint",
        "var(--warn)",
    )
    missing_style = [token for token in required_style if token not in style]
    if missing_style:
        fail(
            "optimus-desktop/ui/style.css: break-glass lacks visible warning "
            f"treatment {missing_style}"
        )


def alias_table(source: str, surface: str) -> list[tuple[str, str]]:
    """(stored word, restored profile) pairs, and the shape they are held in.

    Both composers persist the composer and read it back on boot. ADR-0044 §5
    keeps break-glass out of anything durable, so what a stored value restores
    to is as much a part of this vocabulary as what the menu offers.
    """
    source = strip_comments(source)
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
    restores: list[tuple[str, str]] = []
    property_pattern = re.compile(
        r"(?:(?P<bare>[a-z_]+)|(?P<key_quote>['\"])(?P<quoted>[a-z_]+)"
        r"(?P=key_quote))\s*:\s*(?P<value_quote>['\"])(?P<value>[a-z_]+)"
        r"(?P=value_quote)"
    )
    for entry in logical_arms(aliases, f"{surface}: ACCESS_ALIASES"):
        parsed = property_pattern.fullmatch(entry.strip())
        if not parsed:
            fail(
                f"{surface}: cannot classify ACCESS_ALIASES entry {entry.strip()!r}; "
                "only explicit bare or quoted string keys may migrate persisted access"
            )
        restores.append(
            (parsed.group("bare") or parsed.group("quoted"), parsed.group("value"))
        )
    if not restores:
        fail(f"{surface}: ACCESS_ALIASES maps nothing")
    keys = [stored for stored, _ in restores]
    if len(keys) != len(set(keys)):
        fail(f"{surface}: ACCESS_ALIASES repeats a stored spelling")
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
    expected = {"autoComposer", "offlineComposer", "codexComposer"}
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
    values = [value for value, _, _ in options]
    if values != sorted(values, key=profiles.index) or set(values) != set(profiles):
        fail(
            f"Composer.tsx offers {values}, which is not the profile "
            f"vocabulary in declaration order {profiles}"
        )
    if values[0] != "standard":
        fail(f"Composer.tsx offers {values[0]!r} first; standard must come first")
    if values[-1] != "unrestricted_host":
        fail(f"Composer.tsx offers {values[-1]!r} last; unrestricted_host must come last")

    tiers = {value: tier for value, tier, _ in options}
    if tiers != EXPECTED_TIERS:
        fail(
            f"Composer.tsx assigns access tiers {tiers}; expected exactly "
            f"{EXPECTED_TIERS}"
        )
    hints = {value: hint for value, _, hint in options}
    for value in values:
        if value not in policy_table:
            fail(f"Composer.tsx offers {value!r}, which optimus-policy cannot parse")

    desktop_options_with_tiers = desktop_options(read(DESKTOP))
    desktop = [value for value, _, _, _ in desktop_options_with_tiers]
    if desktop != profiles:
        fail(
            f"optimus-desktop/ui/index.html offers {desktop}, not the profile "
            f"vocabulary in declaration order {profiles}"
        )
    desktop_tiers = {
        value: tier for value, tier, _, _ in desktop_options_with_tiers
    }
    if desktop_tiers != EXPECTED_TIERS:
        fail(
            "optimus-desktop/ui/index.html: access tiers are "
            f"{desktop_tiers}; expected exactly {EXPECTED_TIERS}"
        )
    desktop_hints = {
        value: hint for value, _, _, hint in desktop_options_with_tiers
    }
    if desktop_hints != hints:
        fail(
            "optimus-desktop/ui/index.html: access explanations differ from "
            f"Composer.tsx ({desktop_hints} vs {hints})"
        )
    warned = [
        value for value, _, warning, _ in desktop_options_with_tiers if warning
    ]
    if warned != ["unrestricted_host"]:
        fail(
            "optimus-desktop/ui/index.html: visible warning treatment must name "
            f"only unrestricted_host, not {warned}"
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
    desktop_render_contract(desktop_js, read(DESKTOP_STYLE))
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
        aliases = alias_table(source, surface)
        actual_aliases = dict(aliases)
        if actual_aliases != EXPECTED_ALIASES:
            fail(
                f"{surface}: persisted access migrations are {actual_aliases}; "
                f"expected exactly {EXPECTED_ALIASES}"
            )
        for stored, restored in aliases:
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
