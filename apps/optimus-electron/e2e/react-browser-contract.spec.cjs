const { test, expect } = require('@playwright/test');
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '../../..');
const EVIDENCE_DIR = path.join(ROOT, 'local', 'tmp');
const URL = 'http://127.0.0.1:4174/';

test.beforeAll(() => {
  fs.mkdirSync(EVIDENCE_DIR, { recursive: true });
});

// Repaired against the post-redesign contract. Two rules held while doing it:
//
// 1. Where an authority exists, follow it -- the vitest suites are the redesign's
//    own statement of intent (OptimusApp.test.tsx for chrome and landmarks,
//    Composer.test.tsx for the run-settings popover, ProjectsRail.test.tsx for
//    the rail), and docs/plans/workspace-redesign.md for surface architecture.
// 2. Never re-baseline a pixel expectation by measuring the current build. The
//    geometry numbers below are the pre-existing contract and still hold because
//    layoutStore defaults (leftWidth 240, workspaceWidth 720, executionHeight
//    190) were not changed by the redesign.
//
// The evidence workspace is chat-first and starts closed, so any test that needs
// it must open it explicitly -- that is the redesign's intent, not a defect.
async function openWorkspace(page) {
  const toggle = page.getByRole('button', { name: 'Workspace', exact: true });
  if ((await toggle.getAttribute('aria-pressed')) === 'true') return;
  await toggle.click();
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeVisible();
}

// Provider/Model/Thinking level moved off the composer card into one popover.
function themeSelect(settings) {
  return settings.locator('.settings-row').filter({ hasText: 'Color theme' }).locator('select');
}

async function openRunSettings(page) {
  const trigger = page.getByRole('button', { name: 'Model and run settings' });
  if ((await trigger.getAttribute('aria-expanded')) !== 'true') await trigger.click();
  return page.getByRole('dialog', { name: 'Model and run settings' });
}

test('wide 1600x1000 renders the dense three-surface workbench', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.goto(URL);

  const rail = page.getByRole('complementary', { name: 'Projects and sessions' });
  const work = page.getByRole('region', { name: 'Agent work surface' });
  const workspace = page.getByRole('complementary', { name: 'Evidence workspace' });
  await expect(rail).toBeVisible();
  await expect(work).toBeVisible();
  await openWorkspace(page);
  await expect(workspace).toBeVisible();
  expect((await rail.boundingBox()).width).toBe(240);
  expect((await workspace.boundingBox()).width).toBeGreaterThanOrEqual(700);
  const searchBox = await page.locator('.rail-search').boundingBox();
  const newSessionBox = await page.getByRole('button', { name: 'New thread' }).boundingBox();
  expect(Math.abs(searchBox.y - newSessionBox.y)).toBeLessThanOrEqual(1);
  expect(newSessionBox.x).toBeGreaterThanOrEqual(searchBox.x + searchBox.width);
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  await expect(page.locator('.message').first()).toBeVisible();
  await assertComposerInsideViewport(page);
  expect(errors).toEqual([]);
});

test('medium 960x760 preserves controls without a three-column overflow', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 960, height: 760 });
  await page.goto(URL);
  await openWorkspace(page);
  const rail = await page.getByRole('complementary', { name: 'Projects and sessions' }).boundingBox();
  const workspace = await page.getByRole('complementary', { name: 'Evidence workspace' }).boundingBox();
  expect(rail.width).toBe(52);
  expect(workspace.width).toBeGreaterThanOrEqual(360);
  await assertNoHorizontalOverflow(page);
  await assertComposerInsideViewport(page);
  expect(errors).toEqual([]);
});

