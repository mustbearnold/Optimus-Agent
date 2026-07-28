// @vitest-environment node
//
// Not jsdom: this file runs a real Vite build, and esbuild refuses to start
// under jsdom's `TextEncoder`, whose output fails `instanceof Uint8Array`.

import { fileURLToPath } from 'node:url';
import { build, type Rollup } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import { beforeAll, describe, expect, it } from 'vitest';

// The stylesheet has to be compiled for real to be inspected at all. Importing
// it with `?inline` returns an empty string under vitest, which stubs CSS out;
// jsdom parses stylesheets but never applies them to `getComputedStyle`, so a
// rendering test cannot see any of this either. Every assertion below would
// pass against a completely broken build if it were asserted any other way.
let compiled = '';

beforeAll(async () => {
  const result = await build({
    root: fileURLToPath(new URL('..', import.meta.url)),
    logLevel: 'silent',
    configFile: false,
    plugins: [tailwindcss()],
    build: {
      write: false,
      rollupOptions: { input: fileURLToPath(new URL('./tailwind.css', import.meta.url)) },
    },
  });
  // `build()` is typed as output-or-outputs-or-watcher; only the first is
  // reachable here, since `build.watch` is not set.
  const bundles = (Array.isArray(result) ? result : [result]) as Rollup.RollupOutput[];
  compiled = bundles
    .flatMap((bundle) => bundle.output)
    .filter((chunk) => chunk.type === 'asset' && chunk.fileName.endsWith('.css'))
    .map((chunk) => String((chunk as Rollup.OutputAsset).source))
    .join('\n');
  expect(compiled.length).toBeGreaterThan(1000);
}, 60_000);

/**
 * The cascade contract for ADR-0050.
 *
 * Adding Tailwind to an application with 6,762 lines of hand-written CSS works
 * only because of a handful of decisions in `tailwind.css`, none of which are
 * visible from the component source and every one of which fails silently. A
 * stray `@import "tailwindcss"` would restore preflight and reset the entire
 * app; dropping the `@layer` statement would let unlayered app rules outrank
 * every utility. Both leave the tests green and the application wrong.
 */
describe('the compiled stylesheet', () => {
  it('emits the layer blocks in the order that decides who wins', () => {
    // The `@layer a, b, c;` statement in the source does not survive bundling —
    // Vite flattens it away — so precedence in the shipped file rests entirely
    // on where each block first appears. That is the thing worth asserting:
    // reverse `optimus` and `utilities` here and every Tailwind utility loses to
    // a bare `button` rule, with no error anywhere.
    const seen: string[] = [];
    for (const [, name] of compiled.matchAll(/@layer ([a-z]+)\s*\{/g)) {
      if (!seen.includes(name)) seen.push(name);
    }
    // Only the ranking is asserted, not the roster: a layer that happens to have
    // no rules this week emits no block, and that is not a regression.
    const ranked = ['theme', 'optimus', 'components', 'utilities'];
    expect(seen.filter((name) => ranked.includes(name))).toEqual(
      ranked.filter((name) => seen.includes(name))
    );
    for (const required of ['theme', 'optimus', 'utilities']) {
      expect(seen).toContain(required);
    }
  });

  it('does not ship preflight', () => {
    // Preflight's signatures: the margin reset, the heading font reset, and the
    // border-color reset on the universal selector. Any of them landing here
    // means the whole app was restyled by an import.
    expect(compiled).not.toMatch(/h1,\s*h2,\s*h3/);
    expect(compiled).not.toMatch(/blockquote,\s*dl,\s*dd/);
    expect(compiled).not.toMatch(/box-sizing:\s*border-box[^}]*}\s*::/);
  });

  it('puts the hand-written stylesheets in the optimus layer, not outside every layer', () => {
    // `.composer` is defined in styles.css. Unlayered, its bare `button` and
    // `input` neighbours would beat every Tailwind utility in the document.
    const optimus = compiled.match(/@layer optimus\s*\{/);
    expect(optimus).not.toBeNull();
    expect(compiled).toMatch(/@layer optimus\s*\{[\s\S]*\.composer/);
  });

  it('resolves shadcn colour names to the app tokens rather than a second palette', () => {
    // `@theme inline` substitutes the reference, so this must be `var(--surface)`
    // and not the literal value — a copied literal would freeze the light theme
    // out, since `:root[data-theme="light"]` reassigns `--surface` at runtime.
    expect(compiled).toMatch(/\.bg-background\s*\{\s*background-color:\s*var\(--surface\)/);
    expect(compiled).toMatch(/--color-border:\s*var\(--border\)/);
  });

  it('keeps the radius scale app-owned in both directions', () => {
    // The app defines sm/base/lg and Tailwind adds md/xl. Left alone, `rounded-md`
    // would return Tailwind's stock 6px while `rounded-lg` returns the app's 0px
    // inside the workbench shell — a scale that rounds one corner and squares the
    // next. See workbench-shell.css:49.
    expect(compiled).toMatch(/--radius-md:\s*var\(--radius\)/);
    expect(compiled).toMatch(/--radius-xl:\s*var\(--radius-lg\)/);
  });

  it('gives bordered shadcn surfaces a colour, since preflight is not there to', () => {
    // Without this, `border` falls through to `currentColor` and every shadcn
    // surface is outlined in the text colour — on this palette, a white line
    // around a near-black dialog.
    expect(compiled).toMatch(/\[data-slot\][\s\S]{0,60}border-color:\s*var\(--border\)/);
  });
});
