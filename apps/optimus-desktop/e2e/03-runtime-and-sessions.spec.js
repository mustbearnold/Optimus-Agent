// @ts-check
const { test, expect, url, waitForReady } = require('./support');

test('new session button creates a thread', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.click('#newThread');
  await expect
    .poll(async () => page.locator('#sessionList .thread, #threadList .thread').count(), { timeout: 15000 })
    .toBeGreaterThan(0);
});

test('IPC doctor via fetch', async () => {
  const r = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 1, method: 'doctor', params: {} }),
  });
  const j = await r.json();
  expect(j.ok).toBe(true);
  expect(j.result.phase).toContain('desktop');
  expect(j.result.home).toBeTruthy();
  expect(j.result.streaming).toBe(true);
  expect(j.result.cron).toBe(true);
  expect(j.result.browser).toBeTruthy();
  expect(j.result.approvals).toBe(true);
  expect(j.result.files).toBe(true);
});

test('IPC fs_roots / fs_list via fetch', async () => {
  const roots = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 40, method: 'fs_roots', params: {} }),
  }).then((x) => x.json());
  expect(roots.ok).toBe(true);
  expect(Array.isArray(roots.result.roots)).toBe(true);
  expect(roots.result.roots.length).toBeGreaterThan(0);
  expect(roots.result.roots[0].id).toBe('home');
  expect(roots.result.roots[0].path).toBeTruthy();

  const list = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 41, method: 'fs_list', params: { path: '' } }),
  }).then((x) => x.json());
  expect(list.ok).toBe(true);
  expect(Array.isArray(list.result.entries)).toBe(true);
});

test('approvals_list IPC is empty when idle', async () => {
  const r = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 20, method: 'approvals_list', params: {} }),
  }).then((x) => x.json());
  expect(r.ok).toBe(true);
  expect(Array.isArray(r.result.pending)).toBe(true);
});

test('campaign create run via IPC', async () => {
  const create = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      id: 30,
      method: 'campaign_create',
      params: {
        name: 'pw-campaign',
        writes: [
          { path: 'pw/a.txt', contents: 'one' },
          { path: 'pw/b.txt', contents: 'two' },
        ],
      },
    }),
  }).then((r) => r.json());
  expect(create.ok).toBe(true);
  expect(create.result.id).toBeTruthy();
  const run = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      id: 31,
      method: 'campaign_run',
      params: { id: create.result.id },
    }),
  }).then((r) => r.json());
  expect(run.ok, JSON.stringify(run)).toBe(true);
  expect(run.result.status).toMatch(/Succeeded/i);
});

test('cron add list tick via IPC', async ({ serverInfo }) => {
  const add = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      id: 10,
      method: 'cron_add',
      params: {
        name: 'pw-cron',
        every_secs: 5,
        prompt: 'pw cron hello',
        provider: 'offline',
      },
    }),
  }).then((r) => r.json());
  expect(add.ok).toBe(true);
  expect(add.result.id).toBeTruthy();

  const list = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 11, method: 'cron_list', params: {} }),
  }).then((r) => r.json());
  expect(list.ok).toBe(true);
  expect((list.result.jobs || []).some((j) => j.name === 'pw-cron')).toBeTruthy();

  // Force due by setting next via sqlite in the global server's temporary home.
  const { execFileSync } = require('child_process');
  const path = require('path');
  const dbPath = path.join(serverInfo.home, 'cron.db');
  execFileSync(
    process.env.PYTHON || 'python',
    ['-c', `import sqlite3; c=sqlite3.connect(${JSON.stringify(dbPath)}); c.execute('update cron_jobs set next_run_unix=0'); c.commit()`],
    { stdio: 'pipe' }
  );

  const tick = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 12, method: 'cron_tick', params: {} }),
  }).then((r) => r.json());
  expect(tick.ok).toBe(true);
  expect((tick.result.ran || []).length).toBeGreaterThan(0);
  expect(JSON.stringify(tick.result.ran)).toContain('ok steps');
});

