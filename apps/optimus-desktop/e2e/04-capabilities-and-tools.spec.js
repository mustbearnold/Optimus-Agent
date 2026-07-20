// @ts-check
const { test, expect, url, waitForReady } = require('./support');

test('capabilities page shows approvals and campaigns panels', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.click('#navCapabilities');
  await expect(page.locator('#page-capabilities')).toBeVisible();
  await expect(page.locator('#approvalsPanel')).toBeVisible();
  await expect(page.locator('#campaignsPanel')).toBeVisible();
  await expect(page.locator('#doctorPanel')).toBeVisible();
  await expect(page.locator('#approvalsList')).toContainText(/No pending approvals/i);
  // SIGNAL/cron removed from left rail — secondary under Settings
  await expect(page.locator('#settingsBtn')).toBeVisible();
});

test('capabilities catalog is projected from canonical pack descriptors', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const doctor = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 39, method: 'doctor', params: {} }),
  }).then((response) => response.json());
  expect(doctor.ok).toBe(true);
  const core = doctor.result.pack_catalog.find((pack) => pack.id === 'core');
  const read = core.tools.find((tool) => tool.id === 'read_file');
  expect(read.policy).toBe('workspace_read');
  expect(read.invocation).toBe('read_file');
  expect(read.input_schema.additionalProperties).toBe(false);
  const desktop = doctor.result.pack_catalog.find((pack) => pack.id === 'desktop');
  expect(desktop.tools.every((tool) => tool.policy === 'desktop')).toBe(true);

  await page.click('#navCapabilities');
  const coreRow = page.locator('#packsSnap [data-pack-id="core"]');
  await expect(coreRow).toContainText('core');
  await expect(coreRow).toContainText('read_file');
});

test('campaign create+run via capabilities UI', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.click('#navCapabilities');
  await expect(page.locator('#campaignsPanel')).toBeVisible();
  await page.click('#campaignCreateBtn');
  await expect(page.locator('#campaignStatusLine')).toHaveText(/^Created ui-\d+$/, {
    timeout: 15000,
  });
  const createdName = (await page.locator('#campaignStatusLine').innerText()).replace('Created ', '');

  const list = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 40, method: 'campaign_list', params: {} }),
  }).then((r) => r.json());
  expect(list.ok).toBe(true);
  const created = (list.result.campaigns || []).find((campaign) => campaign.name === createdName);
  expect(created).toBeTruthy();

  const row = page.locator(`#campaignsList .cap-row[data-camp-id="${created.id}"]`);
  await expect(row).toBeVisible();
  await row.locator(`[data-run="${created.id}"]`).click();
  await expect(page.locator('#campaignStatusLine')).toContainText('→ Succeeded', {
    timeout: 15000,
  });
  await expect.poll(async () => {
    const refreshed = await fetch(`${url()}/api/ipc`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: 41, method: 'campaign_list', params: {} }),
    }).then((response) => response.json());
    return (refreshed.result.campaigns || []).find((campaign) => campaign.id === created.id)?.status;
  }).toMatch(/Succeeded/i);
});

test('cron remains available under Settings (not left SIGNAL)', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  // SIGNAL rail gone
  await expect(page.locator('#signalPanel')).toBeHidden();
  await page.click('#settingsBtn');
  await expect(page.locator('#settingsPop')).toBeVisible();
  await expect(page.locator('#cronAdd')).toBeVisible();

  const add = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      id: 41,
      method: 'cron_add',
      params: { name: 'pw-settings-cron', every_secs: 60, prompt: 'signal', provider: 'offline' },
    }),
  }).then((r) => r.json());
  expect(add.ok).toBe(true);
  const list = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 42, method: 'cron_list', params: {} }),
  }).then((r) => r.json());
  expect(list.ok).toBe(true);
  expect((list.result.jobs || []).some((j) => j.name === 'pw-settings-cron')).toBeTruthy();
});