test('compact 640x800 switches one primary surface at a time', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 640, height: 800 });
  await page.goto(URL);
  const switcher = page.getByRole('tablist', { name: 'Primary surface' });
  await expect(switcher).toBeVisible();
  await expect(page.getByRole('region', { name: 'Agent work surface' })).toBeVisible();
  await page.getByRole('tab', { name: 'browser', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Preview browser' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Agent work surface' })).toBeHidden();
  await page.getByRole('tab', { name: 'work', exact: true }).click();
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  await expect(page.getByRole('button', { name: /^Access: / })).toBeVisible();
  const runSettings = await openRunSettings(page);
  await expect(runSettings.getByLabel('Thinking level')).toBeVisible();
  await expect(runSettings.getByRole('switch', { name: 'Fast mode' })).toBeVisible();
  await page.keyboard.press('Escape');
  await assertComposerInsideViewport(page);
  await assertWorkSurfaceContrast(page);
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-workbench-compact-640x800.png'),
  });
  expect(errors).toEqual([]);
});

test('compact terminal takes the primary surface and returns to chat when closed', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 840, height: 800 });
  await page.goto(URL);

  await page.getByRole('tab', { name: 'browser', exact: true }).click();
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeVisible();

  const terminal = page.getByRole('button', { name: 'Terminal' });
  await terminal.click();
  await expect(page.getByRole('complementary', { name: 'Execution dock' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Agent work surface' })).toBeHidden();
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeHidden();
  await assertNoHorizontalOverflow(page);

  await terminal.click();
  await expect(page.getByRole('complementary', { name: 'Execution dock' })).toBeHidden();
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  await expect(page.getByRole('region', { name: 'Evidence workspace' })).toBeHidden();
  await assertComposerInsideViewport(page);
  await assertNoHorizontalOverflow(page);
  expect(errors).toEqual([]);
});

test('composer access menu is opaque over transcript content', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 840, height: 800 });
  await page.goto(URL);

  await page.getByRole('button', { name: /^Access: / }).click();
  const menu = page.getByRole('listbox', { name: 'Access' });
  await expect(menu).toBeVisible();
  const surface = await menu.evaluate((element) => {
    const color = getComputedStyle(element).backgroundColor;
    const channels = (color.match(/[\d.]+/g) || []).map(Number);
    return {
      color,
      alpha: channels.length === 4 ? channels[3] : 1,
      image: getComputedStyle(element).backgroundImage,
    };
  });
  expect(surface.alpha).toBe(1);
  expect(surface.image).toBe('none');
  expect(errors).toEqual([]);
});

test('320 CSS px reflow and reduced motion preserve state and focus', async ({ page }) => {
  const errors = collectErrors(page);
  await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: 'dark' });
  await page.setViewportSize({ width: 320, height: 800 });
  await page.goto(URL);
  await expect(page.getByRole('tablist', { name: 'Primary surface' })).toBeVisible();
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  const runSettings = await openRunSettings(page);
  await expect(runSettings.getByLabel('Provider')).toBeVisible();
  await expect(runSettings.getByLabel('Model')).toBeVisible();
  await expect(runSettings.getByLabel('Thinking level')).toBeVisible();
  await expect(runSettings.getByRole('switch', { name: 'Fast mode' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: /^Access: / })).toBeVisible();
  await assertComposerInsideViewport(page);
  await assertComposerControlsInsideCard(page);
  await assertNoHorizontalOverflow(page);
  // Access is the first control after the message box now that the remaining run
  // settings live behind the popover trigger.
  await page.getByLabel('Message Optimus').focus();
  await page.keyboard.press('Tab');
  const accessTrigger = page.getByRole('button', { name: /^Access: / });
  await expect(accessTrigger).toBeFocused();
  const focusShadow = await accessTrigger.evaluate((element) =>
    getComputedStyle(element).boxShadow
  );
  expect(focusShadow).not.toBe('none');
  // .workspace-shell carries the workspace-in animation, so it has to be open for
  // the reduced-motion contract to be observable at all. At narrow widths the
  // topbar toggle is hidden and the Primary surface tablist owns this choice.
  await page.getByRole('tab', { name: 'browser', exact: true }).click();
  await expect(page.locator('.workspace-shell')).toBeVisible();
  const duration = await page.locator('.workspace-shell').evaluate((element) =>
    getComputedStyle(element).animationDuration
  );
  expect(['0.001ms', '1e-06s']).toContain(duration);
  expect(errors).toEqual([]);
});

