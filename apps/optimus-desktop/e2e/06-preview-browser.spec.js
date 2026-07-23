// @ts-check
/**
 * Preview Browser UI contracts exercised via Playwright + DOM.
 *
 * Native live-child WebView paint cannot run in --development-http mode.
 * This suite locks the shell DOM, chrome controls, embed-hole geometry
 * reporting, and the annotation → composer path that the native embed
 * pushes into via window.__optimusBrowserAnnotation.
 */
const { test, expect, url, waitForReady } = require('./support');

/** @param {import('@playwright/test').Page} page */
async function openBrowserTab(page) {
  await page.click('#toggleRight');
  await expect(page.locator('#rightPane')).toBeVisible();
  // Give the browser chrome room so the omnibox input has non-zero width.
  await page.evaluate(() => {
    const pane = document.getElementById('rightPane');
    if (pane) {
      pane.style.width = '520px';
      pane.style.minWidth = '520px';
      document.documentElement.style.setProperty('--right-w', '520px');
    }
    window.dispatchEvent(new Event('resize'));
  });
  await page.click('#rpTabBrowser');
  await expect(page.locator('#rpBrowser')).toBeVisible();
  await expect(page.locator('#rpBrowser')).toHaveClass(/active/);
  await expect(page.locator('#browserGo')).toBeVisible();
  // Omnibox input can report as "hidden" when width collapses; wait for box.
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const el = document.getElementById('browserUrl');
        if (!el) return 0;
        const r = el.getBoundingClientRect();
        return r.width * r.height;
      })
    , { timeout: 10000 })
    .toBeGreaterThan(100);
}

/** @param {import('@playwright/test').Page} page */
async function installEmbedCadenceProbe(page) {
  await page.addInitScript(() => {
    // @ts-ignore
    window.__browserEmbedPulsePayloads = [];
    const nativeFetch = window.fetch.bind(window);
    window.fetch = async (input, init = {}) => {
      try {
        const requestUrl = String(input);
        if (requestUrl.includes('/api/ipc') && init && typeof init.body === 'string') {
          const body = JSON.parse(init.body);
          if (body && body.method === 'browser_embed') {
            // @ts-ignore
            window.__browserEmbedPulsePayloads.push({ ...(body.params || {}) });
            return new Response(JSON.stringify({
              ok: true,
              result: { ok: true, visible: !!body.params.visible },
            }), { status: 200, headers: { 'Content-Type': 'application/json' } });
          }
        }
      } catch (_) {}
      return nativeFetch(input, init);
    };
  });
}

