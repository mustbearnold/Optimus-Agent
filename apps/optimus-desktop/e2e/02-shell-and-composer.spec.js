// @ts-check
// spec-015 A3: the React workbench shell + composer, driven over the WS
// transport against a spawned `optimus serve`. Every assertion targets a
// real React surface (roles/aria-labels), never a test hook.
const { test, expect, url, waitForReady } = require('./support');

test('topbar: rail toggle, home mark, workspace, terminal, window controls', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  // The rail toggle flips its label between close/open.
  const railToggle = page.getByRole('button', { name: 'Close project rail' });
  await expect(railToggle).toBeVisible();
  await railToggle.click();
  await expect(page.getByRole('button', { name: 'Open project rail' })).toBeVisible();
  await page.getByRole('button', { name: 'Open project rail' }).click();

  // The product mark returns home (work route).
  await expect(page.getByRole('button', { name: 'Optimus' })).toBeVisible();

  // Workspace / Terminal toggles exist and toggle their pressed state.
  const workspace = page.getByRole('button', { name: 'Workspace', exact: true });
  await expect(workspace).toBeVisible();
  await workspace.click();
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeVisible();
  await workspace.click();

  const terminal = page.getByRole('button', { name: 'Terminal' });
  await terminal.click();
  await expect(page.getByRole('complementary', { name: 'Execution dock' })).toBeVisible();
  await terminal.click();

  // Window controls: minimize, maximize, close (the shell forwards these;
  // in the browser fixture they are present and wired, never silent).
  await expect(page.getByRole('button', { name: 'Minimize' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Maximize', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Close', exact: true })).toBeVisible();
});

test('composer: send button flips to Stop while a run is in flight', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const composer = page.getByLabel('Message Optimus');
  await composer.fill('compose a stop test');
  const send = page.getByRole('button', { name: 'Send message' });
  await expect(send).toBeVisible();
  await send.click();

  // While the (paced) offline turn runs, the send button becomes Stop.
  await expect(page.getByRole('button', { name: 'Stop current run' })).toBeVisible({ timeout: 5000 });

  // Stopping ends the run and the button returns to Send.
  await page.getByRole('button', { name: 'Stop current run' }).click();
  await expect(page.getByRole('button', { name: 'Send message' })).toBeVisible({ timeout: 10000 });
});

test('composer settings: provider defaults to Auto on a fresh home', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.getByRole('button', { name: 'Model and run settings' }).click();
  const dialog = page.getByRole('dialog', { name: 'Model and run settings' });
  await expect(dialog).toBeVisible();
  // Fresh home: Auto provider, Auto model, no fast mode.
  await expect(dialog.locator('select').nth(0)).toHaveValue('auto');
  await expect(dialog.locator('select').nth(1)).toHaveValue('');
  await expect(dialog.getByRole('switch', { name: 'Fast mode' })).toHaveAttribute('aria-checked', 'false');
});

test('composer settings: offline provider selection reaches the status bar', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.getByRole('button', { name: 'Model and run settings' }).click();
  const dialog = page.getByRole('dialog', { name: 'Model and run settings' });
  await dialog.locator('select').nth(0).selectOption('offline');
  await page.keyboard.press('Escape');

  // The status bar names the offline model (Auto → Offline fallback label).
  await expect(page.getByLabel('Session status').locator('.workbench-status-primary')).toContainText('Offline');
});

test('composer access menu: tiers render and selection persists in the status bar', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const accessTrigger = page.getByRole('button', { name: /^Access: / });
  await expect(accessTrigger).toContainText('Standard');
  await accessTrigger.click();
  const listbox = page.getByRole('listbox', { name: 'Access' });
  await expect(listbox).toBeVisible();
  // The tier list includes the break-glass tier at the bottom.
  await expect(listbox.getByRole('option', { name: /Unrestricted host/ })).toBeVisible();
  await listbox.getByRole('option', { name: /Read only/ }).click();
  await expect(page.getByLabel('Session status')).toContainText('Read only');
});

test('status bar: Ready before a run, Working during, terminal state after', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  const status = page.getByLabel('Session status');
  await expect(status).toContainText('Ready');

  await page.getByLabel('Message Optimus').fill('status lifecycle');
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect(status).toContainText('Working', { timeout: 5000 });
  // The paced offline turn terminates; the status settles on a terminal state.
  await expect(status).toContainText('Completed', { timeout: 15000 });
});

test('workbench status segments: model, thinking, access, project', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  const status = page.getByLabel('Session status');
  // Fresh home: Auto model, Minimal thinking default (R8 latency shaping),
  // Standard access, and no project folder (Local session).
  await expect(status.locator('.workbench-status-primary')).toContainText('Auto');
  await expect(status.locator('.workbench-status-secondary').first()).toContainText('Minimal');
  await expect(status.locator('.workbench-status-secondary').last()).toContainText('Standard');
  await expect(status).toContainText('Local session');
});