test('light theme and secondary routes settle without console errors', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto(URL);
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  // The redesign consolidated the topbar: theme is a Settings choice, not a
  // chrome toggle (OptimusApp.test.tsx asserts no topbar theme button).
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  // Settings opens on General; theme lives under Appearance.
  await settings.getByRole('navigation', { name: 'Settings categories' })
    .getByRole('button', { name: 'Appearance' })
    .click();
  // Structural locator on purpose: SettingRow renders its title in a <strong>, so
  // the select has no accessible name and getByLabel cannot reach it. Tracked as
  // an accessibility defect rather than papered over here.
  await themeSelect(settings).selectOption('dark');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  // Structural locator on purpose: SettingRow renders its title in a <strong>, so
  // the select has no accessible name and getByLabel cannot reach it. Tracked as
  // an accessibility defect rather than papered over here.
  await themeSelect(settings).selectOption('light');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: 'Done' }).click();
  // Capabilities left the topbar too; the command palette is its route.
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('dialog', { name: 'Command palette' })).toBeVisible();
  // `option`, not `button`: the palette is a cmdk listbox now, so its rows carry
  // real listbox semantics instead of being a stack of unrelated buttons (ADR-0050).
  await page.getByRole('option', { name: /capabilities/i }).first().click();
  await expect(page.getByRole('main', { name: 'Capabilities' })).toBeVisible();
  const specialistBoundary = page.locator('.capability-boundary li').filter({ hasText: 'Specialist agents' });
  await expect(specialistBoundary.getByText('Unavailable', { exact: true })).toBeVisible();
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-capabilities-light-1280x820.png'),
  });
  // Artifacts is a workspace surface now rather than a topbar destination.
  await openWorkspace(page);
  await page.getByRole('tab', { name: 'Artifacts' }).click();
  await expect(page.getByRole('region', { name: 'Artifacts' })).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Settings' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Settings categories' })).toBeVisible();
  await page.getByRole('button', { name: 'Projects', exact: true }).last().click();
  await expect(page.getByRole('main', { name: 'Projects settings' })).toBeVisible();
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-settings-projects-1280x820.png'),
  });
  await page.getByRole('button', { name: 'Done' }).click();
  expect(errors).toEqual([]);
});

test('measured maximum shell preserves Codex geometry and terminal ownership', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 1919, height: 1079 });
  await page.goto(URL);
  await openWorkspace(page);
  const topbar = await page.locator('.topbar').boundingBox();
  const rail = await page.getByRole('complementary', { name: 'Projects and sessions' }).boundingBox();
  const workspace = await page.getByRole('complementary', { name: 'Evidence workspace' }).boundingBox();
  const composer = await page.locator('.composer-card').boundingBox();
  const browserChrome = await page.getByRole('toolbar', { name: 'Browser navigation' }).boundingBox();
  expect(topbar.height).toBe(36);
  expect(rail.width).toBe(240);
  expect(workspace.width).toBe(720);
  expect(composer.width).toBe(736);
  expect(browserChrome.height).toBe(40);
  await page.getByRole('button', { name: 'Terminal' }).click();
  const dock = await page.getByRole('complementary', { name: 'Execution dock' }).boundingBox();
  const workColumn = await page.locator('.work-column').boundingBox();
  const workspaceAfter = await page.getByRole('complementary', { name: 'Evidence workspace' }).boundingBox();
  expect(dock.x).toBeGreaterThanOrEqual(workColumn.x - 1);
  expect(dock.x + dock.width).toBeLessThanOrEqual(workColumn.x + workColumn.width + 1);
  expect(workspaceAfter.x).toBeCloseTo(workspace.x, 2);
  expect(workspaceAfter.width).toBeCloseTo(workspace.width, 2);
  // CSS layout can land a declared 190px track a fraction below the integer
  // in Chromium (for example 189.99993896484375 at this viewport). Assert the
  // contract at a browser-meaningful precision rather than making a subpixel
  // rounding artifact fail the desktop gate.
  expect(dock.height).toBeCloseTo(190, 2);
  await page.locator('.execution-dock').evaluate((element) =>
    Promise.all(
      element.getAnimations({ subtree: true })
        .filter((animation) => animation.effect?.getTiming().iterations !== Infinity)
        .map((animation) => animation.finished)
    )
  );
  await assertNoHorizontalOverflow(page);
  await assertVisibleElementsStayInViewport(page);
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-workbench-maximum-1919x1079.png'),
  });
  expect(errors).toEqual([]);
});

