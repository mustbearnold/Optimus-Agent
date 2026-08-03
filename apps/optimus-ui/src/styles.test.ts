import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

describe('motion contract', () => {
  it('contains no broad expensive animation declarations or text-blurring effects', () => {
    const baseCss = [
      readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8'),
      readFileSync(resolve(process.cwd(), 'src/codex-shell.css'), 'utf8'),
    ].join('\n');
    const workbenchCss = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    const css = `${baseCss}\n${workbenchCss}`;
    expect(css).not.toMatch(/transition\s*:\s*all/i);
    expect(css).not.toMatch(/will-change/i);
    expect(css).not.toMatch(/^[ \t]*filter\s*:\s*blur/im);
    expect(css).not.toMatch(/backdrop-filter/i);
    expect(css).not.toMatch(/text-shadow/i);
    expect(css).not.toMatch(/filter\s*:\s*(?:blur|drop-shadow)/i);
    expect(css).not.toMatch(/text-rendering\s*:\s*geometricPrecision/i);
  });

  it('does not reserve an evidence column when the workspace is closed', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.surface-row:not\(:has\(\.workspace-shell\)\)\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\)/s);
  });

  it('paints the work area as one translucent pane without an opaque transcript layer', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/:root\[data-theme="dark"\]\s*\{[^}]*--work-pane-bg:\s*rgba\(7,\s*7,\s*7,\s*0\.78\)/s);
    expect(css).toMatch(/\.work-surface\s*\{[^}]*background:\s*var\(--work-pane-bg\)[^}]*box-shadow:\s*inset/s);
    expect(css).toMatch(/\.transcript\s*\{[^}]*background:\s*transparent/s);
    expect(css).toMatch(/\.composer-shell\s*\{[^}]*var\(--work-pane-bg\)/s);
  });

  it('paints the project sidebar as translucent black without blurring its text', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/:root\[data-theme="dark"\]\s*\{[^}]*--project-rail-bg:\s*rgba\(0,\s*0,\s*0,\s*0\.82\)/s);
    expect(css).toMatch(/\.project-rail\s*\{[^}]*background:\s*var\(--project-rail-bg\)/s);
    expect(css).not.toMatch(/backdrop-filter/i);
  });

  it('uses the black glass palette in both saved theme modes', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/:root,\s*:root\[data-theme="light"\]\s*\{[^}]*color-scheme:\s*dark[^}]*--canvas:\s*#000000[^}]*--accent:\s*#f47742/s);
    expect(css).toMatch(/:root\[data-theme="dark"\]\s*\{[^}]*--canvas:\s*#000000[^}]*--surface:\s*rgba\(10,\s*10,\s*10,\s*0\.84\)[^}]*--accent:\s*#f47742/s);
  });

  it('keeps thread rows flat and reserves full-contrast titles for selection', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.session-row\s*\{[^}]*border-radius:\s*0/s);
    expect(css).toMatch(/\.rail-project-scope \.project-scope-trigger\s*\{[^}]*border-radius:\s*0/s);
    expect(css).toMatch(/\.project-rail \.session-row \.session-title\s*\{[^}]*color:\s*var\(--faint\)[^}]*font-size:\s*15px[^}]*font-weight:\s*450/s);
    expect(css).toMatch(/\.project-rail \.session-row\.is-active \.session-title\s*\{[^}]*color:\s*var\(--text\)/s);
    expect(css).toMatch(/\.session-row\s*\{[^}]*background:\s*var\(--session-card\)/s);
    expect(css).toMatch(/\.session-row:hover\s*\{[^}]*background:\s*var\(--session-card-hover\)[^}]*box-shadow:\s*inset 0 0 15px/s);
    expect(css).toMatch(/\.session-row\.is-active\s*\{[^}]*background:\s*var\(--session-card-selected\)/s);
    expect(css).not.toMatch(/\.session-row(?:\.is-active|:hover)[^{]*\{[^}]*background:\s*linear-gradient/s);
  });

  it('keeps project and settings labels readable without outlining the search field', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.rail-search:focus-within\s*\{[^}]*box-shadow:\s*none/s);
    expect(css).toMatch(/\.session-card-meta\s*\{[^}]*color:\s*var\(--text-2\)[^}]*font-size:\s*14px[^}]*font-weight:\s*500/s);
    expect(css).toMatch(/\.session-card-meta > svg\s*\{[^}]*width:\s*16px[^}]*height:\s*16px/s);
    expect(css).toMatch(/\.rail-footer \.rail-settings-button\s*\{[^}]*color:\s*var\(--text-2\)[^}]*font-size:\s*14px[^}]*font-weight:\s*500/s);
  });

  it('renders shared icons with sharp square stroke geometry', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.optimus-icon\s*\{[^}]*shape-rendering:\s*geometricPrecision/s);
    expect(css).toMatch(/\.optimus-icon path,[\s\S]*?\.optimus-icon ellipse\s*\{[^}]*stroke-linecap:\s*square[^}]*stroke-linejoin:\s*miter/s);
  });

  it('limits the Unrestricted host flame effect to its text and icon', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.composer-access-trigger\.is-unrestricted-host,[\s\S]*?background:\s*transparent;[^}]*box-shadow:\s*none/s);
    expect(css).toMatch(/\.composer-access-trigger\.is-unrestricted-host > span,\s*\.composer-access-trigger\.is-unrestricted-host > svg\s*\{[^}]*animation:\s*unrestricted-host-flame 3600ms ease-in-out infinite alternate/s);
    expect(css).not.toMatch(/animation:\s*unrestricted-host-flame[^;]*steps\(/s);
    expect(css).toMatch(/@media \(prefers-reduced-motion:\s*reduce\)[\s\S]*?\.composer-access-trigger\.is-unrestricted-host > span,[\s\S]*?animation:\s*none/s);
  });

  // The warning colour must reach the menu option itself, not only the trigger
  // after break-glass is already chosen (#118). It reads the token rather than
  // a hex literal so the light theme gets its own contrast-checked value.
  it('colours the Expert tier before it is picked', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8');
    const expertRule = css.match(/\.composer-access-tier\.is-expert button > svg,[^{]*\{([^}]*)\}/);
    expect(expertRule?.[1]).toContain('color: var(--warning)');
    expect(expertRule?.[1]).not.toMatch(/#[0-9a-f]{3,8}/i);
    expect(css).toMatch(/\.composer-access-tier\.is-advanced,\s*\.composer-access-tier\.is-expert\s*\{[^}]*border-top/s);
  });

  it('keeps the send control dark in enabled, hover, and disabled states', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.send-button\s*\{[^}]*color:\s*var\(--text-2\)[^}]*background:\s*var\(--send-control\)/s);
    expect(css).toMatch(/\.send-button:hover:not\(:disabled\)\s*\{[^}]*background:\s*var\(--send-control-hover\)/s);
    expect(css).toMatch(/\.send-button:disabled\s*\{[^}]*background:\s*var\(--send-control-disabled\)/s);
    expect(css).not.toMatch(/\.send-button\s*\{[^}]*background:\s*var\(--accent\)/s);
  });

  it('keeps the chat surface rounded without adding broad visual effects', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    for (const selector of [
      '.app-stage',
      '.message-user',
      '.composer-card',
      '.approval-card',
      '.artifact-tile',
      '.console-panel',
      // `.command-palette` was here until that surface moved to shadcn and the
      // class stopped existing. It is not unguarded: the universal reset
      // asserted below is what actually enforces this contract, and it reaches
      // the shadcn primitives too — they are in a cascade layer, and `!important`
      // beats every layer.
      '.settings-dialog',
      '.project-sources-dialog',
    ]) {
      expect(css).toContain(selector);
    }
    expect(css).toMatch(/--radius-sm:\s*6px/);
    expect(css).toMatch(/--radius:\s*8px/);
    expect(css).toMatch(/--radius-lg:\s*12px/);
    expect(css).toMatch(/\.message\s*\{[^}]*animation:\s*message-in\s+160ms/s);
    expect(css).toMatch(/\.activity-timeline\[data-open="true"\]\s*\{[^}]*border-radius:\s*var\(--radius-lg\)/s);
    expect(css).toMatch(/\.send-button\s*\{[^}]*border-radius:\s*var\(--radius-sm\)/s);
    expect(css).not.toMatch(/border-radius:\s*0\s*!important/);
  });
});