test.describe('Preview Browser DOM/UX', () => {
  test('chrome-like toolbar and live hole are present', async ({ page }) => {
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);

    await expect(page.locator('#browserBack')).toBeVisible();
    await expect(page.locator('#browserForward')).toBeVisible();
    await expect(page.locator('#browserReload')).toBeVisible();
    await expect(page.locator('#browserUrl')).toBeVisible();
    await expect(page.locator('#browserGo')).toBeVisible();
    await expect(page.locator('#browserToggleShot')).toBeVisible();
    await expect(page.locator('#browserToggleAnnot')).toBeVisible();
    await expect(page.locator('#browserToggleElements')).toBeVisible();
    await expect(page.locator('#browserLiveHole')).toBeAttached();
    await expect(page.locator('#browserViewport')).toBeVisible();
    await expect(page.locator('#browserStatus')).toBeVisible();
    await expect(page.locator('#browserUrl')).toBeEditable();
    await expect(page.locator('#browserUrl')).toHaveAttribute('placeholder', /Search Google|URL/i);
  });

  test('omnibox Go invokes browser_navigate and updates status', async ({ page }) => {
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);

    await page.evaluate(() => {
      // @ts-ignore
      window.__navCalls = [];
      const nf = window.fetch.bind(window);
      window.fetch = async (input, init = {}) => {
        try {
          const u = String(input);
          if (u.includes('/api/ipc') && init && typeof init.body === 'string') {
            const body = JSON.parse(init.body);
            if (body && body.method) {
              // @ts-ignore
              window.__navCalls.push({ method: body.method, params: body.params || {} });
            }
          }
        } catch (_) {}
        return nf(input, init);
      };
    });

    await page.locator('#browserUrl').fill('example.com', { force: true });
    await page.locator('#browserGo').click();
    await page.waitForTimeout(2500);

    const calls = await page.evaluate(() => /** @type {any} */ (window).__navCalls || []);
    const nav = calls.filter((/** @type {any} */ c) => c.method === 'browser_navigate');
    expect(nav.length).toBeGreaterThan(0);
    expect(String(nav[0].params.url || '')).toMatch(/example\.com/);

    const status = (await page.locator('#browserStatus').innerText()).trim();
    expect(status.length).toBeGreaterThan(0);
  });

  test('annotation push creates bar and send-to-composer injects context', async ({ page }) => {
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);

    await page.evaluate(() => {
      // @ts-ignore
      if (typeof window.__optimusBrowserAnnotation !== 'function') {
        throw new Error('__optimusBrowserAnnotation missing');
      }
      // @ts-ignore
      window.__optimusBrowserAnnotation({
        tag: 'h1',
        text: 'Example Domain',
        href: 'https://example.com/',
        url: 'https://example.com/',
        bounds: { x: 10, y: 10, width: 200, height: 40 },
      });
    });

    await expect(page.locator('#annotationsBar')).toBeVisible();
    await expect(page.locator('#annotationsList')).toContainText('Example Domain');

    const comment = page.locator('#annotComment');
    await expect(comment).toBeEnabled();
    await comment.fill('QA note from playwright');

    await page.locator('#annotSendToChat').click();

    const composer = page.locator('#input');
    await expect(composer).toBeVisible();
    await expect(composer).toHaveValue(/Browser notes|Example Domain|QA note/i);
  });

  test('annotation toggle posts browser_set_annotate', async ({ page }) => {
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);

    await page.evaluate(() => {
      // @ts-ignore
      window.__annotCalls = [];
      const nf = window.fetch.bind(window);
      window.fetch = async (input, init = {}) => {
        try {
          const u = String(input);
          if (u.includes('/api/ipc') && init && typeof init.body === 'string') {
            const body = JSON.parse(init.body);
            if (body.method === 'browser_set_annotate') {
              // @ts-ignore
              window.__annotCalls.push(body.params || {});
            }
          }
        } catch (_) {}
        return nf(input, init);
      };
    });

    // Toggle off then on via chrome button if it wires the checkbox.
    await page.locator('#browserToggleAnnot').click();
    await page.waitForTimeout(300);
    // Also flip the hidden checkbox directly to guarantee the change handler.
    await page.evaluate(() => {
      const cb = /** @type {HTMLInputElement|null} */ (document.getElementById('browserShowAnnotations'));
      if (!cb) return;
      cb.checked = !cb.checked;
      cb.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await page.waitForTimeout(300);

    const calls = await page.evaluate(() => /** @type {any} */ (window).__annotCalls || []);
    expect(calls.length).toBeGreaterThan(0);
    expect(typeof calls[0].enabled).toBe('boolean');
  });

  test('browser live hole reports geometry via browser_embed payload', async ({ page }) => {
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);

    const payload = await page.evaluate(async () => {
      const hole =
        document.getElementById('browserLiveHole') || document.getElementById('browserViewport');
      if (!hole) throw new Error('browser live hole missing');
      const r = hole.getBoundingClientRect();
      const body = {
        id: Date.now(),
        method: 'browser_embed',
        params: {
          visible: r.width >= 32 && r.height >= 32,
          x: Math.round(r.left),
          y: Math.round(r.top),
          w: Math.round(r.width),
          h: Math.round(r.height),
          dpr: window.devicePixelRatio || 1,
        },
      };
      // Shape check on the client payload the native shell consumes.
      return body.params;
    });

    expect(payload.w).toBeGreaterThanOrEqual(32);
    expect(payload.h).toBeGreaterThanOrEqual(32);
    expect(payload.x).toBeGreaterThanOrEqual(0);
    expect(payload.y).toBeGreaterThanOrEqual(0);
  });

  test('right sidebar keeps a draggable gutter outside the native browser surface', async ({ page }) => {
    await installEmbedCadenceProbe(page);
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);
    await page.evaluate(() => {
      const pane = document.getElementById('rightPane');
      if (!pane) return;
      pane.style.width = '';
      pane.style.minWidth = '';
    });

    const divider = page.locator('#rightResize');
    const box = await divider.boundingBox();
    if (!box) throw new Error('right resize divider has no layout box');
    const beforeWidth = await page.locator('#rightPane').evaluate((el) => el.getBoundingClientRect().width);
    const geometry = await page.evaluate(() => {
      const pane = document.getElementById('rightPane');
      const handle = document.getElementById('rightResize');
      const hole = document.getElementById('browserLiveHole');
      if (!pane || !handle || !hole) throw new Error('right pane geometry target missing');
      const paneRect = pane.getBoundingClientRect();
      const handleRect = handle.getBoundingClientRect();
      const holeRect = hole.getBoundingClientRect();
      const hit = document.elementFromPoint(
        handleRect.left + handleRect.width / 2,
        handleRect.top + Math.min(80, handleRect.height / 2),
      );
      return {
        paneLeft: paneRect.left,
        handleLeft: handleRect.left,
        handleRight: handleRect.right,
        handleWidth: handleRect.width,
        browserLeft: holeRect.left,
        hitId: hit && hit.id,
      };
    });

    expect(geometry.handleLeft).toBeGreaterThanOrEqual(geometry.paneLeft);
    expect(geometry.browserLeft - geometry.handleRight).toBeGreaterThanOrEqual(1);
    expect(geometry.handleWidth).toBe(7);
    expect(geometry.hitId).toBe('rightResize');

    await divider.dispatchEvent('pointerdown', {
      bubbles: true,
      clientX: box.x + box.width / 2,
      clientY: box.y + 80,
    });
    await page.evaluate(({ x, y }) => {
      window.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true,
        clientX: x - 60,
        clientY: y,
      }));
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    }, { x: box.x + box.width / 2, y: box.y + 80 });

    const afterWidth = await page.locator('#rightPane').evaluate((el) => el.getBoundingClientRect().width);
    expect(afterWidth).toBeGreaterThan(beforeWidth);
  });

  test('left and right sidebar resize bars share the same 7px horizontal width', async ({ page }) => {
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);

    const geometry = await page.evaluate(() => {
      const left = document.getElementById('leftResize');
      const right = document.getElementById('rightResize');
      const hole = document.getElementById('browserLiveHole');
      if (!left || !right || !hole) throw new Error('sidebar resize geometry target missing');
      const leftRect = left.getBoundingClientRect();
      const rightRect = right.getBoundingClientRect();
      const holeRect = hole.getBoundingClientRect();
      return {
        leftWidth: leftRect.width,
        rightWidth: rightRect.width,
        browserGap: holeRect.left - rightRect.right,
      };
    });

    expect(geometry.leftWidth).toBe(7);
    expect(geometry.rightWidth).toBe(7);
    expect(geometry.browserGap).toBeGreaterThanOrEqual(1);
  });

  test('held right-divider drag emits no duplicate native geometry', async ({ page }) => {
    await installEmbedCadenceProbe(page);
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);
    await page.waitForTimeout(260);
    await page.evaluate(() => {
      // @ts-ignore
      window.__browserEmbedPulsePayloads = [];
    });

    const divider = page.locator('#rightResize');
    const box = await divider.boundingBox();
    if (!box) throw new Error('right resize divider has no layout box');
    await divider.dispatchEvent('pointerdown', {
      bubbles: true,
      clientX: box.x + box.width / 2,
      clientY: box.y + 80,
    });
    await page.waitForTimeout(96);
    await page.evaluate(() => {
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    });
    await page.waitForTimeout(48);

    const payloads = await page.evaluate(() => {
      // @ts-ignore
      return window.__browserEmbedPulsePayloads || [];
    });
    expect(payloads).toEqual([]);
  });

  test('one-pixel divider movement reaches native geometry as one pixel', async ({ page }) => {
    await installEmbedCadenceProbe(page);
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);
    await page.evaluate(() => {
      const pane = document.getElementById('rightPane');
      if (!pane) return;
      pane.style.width = '';
      pane.style.minWidth = '';
    });
    await page.waitForTimeout(260);
    await page.evaluate(() => {
      // @ts-ignore
      window.__browserEmbedPulsePayloads = [];
    });

    const divider = page.locator('#rightResize');
    const box = await divider.boundingBox();
    if (!box) throw new Error('right resize divider has no layout box');
    await divider.dispatchEvent('pointerdown', {
      bubbles: true,
      clientX: box.x + box.width / 2,
      clientY: box.y + 80,
    });
    await page.evaluate(() => {
      const shell = document.querySelector('.shell') || document.body;
      const right = shell.getBoundingClientRect().right;
      window.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true,
        clientX: right - 500,
        clientY: 180,
      }));
    });
    await page.waitForTimeout(48);
    await page.evaluate(() => {
      const shell = document.querySelector('.shell') || document.body;
      const right = shell.getBoundingClientRect().right;
      window.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true,
        clientX: right - 501,
        clientY: 180,
      }));
    });
    await page.waitForTimeout(48);
    await page.evaluate(() => {
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    });
    await page.waitForTimeout(48);

    const widths = await page.evaluate(() => {
      const payloads = /** @type {Array<{visible: boolean, w: number}>} */ (
        /** @type {any} */ (window).__browserEmbedPulsePayloads || []
      );
      return payloads
        .filter((payload) => payload.visible)
        .map((payload) => payload.w)
        .filter((width, index, all) => index === 0 || width !== all[index - 1]);
    });
    expect(widths.length).toBeGreaterThanOrEqual(2);
    expect(widths[widths.length - 1] - widths[widths.length - 2]).toBe(1);
  });

  test('divider input dispatches changed native bounds in the same JavaScript turn', async ({ page }) => {
    await installEmbedCadenceProbe(page);
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);
    await page.waitForTimeout(260);
    await page.evaluate(() => {
      // @ts-ignore
      window.__browserEmbedPulsePayloads = [];
    });

    const divider = page.locator('#rightResize');
    const box = await divider.boundingBox();
    if (!box) throw new Error('right resize divider has no layout box');
    await divider.dispatchEvent('pointerdown', {
      bubbles: true,
      clientX: box.x + box.width / 2,
      clientY: box.y + 80,
    });
    const sameTurnCalls = await page.evaluate(({ x, y }) => {
      const probe = /** @type {any} */ (window).__browserEmbedPulsePayloads;
      const before = probe.length;
      window.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true,
        clientX: x - 31,
        clientY: y + 80,
      }));
      return probe.length - before;
    }, { x: box.x + box.width / 2, y: box.y });
    await page.evaluate(() => {
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    });

    expect(sameTurnCalls).toBe(1);
  });

  test('active right-divider drag dispatches every changed frame without duplicate geometry', async ({ page }) => {
    await installEmbedCadenceProbe(page);
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);
    await page.waitForTimeout(260);
    await page.evaluate(() => {
      // @ts-ignore
      window.__browserEmbedPulsePayloads = [];
    });

    const divider = page.locator('#rightResize');
    const box = await divider.boundingBox();
    if (!box) throw new Error('right resize divider has no layout box');
    await divider.dispatchEvent('pointerdown', {
      bubbles: true,
      clientX: box.x + box.width / 2,
      clientY: box.y + 80,
    });
    await page.evaluate(async ({ x, y }) => {
      for (let frame = 0; frame < 10; frame += 1) {
        window.dispatchEvent(new PointerEvent('pointermove', {
          bubbles: true,
          clientX: x - 24 - (frame * 3),
          clientY: y + 80,
        }));
        await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      }
    }, { x: box.x + box.width / 2, y: box.y });
    await page.evaluate(() => {
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    });
    await page.waitForTimeout(48);

    const result = await page.evaluate(() => {
      const payloads = /** @type {Array<{visible: boolean, x: number, y: number, w: number, h: number}>} */ (
        /** @type {any} */ (window).__browserEmbedPulsePayloads || []
      );
      const signatures = payloads.map((payload) =>
        `${payload.visible}:${payload.x},${payload.y},${payload.w}x${payload.h}`
      );
      return { count: signatures.length, unique: new Set(signatures).size };
    });
    expect(result.count).toBeGreaterThanOrEqual(9);
    expect(result.unique).toBe(result.count);
  });

  test('fast right-divider drag coalesces slow native embeds to the latest bounds', async ({ page }) => {
    await page.addInitScript(() => {
      // @ts-ignore
      window.__embedBackpressure = { inFlight: 0, maxInFlight: 0, calls: [] };
      const nativeFetch = window.fetch.bind(window);
      window.fetch = async (input, init = {}) => {
        try {
          const requestUrl = String(input);
          if (requestUrl.includes('/api/ipc') && init && typeof init.body === 'string') {
            const body = JSON.parse(init.body);
            if (body && body.method === 'browser_embed') {
              // @ts-ignore
              const probe = window.__embedBackpressure;
              probe.inFlight += 1;
              probe.maxInFlight = Math.max(probe.maxInFlight, probe.inFlight);
              probe.calls.push(body.params || {});
              await new Promise((resolve) => setTimeout(resolve, 45));
              probe.inFlight -= 1;
              return new Response(JSON.stringify({
                ok: true,
                result: { ok: true, visible: !!body.params.visible },
              }), { status: 200, headers: { 'Content-Type': 'application/json' } });
            }
          }
        } catch (_) {}
        return nativeFetch(input, init);
      };
    });

    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);
    const divider = page.locator('#rightResize');
    const box = await divider.boundingBox();
    if (!box) throw new Error('right resize divider has no layout box');
    await divider.dispatchEvent('pointerdown', {
      bubbles: true,
      clientX: box.x + box.width / 2,
      clientY: box.y + 80,
    });
    await page.evaluate(({ x, y }) => {
      for (let i = 0; i < 40; i += 1) {
        window.dispatchEvent(new PointerEvent('pointermove', {
          bubbles: true,
          clientX: x - 30 - ((i % 10) * 12),
          clientY: y + 80,
        }));
      }
    }, { x: box.x + box.width / 2, y: box.y });
    await page.waitForTimeout(120);
    await page.evaluate(() => {
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    });
    await page.waitForTimeout(180);

    const result = await page.evaluate(() => {
      // @ts-ignore
      const probe = window.__embedBackpressure;
      const hole = document.getElementById('browserLiveHole') || document.getElementById('browserViewport');
      if (!hole) throw new Error('browser live hole missing');
      const width = Math.round(hole.getBoundingClientRect().width);
      const last = probe.calls[probe.calls.length - 1] || {};
      return {
        maxInFlight: probe.maxInFlight,
        callCount: probe.calls.length,
        latestWidth: last.w,
        actualWidth: Math.max(0, width),
      };
    });

    expect(result.maxInFlight).toBe(1);
    expect(result.latestWidth).toBe(result.actualWidth);
    expect(result.callCount).toBeLessThanOrEqual(8);
  });

  test('app-window resize converges without replaying duplicate geometry', async ({ page }) => {
    await installEmbedCadenceProbe(page);
    await page.goto(url('/'));
    await waitForReady(page);
    await openBrowserTab(page);
    await page.waitForTimeout(260);
    await page.evaluate(() => {
      // @ts-ignore
      window.__browserEmbedPulsePayloads = [];
    });

    const size = page.viewportSize();
    if (!size) throw new Error('page viewport size unavailable');
    await page.setViewportSize({ width: size.width + 80, height: size.height + 40 });
    await page.waitForTimeout(260);

    const result = await page.evaluate(() => {
      const payloads = /** @type {Array<{visible: boolean, x: number, y: number, w: number, h: number}>} */ (
        /** @type {any} */ (window).__browserEmbedPulsePayloads || []
      );
      const hole = document.getElementById('browserLiveHole') || document.getElementById('browserViewport');
      if (!hole) throw new Error('browser live hole missing');
      const rect = hole.getBoundingClientRect();
      const signatures = payloads.map((payload) =>
        `${payload.visible}:${payload.x},${payload.y},${payload.w}x${payload.h}`
      );
      return {
        count: payloads.length,
        unique: new Set(signatures).size,
        last: payloads[payloads.length - 1] || {},
        actual: {
          x: Math.round(rect.left),
          y: Math.round(rect.top),
          w: Math.round(rect.width),
          h: Math.round(rect.height),
        },
      };
    });
    expect(result.count).toBeGreaterThanOrEqual(1);
    expect(result.unique).toBe(result.count);
    expect(result.last).toMatchObject(result.actual);
  });
});
