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

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location(
    "check_autonomy_profiles", ROOT / "scripts" / "gates" / "check-autonomy-profiles.py"
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
            Self::DeveloperFullAccess => "developer_full_access",
            Self::UnrestrictedHost => "unrestricted_host",
        }}
    }}

    pub fn parse(raw: &str) -> Option<Self> {{
        match raw.trim().to_ascii_lowercase().as_str() {{
            "standard" | "std" => Some(Self::Standard),
            "review_changes" | "ask" => Some(Self::ReviewChanges),
            "read_only" | "read" => Some(Self::ReadOnly),
            "full_project" => Some(Self::FullProject),
            "developer_full_access" => Some(Self::DeveloperFullAccess),
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
    for tier in ("primary", "advanced", "developer", "expert")
)

DESKTOP_TEMPLATE = """
<select id="access" tabindex="-1">
{options}
</select>
"""

FIVE_DESKTOP_OPTIONS = "\n".join(
    '  <option value="{value}" data-tier="{tier}" data-hint="{hint}"{warning}{selected}>x</option>'.format(
        value=value,
        tier=tier,
        hint=hint,
        warning=' data-warning="true"' if value == "unrestricted_host" else "",
        selected=" selected" if value == "standard" else "",
    )
    for value, tier, hint in (
        ("standard", "primary", "y"),
        ("review_changes", "primary", "y"),
        ("read_only", "primary", "y"),
        ("full_project", "advanced", "y"),
        ("developer_full_access", "developer", "y"),
        ("unrestricted_host", "expert", "y"),
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
        ("developer_full_access", "developer"),
        ("unrestricted_host", "expert"),
    )
)

# After the AutonomyProfile collapse (issue #143), optimus-graph carries no
# parse table of its own: it re-exports the canonical type from optimus-policy.
GRAPH_STUB = "\npub use optimus_policy::AutonomyProfile;\n"

STORE_TEMPLATE = """
type ComposerSettings = {{ access: string }};
const ACCESS_ALIASES: Readonly<Record<string, string>> = {prototype}{{
  standard: 'standard',
  review_changes: 'review_changes',
  smart_deny: 'review_changes',
  ask: 'review_changes',
  read_only: 'read_only',
  read: 'read_only',
  full_project: '{alias}',
  developer_full_access: 'developer_full_access',
{extra_alias}
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
export const autoComposer: ComposerSettings = {{
  access: '{third}',
}};
export function loadComposer(parsed: Record<string, unknown>) {{
  return {{ settings: {{ access: restoredAccess(parsed.access) }} }};
}}
"""

NULL_PROTOTYPE = ("Object.assign(Object.create(null), ", ")")

ACCESS_RENDER_TEMPLATE = """
  if (kind === 'access') {
    const groups = [];
    Array.from($('access').options).forEach((o) => {
      const tier = o.dataset.tier || '';
      let group = groups[groups.length - 1];
      if (!group || group.tier !== tier) {
        group = { tier, options: [] };
        groups.push(group);
      }
      group.options.push(o);
    });
    return groups.map((group) => {
      const tierLabel = group.tier === 'primary' ? 'Recommended' : group.tier;
      const heading = group.tier === 'primary'
        ? ''
        : `<div class="cdd-sec" data-tier="${esc(group.tier)}">${esc(tierLabel)}</div>`;
      const options = group.options.map((o) => {
        const warning = o.dataset.warning === 'true';
        const hint = o.dataset.hint || '';
        const active = o.value === $('access').value;
        const warningClass = warning ? ' access-warning' : '';
        const risk = warning ? '<span class="access-risk">!</span>' : '';
        const label = `${o.textContent}. ${hint}`;
        return `<button role="option" aria-label="${esc(label)}" aria-selected="${active ? 'true' : 'false'}" class="${warningClass}" data-tier="${esc(group.tier)}" data-v="${esc(o.value)}">${risk}<span class="access-copy"><small class="access-hint">${esc(hint)}</small></span></button>`;
      }).join('');
      return `<div class="cdd-access-tier" role="group" aria-label="${esc(tierLabel)}">${heading}${options}</div>`;
    }).join('');
  }
"""

DEAD_ACCESS_RENDER = """
function unusedAccessRenderer(o) {
  const tier = o.dataset.tier;
  const warning = o.dataset.warning;
  const hint = o.dataset.hint;
  return '<div class="cdd-access-tier" role="group" aria-label="x">' +
    '<div class="cdd-sec">' + tier + '</div>' +
    '<button aria-selected="false" class="access-warning">' +
    '<span class="access-risk">!</span><span class="access-copy">' +
    '<small class="access-hint">' + hint + warning + '</small></span></button></div>';
}
"""

NESTED_DEAD_ACCESS_RENDER = """
  if (kind === 'access') {
    function unusedAccessRenderer() {
      const groups = [];
      Array.from($('access').options).forEach((o) => {
        const tier = o.dataset.tier;
        let group = { tier, options: [] };
        groups.push(group);
        group.options.push(o);
      });
      return groups.map((group) => {
        const heading = `<div class="cdd-sec" data-tier="${esc(group.tier)}"></div>`;
        const options = group.options.map((o) => {
          const warning = o.dataset.warning;
          const hint = o.dataset.hint;
          const active = o.value === $('access').value;
          const warningClass = warning ? 'access-warning' : '';
          const risk = warning ? '<span class="access-risk"></span>' : '';
          const label = o.textContent;
          return `<button role="option" aria-label="${esc(label)}" aria-selected="${active}" class="${warningClass}" data-tier="${esc(group.tier)}" data-v="${esc(o.value)}">${risk}<span class="access-copy"><small class="access-hint">${hint}</small></span></button>`;
        }).join('');
        return `<div class="cdd-access-tier" role="group" aria-label="${esc('Expert')}">${heading}${options}</div>`;
      }).join('');
    }
    return '<button>flat</button>';
  }
"""

DESKTOP_JS_TEMPLATE = """
const ACCESS_ALIASES = Object.assign(Object.create(null), {{
  standard: 'standard',
  review_changes: 'review_changes',
  smart_deny: 'review_changes',
  ask: 'review_changes',
  read_only: 'read_only',
  read: 'read_only',
  full_project: '{alias}',
  developer_full_access: 'developer_full_access',
{extra_alias}
}});
function restoredAccess(raw) {{
  if (typeof raw !== 'string') return '{fallback}';
  return ACCESS_ALIASES[raw] || '{fallback}';
}}
function restoreComposer(c) {{
  $('access').value = {restore};
}}
function buildCddHtml(kind) {{
{access_render}
  return '';
}}
{extra_js}
"""

DESKTOP_STYLE_TEMPLATE = """
#cddPortal .cdd-access-tier {{ display: flex; }}
#cddPortal .cdd-sec[data-tier="expert"] {{ color: var(--warn); }}
#cddPortal button.access-warning {{ color: var(--warn); }}
#cddPortal .access-copy {{ display: flex; }}
#cddPortal .access-hint {{ color: var(--warn); }}
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
        GATE.DESKTOP_STYLE = root / "style.css"
        GATE.CLI_PARSERS = root / "parsers.rs"
        self.write()

    def tearDown(self) -> None:
        self.dir.cleanup()

    def write(
        self,
        *,
        policy_break_glass: str = '"unrestricted_host" | "unrestricted" | "unrestricted-host" | "yolo"',
        graph_source: str = GRAPH_STUB,
        menu: str = FIVE_ITEMS,
        tiers: str = DEFAULT_TIERS,
        store: tuple[str, str, str] = ("standard", "standard", "standard"),
        alias: str = "full_project",
        store_extra_alias: str = "",
        desktop: str = FIVE_DESKTOP_OPTIONS,
        prototype: tuple[str, str] = NULL_PROTOTYPE,
        desktop_alias: str = "full_project",
        desktop_extra_alias: str = "",
        desktop_restore: str = "restoredAccess(c.access)",
        desktop_fallback: str = "standard",
        desktop_access_render: str = ACCESS_RENDER_TEMPLATE,
        desktop_extra_js: str = "",
        desktop_style: str = DESKTOP_STYLE_TEMPLATE,
        cli_unrestricted: str = '"unrestricted"',
    ) -> None:
        GATE.POLICY.write_text(POLICY_TEMPLATE.format(break_glass=policy_break_glass))
        GATE.GRAPH.write_text(graph_source)
        GATE.COMPOSER.write_text(MENU_TEMPLATE.format(items=menu, tiers=tiers))
        GATE.STORE.write_text(
            STORE_TEMPLATE.format(
                first=store[0],
                second=store[1],
                third=store[2],
                alias=alias,
                extra_alias=store_extra_alias,
                prototype=prototype[0],
                close=prototype[1],
            )
        )
        GATE.DESKTOP.write_text(DESKTOP_TEMPLATE.format(options=desktop))
        GATE.DESKTOP_JS.write_text(
            DESKTOP_JS_TEMPLATE.format(
                alias=desktop_alias,
                extra_alias=desktop_extra_alias,
                restore=desktop_restore,
                fallback=desktop_fallback,
                access_render=desktop_access_render,
                extra_js=desktop_extra_js,
            )
        )
        GATE.DESKTOP_STYLE.write_text(desktop_style)
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

    def test_a_graph_parse_table_fails(self) -> None:
        """The collapse (issue #143) left one table; a second one is drift."""
        self.write(
            graph_source=POLICY_TEMPLATE.format(
                break_glass='"unrestricted_host" | "unrestricted"'
            )
        )
        self.assert_fails("a second AutonomyProfile parse table in graph must fail")

    def test_a_default_other_than_standard_fails(self) -> None:
        self.write(store=("standard", "unrestricted_host", "standard"))
        self.assert_fails("a stored default of unrestricted_host must fail")

    def test_the_auto_default_cannot_be_break_glass(self) -> None:
        self.write(store=("standard", "standard", "unrestricted_host"))
        self.assert_fails("the Auto preset must default to Standard")

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

    def test_a_rustfmt_block_arm_without_a_comma_is_classified(self) -> None:
        source = """
impl AutonomyProfile {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "standard" => {
                Some(Self::Standard)
            }
            "unrestricted_host" => Some(Self::UnrestrictedHost),
            _ => None,
        }
    }
}
"""
        self.assertEqual(
            GATE.parse_table(source, "fixture"),
            {"standard": "Standard", "unrestricted_host": "UnrestrictedHost"},
        )

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
        self.assert_fails("every unknown string meaning break-glass must fail")

    def test_a_fully_qualified_variant_is_still_classified(self) -> None:
        source = POLICY_TEMPLATE.format(break_glass='"unrestricted_host"').replace(
            '"full_project" => Some(Self::FullProject),',
            '"full_project" | "full" => Some(AutonomyProfile::UnrestrictedHost),',
        )
        GATE.POLICY.write_text(source)
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

    def test_react_legacy_full_must_restore_to_standard(self) -> None:
        self.write(store_extra_alias="  full: 'full_project',")
        self.assert_fails("legacy React full must not migrate to broader authority")

    def test_a_quoted_legacy_alias_cannot_hide_from_the_gate(self) -> None:
        self.write(store_extra_alias="  'full': 'full_project',")
        self.assert_fails("quoted persisted aliases must be classified too")

    def test_a_computed_legacy_alias_cannot_hide_from_the_gate(self) -> None:
        self.write(desktop_extra_alias='  ["full"]: "full_project",')
        self.assert_fails("computed persisted aliases must fail closed")

    def test_a_spread_alias_table_cannot_hide_from_the_gate(self) -> None:
        self.write(store_extra_alias="  ...legacyAliases,")
        self.assert_fails("spread persisted aliases must fail closed")

    def test_desktop_legacy_full_must_restore_to_standard(self) -> None:
        self.write(desktop_extra_alias="  full: 'full_project',")
        self.assert_fails("legacy Wry full must not migrate to broader authority")

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

    def test_a_commented_desktop_access_menu_is_not_live(self) -> None:
        commented = "<!--\n" + DESKTOP_TEMPLATE.format(options=FIVE_DESKTOP_OPTIONS) + "\n-->"
        with self.assertRaises(SystemExit) as caught:
            GATE.desktop_options(commented)
        self.assertEqual(caught.exception.code, 1)

    def test_desktop_break_glass_must_render_under_expert(self) -> None:
        self.write(
            desktop=FIVE_DESKTOP_OPTIONS.replace(
                'value="unrestricted_host" data-tier="expert"',
                'value="unrestricted_host" data-tier="primary"',
            )
        )
        self.assert_fails("Wry break-glass must be visibly separated under Expert")

    def test_desktop_full_project_must_render_under_advanced(self) -> None:
        self.write(
            desktop=FIVE_DESKTOP_OPTIONS.replace(
                'value="full_project" data-tier="advanced"',
                'value="full_project" data-tier="primary"',
            )
        )
        self.assert_fails("Wry full-project authority must sit under Advanced")

    def test_desktop_break_glass_must_carry_warning_treatment(self) -> None:
        self.write(
            desktop=FIVE_DESKTOP_OPTIONS.replace(' data-warning="true"', "")
        )
        self.assert_fails("Wry break-glass must be visually distinct")

    def test_desktop_warning_style_is_part_of_the_gate(self) -> None:
        self.write(desktop_style="")
        self.assert_fails("Wry warning metadata without visible CSS must fail")

    def test_desktop_access_render_tokens_in_dead_code_do_not_pass(self) -> None:
        self.write(
            desktop_access_render="  if (kind === 'access') { return '<button>flat</button>'; }",
            desktop_extra_js=DEAD_ACCESS_RENDER,
        )
        self.assert_fails("unused render code must not satisfy the live access branch")

    def test_desktop_access_render_nested_dead_code_does_not_pass(self) -> None:
        self.write(desktop_access_render=NESTED_DEAD_ACCESS_RENDER)
        self.assert_fails("nested dead code inside the access branch must not satisfy it")

    def test_desktop_access_hints_must_match_react(self) -> None:
        self.write(desktop=FIVE_DESKTOP_OPTIONS.replace('data-hint="y"', 'data-hint="different"', 1))
        self.assert_fails("both shipped composers must explain a profile the same way")

    def test_desktop_accessible_names_must_include_the_hint(self) -> None:
        self.write(
            desktop_access_render=ACCESS_RENDER_TEMPLATE.replace(
                "const label = `${o.textContent}. ${hint}`;",
                "const label = o.textContent;",
            )
        )
        self.assert_fails("every access option must expose its explanation by name")

    def test_a_desktop_default_other_than_standard_fails(self) -> None:
        self.write(
            desktop=FIVE_DESKTOP_OPTIONS.replace(
                'data-hint="y" selected',
                'data-hint="y"',
                1,
            ).replace(
                'data-hint="y" data-warning="true"',
                'data-hint="y" data-warning="true" selected',
            )
        )
        self.assert_fails("a pre-selected break-glass option must fail")

    def test_the_fixed_shape_passes(self) -> None:
        self.assertEqual(GATE.main(), 0)


if __name__ == "__main__":
    unittest.main()