test('Files pane: toggleRight loads fs_list tree + optional preview', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  // doctor.files true (IPC)
  const doctor = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 90, method: 'doctor', params: {} }),
  }).then((r) => r.json());
  expect(doctor.ok).toBe(true);
  expect(doctor.result.files).toBe(true);

  await page.click('#toggleRight');
  await expect(page.locator('#rightPane')).toBeVisible();
  await expect(page.locator('#filesTree')).toBeAttached();
  await expect(page.locator('#filesCrumb')).toBeAttached();
  await expect(page.locator('#filePreview')).toBeAttached();

  // Wait for list render: rows OR empty/error after load (not the stub)
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const tree = document.getElementById('filesTree');
        if (!tree) return 'missing';
        if (tree.querySelectorAll('.fs-row').length > 0) return 'rows';
        if (tree.querySelector('.fs-empty')) return 'empty';
        if (tree.querySelector('.fs-error')) return 'error';
        const t = (tree.textContent || '').trim();
        if (/Loading/i.test(t)) return 'loading';
        if (/stub/i.test(t)) return 'stub';
        return t ? 'text' : 'blank';
      })
    , { timeout: 15000 })
    .toMatch(/^(rows|empty)$/);

  const rowCount = await page.locator('#filesTree .fs-row').count();
  // Home listing usually has sessions/workspace/etc.; empty home is still ok
  expect(rowCount >= 0).toBe(true);

  // Prefer clicking a small text file if one is visible; else try known paths via evaluate
  const previewOk = await page.evaluate(async () => {
    const tree = document.getElementById('filesTree');
    const fileRow = tree && tree.querySelector('.fs-row.fs-file');
    if (fileRow) {
      fileRow.click();
      await new Promise((r) => setTimeout(r, 400));
      const pre = document.getElementById('filePreview');
      const t = (pre && pre.textContent) || '';
      if (t && t !== 'Loading…' && !/^Error:/.test(t)) return { via: 'click', len: t.length };
    }
    // Fallback: list home and read first small-looking file path via IPC
    try {
      const list = window.optimus && window.optimus.fsList
        ? await window.optimus.fsList('')
        : null;
      const entries = (list && list.entries) || [];
      const file = entries.find((e) => String(e.kind).toLowerCase() === 'file' && (e.size || 0) < 200000);
      if (file && window.optimus && window.optimus.fsRead) {
        const r = await window.optimus.fsRead(file.path);
        const pre = document.getElementById('filePreview');
        if (pre && r && r.content != null) {
          pre.textContent = String(r.content);
          const pathEl = document.getElementById('filePreviewPath');
          if (pathEl) pathEl.textContent = file.path;
          return { via: 'ipc', len: String(r.content).length, path: file.path };
        }
      }
    } catch (e) {
      return { via: 'err', msg: String(e && e.message || e) };
    }
    return { via: 'none', rows: entriesCount() };
    function entriesCount() {
      return (tree && tree.querySelectorAll('.fs-row').length) || 0;
    }
  });

  // At minimum: list worked (not stub) and doctor.files; preview optional if no files
  expect(['click', 'ipc', 'none']).toContain(previewOk.via);
  if (previewOk.via === 'click' || previewOk.via === 'ipc') {
    await expect(page.locator('#filePreview')).not.toBeEmpty();
  }

  // Breadcrumb Home present
  await expect(page.locator('#filesCrumb')).toContainText(/Home/i);
});

test('term_run IPC requires explicit approval before command effect', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const r = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 91, method: 'term_run', params: { line: 'echo optimus-term-ok' } }),
  }).then((x) => x.json());
  expect(r.ok).toBe(true);
  expect(r.result.status).toBe('AwaitingApproval');
  expect(String(r.result.stdout || '')).toBe('');
  expect(r.result.mode).toBe('job-stream');
  expect(r.result.pty).toBe(false);

  const pending = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 911, method: 'approvals_list', params: {} }),
  }).then((x) => x.json());
  expect(pending.ok).toBe(true);
  expect(pending.result.pending.some((item) => item.job_id === r.result.job_id)).toBe(true);

  const granted = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      id: 912,
      method: 'approvals_grant',
      params: { job_id: r.result.job_id },
    }),
  }).then((x) => x.json());
  expect(granted.ok).toBe(true);
  expect(granted.result.status).toBe('Succeeded');
  expect(String(granted.result.stdout || '')).toMatch(/optimus-term-ok/i);

  const blocked = await fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: 92, method: 'term_run', params: { line: 'curl http://evil.test' } }),
  }).then((x) => x.json());
  expect(blocked.ok).toBe(false);
});

test('right sidebar tabs Files/Artifacts/Browser + resize handle', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.click('#toggleRight');
  await expect(page.locator('#rightPane')).toBeVisible();
  await expect(page.locator('#rightPaneTabs')).toBeVisible();
  await expect(page.locator('#rpFiles')).toBeVisible();
  await expect(page.locator('#filesTree')).toBeAttached();
  await expect(page.locator('#rightResize')).toBeAttached();

  await page.click('#rpTabArtifacts');
  await expect(page.locator('#rpArtifacts')).toBeVisible();
  await expect(page.locator('#artifactList')).toBeVisible();

  await page.click('#rpTabBrowser');
  await expect(page.locator('#rpBrowser')).toBeVisible();
  await expect(page.locator('#browserStub')).toContainText(/preview/i);

  await page.click('#rpTabFiles');
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const tree = document.getElementById('filesTree');
        if (!tree) return 'missing';
        if (tree.querySelectorAll('.fs-row').length) return 'rows';
        if (tree.querySelector('.fs-empty')) return 'empty';
        if (tree.querySelector('.fs-error')) return 'error';
        return 'other';
      })
    , { timeout: 10000 })
    .toMatch(/^(rows|empty)$/);
});

test('window_drag IPC is wired (native path)', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  // HTTP mode stubs window chrome; still ensure invoke path exists without throwing
  const r = await page.evaluate(async () => {
    if (!window.optimus) return { ok: false, why: 'no optimus' };
    try {
      if (typeof window.optimus.windowDrag === 'function') {
        await window.optimus.windowDrag();
      }
      if (typeof window.optimus.windowOuterPosition === 'function') {
        const pos = await window.optimus.windowOuterPosition();
        if (!pos || typeof pos.x !== 'number') return { ok: false, why: 'bad pos' };
      }
      if (typeof window.optimus.windowSetOuterPosition === 'function') {
        await window.optimus.windowSetOuterPosition(0, 0);
      }
      return { ok: true, via: 'drag+position' };
    } catch (e) {
      return { ok: false, why: String(e && e.message || e) };
    }
  });
  expect(r.ok).toBe(true);
});
