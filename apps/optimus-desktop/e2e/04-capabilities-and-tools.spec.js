// @ts-check
// spec-015 A3: capabilities + tools surfaces of the React workbench over
// the WS transport. The capabilities page opens through the command
// palette; the catalog is projected from `doctor` over the wire; cron
// lives under Settings → Automations; approvals surface in the execution
// dock.
const { test, expect, url, waitForReady, rpc } = require('./support');

async function openCapabilities(page) {
  // The palette opens with Ctrl/Cmd+K and routes via command ids.
  await page.keyboard.press('ControlOrMeta+k');
  const palette = page.getByRole('dialog');
  await expect(palette).toBeVisible();
  await palette.getByRole('option', { name: /capabilities/i }).click();
  await expect(page.getByRole('main', { name: 'Capabilities' })).toBeVisible();
}

test('capabilities page renders the runtime summary from doctor', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);
  await openCapabilities(page);

  const summary = page.getByRole('region', { name: 'Runtime summary' });
  await expect(summary).toBeVisible();
  // The summary is projected from the real doctor answer.
  const doctor = await rpc(serverInfo, 'doctor');
  expect(doctor.ok).toBe(true);
  await expect(summary).toContainText(String(doctor.campaigns_active));
  // The capabilities main surface carries the page name.
  await expect(page.getByRole('heading', { name: 'Runtime capabilities' })).toBeVisible();
});

test('capabilities catalog shows the canonical core pack', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);
  await openCapabilities(page);

  // The tool catalog rows render from the host's pack descriptors.
  await expect(page.locator('.tool-row').first()).toBeVisible({ timeout: 5000 });
  const doctor = await rpc(serverInfo, 'doctor');
  expect(doctor.pack_catalog?.length || 0).toBeGreaterThan(0);
});

test('cron workbench lives under Settings → Automations', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);
  // Settings is the rail's footer action.
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog');
  await expect(settings).toBeVisible();
  await settings.getByRole('button', { name: /Automations/ }).click();
  // The cron workbench renders inside the settings surface.
  await expect(settings.getByLabel('Cron schedules')).toBeVisible({ timeout: 5000 });
});

test('execution dock: approvals tab lists pending approvals', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.getByRole('button', { name: 'Terminal' }).click();
  const dock = page.getByRole('complementary', { name: 'Execution dock' });
  await expect(dock).toBeVisible();
  await dock.getByRole('tab', { name: /Approvals/ }).click();
  // Fresh home: no pending approvals.
  await expect(dock.getByLabel('Pending approvals')).toContainText('No pending approvals');
});

test('term_run requires explicit approval before a command effect', async ({ serverInfo }) => {
  // The terminal effect is gated: term_run parks the command in the
  // approval list (job-stream mode, no PTY) instead of running it.
  const run = await rpc(serverInfo, 'term_run', { line: 'echo optimus-term-ok' });
  expect(run.ok).toBe(true);
  expect(run.status).toBe('AwaitingApproval');
  expect(String(run.stdout || '')).toBe('');
  expect(run.mode).toBe('job-stream');
  expect(run.pty).toBe(false);

  const pending = await rpc(serverInfo, 'approvals_list');
  expect(pending.pending || []).toHaveLength(1);
  expect(String(pending.pending[0].job_label || '')).toContain('echo optimus-term-ok');
});
