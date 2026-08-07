// @ts-check
// spec-015 A3: runtime + sessions over the WS wire. Protocol-level
// assertions go through `rpc()` (the same JSON-RPC 2.0 frames the
// renderer's wsTransport speaks); UI assertions drive the React workbench.
const { test, expect, url, waitForReady, rpc, chatStream } = require('./support');

test('doctor answers over the wire with home and version', async ({ serverInfo }) => {
  const doctor = await rpc(serverInfo, 'doctor');
  expect(doctor.ok).toBe(true);
  expect(doctor.home).toBe(serverInfo.home);
  expect(doctor.version).toBeTruthy();
});

test('new thread creates a session row in the rail', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.getByRole('button', { name: 'New thread' }).click();
  // The rail shows at least one session row after the create.
  await expect(page.locator('.session-row').first()).toBeVisible({ timeout: 5000 });
  // The composer is ready for the fresh session.
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
});

test('multi-turn offline chat streams into the transcript', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const composer = page.getByLabel('Message Optimus');
  await composer.fill('first turn please');
  await page.getByRole('button', { name: 'Send message' }).click();
  // The user message renders, then the offline echo streams in.
  await expect(page.locator('.message.message-user').last()).toContainText('first turn please');
  await expect(page.locator('.message.message-assistant').last()).toContainText('offline echo: first turn please', { timeout: 20000 });

  // Second turn continues the same session (same transcript, new rows).
  await composer.fill('second turn please');
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect(page.locator('.message.message-user')).toHaveCount(2, { timeout: 5000 });
  await expect(page.locator('.message.message-assistant').last()).toContainText('offline echo: second turn please', { timeout: 20000 });
});

test('pin + unpin a session from the rail context menu', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.getByRole('button', { name: 'New thread' }).click();
  const row = page.locator('.session-row').first();
  await expect(row).toBeVisible({ timeout: 5000 });

  // The actions menu opens on context menu (right-click) of the row.
  await row.click({ button: 'right' });
  const menu = page.getByRole('menu', { name: /Actions for / });
  await expect(menu).toBeVisible();
  await menu.getByRole('menuitem', { name: 'Pin session' }).click();
  // The pinned band now holds the session.
  await expect(page.locator('.pinned-section .session-row')).toHaveCount(1, { timeout: 5000 });

  // Unpin from the same menu.
  const pinnedRow = page.locator('.pinned-section .session-row').first();
  await pinnedRow.click({ button: 'right' });
  await page.getByRole('menu', { name: /Actions for / }).getByRole('menuitem', { name: 'Unpin session' }).click();
  await expect(page.locator('.pinned-section .session-row')).toHaveCount(0, { timeout: 5000 });
});

test('fs_roots and fs_list answer over the wire', async ({ serverInfo }) => {
  const roots = await rpc(serverInfo, 'fs_roots');
  expect(roots.ok).toBe(true);
  expect(Array.isArray(roots.roots)).toBe(true);
  expect(roots.roots.some((root) => root.path === serverInfo.home)).toBe(true);
  // The home itself is a scope the host can list.
  const listed = await rpc(serverInfo, 'fs_list', { path: serverInfo.home });
  expect(listed.ok).toBe(true);
  expect(Array.isArray(listed.entries)).toBe(true);
});

test('approvals list is empty on a fresh home; a terminal effect parks an approval', async ({ serverInfo }) => {
  const before = await rpc(serverInfo, 'approvals_list');
  expect(before.ok).toBe(true);
  expect(before.pending || []).toHaveLength(0);

  // term_run parks a command approval (job-stream mode) — the grant path
  // needs a REAL pending job_id, not a synthetic one.
  const run = await rpc(serverInfo, 'term_run', { line: 'echo grant-me' });
  expect(run.ok).toBe(true);
  expect(run.status).toBe('AwaitingApproval');

  const pending = await rpc(serverInfo, 'approvals_list');
  expect(pending.pending || []).toHaveLength(1);
  const jobId = pending.pending[0].job_id;
  expect(jobId).toBeTruthy();

  const granted = await rpc(serverInfo, 'approvals_grant', { job_id: jobId });
  expect(granted.ok).toBe(true);
  expect(String(granted.status || '')).toMatch(/Done|Running|Succeeded|Completed/);
  const after = await rpc(serverInfo, 'approvals_list');
  expect(after.pending || []).toHaveLength(0);
});

test('cron add/list round-trips a named schedule', async ({ serverInfo }) => {
  const name = `pw-cron-${Date.now()}`;
  const added = await rpc(serverInfo, 'cron_add', {
    name,
    cron: '0 9 * * 1-5',
    command: 'offline echo: cron',
  });
  expect(added.ok).toBe(true);
  const listed = await rpc(serverInfo, 'cron_list');
  const hit = (listed.jobs || []).find((j) => j.name === name);
  expect(hit).toBeTruthy();
});

test('campaign create + run over the wire with a tracked job id', async ({ serverInfo }) => {
  const created = await rpc(serverInfo, 'campaign_create', {
    name: `pw-campaign-${Date.now()}`,
    writes: [{ path: 'out.txt', contents: 'offline campaign' }],
  });
  expect(created.ok).toBe(true);
  expect(created.id).toBeTruthy();
  expect(created.steps).toBe(1);
  const run = await rpc(serverInfo, 'campaign_run', { id: created.id });
  // Campaign writes are approval-gated: the run parks in AwaitingApproval
  // and the approval list carries the tracked job (R9's tracked-job
  // contract), or completes when a grant already exists.
  expect(run.ok).toBe(true);
  expect(run.id).toBeTruthy();
  expect(run.status).toMatch(/Pending|Running|Succeeded|Failed|Cancelled|AwaitingApproval/);

  // Resolve the parked approval so the worker home is clean for later
  // specs (the dock test asserts a fresh approvals list).
  const pending = await rpc(serverInfo, 'approvals_list');
  const parked = (pending.pending || []).find((a) => /campaign/i.test(String(a.job_label || '')));
  if (parked?.job_id) {
    await rpc(serverInfo, 'approvals_grant', { job_id: parked.job_id });
  }
});
