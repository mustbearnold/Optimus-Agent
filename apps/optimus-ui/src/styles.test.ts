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

  it('paints the work area as one flat square pane without text bleed', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/--work-pane-bg:\s*#0e0e0e/);
    expect(css).toMatch(/\.work-surface\s*\{[^}]*background:\s*var\(--work-pane-bg\)/s);
    expect(css).toMatch(/\.transcript\s*\{[^}]*background:\s*var\(--work-pane-bg\)/s);
    expect(css).toMatch(/\.composer-shell\s*\{[^}]*var\(--work-pane-bg\)/s);
  });

  it('paints the project sidebar solid without blurring its text', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/--project-rail-bg:\s*#0a0a0a/);
    expect(css).toMatch(/\.project-rail\s*\{[^}]*background:\s*#0a0a0a/s);
    expect(css).not.toMatch(/backdrop-filter/i);
  });

  it('keeps elevated menus opaque over the transcript', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/:root,\s*:root\[data-theme="light"\]\s*\{[^}]*--elevated:\s*#181818/s);
    expect(css).toMatch(/:root\[data-theme="dark"\]\s*\{[^}]*--elevated:\s*#181818/s);
  });

  it('uses the neutral mono palette in both saved theme modes', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/:root,\s*:root\[data-theme="light"\]\s*\{[^}]*color-scheme:\s*dark[^}]*--canvas:\s*#0e0e0e[^}]*--accent:\s*#9a9a9a/s);
    expect(css).toMatch(/:root\[data-theme="dark"\]\s*\{[^}]*--canvas:\s*#0e0e0e[^}]*--surface:\s*#141414[^}]*--accent:\s*#9a9a9a/s);
  });

  it('keeps thread rows flat and reserves full-contrast titles for selection', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.session-row\s*\{[^}]*border-radius:\s*0/s);
    expect(css).toMatch(/\.rail-project-scope \.project-scope-trigger\s*\{[^}]*border-radius:\s*0/s);
    expect(css).toMatch(/\.project-rail \.session-row \.session-title\s*\{[^}]*color:\s*var\(--faint\)[^}]*font-size:\s*15px[^}]*font-weight:\s*400/s);
    expect(css).toMatch(/\.project-rail \.session-row\.is-active \.session-title\s*\{[^}]*color:\s*var\(--text\)/s);
    expect(css).toMatch(/\.session-row\s*\{[^}]*background:\s*var\(--session-card\)/s);
    expect(css).toMatch(/\.session-row:hover\s*\{[^}]*background:\s*var\(--session-card-hover\)[^}]*box-shadow:\s*inset 0 0 15px/s);
    expect(css).toMatch(/\.session-row\.is-active\s*\{[^}]*background:\s*var\(--session-card-selected\)/s);
    expect(css).not.toMatch(/\.session-row(?:\.is-active|:hover)[^{]*\{[^}]*background:\s*linear-gradient/s);
  });

  // The rail is a history list, so its row height decides how much history is
  // reachable without scrolling. Every line in a row has a stated line-height so
  // the height is the sum of its content rather than a number someone liked, and
  // the assertion is that sum — a row that grows again has to say why.
  it('sizes chat rows to the lines they carry instead of padding them out', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    // `.session-worktree` and friends are declared more than once. Joining every
    // block for a selector and reading the last value is what the cascade does.
    // Comments are stripped first so the selector boundary is a real one and a
    // rule that happens to follow prose is not skipped.
    const declarations = css.replace(/\/\*[\s\S]*?\*\//g, '');
    const rule = (selector: string) =>
      [
        ...declarations.matchAll(
          new RegExp(`(?:^|\\})\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{([^}]*)\\}`, 'g')
        ),
      ]
        .map((match) => match[1])
        .join(';');
    const px = (declarations: string, property: string) =>
      Number(
        [...declarations.matchAll(new RegExp(`(?:^|[;{\\s])${property}:\\s*(-?\\d+)px`, 'g'))].at(-1)?.[1] ??
          NaN
      );

    const assigned = rule('.session-select');
    const lines =
      px(rule('.session-card-meta'), 'line-height') +
      px(rule('.project-rail .session-row .session-title'), 'line-height') +
      px(rule('.session-worktree'), 'line-height');
    const gaps = px(assigned, 'gap') * 2;
    const padding = Number([...assigned.matchAll(/padding:\s*(\d+)px/g)].at(-1)?.[1] ?? NaN) * 2;
    expect(px(assigned, 'height')).toBe(lines + gaps + padding);

    // A recent chat is a single title line, and that band holds the most rows.
    const unassigned = rule('.session-row.is-unassigned .session-select');
    expect(px(unassigned, 'height')).toBeLessThanOrEqual(32);
    expect(px(unassigned, 'height')).toBeGreaterThanOrEqual(
      px(rule('.project-rail .session-row .session-title'), 'line-height')
    );
  });

  // The nested session boxes under a project folder share the Recent Chats
  // geometry: flush with the rail (the old 22px folder indent is gone) and a
  // 32px floor for the single title line. A regression here re-creates the
  // offset, taller cards of the pre-compact rail.
  it('keeps nested project sessions flush with the rail at the 32px floor', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.project-sessions\s*\{\s*display:\s*none;\s*padding:\s*0 0 3px;\s*\}/);
    expect(css).toMatch(
      /\.project-sessions \.session-row \.session-select\s*\{[^}]*min-height:\s*32px/s
    );
  });

  it('keeps the empty-folder drop hint flush like the rows around it', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    // The 1px left border was a leftover of the 22px folder indent; with nested
    // rows flush to the rail it painted a stray vertical line beside the hint.
    expect(css).not.toMatch(/\.project-drop-hint\s*\{[^}]*border-left\s*:/s);
  });

  it('keeps project and settings labels readable without outlining the search field', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.rail-search:focus-within\s*\{[^}]*box-shadow:\s*none/s);
    expect(css).toMatch(/\.session-card-meta\s*\{[^}]*color:\s*var\(--text-2\)[^}]*font-size:\s*14px[^}]*font-weight:\s*400/s);
    expect(css).toMatch(/\.session-card-meta > \.optimus-icon\s*\{[^}]*min-width:\s*16px[^}]*min-height:\s*16px/s);
    expect(css).toMatch(/\.rail-footer \.rail-settings-button\s*\{[^}]*color:\s*var\(--text-2\)[^}]*font-size:\s*14px[^}]*font-weight:\s*400/s);
  });

  it('does not leak recent session titles into the icon-only rail', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.project-rail\.is-collapsed \.recent-section,[\s\S]*?\.project-rail\.is-collapsed \.archived-section,[\s\S]*?display:\s*none/s);
    expect(css).toMatch(/\.project-rail\.is-collapsed \.rail-primary,\s*\.project-rail\.is-collapsed \.rail-scroll\s*\{[^}]*display:\s*none/s);
    expect(css).toMatch(/\.project-rail\.is-collapsed \.rail-project-scope\s*\{[^}]*grid-template-columns:\s*36px/s);
    expect(css).toMatch(/\.project-rail\.is-collapsed \.rail-project-scope \.project-scope-trigger span,[\s\S]*?display:\s*none/s);
    expect(css).toMatch(/\.project-rail\.is-collapsed \.project-drop-hint\s*\{[^}]*display:\s*none/s);
  });

  it('keeps the collapsed rail self-contained and the composer on the work surface', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.project-rail\.is-collapsed\s*\{[^}]*width:\s*52px[^}]*min-width:\s*52px[^}]*max-width:\s*52px[^}]*overflow:\s*hidden/s);
    expect(css).toMatch(/\.project-rail\.is-collapsed \.rail-primary,\s*\.project-rail\.is-collapsed \.rail-scroll\s*\{[^}]*display:\s*none/s);
    expect(css).toMatch(/\.composer-shell\s*\{[^}]*background:\s*var\(--work-pane-bg\)/s);
    // The composer is not a card: it sits flush on the work surface (transparent,
    // borderless) so the shell's pane background is the only paint behind it.
    expect(css).toMatch(/\.composer-card\s*\{[^}]*background:\s*transparent/s);
    expect(css).toMatch(/\.composer-card\s*\{[^}]*border:\s*0/s);
    const focusRule = [...css.matchAll(/\.composer-card:focus-within\s*\{([^}]*)\}/g)].at(-1)?.[1] || '';
    expect(focusRule).toContain('border-bottom-color: var(--border-strong)');
    expect(focusRule).not.toContain('var(--accent)');
  });

  it('leaves the responsive rail width under the layout state controller', () => {
    // Regression: a breakpoint in styles.css (<=959px) and another in
    // codex-shell.css (<=1099px) both pinned `--rail-width: 52px !important`,
    // which outranks the inline width React writes from `leftCollapsed`. The
    // toggle kept flipping its class and its "Open/Close project rail" label
    // while the rail stayed a 52px strip, so on any window narrower than
    // 1100px the left panel simply would not open or close. No stylesheet may
    // force the rail width again — the layout state is the only owner.
    for (const file of ['src/styles.css', 'src/codex-shell.css', 'src/workbench-shell.css']) {
      const css = readFileSync(resolve(process.cwd(), file), 'utf8');
      expect(css, `${file} must not force --rail-width`).not.toMatch(
        /--rail-width\s*:[^;]*!important/
      );
    }
  });

  it('renders shared icons as Nerd Font glyphs with a uniform baseline', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.optimus-icon\s*\{[^}]*font-family:\s*"0xProto Nerd Font Propo"/s);
    expect(css).toMatch(/\.optimus-icon\s*\{[^}]*display:\s*inline-flex[^}]*line-height:\s*1\.2/s);
    expect(css).not.toMatch(/\.optimus-icon path/s);
  });

  it('limits the Unrestricted host flame effect to its text and icon', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    expect(css).toMatch(/\.composer-access-trigger\.is-unrestricted-host,[\s\S]*?background:\s*transparent;[^}]*box-shadow:\s*none/s);
    expect(css).toMatch(/\.composer-access-trigger\.is-unrestricted-host > span,\s*\.composer-access-trigger\.is-unrestricted-host > \.optimus-icon\s*\{[^}]*animation:\s*unrestricted-host-flame 3600ms ease-in-out infinite alternate/s);
    expect(css).not.toMatch(/animation:\s*unrestricted-host-flame[^;]*steps\(/s);
    expect(css).toMatch(/@media \(prefers-reduced-motion:\s*reduce\)[\s\S]*?\.composer-access-trigger\.is-unrestricted-host > span,[\s\S]*?animation:\s*none/s);
  });

  // The warning colour must reach the menu option itself, not only the trigger
  // after break-glass is already chosen (#118). It reads the token rather than
  // a hex literal so the light theme gets its own contrast-checked value.
  it('colours the Expert tier before it is picked', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8');
    const expertRule = css.match(/\.composer-access-tier\.is-expert button > \.optimus-icon,[^{]*\{([^}]*)\}/);
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

  it('keeps every workbench surface square without adding broad visual effects', () => {
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
    expect(css).toMatch(/\*,\s*\*::before,\s*\*::after\s*\{[^}]*border-radius:\s*0\s*!important/s);
    expect(css).toMatch(/--radius-sm:\s*0px/);
    expect(css).toMatch(/--radius:\s*0px/);
    expect(css).toMatch(/--radius-lg:\s*0px/);
    expect(css).toMatch(/\.activity-detail\s*\{/);
    expect(css).not.toContain('prompt-history-rail');
    expect(css).toMatch(/scrollbar-width:\s*none\s*!important/);
    expect(css).toMatch(/::-webkit-scrollbar\s*\{[^}]*width:\s*0\s*!important/s);
    expect(css).not.toMatch(/scrollbar-width:\s*(?:thin|auto)/i);
    expect(css).not.toMatch(/scrollbar-color\s*:/i);
  });

  it('keeps one converged design system across every shell', () => {
    const baseCss = [
      readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8'),
      readFileSync(resolve(process.cwd(), 'src/codex-shell.css'), 'utf8'),
    ].join('\n');
    const workbenchCss = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    const css = `${baseCss}\n${workbenchCss}`;
    // The codex-measured shell owns no palette of its own; it consumes the
    // workbench tokens. A second `:root` palette is the split-personality tell.
    expect(css).not.toMatch(/:root,\s*:root\[data-theme="light"\]\s*\{[^}]*color-scheme:\s*light/s);
    // No pill/circle radius anywhere: the only radii are the 0px tokens and
    // the global square reset (layout 50% positions are allowed).
    expect(css).not.toMatch(/border-radius:\s*999px/);
    expect(css).not.toMatch(/border-radius:\s*50%/);
    // The canvas is the neutral mono near-black, never pure black.
    expect(css).not.toMatch(/--canvas:\s*#000000/);
    // One neutral midground accent; no blue/indigo accent remnants anywhere.
    expect(css).not.toMatch(/--accent:\s*#(?:2f6feb|7aa2f7|8b92ff|5c63d8|f47742)/);
    expect(css).toMatch(/--accent:\s*#9a9a9a/);
  });

  // Execution-state colours are semantic, not a second palette: the rail dots
  // and session rows read the tokens, and no chromatic remnant of the
  // pre-mono palette (neon status hues, the old glyph gradient, GitHub-dark
  // terminal blues, near-black button-text literals) may hide anywhere.
  it('keeps execution-state colours and the terminal on the mono tokens', () => {
    const baseCss = [
      readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8'),
      readFileSync(resolve(process.cwd(), 'src/codex-shell.css'), 'utf8'),
    ].join('\n');
    const workbenchCss = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    const css = `${baseCss}\n${workbenchCss}`;
    expect(css).toMatch(/\.session-status-dot\.is-working\s*\{[^}]*background:\s*var\(--success\)/s);
    expect(css).toMatch(/\.session-status-dot\.is-attention\s*\{[^}]*background:\s*var\(--warning\)/s);
    expect(css).toMatch(/\.session-status-dot\.is-error\s*\{[^}]*background:\s*var\(--danger\)/s);
    expect(css).toMatch(/\.session-state\.is-working\s*\{[^}]*color:\s*var\(--success\)/s);
    expect(css).toMatch(/\.session-state\.is-attention\s*\{[^}]*color:\s*var\(--warning\)/s);
    expect(css).toMatch(/\.session-state\.is-error\s*\{[^}]*color:\s*var\(--danger\)/s);
    expect(css).toMatch(/\.send-button\.is-stop\s*\{[^}]*background:\s*var\(--danger\)/s);
    expect(css).toMatch(/\.terminal-panel,\s*:root\[data-theme="light"\] \.terminal-panel\s*\{[^}]*background:\s*var\(--canvas\)/s);
    expect(css).toMatch(/\.terminal-output\s*\{[^}]*color:\s*var\(--text-2\)/s);
    expect(css).toMatch(/\.terminal-command\s*\{[^}]*border-top-color:\s*var\(--border-strong\)/s);
    expect(css).not.toMatch(
      /#(?:aa6aff|656fff|3cd7d0|47ff83|ff405f|16803c|6ee7a0|b45309|f6bd60|c93636|fb8787|15171a|e6edf3|c9d1d9|30343a|aeb8c6|667180|202936|d5dce6|090b11|0a0d12|101216|e54d5d)/i
    );
  });

  it('spans the session status footer across the entire app', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/workbench-shell.css'), 'utf8');
    // The footer is an app-wide bar (Hermes-style), not a card-scoped strip:
    // the app grid owns an explicit auto row for it.
    expect(css).toMatch(/\.optimus-app\s*\{[^}]*grid-template-rows:\s*var\(--topbar-h\)\s+1fr\s+auto/s);
    expect(css).toMatch(/\.workbench-statusbar\s*\{[^}]*justify-content:\s*space-between/s);
    expect(css).toMatch(/\.workbench-statusbar\s*\{[^}]*background:\s*var\(--rail\)/s);
  });
});
