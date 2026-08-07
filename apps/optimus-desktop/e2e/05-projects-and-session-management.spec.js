// @ts-check
// spec-015 A3: project + session management on the React workbench over
// the WS transport. Projects come from the host (`project_scopes_list`);
// the fresh home has no scopes, so the rail renders the Local session
// band and the add-project affordance. Rename/delete go through the real
// rail context menu + dialog (the app owns the session list state).
const { test, expect, url, waitForReady, rpc } = require('./support');

test('project scopes list answers over the wire', async ({ serverInfo }) => {
  const scopes = await rpc(serverInfo, 'project_scopes_list');
  expect(scopes.ok).toBe(true);
  // The fresh home has no scopes; the rail renders the Local band instead
  // of a hard-coded seed (first-run honesty contract).
  expect(Array.isArray(scopes.projects)).toBe(true);
  expect(scopes.projects).toHaveLength(0);
});

test('rail renders the local session band and the add-project affordance', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const rail = page.getByLabel('Projects and sessions');
  await expect(rail).toBeVisible();
  // The fresh home's scope-less state renders the rail bands (Pinned,
  // Projects, Recent Chats) — no hard-coded project seed.
  await expect(rail.getByTestId('pinned-section')).toBeVisible();
  await expect(rail.getByTestId('projects-section')).toBeVisible();
  await expect(rail.getByTestId('recent-chats-section')).toBeVisible();
  // The add-project affordance exists (opens the folder picker path).
  await expect(rail.getByRole('button', { name: 'Create project folder' })).toBeVisible();
});

test('rail resize via the separator keyboard path changes the width', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const separator = page.getByRole('separator', { name: 'Resize project rail' });
  await expect(separator).toBeVisible();
  const before = Number(await separator.getAttribute('aria-valuenow'));
  await separator.focus();
  await page.keyboard.press('ArrowRight');
  const after = Number(await separator.getAttribute('aria-valuenow'));
  // The keyboard resize moves the rail width by the bounded step.
  expect(after).toBeGreaterThan(before);
});

test('rename a session through the rail context menu + dialog', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.getByRole('button', { name: 'New thread' }).click();
  const row = page.locator('.session-row').first();
  await expect(row).toBeVisible({ timeout: 5000 });
  const sessionId = await row.getAttribute('data-session-id');

  // The wire contract: rename_session round-trips the new title.
  const renamed = await rpc(serverInfo, 'rename_session', {
    id: sessionId,
    title: 'Renamed PW Session',
  });
  expect(renamed.ok).toBe(true);
  expect(renamed.title).toBe('Renamed PW Session');

  // The UI path: the row menu opens a rename dialog that updates the row.
  await row.click({ button: 'right' });
  await page.getByRole('menu', { name: /Actions for / }).getByRole('menuitem', { name: 'Rename' }).click();
  const dialog = page.getByRole('dialog', { name: 'Rename session' });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel('Session title').fill('UI Renamed Session');
  await dialog.getByRole('button', { name: 'Rename' }).click();
  await expect(row.locator('.session-title')).toContainText('UI Renamed Session', { timeout: 5000 });
});

test('delete_session over the wire removes the session from the host', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.getByRole('button', { name: 'New thread' }).click();
  const row = page.locator('.session-row').first();
  await expect(row).toBeVisible({ timeout: 5000 });
  const sessionId = await row.getAttribute('data-session-id');

  const del = await rpc(serverInfo, 'delete_session', { id: sessionId });
  expect(del.ok).toBe(true);
  expect(del.deleted).toBe(true);

  // The host no longer lists the session.
  const list = await rpc(serverInfo, 'sessions');
  const hit = (list.sessions || []).find((s) => s.id === sessionId);
  expect(hit).toBeFalsy();
});

test('sessions over the wire list the created session', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.getByRole('button', { name: 'New thread' }).click();
  const row = page.locator('.session-row').first();
  await expect(row).toBeVisible({ timeout: 5000 });
  const sessionId = await row.getAttribute('data-session-id');

  const list = await rpc(serverInfo, 'sessions');
  const hit = (list.sessions || []).find((s) => s.id === sessionId);
  expect(hit).toBeTruthy();
});