test('native minimum-sized shell uses one surface and keeps stateful controls reachable', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 480, height: 600 });
  await page.goto(URL);
  await expect(page.getByRole('tablist', { name: 'Primary surface' })).toBeVisible();
  const topbar = await page.locator('.topbar').boundingBox();
  const switcher = await page.getByRole('tablist', { name: 'Primary surface' }).boundingBox();
  expect(topbar.height).toBe(36);
  expect(switcher.height).toBe(34);
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  await page.getByRole('tab', { name: 'browser', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Preview browser' })).toBeVisible();
  await page.getByRole('tab', { name: 'work', exact: true }).click();
  await assertComposerInsideViewport(page);
  await assertNoHorizontalOverflow(page);
  await assertVisibleElementsStayInViewport(page);
  await assertWorkSurfaceContrast(page);
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-workbench-native-minimum-480x600.png'),
  });
  expect(errors).toEqual([]);
});

// Was 'multi-folder project sources migrate into one project identity', driven by
// a hover-revealed rail row on a seeded "Optimus Agent" project. The redesign
// removed the first-run project seed (#42) and moved projects into the rail's
// scope menu, so neither the seed, the "N source" count label, nor the
// #project-manage-optimus-agent focus target exists. The fixture picker also
// returns one fixed path, so a second distinct source is unreachable from here --
// multi-root draft logic stays covered by ProjectSourcesDialog.test.tsx. What only
// an e2e can prove is the rail-menu integration and the focus handoff.
test('project sources open from the rail menu and restore focus on close', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto(URL);

  // Add project authorizes the picked folder and opens its sources dialog.
  await page.getByRole('button', { name: 'Add project' }).click();
  const dialog = page.getByRole('dialog', { name: 'Project sources' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('1 source in this local project')).toBeVisible();
  await expect(dialog.getByText('/projects/new-project')).toBeVisible();
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-project-sources-1280x820.png'),
  });
  // Deliberately not exercising the save/authorize gate here: whether a root
  // counts as authorized is dialog-local state owned by
  // ProjectSourcesDialog.test.tsx. This test is about the rail wiring.
  await page.getByRole('button', { name: 'Close project sources' }).click();
  await expect(dialog).toHaveCount(0);

  // The saved project is reachable from the scope menu, not a hover-only row.
  await page.getByRole('button', { name: 'All projects' }).click();
  const scopeMenu = page.getByRole('menu', { name: 'Filter sessions by project' });
  const manage = scopeMenu.getByRole('menuitem', { name: 'Manage sources for new-project' });
  await expect(manage).toBeVisible();
  await manage.click();
  await expect(page.getByRole('dialog', { name: 'Project sources' })).toBeVisible();

  await page.getByRole('button', { name: 'Close project sources' }).click();
  await expect(page.getByRole('dialog', { name: 'Project sources' })).toHaveCount(0);
  expect(errors).toEqual([]);
});

function collectErrors(page) {
  const errors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', (error) => errors.push(error.message));
  return errors;
}

async function assertNoHorizontalOverflow(page) {
  const dimensions = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));
  expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth);
}

async function assertComposerInsideViewport(page) {
  const box = await page.locator('.composer-card').boundingBox();
  const viewport = page.viewportSize();
  expect(box).not.toBeNull();
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
  expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
}

