// @ts-check
// spec-015 A3: the React workbench's evidence workspace over the WS
// transport. The browser capability is native-only (packaged shell), so
// the WS-driven workbench renders the deterministic fixture page — the
// annotation capture path (annotate → select → gallery → add to prompt)
// is exercised end to end against that fixture, never against a stubbed
// global.
const { test, expect, url, waitForReady, rpc } = require('./support');

async function openWorkspace(page) {
  await page.goto('/');
  await waitForReady(page);
  await page.getByRole('button', { name: 'Workspace', exact: true }).click();
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeVisible();
}

test('workspace opens with browser, files, artifacts tabs', async ({ page }) => {
  await openWorkspace(page);
  const tablist = page.getByRole('tablist', { name: 'Evidence surface' });
  await expect(tablist).toBeVisible();
  await expect(tablist.getByRole('tab', { name: 'Browser' })).toHaveAttribute('aria-selected', 'true');
  await expect(tablist.getByRole('tab', { name: 'Files' })).toBeVisible();
  await expect(tablist.getByRole('tab', { name: 'Artifacts' })).toBeVisible();
});

test('browser surface renders the deterministic fixture page over WS', async ({ page }) => {
  await openWorkspace(page);
  const surface = page.getByRole('tabpanel', { name: 'Preview browser' });
  await expect(surface).toBeVisible();
  // The browser hole carries the fixture page — no native browser over WS.
  await expect(page.getByTestId('browser-hole')).toBeVisible();
  await expect(page.getByLabel('Deterministic browser fixture')).toBeVisible();
  // The toolbar and address affordances are present and wired.
  await expect(page.getByLabel('Browser address')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Reload' })).toBeVisible();
});

test('annotation capture: select in the fixture, note lands in the gallery, add to prompt', async ({ page }) => {
  await openWorkspace(page);
  // Enter annotation mode from the toolbar.
  await page.getByRole('button', { name: 'Annotate preview' }).click();
  await expect(page.getByTestId('browser-hole')).toHaveClass(/is-annotating/);
  // Selecting one element in the fixture captures a gallery note.
  await page.getByTestId('browser-hole').locator('.fixture-page').click();
  const gallery = page.getByLabel('Preview annotation gallery');
  await expect(gallery).toBeVisible();
  await expect(gallery.locator('li').first()).toBeVisible({ timeout: 5000 });
  // The note's action is explicit — add to prompt, never auto-inject.
  const note = gallery.locator('li').first();
  await note.getByRole('button', { name: 'Add to prompt' }).click();
  // The composer receives the note text (onAddToPrompt, ADR-0040).
  await expect(page.getByLabel('Message Optimus')).toHaveValue(/Preview context:/);
});

test('files tab lists the home directory over fs_list', async ({ page, serverInfo }) => {
  await openWorkspace(page);
  await page.getByRole('tab', { name: 'Files' }).click();
  const files = page.getByRole('tabpanel', { name: 'Files' });
  await expect(files).toBeVisible();
  // The tree renders host-side entries — the home path is the root.
  await expect(files.getByRole('treeitem').first()).toBeVisible({ timeout: 5000 });
  const listed = await rpc(serverInfo, 'fs_list', { path: serverInfo.home });
  expect(listed.ok).toBe(true);
});

test('workspace resize via the separator keyboard path changes the width', async ({ page }) => {
  await openWorkspace(page);
  const separator = page.getByRole('separator', { name: 'Resize evidence workspace' });
  await expect(separator).toBeVisible();
  const before = Number(await separator.getAttribute('aria-valuenow'));
  await separator.focus();
  await page.keyboard.press('ArrowRight');
  const after = Number(await separator.getAttribute('aria-valuenow'));
  expect(after).toBeLessThan(before); // the workspace narrows as it grows
});
