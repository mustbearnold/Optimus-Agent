---
knowledge_type: decision
status: current
covers:
  - apps/optimus-ui/src/tailwind.css
  - apps/optimus-ui/vite.config.ts
  - apps/optimus-ui/components.json
  - apps/optimus-ui/src/components/ui
depends_on:
  - docs/decisions/0045-agent-host-and-surface-transports.md
validated_by:
  - apps/optimus-ui/src/components/chrome/TextPromptDialog.test.tsx
last_verified_commit: null
---

# ADR-0050: Overlays come from Radix via shadcn/ui, not from hand-written CSS

- **Status:** Proposed
- **Date:** 2026-07-28

## Context

`apps/optimus-ui` is the React 19 renderer and the repository-level default
Electron surface (DESIGN.md, "Implementation status — 2026-07-23"). It carries
**6,762 lines of hand-written CSS** across `styles.css` (4,067),
`codex-shell.css` (1,649), and `workbench-shell.css` (1,045), and four
hand-rolled overlay components: `CommandPalette`, `SettingsDialog`,
`TextPromptDialog`, and `ProjectSourcesDialog`.

Overlays are where hand-rolling costs the most, and the cost is invisible in a
screenshot. A dialog is not a positioned box. It is a focus trap, focus restore
to the element that opened it, `aria-modal` and a labelled title, Escape
handling that does not fight the composer's own Escape, scroll locking on the
body, an inert background for assistive technology, and collision-aware
positioning for anything that pops. Each of those is a separate defect class,
none of them is visible until someone navigating by keyboard or screen reader
hits it, and none of them is covered by the existing tests.

The second surface, `apps/optimus-desktop`, is a Rust `tao`/`wry` shell serving
4,640 lines of vanilla JS. It is not a candidate for this decision: shadcn/ui
distributes React components, so adopting it there would mean rewriting that
front end first. That is a separate question with a separate answer.

There is also a live warning in the tree about what happens when stylesheets are
layered without discipline. `optimus-desktop` compiles `style.css` and
`vantage.css` into one document by concatenation order alone — no `@layer`, no
lint — and the second file uses `!important` **221 times in 805 lines** to win
the resulting fight. Ninety-three selectors are declared in both. Reading either
file never tells you what actually renders.

## Decision

Adopt **Tailwind v4 + shadcn/ui in `apps/optimus-ui` only**, and take Radix
primitives for every overlay.

Three constraints make the adoption safe alongside 6,762 lines of existing CSS:

1. **Preflight is not imported.** Tailwind v4's `@import "tailwindcss"` pulls in
   `theme`, `preflight`, and `utilities`. We import `theme` and `utilities` and
   omit preflight, so Tailwind never resets an element the existing stylesheets
   already style. This is the documented mechanism for adding v4 to an app that
   already has CSS, and it is the whole reason this can be incremental.
2. **Everything lands in a named `@layer`.** Cascade order stops being an
   accident of import order, which is precisely the failure mode already burning
   in the other surface.
3. **Components are copied into the tree, not depended on.** That is what
   shadcn/ui is: `src/components/ui/*.tsx` are ours to read, diff, and change.
   The runtime dependencies added are the Radix primitives behind them plus
   `clsx`, `tailwind-merge`, and `class-variance-authority`.

Migration is one component per pull request, each deleting the CSS it makes
dead, with the line count reported. A conversion that deletes nothing has not
finished.

## Alternatives considered

**Keep hand-rolling, fix the accessibility defects directly.** Rejected: it is
the same work Radix has already done and tested across browsers and assistive
technology, and it would have to be re-done for each of the four overlays and
every future one. The defects are not hard to fix individually; they are hard to
*keep* fixed with no primitive enforcing them.

**Adopt a batteries-included component library (MUI, Mantine, Chakra).**
Rejected: they own their own theming and DOM, and this app has a specific visual
identity in 6,762 lines of CSS that is not up for replacement. shadcn is the
only mainstream option where the component source lands in the repository and
can be restyled without fighting a framework's own cascade.

**Put shadcn in `apps/optimus-desktop` instead.** Rejected on stack grounds, not
effort grounds: that surface has no React, so this would be a rewrite of 4,640
lines of vanilla JS before the first component could exist.

**Adopt Tailwind wholesale and delete the existing CSS in one pass.** Rejected:
a 6,762-line replacement in one change is unreviewable, and its regressions
would be visual — the kind no existing test catches.

## Reasons

- The accessibility work is already done, tested, and maintained upstream.
- The component source is in the repository, so restyling never means fighting a
  vendor's cascade.
