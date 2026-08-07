// @ts-check
// spec-015 A3: the React workbench's shell geometry and design tokens,
// driven over the WS transport. The vanilla chrome (custom titlebar ids,
// --v-* token names) does not exist here; this spec pins the React
// workbench's real shell contract instead.
const { test, expect, url, waitForReady } = require('./support');

test('window controls are present and labeled', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  // The workbench shell owns the window-control chrome.
  await expect(page.getByRole('button', { name: 'Minimize' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Maximize', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Close', exact: true })).toBeVisible();
});

test('maximize workspace toggles the surface-row state', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const row = page.locator('.surface-row');
  await expect(row).not.toHaveClass(/is-workspace-maximized/);
  await page.getByRole('button', { name: 'Maximize workspace' }).click();
  await expect(row).toHaveClass(/is-workspace-maximized/);
  await page.getByRole('button', { name: 'Restore workspace' }).click();
  await expect(row).not.toHaveClass(/is-workspace-maximized/);
});

test('rail collapse toggles the collapsed rail class', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const rail = page.getByLabel('Projects and sessions');
  await expect(rail).not.toHaveClass(/is-collapsed/);
  await page.getByRole('button', { name: 'Close project rail' }).click();
  await expect(rail).toHaveClass(/is-collapsed/);
  await page.getByRole('button', { name: 'Open project rail' }).click();
  await expect(rail).not.toHaveClass(/is-collapsed/);
});

test('theme select in settings flips the data-theme attribute', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  // Light is the fresh-home default shell theme.
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  await expect(settings).toBeVisible();
  await settings.getByRole('button', { name: /Appearance/ }).click();
  const themeRow = settings.locator('.settings-row', { hasText: 'Color theme' });
  const themeSelect = themeRow.locator('select');
  await themeSelect.selectOption('dark');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await themeSelect.selectOption('light');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});

test('status bar segments render the session identity', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  const status = page.getByLabel('Session status');
  // The state chip, the model segment, and the project segment all render.
  await expect(status.locator('.workbench-status-state')).toBeVisible();
  await expect(status.locator('.workbench-status-primary')).toBeVisible();
  await expect(status.locator('.workbench-status-secondary').first()).toBeVisible();
  await expect(status.locator('.workbench-status-secondary').last()).toBeVisible();
});

test('terminal opens the execution dock; its resize separator is keyboard-driven', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  // The compact surface flips to execution while the dock is open.
  const app = page.locator('.optimus-app');
  await expect(app).toHaveAttribute('data-compact-surface', 'work');
  await page.getByRole('button', { name: 'Terminal' }).click();
  await expect(app).toHaveAttribute('data-compact-surface', 'execution');

  const dock = page.getByRole('complementary', { name: 'Execution dock' });
  await expect(dock).toBeVisible();
  const separator = page.getByRole('separator', { name: 'Resize execution dock' });
  await expect(separator).toBeVisible();
  const before = Number(await separator.getAttribute('aria-valuenow'));
  await separator.focus();
  await page.keyboard.press('ArrowUp');
  const after = Number(await separator.getAttribute('aria-valuenow'));
  expect(after).toBeGreaterThan(before);

  await page.getByRole('button', { name: 'Close execution dock' }).click();
  await expect(dock).toHaveCount(0);
  await expect(app).toHaveAttribute('data-compact-surface', 'work');
});
