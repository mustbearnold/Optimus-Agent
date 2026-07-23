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
  expect((await rail.boundingBox()).width).toBe(232);
  expect((await workspace.boundingBox()).width).toBeGreaterThanOrEqual(700);
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  await assertComposerInsideViewport(page);
  await page.screenshot({
    path: path.join(EVIDENCE_DIR, 'react-workbench-wide-1600x1000.png'),
  });
  expect(errors).toEqual([]);
});

test('medium 960x760 preserves controls without a three-column overflow', async ({ page }) => {
  const errors = collectErrors(page);
  await page.setViewportSize({ width: 960, height: 760 });
  await page.goto(URL);
  const rail = await page.getByRole('complementary', { name: 'Projects and sessions' }).boundingBox();
  const workspace = await page.getByRole('complementary', { name: 'Evidence workspace' }).boundingBox();
  expect(rail.width).toBe(208);
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
  await assertComposerInsideViewport(page);
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
  await assertComposerInsideViewport(page);
  await assertNoHorizontalOverflow(page);
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
  await page.getByRole('button', { name: 'Use light theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: 'Capabilities' }).click();
  await expect(page.getByRole('main', { name: 'Capabilities' })).toBeVisible();
  await expect(page.getByText('Specialist agents — unavailable')).toBeVisible();
  await page.getByRole('button', { name: 'Artifacts', exact: true }).first().click();
  await expect(page.getByRole('region', { name: 'Artifacts' })).toBeVisible();
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByRole('dialog', { name: 'Settings' })).toBeVisible();
  await page.getByRole('button', { name: 'Done' }).click();
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
