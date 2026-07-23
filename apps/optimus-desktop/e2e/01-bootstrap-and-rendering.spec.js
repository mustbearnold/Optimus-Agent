// @ts-check
const { test, expect, url, waitForReady } = require('./support');

test('health API is up', async () => {
  const r = await fetch(`${url()}/api/health`);
  expect(r.ok).toBeTruthy();
  const j = await r.json();
  expect(j.ok).toBe(true);
  expect(j.streaming).toBe(true);
  expect(j).not.toHaveProperty('home');
});

test('UI boots and leaves Starting… state', async ({ page }) => {
  const startupErrors = [];
  page.on('pageerror', (error) => startupErrors.push(`pageerror: ${error.message}`));
  page.on('console', (message) => {
    if (message.type() === 'error') startupErrors.push(`console: ${message.text()}`);
  });
  await page.goto('/');
  await waitForReady(page);
  await expect(page.locator('#stVer')).not.toHaveText('ver…');
  expect(startupErrors).toEqual([]);
  // Settled shell: left nav + list chrome present (Hermes-compact rail)
  await expect(page.locator('#newThread')).toBeVisible();
  await expect(page.locator('#navCapabilities')).toBeVisible();
  await expect(page.locator('#railSplit')).toBeVisible();
  await expect(page.locator('#pinnedPane')).toBeVisible();
  await expect(page.locator('#listPane')).toBeVisible();
  await expect(page.locator('#modeProjects')).toBeVisible();
  await expect(page.locator('#settingsBtn')).toBeVisible();
  // SIGNAL rail removed
  await expect(page.locator('#signalPanel')).toBeHidden();
});

test('Enter streams offline reply progressively', async ({ page }) => {
  await page.goto('/');
  await page.waitForFunction(() => window.__optimusBridgeInstalled === true);
  await waitForReady(page);

  await page.selectOption('#provider', 'offline');
  const input = page.locator('#input');
  await input.fill('stream-me-please-xyz');
  await input.press('Enter');

  // Progressive: assistant bubble appears before full done
  await expect(page.locator('.msg.assistant .bubble')).toBeVisible({ timeout: 15000 });
  await expect(page.locator('.msg.user .bubble')).toContainText('stream-me-please-xyz');
  await expect(page.locator('.msg.assistant .bubble')).toContainText('offline echo', {
    timeout: 30000,
  });
  await expect(page.locator('.msg.assistant .bubble')).toContainText('stream-me-please-xyz');
  await expect(input).toHaveValue('');
});

test('SSE stream endpoint emits delta then done', async () => {
  // create session
  const ns = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 1, method: 'new_session', params: {} }),
  }).then((r) => r.json());
  const session = ns.result.id;
  const r = await fetch(`${url()}/api/chat/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      message: 'sse-chunk-test',
      provider: 'offline',
      session,
    }),
  });
  expect(r.ok).toBeTruthy();
  const text = await r.text();
  expect(text).toContain('data: ');
  expect(text).toContain('"type":"delta"');
  expect(text).toContain('"type":"done"');
  expect(text).toContain('offline echo');
});

test('two held SSE responses do not block the HTTP accept loop', async () => {
  test.setTimeout(60000);
  const message = `held-stream-${'x'.repeat(512 * 1024)}`;
  const startStream = () => fetch(`${url()}/api/chat/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message, provider: 'offline' }),
  });

  const [first, second] = await Promise.all([startStream(), startStream()]);
  expect(first.ok).toBeTruthy();
  expect(second.ok).toBeTruthy();

  const health = await Promise.race([
    fetch(`${url()}/api/health`),
    new Promise((_, reject) => setTimeout(() => reject(new Error('health blocked by SSE')), 3000)),
  ]);
  expect(health.ok).toBeTruthy();
  expect((await health.json()).ok).toBeTruthy();

  await Promise.all([first.body.cancel(), second.body.cancel()]);
});

test('bridge HTTP stream cancellation is local and one-shot', async ({ page }) => {
  await page.route('**/api/chat/stream', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 500));
    await route.fulfill({
      status: 200,
      contentType: 'text/event-stream',
      body: 'data: {"type":"done","result":{}}\n\n',
    }).catch(() => {});
  });
  await page.goto('/');
  await waitForReady(page);

  const observed = await page.evaluate(async () => {
    const stream = window.optimus.chatStream('cancel locally', { provider: 'offline' }, () => {});
    const first = stream.cancel();
    const second = stream.cancel();
    try {
      await stream;
      return { first, second, settled: 'resolved' };
    } catch (error) {
      return { first, second, settled: error && error.name };
    }
  });

  expect(observed).toEqual({ first: true, second: false, settled: 'AbortError' });
});

test('coalesceTools merges repeated web_search', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const n = await page.evaluate(() => {
    const tools = [
      { name: 'web_search', detail: 'a', status: 'ok' },
      { name: 'web_search', detail: 'b', status: 'ok' },
      { name: 'web_search', detail: 'c', status: 'ok' },
      { name: 'web_search', detail: 'd', status: 'ok' },
      { name: 'web_search', detail: 'e', status: 'ok' },
    ];
    const g = window.__optimusTest.coalesceTools(tools);
    return { groups: g.length, count: g[0].count, label: g[0].count > 1 ? `${g[0].name} ×${g[0].count}` : g[0].name };
  });
  expect(n.groups).toBe(1);
  expect(n.count).toBe(5);
  expect(n.label).toBe('web_search ×5');
});

test('formatRich hides tool JSON and keeps prose', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const cleaned = await page.evaluate(() => {
    const only = '[{"id":"call_x","name":"web_search","arguments":{"query":"nz"}}]';
    const mixed = only + '\n\n**Hello** world\n- item';
    const strip = window.__optimusTest.stripToolCallNoise;
    return { only: strip(only), mixed: strip(mixed) };
  });
  expect(cleaned.only).toBe('');
  expect(cleaned.mixed).toContain('**Hello**');
  expect(cleaned.mixed).not.toContain('call_x');
});

test('formatRich cannot create event-handler attributes from markdown links', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const rendered = await page.evaluate(() => {
    const host = document.createElement('div');
    const hooks = (/** @type {any} */ (window)).__optimusTest;
    host.innerHTML = hooks.formatRich(
      '[safe](https://example.com/"onmouseover="globalThis.__optimusLinkPwned=1")'
    );
    const link = host.querySelector('a.md-link');
    return {
      linkCount: host.querySelectorAll('a.md-link').length,
      eventAttributes: link
        ? [...link.attributes].map((attr) => attr.name).filter((name) => /^on/i.test(name))
        : [],
    };
  });

  expect(rendered.linkCount).toBe(1);
  expect(rendered.eventAttributes).toEqual([]);
});

test('active turns retain session ownership until streaming settles', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const blocked = await page.evaluate(async () => {
    const hooks = (/** @type {any} */ (window)).__optimusTest;
    hooks.setBusy(true);
    try {
      return {
        created: await hooks.newSession(),
        opened: await hooks.openSession('must-not-be-requested'),
      };
    } finally {
      hooks.setBusy(false);
    }
  });

  expect(blocked).toEqual({ created: false, opened: false });
});
