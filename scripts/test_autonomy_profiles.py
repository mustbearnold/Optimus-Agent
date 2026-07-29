#!/usr/bin/env python3
"""Self-tests for the autonomy-profile gate.

The first test is the point of the whole file: the tree as issue #118 found it
must fail this gate. A gate that only passes on the fixed tree proves nothing
about the regression it exists to catch.
"""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location(
    "check_autonomy_profiles", ROOT / "scripts" / "check-autonomy-profiles.py"
)
GATE = importlib.util.module_from_spec(_spec)
assert _spec.loader is not None
_spec.loader.exec_module(GATE)

POLICY_TEMPLATE = """
impl AutonomyProfile {{
    pub fn as_str(self) -> &'static str {{
        match self {{
            Self::Standard => "standard",
            Self::ReviewChanges => "review_changes",
            Self::ReadOnly => "read_only",
            Self::FullProject => "full_project",
            Self::UnrestrictedHost => "unrestricted_host",
        }}
    }}

    pub fn parse(raw: &str) -> Option<Self> {{
        match raw.trim().to_ascii_lowercase().as_str() {{
            "standard" | "std" => Some(Self::Standard),
            "review_changes" | "ask" => Some(Self::ReviewChanges),
            "read_only" | "read" => Some(Self::ReadOnly),
            "full_project" => Some(Self::FullProject),
            {break_glass} => Some(Self::UnrestrictedHost),
            _ => None,
        }}
    }}
}}
"""

MENU_TEMPLATE = """
const accessOptions = [
{items}
] as const;

const accessTiers = [
{tiers}
] as const;
"""

DEFAULT_TIERS = "\n".join(
    f"  {{ tier: '{tier}', heading: '{tier}' }},"
    for tier in ("primary", "advanced", "expert")
)

DESKTOP_TEMPLATE = """
<select id="access" tabindex="-1">
{options}
</select>
"""

FIVE_DESKTOP_OPTIONS = "\n".join(
    f'  <option value="{value}"{" selected" if value == "standard" else ""}>x</option>'
    for value in (
        "standard",
        "review_changes",
        "read_only",
        "full_project",
        "unrestricted_host",
    )
)

ITEM = "  {{ value: '{value}', label: 'x', hint: 'y', icon: 'shield', tier: '{tier}' }},"

FIVE_ITEMS = "\n".join(
    ITEM.format(value=value, tier=tier)
    for value, tier in (
        ("standard", "primary"),
        ("review_changes", "primary"),
        ("read_only", "primary"),
        ("full_project", "advanced"),
        ("unrestricted_host", "expert"),
    )
)

STORE_TEMPLATE = """
type ComposerSettings = {{ access: string }};
const ACCESS_ALIASES: Readonly<Record<string, string>> = {prototype}{{
  standard: 'standard',
  review_changes: 'review_changes',
  ask: 'review_changes',
  read_only: 'read_only',
  read: 'read_only',
  full_project: '{alias}',
}}{close};
export function restoredAccess(raw: unknown): string {{
  if (typeof raw !== 'string') return 'standard';
  return ACCESS_ALIASES[raw] ?? 'standard';
}}
export const offlineComposer: ComposerSettings = {{
  access: '{first}',
}};
export const codexComposer: ComposerSettings = {{
  access: '{second}',
}};
export function loadComposer(parsed: Record<string, unknown>) {{
  return {{ settings: {{ access: restoredAccess(parsed.access) }} }};
}}
"""

NULL_PROTOTYPE = ("Object.assign(Object.create(null), ", ")")

DESKTOP_JS_TEMPLATE = """
const ACCESS_ALIASES = Object.assign(Object.create(null), {{
  standard: 'standard',
  review_changes: 'review_changes',
  ask: 'review_changes',
  read_only: 'read_only',
  read: 'read_only',
  full_project: '{alias}',
}});
function restoredAccess(raw) {{
  if (typeof raw !== 'string') return '{fallback}';
  return ACCESS_ALIASES[raw] || '{fallback}';
}}
function restoreComposer(c) {{
  $('access').value = {restore};
}}
"""

CLI_TEMPLATE = """
pub fn parse_policy_mode(policy: &str) -> Result<PolicyMode, Error> {{
    match policy.to_ascii_lowercase().as_str() {{
        "smart_deny" | "deny" => Ok(PolicyMode::SmartDeny),
        {unrestricted} => Ok(PolicyMode::Unrestricted),
        other => Err(format!("unknown policy {{other}}")),
    }}
}}
"""


class AutonomyGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = tempfile.TemporaryDirectory()
        root = Path(self.dir.name)
        GATE.POLICY = root / "policy.rs"
        GATE.GRAPH = root / "graph.rs"
        GATE.COMPOSER = root / "Composer.tsx"
        GATE.STORE = root / "composerStore.ts"
        GATE.DESKTOP = root / "index.html"
        GATE.DESKTOP_JS = root / "app.js"
        GATE.CLI_PARSERS = root / "parsers.rs"
        self.write()

    def tearDown(self) -> None:
        self.dir.cleanup()

    def write(
        self,
        *,
        policy_break_glass: str = '"unrestricted_host" | "unrestricted"',
        graph_break_glass: str | None = None,
        menu: str = FIVE_ITEMS,
        tiers: str = DEFAULT_TIERS,
        store: tuple[str, str] = ("standard", "standard"),
        alias: str = "full_project",
        desktop: str = FIVE_DESKTOP_OPTIONS,
        prototype: tuple[str, str] = NULL_PROTOTYPE,
        desktop_alias: str = "full_project",
        desktop_restore: str = "restoredAccess(c.access)",
        desktop_fallback: str = "standard",
        cli_unrestricted: str = '"unrestricted"',
    ) -> None:
        GATE.POLICY.write_text(POLICY_TEMPLATE.format(break_glass=policy_break_glass))
        GATE.GRAPH.write_text(
            POLICY_TEMPLATE.format(
                break_glass=graph_break_glass
                if graph_break_glass is not None
                else f'{policy_break_glass} | "yolo"'
            )
        )
        GATE.COMPOSER.write_text(MENU_TEMPLATE.format(items=menu, tiers=tiers))
        GATE.STORE.write_text(
            STORE_TEMPLATE.format(
                first=store[0],
                second=store[1],
                alias=alias,
                prototype=prototype[0],
                close=prototype[1],
            )
        )
        GATE.DESKTOP.write_text(DESKTOP_TEMPLATE.format(options=desktop))
        GATE.DESKTOP_JS.write_text(
            DESKTOP_JS_TEMPLATE.format(
                alias=desktop_alias,
                restore=desktop_restore,
                fallback=desktop_fallback,
            )
        )
        GATE.CLI_PARSERS.write_text(
            CLI_TEMPLATE.format(unrestricted=cli_unrestricted)
        )

    def assert_fails(self, reason: str) -> None:
        with self.assertRaises(SystemExit) as caught:
            GATE.main()
        self.assertEqual(caught.exception.code, 1, reason)

    def test_the_tree_that_issue_118_found_fails(self) -> None:
        """Full access first, and 'full' meaning the whole host."""
        self.write(
            policy_break_glass='"unrestricted_host" | "unrestricted" | "full" | "host"',
            menu="\n".join(
                ITEM.format(value=value, tier="primary")
                for value in ("full", "ask", "read")
            ),
        )
        self.assert_fails("the pre-fix composer must not pass this gate")

    def test_an_ordinary_word_aliased_to_break_glass_fails(self) -> None:
        self.write(policy_break_glass='"unrestricted_host" | "full"')
        self.assert_fails("'full' must not parse to UnrestrictedHost")

    def test_something_other_than_standard_rendering_first_fails(self) -> None:
        """Order within the recommended tier is the order a human sees."""
        reordered = "\n".join(
            ITEM.format(value=value, tier=tier)
            for value, tier in (
                ("read_only", "primary"),
                ("standard", "primary"),
                ("review_changes", "primary"),
                ("full_project", "advanced"),
                ("unrestricted_host", "expert"),
            )
        )
        self.write(menu=reordered)
        self.assert_fails("standard must render first, not merely exist")

    def test_break_glass_outside_the_expert_tier_fails(self) -> None:
        demoted = FIVE_ITEMS.replace(
            "value: 'unrestricted_host', label: 'x', hint: 'y', icon: 'shield', tier: 'expert'",
            "value: 'unrestricted_host', label: 'x', hint: 'y', icon: 'shield', tier: 'primary'",
        )
        self.write(menu=demoted)
        self.assert_fails("break-glass must sit under Expert")

    def test_a_missing_profile_fails(self) -> None:
        self.write(menu="\n".join(FIVE_ITEMS.splitlines()[:4]))
        self.assert_fails("a menu missing a profile must fail")

    def test_crates_that_disagree_fail(self) -> None:
        self.write(
            policy_break_glass='"unrestricted_host" | "unrestricted"',
            graph_break_glass='"unrestricted_host" | "unrestricted" | "ask"',
        )
        self.assert_fails("'ask' meaning two things across crates must fail")

    def test_a_default_other_than_standard_fails(self) -> None:
        self.write(store=("standard", "unrestricted_host"))
        self.assert_fails("a stored default of unrestricted_host must fail")

    # Everything below reproduces an attack an independent reviewer landed on
    # the first version of this gate. Each one passed it; each one now fails.

    def test_reordering_the_tiers_puts_break_glass_first(self) -> None:
        """The rendered order is tiers x options, not options alone."""
        self.write(
            tiers="\n".join(
                f"  {{ tier: '{tier}', heading: '{tier}' }},"
                for tier in ("expert", "advanced", "primary")
            )
        )
        self.assert_fails("expert tier rendered first must fail")

    def test_a_second_option_in_double_quotes_is_still_read(self) -> None:
        smuggled = (
            '  { value: "unrestricted_host", label: "Full access", '
            "hint: 'y', icon: 'shield', tier: \"primary\" },\n" + FIVE_ITEMS
        )
        self.write(menu=smuggled)
        self.assert_fails("a double-quoted duplicate option must not hide")

    def test_a_guarded_arm_cannot_smuggle_break_glass(self) -> None:
        self.write(policy_break_glass='"unrestricted_host" if true')
        self.assert_fails("a match guard must not be invisible")

    def test_a_wrapped_or_pattern_cannot_smuggle_break_glass(self) -> None:
        self.write(
            policy_break_glass=(
                '"unrestricted_host"\n'
                '                | "unrestricted"\n'
                '                | "surprise"'
            )
        )
        self.assert_fails("a literal on a wrapped line must still be classified")

    def test_a_catch_all_that_returns_a_profile_fails(self) -> None:
        source = POLICY_TEMPLATE.format(break_glass='"unrestricted_host"').replace(
            "_ => None,", "_ => Some(Self::UnrestrictedHost),"
        )
        GATE.POLICY.write_text(source)
        GATE.GRAPH.write_text(source)
        self.assert_fails("every unknown string meaning break-glass must fail")

    def test_a_fully_qualified_variant_is_still_classified(self) -> None:
        source = POLICY_TEMPLATE.format(break_glass='"unrestricted_host"').replace(
            '"full_project" => Some(Self::FullProject),',
            '"full_project" | "full" => Some(AutonomyProfile::UnrestrictedHost),',
        )
        GATE.POLICY.write_text(source)
        GATE.GRAPH.write_text(source)
        self.assert_fails("Self:: and AutonomyProfile:: must be read alike")

    def test_a_stored_alias_restoring_to_break_glass_fails(self) -> None:
        self.write(alias="unrestricted_host")
        self.assert_fails("no stored word may restore to break-glass")

    def test_an_alias_table_on_the_object_prototype_fails(self) -> None:
        """'constructor' is a stored value like any other."""
        self.write(prototype=("", ""))
        self.assert_fails("a plain literal answers for words nobody put in it")

    def test_desktop_persistence_cannot_restore_break_glass(self) -> None:
        self.write(desktop_alias="unrestricted_host")
        self.assert_fails("the Wry composer must not persist break-glass")

    def test_desktop_persistence_must_use_the_restore_filter(self) -> None:
        self.write(desktop_restore="c.access")
        self.assert_fails("the Wry composer must filter persisted access")

    def test_desktop_restore_fallback_must_be_standard(self) -> None:
        self.write(desktop_fallback="unrestricted_host")
        self.assert_fails("unknown persisted access must not become break-glass")

    def test_cli_open_cannot_turn_off_effect_checks(self) -> None:
        self.write(cli_unrestricted='"unrestricted" | "open"')
        self.assert_fails("only an explicit unrestricted word may disable policy")

    def test_cli_nonliteral_cannot_turn_off_effect_checks(self) -> None:
        self.write(cli_unrestricted='"unrestricted" | other')
        self.assert_fails("a catch-all policy pattern must not disable policy")

    def test_the_desktop_composer_is_checked_too(self) -> None:
        self.write(
            desktop='  <option value="smart_deny">x</option>\n'
            '  <option value="full" selected>Full</option>\n'
            '  <option value="read_only">x</option>'
        )
        self.assert_fails("the Wry composer's own #118 shape must fail")

    def test_a_desktop_default_other_than_standard_fails(self) -> None:
        self.write(
            desktop=FIVE_DESKTOP_OPTIONS.replace(
                '<option value="standard" selected>', '<option value="standard">'
            ).replace(
                '<option value="unrestricted_host">',
                '<option value="unrestricted_host" selected>',
            )
        )
        self.assert_fails("a pre-selected break-glass option must fail")

    def test_the_fixed_shape_passes(self) -> None:
        self.assertEqual(GATE.main(), 0)


if __name__ == "__main__":
    unittest.main()