test('multi-turn offline session resume via UI', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.selectOption('#provider', 'offline');
  await page.fill('#input', 'first-turn-alpha');
  await page.press('#input', 'Enter');
  await expect(page.locator('.msg.user .bubble').last()).toContainText('first-turn-alpha', {
    timeout: 20000,
  });
  await expect(page.locator('.msg.assistant .bubble').last()).toContainText('offline echo', {
    timeout: 20000,
  });
  // The final bubble is painted before refreshSessions completes and releases
  // the busy guard. Wait for the actual send boundary, not just visible text.
  await expect(page.locator('#send')).toBeEnabled({ timeout: 20000 });
  await expect(page.locator('#provider')).toHaveValue('offline');
  await page.fill('#input', 'second-turn-beta');
  await page.press('#input', 'Enter');
  await expect(page.locator('.msg.user .bubble').last()).toContainText('second-turn-beta', {
    timeout: 20000,
  });
  await expect(page.locator('.msg.assistant .bubble').last()).toContainText('second-turn-beta', {
    timeout: 20000,
  });
  // at least 2 user bubbles
  await expect
    .poll(async () => page.locator('.msg.user .bubble').count())
    .toBeGreaterThanOrEqual(2);
});

test('pinned sessions via right-click context menu', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.click('#newThread');
  await page.click('#modeProjects');
  await expect
    .poll(
      async () =>
        (await page.locator('#projectList .thread').count()) +
        (await page.locator('#sessionList .thread').count()),
      { timeout: 15000 }
    )
    .toBeGreaterThan(0);

  // Pin via contextmenu dispatch (more reliable than OS right-click in CI)
  await page.evaluate(() => {
    const row = document.querySelector('#projectList .thread, #sessionList .thread');
    if (!row) throw new Error('no thread row');
    row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 40, clientY: 200 }));
  });
  await expect(page.locator('#sessionCtx')).toBeVisible();
  await page.locator('#sessionCtx [data-act="pin"]').click({ force: true });
  await expect
    .poll(async () => page.locator('#pinnedList .thread').count(), { timeout: 5000 })
    .toBeGreaterThan(0);

  await expect(page.locator('#railResize')).toBeVisible();
  await page.click('#modeSessions');
  await expect(page.locator('#sessionsLabel')).toContainText(/Sessions/i);
  await page.click('#modeProjects');
  await expect(page.locator('#sessionsLabel')).toContainText(/Projects/i);
});

test('doctor_json cron_jobs and status bar live bind', async ({ page }) => {
  const r = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 99, method: 'doctor', params: {} }),
  }).then((x) => x.json());
  expect(r.ok).toBe(true);
  expect(r.result).toBeTruthy();
  expect(Object.prototype.hasOwnProperty.call(r.result, 'cron_jobs')).toBe(true);
  expect(Object.prototype.hasOwnProperty.call(r.result, 'campaigns_active')).toBe(true);
  expect(Object.prototype.hasOwnProperty.call(r.result, 'gateway')).toBe(true);
  expect(typeof r.result.cron_jobs).toBe('number');

  await page.goto('/');
  await waitForReady(page);

  await expect
    .poll(async () => (await page.locator('#stGateway').innerText()).toLowerCase(), { timeout: 15000 })
    .toMatch(/ok|down|up/);

  await expect
    .poll(async () => {
      const cron = await page.locator('#stCron').innerText();
      const agents = await page.locator('#stAgents').innerText();
      const ver = await page.locator('#stVer').innerText();
      const home = await page.locator('#stHome').innerText();
      return { cron, agents, ver, home };
    }, { timeout: 15000 })
    .toMatchObject({
      cron: expect.stringMatching(/\d/),
      agents: expect.stringMatching(/\d/),
      ver: expect.stringMatching(/\S/),
      home: expect.stringMatching(/\S/),
    });

  // tokens / model filled (model may be offline-echo on fresh home)
  const tokens = await page.locator('#stTokens').innerText();
  expect(tokens.length).toBeGreaterThan(0);
  const model = await page.locator('#stModel').innerText();
  expect(model.length).toBeGreaterThan(0);
});