- shadcn's 2026 line targets React 19, which is what this surface already runs,
  so adoption costs no framework migration.
- Overlay components are the highest-defect, lowest-visibility part of the CSS,
  so converting them first buys the most correctness per line changed.

## Consequences

- New runtime dependencies: `@radix-ui/react-*` per primitive used, `clsx`,
  `tailwind-merge`, `class-variance-authority`. This surface previously had four
  runtime dependencies; that number grows, and each primitive is a supply-chain
  entry that did not exist before.
- New build-time dependency: `tailwindcss` and `@tailwindcss/vite`.
- A `@/*` path alias is added, which shadcn's generator assumes.
- CSS shrinks per conversion rather than all at once, so for the duration of the
  migration the surface has two styling systems. That is the cost of it being
  reviewable.

## Risks

**A specificity war with the existing 6,762 lines.** The exact disease already
present in `optimus-desktop`. Mitigated by omitting preflight and by layering,
and bounded by a rule: if containment cannot hold, the work stops and is
reported. `!important` is not an acceptable resolution — it is the symptom the
other surface is showing.

**Dependency surface growth.** Real and not fully mitigable. The trade is a
handful of Radix packages against four hand-written overlays whose accessibility
defects nothing currently tests.

**Half-migrated for a long time.** Two styling systems coexisting is a real cost
if the migration stalls. Bounded by requiring each conversion to delete the CSS
it replaces, which makes progress measurable rather than assertable.

## Evaluation evidence

Each pull request reports: lines of CSS deleted, tests passing, and the
accessibility behaviours the Radix primitive now provides that the hand-rolled
version did not.

**Conversion 1 — `TextPromptDialog`.** 24 lines of CSS deleted, 13 tests added,
110 passing, build clean, no `!important` added. Gained over the hand-written
version: focus trapping, focus restoration to the opener, scroll locking, and
removal of the rest of the application from the accessibility tree.

Containment held, but only because two things were caught by rendering the
converted component in a browser — neither was visible to the component tests,
and both would have shipped:

1. **`border` painted in the text colour.** Tailwind's `border` utility sets a
   width and a style and leaves the colour to preflight, which is deliberately
   absent here. The colour therefore fell back to `currentColor`, outlining a
   near-black dialog in `#f1f1f1`. Fixed with a `border-color` rule scoped to
   `[data-slot]`, the attribute every shadcn primitive carries and nothing else
   in this app does.
2. **A radius scale that disagreed with itself.** The app owns `--radius-sm`,
   `--radius` and `--radius-lg`, and `workbench-shell.css:49` zeroes all three
   because that shell is square by design — so `rounded-lg` correctly returned
   `0px`. But Tailwind also defines `--radius-md` and `--radius-xl`, which the
   app has no opinion on, so `rounded-md` silently returned Tailwind's stock
   `6px`. Fixed by aliasing the two extra steps onto the app's own.

The general lesson, which applies to every remaining conversion: **omitting
preflight is what makes this migration incremental, and it is also what makes
Tailwind's utilities incomplete.** Utilities that assume a reset — `border` is
the first, and it will not be the last — need their missing half supplied
explicitly and scoped to shadcn's own subtree. A conversion is not verified
until the component has been rendered against the real stylesheet.

That verification is now automated rather than repeated by hand:
`tailwind.css.test.ts` compiles the stylesheet through Vite and asserts the
cascade contract directly, because none of it is observable from a component
test — jsdom parses stylesheets but never applies them to `getComputedStyle`,
so every assertion in that file would pass against a completely broken build.

## Conditions for reconsideration

- Tailwind's preflight-free import path stops being supported, forcing a reset
  that the existing stylesheets cannot survive.
- Layer containment fails in practice and `!important` starts appearing.
- `apps/optimus-desktop` becomes the default surface again, making a React-only
  decision the wrong shape.

## Relevant code

- `apps/optimus-ui/src/tailwind.css`
- `apps/optimus-ui/vite.config.ts`
- `apps/optimus-ui/components.json`
- `apps/optimus-ui/src/components/ui/`

## Relevant tests

- `apps/optimus-ui/src/tailwind.css.test.ts` — the cascade contract: layer
  ranking, preflight absence, app tokens reached through `@theme inline`, and
  the two utilities that need preflight's missing half supplied.
- `apps/optimus-ui/src/components/chrome/TextPromptDialog.test.tsx` — the four
  accessibility behaviours the hand-written dialog lacked.