async function assertComposerControlsInsideCard(page) {
  const card = await page.locator('.composer-card').boundingBox();
  expect(card).not.toBeNull();
  const controls = [
    page.getByRole('button', { name: /^Access: / }),
    page.getByRole('button', { name: 'Model and run settings' }),
    page.getByRole('button', { name: 'Send message' }),
  ];
  for (const control of controls) {
    const box = await control.boundingBox();
    expect(box).not.toBeNull();
    expect(box.x).toBeGreaterThanOrEqual(card.x);
    expect(box.x + box.width).toBeLessThanOrEqual(card.x + card.width);
  }
}

async function assertVisibleElementsStayInViewport(page) {
  const offenders = await page.evaluate(() => {
    const viewport = document.documentElement.clientWidth;
    return Array.from(document.querySelectorAll('*')).flatMap((element) => {
      const style = getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      if (
        style.display === 'none' ||
        style.visibility === 'hidden' ||
        rect.width === 0 ||
        rect.height === 0
      ) {
        return [];
      }
      if (rect.left >= -1 && rect.right <= viewport + 1) return [];
      const overflowOwner = element.closest(
        '.rail-scroll, .transcript, .execution-body, .settings-content, .compact-switcher'
      );
      if (overflowOwner && overflowOwner !== element) return [];
      return [{
        tag: element.tagName,
        className: typeof element.className === 'string' ? element.className : '',
        left: Math.round(rect.left),
        right: Math.round(rect.right),
      }];
    }).slice(0, 20);
  });
  expect(offenders).toEqual([]);
}

// Assert readable assistant text by contrast ratio rather than by a hardcoded
// palette value. The previous form pinned rgb(48, 48, 48) -- the old light
// theme's body colour -- so it broke the moment the redesign restyled tokens
// while saying nothing about whether the result was still legible. A WCAG AA
// ratio holds in either theme and still fails on a real regression.
async function assertWorkSurfaceContrast(page) {
  // Newly mounted turns use a short entry motion. Wait for that finite motion
  // before sampling opacity so this gate checks the settled surface rather
  // than a frame midway through the animation.
  await page.locator('.message').evaluateAll((elements) =>
    Promise.all(
      elements
        .flatMap((element) => element.getAnimations())
        .filter((animation) => animation.effect?.getTiming().iterations !== Infinity)
        .map((animation) => animation.finished)
    )
  );
  const contract = await page.locator('.message-assistant .message-body').first().evaluate((element) => {
    const parse = (value) => {
      const parts = (value.match(/[\d.]+/g) || []).map(Number);
      return { r: parts[0] ?? 0, g: parts[1] ?? 0, b: parts[2] ?? 0, a: parts[3] ?? 1 };
    };
    // Walk up for the first background that is effectively opaque, compositing
    // translucent surfaces onto what sits behind them.
    let background = { r: 255, g: 255, b: 255 };
    let node = element;
    const layers = [];
    while (node) {
      const layer = parse(getComputedStyle(node).backgroundColor);
      if (layer.a > 0) layers.unshift(layer);
      if (layer.a >= 0.999) break;
      node = node.parentElement;
    }
    for (const layer of layers) {
      background = {
        r: layer.r * layer.a + background.r * (1 - layer.a),
        g: layer.g * layer.a + background.g * (1 - layer.a),
        b: layer.b * layer.a + background.b * (1 - layer.a),
      };
    }

    const ancestors = [];
    let current = element;
    while (current) {
      const style = getComputedStyle(current);
      ancestors.push({
        className: typeof current.className === 'string' ? current.className : current.tagName,
        opacity: style.opacity,
      });
      current = current.parentElement;
    }

    const luminance = ({ r, g, b }) => {
      const channel = (value) => {
        const ratio = value / 255;
        return ratio <= 0.03928 ? ratio / 12.92 : ((ratio + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
    };
    const text = parse(getComputedStyle(element).color);
    const lighter = Math.max(luminance(text), luminance(background));
    const darker = Math.min(luminance(text), luminance(background));

    return {
      color: getComputedStyle(element).color,
      ratio: (lighter + 0.05) / (darker + 0.05),
      ancestors,
    };
  });
  expect(contract.ratio).toBeGreaterThanOrEqual(4.5);
  expect(contract.ancestors.filter((item) => item.opacity !== '1')).toEqual([]);
}
