const { test, expect } = require('@playwright/test');
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '../../..');
const EVIDENCE_DIR = path.join(ROOT, 'local', 'tmp');
const URL = 'http://127.0.0.1:4174/';

test.beforeAll(() => {
  fs.mkdirSync(EVIDENCE_DIR, { recursive: true });
});

test('wide 1600x1000 renders the dense three-surface workbench', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.goto(URL);

  const rail = page.getByRole('complementary', { name: 'Projects and sessions' });
  const work = page.getByRole('region', { name: 'Agent work surface' });
  const workspace = page.getByRole('complementary', { name: 'Evidence workspace' });
  await expect(rail).toBeVisible();
  await expect(work).toBeVisible();
  await expect(workspace).toBeVisible();
  expect((await rail.boundingBox()).width).toBe(240);
  expect((await workspace.boundingBox()).width).toBeGreaterThanOrEqual(700);
  const searchBox = await page.locator('.rail-search').boundingBox();
  const newSessionBox = await page.getByRole('button', { name: 'New session', exact: true }).boundingBox();
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
  await expect(page.getByLabel('Thinking level')).toBeVisible();
  await expect(page.getByLabel('Access')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Fast' })).toBeVisible();
  await assertComposerInsideViewport(page);
  await assertWorkSurfaceContrast(page);
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-workbench-compact-640x800.png'),
  });
  expect(errors).toEqual([]);
});

test('320 CSS px reflow and reduced motion preserve state and focus', async ({ page }) => {
  const errors = collectErrors(page);
  await page.emulateMedia({ reducedMotion: 'reduce', colorScheme: 'dark' });
  await page.setViewportSize({ width: 320, height: 800 });
  await page.goto(URL);
  await expect(page.getByRole('tablist', { name: 'Primary surface' })).toBeVisible();
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  await expect(page.getByLabel('Provider')).toBeVisible();
  await expect(page.getByLabel('Model')).toBeVisible();
  await expect(page.getByLabel('Thinking level')).toBeVisible();
  await expect(page.getByLabel('Access')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Fast' })).toBeVisible();
  await assertComposerInsideViewport(page);
  await assertComposerControlsInsideCard(page);
  await assertNoHorizontalOverflow(page);
  await page.getByLabel('Message Optimus').focus();
  await page.keyboard.press('Tab');
  await expect(page.getByLabel('Provider')).toBeFocused();
  const focusShadow = await page.getByLabel('Provider').evaluate((element) =>
    getComputedStyle(element).boxShadow
  );
  expect(focusShadow).not.toBe('none');
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
  await page.getByRole('button', { name: 'Use dark theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByRole('button', { name: 'Use light theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: 'Capabilities' }).click();
  await expect(page.getByRole('main', { name: 'Capabilities' })).toBeVisible();
  const specialistBoundary = page.locator('.capability-boundary li').filter({ hasText: 'Specialist agents' });
  await expect(specialistBoundary.getByText('Unavailable', { exact: true })).toBeVisible();
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-capabilities-light-1280x820.png'),
  });
  await page.getByRole('button', { name: 'Artifacts', exact: true }).first().click();
  await expect(page.getByRole('region', { name: 'Artifacts' })).toBeVisible();
  await page.getByRole('button', { name: 'Settings' }).click();
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
  expect(dock.height).toBe(190);
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

test('multi-folder project sources migrate into one project identity', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto(URL);
  await page.getByRole('button', { name: 'Optimus Agent 1 source' }).hover();
  await page.getByRole('button', { name: 'Manage sources for Optimus Agent' }).click();
  const dialog = page.getByRole('dialog', { name: 'Project sources' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('1 source in this local project')).toBeVisible();
  await dialog.getByRole('button', { name: 'Add source' }).click();
  await expect(dialog.getByText('2 sources in this local project')).toBeVisible();
  await expect(dialog.getByText('/home/dev/Projects/New Project')).toBeVisible();
  await dialog.getByRole('button', { name: 'Make primary' }).click();
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-project-sources-1280x820.png'),
  });
  await dialog.getByRole('button', { name: 'Save project' }).click();
  await page.getByRole('button', { name: 'Optimus Agent 2 sources' }).hover();
  await page.getByRole('button', { name: 'Manage sources for Optimus Agent' }).click();
  await expect(page.getByRole('button', { name: 'Primary', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close project sources' }).click();
  const restoredManageButton = page.locator('#project-manage-optimus-agent');
  await expect(restoredManageButton).toBeFocused();
  await expect(restoredManageButton).toHaveCSS('visibility', 'visible');
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
    page.getByLabel('Provider'),
    page.getByLabel('Model'),
    page.getByLabel('Thinking level'),
    page.getByLabel('Access'),
    page.getByRole('button', { name: 'Fast' }),
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

async function assertWorkSurfaceContrast(page) {
  const contract = await page.locator('.message-assistant .message-body').first().evaluate((element) => {
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
    return {
      color: getComputedStyle(element).color,
      ancestors,
    };
  });
  expect(contract.color).toBe('rgb(48, 48, 48)');
  expect(contract.ancestors.filter((item) => item.opacity !== '1')).toEqual([]);
}
