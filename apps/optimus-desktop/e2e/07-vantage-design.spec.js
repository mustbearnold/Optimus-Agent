// @ts-check
const { test, expect, waitForReady } = require('./support');

test('Vantage shell owns compact density and high-refresh motion tokens', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const snapshot = await page.evaluate(() => {
    const root = getComputedStyle(document.documentElement);
    const rect = (/** @type {string} */ selector) => document.querySelector(selector)?.getBoundingClientRect();
    const composer = document.querySelector('.composer');
    const appRow = document.getElementById('appRow');
    const right = document.getElementById('rightPane');
    const term = document.getElementById('termPane');
    return {
      tokens: {
        canvas: root.getPropertyValue('--v-canvas').trim(),
        railCollapsed: root.getPropertyValue('--rail-collapsed').trim(),
        motionPane: root.getPropertyValue('--motion-pane').trim(),
        motionPress: root.getPropertyValue('--motion-press').trim(),
        motionBlurPress: root.getPropertyValue('--motion-blur-press').trim(),
        easeSmooth: root.getPropertyValue('--ease-v-smooth').trim(),
        motionAttr: document.documentElement.getAttribute('data-v-motion'),
      },
      sendTransition: (() => {
        const send = document.getElementById('send');
        return send ? getComputedStyle(send).transitionProperty : '';
      })(),
      hasLeftToggle: !!document.getElementById('toggleLeft'),
      titlebarH: rect('#titlebar')?.height || 0,
      navRowH: rect('#navPrimary .nav-item')?.height || 0,
      sendH: rect('#send')?.height || 0,
      statusH: rect('#statusBar')?.height || 0,
      composerRadius: composer ? parseFloat(getComputedStyle(composer).borderRadius) : 0,
      closedRightMounted: !!right && !right.hasAttribute('hidden'),
      closedTermMounted: !!term && !term.hasAttribute('hidden'),
      rightVisibility: right ? getComputedStyle(right).visibility : '',
      termVisibility: term ? getComputedStyle(term).visibility : '',
      gridTransition: appRow ? getComputedStyle(appRow).transitionProperty : '',
    };
  });

  expect(snapshot.tokens.canvas.toLowerCase()).toBe('#080b10');
  expect(snapshot.tokens.railCollapsed).toBe('48px');
  expect(snapshot.tokens.motionPane).toMatch(/ms$/);
  expect(snapshot.tokens.motionPress).toMatch(/ms$/);
  expect(snapshot.tokens.motionBlurPress).toMatch(/px$/);
  expect(snapshot.tokens.easeSmooth).toContain('cubic-bezier');
  expect(['on', 'off']).toContain(snapshot.tokens.motionAttr);
  expect(snapshot.sendTransition).toMatch(/transform|filter|background/);
  expect(snapshot.hasLeftToggle).toBe(true);
  expect(snapshot.titlebarH).toBeGreaterThanOrEqual(38);
  expect(snapshot.titlebarH).toBeLessThanOrEqual(42);
  expect(snapshot.navRowH).toBeLessThanOrEqual(32);
  expect(snapshot.sendH).toBeGreaterThanOrEqual(30);
  expect(snapshot.sendH).toBeLessThanOrEqual(33);
  expect(snapshot.statusH).toBeLessThanOrEqual(23);
  expect(snapshot.composerRadius).toBeLessThanOrEqual(12);
  expect(snapshot.closedRightMounted).toBe(true);
  expect(snapshot.closedTermMounted).toBe(true);
  expect(snapshot.rightVisibility).toBe('hidden');
  expect(snapshot.termVisibility).toBe('hidden');
  expect(snapshot.gridTransition).toContain('grid-template-columns');
});

test('left rail, inspector, and execution dock share one reversible presentation state', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.click('#toggleLeft');
  await expect(page.locator('#appRow')).toHaveClass(/left-collapsed/);
  await expect(page.locator('#toggleLeft')).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(
    () => page.locator('#leftRail').evaluate((el) => el.getBoundingClientRect().width),
  ).toBeLessThanOrEqual(49);
  const collapsedWidth = await page.locator('#leftRail').evaluate((el) => el.getBoundingClientRect().width);
  expect(collapsedWidth).toBeGreaterThanOrEqual(47);

  await page.click('#toggleRight');
  await expect(page.locator('#rightPane')).toHaveClass(/open/);
  await expect(page.locator('#rightPane')).toHaveAttribute('aria-hidden', 'false');
  await expect(page.locator('#rightPane')).toBeVisible();

  await page.click('#toggleTerm');
  await expect(page.locator('#termPane')).toHaveClass(/open/);
  await expect(page.locator('#termPane')).toHaveAttribute('aria-hidden', 'false');
  await expect.poll(
    () => page.locator('#termPane').evaluate((el) => el.getBoundingClientRect().height),
  ).toBeGreaterThanOrEqual(120);
  const openTermHeight = await page.locator('#termPane').evaluate((el) => el.getBoundingClientRect().height);
  expect(openTermHeight).toBeGreaterThanOrEqual(120);

  await page.click('#toggleRight');
  await page.click('#toggleTerm');
  await expect(page.locator('#rightPane')).toHaveClass(/pane-hidden/);
  await expect(page.locator('#termPane')).toHaveClass(/pane-hidden/);
  await expect(page.locator('#rightPane')).toHaveAttribute('aria-hidden', 'true');
  await expect(page.locator('#termPane')).toHaveAttribute('aria-hidden', 'true');

  await page.click('#toggleLeft');
  await expect(page.locator('#appRow')).not.toHaveClass(/left-collapsed/);
  await expect(page.locator('#toggleLeft')).toHaveAttribute('aria-pressed', 'false');
});

test('streaming keeps one live text node and commits at most once per display frame', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await page.evaluate(() => {
    const w = /** @type {any} */ (window);
    w.optimus.chatStream = (/** @type {string} */ _text, /** @type {any} */ _opts, /** @type {(event:any)=>void} */ onEvent) => {
      /** @type {(value:any)=>void} */
      let resolveTask = () => {};
      const task = /** @type {any} */ (new Promise((resolve) => { resolveTask = resolve; }));
      task.cancel = () => true;
      w.__vantageStream = {
        emit(/** @type {string} */ text) { onEvent({ type: 'delta', text }); },
        finish() {
          onEvent({ type: 'timing', kind: 'turn_finished', duration_ms: 30, elapsed_ms: 30, status: 'succeeded' });
          resolveTask({
            assistant_text: 'alpha beta gamma',
            session_id: 'vantage-stream',
            title: 'Stable stream',
            provider: 'offline',
            steps: 1,
            schema_tokens_final: 1,
            tool_trace: [],
            timings: { total_ms: 30, first_response_ms: 5, model_ms: 30, tool_ms: 0 },
          });
        },
      };
      return task;
    };
  });

  await page.fill('#input', 'stream without replacing nodes');
  await page.click('#send');
  await page.evaluate(() => /** @type {any} */ (window).__vantageStream.emit('alpha'));
  await expect(page.locator('.msg.assistant [data-stream-body="1"]').last()).toHaveText('alpha');
  await page.evaluate(() => {
    /** @type {any} */ (window).__vantageFirstStreamBody = document.querySelector('.msg.assistant:last-child [data-stream-body="1"]');
  });

  await page.evaluate(() => {
    const stream = /** @type {any} */ (window).__vantageStream;
    stream.emit(' beta');
    stream.emit(' gamma');
  });
  const live = page.locator('.msg.assistant [data-stream-body="1"]').last();
  await expect(live).toHaveText('alpha beta gamma');
  const stable = await page.evaluate(() => {
    const body = document.querySelector('.msg.assistant:last-child [data-stream-body="1"]');
    const style = body ? getComputedStyle(body) : null;
    return {
      sameNode: body === /** @type {any} */ (window).__vantageFirstStreamBody,
      animation: style?.animationName || '',
      filter: style?.filter || '',
      transform: style?.transform || '',
    };
  });

  await page.evaluate(() => /** @type {any} */ (window).__vantageStream.finish());
  await expect(page.locator('#send')).toHaveAttribute('aria-label', 'Send');
  expect(stable.sameNode).toBe(true);
  expect(stable.animation).toBe('none');
  expect(stable.filter).toBe('none');
  expect(stable.transform).toBe('none');
});

test('focus breakpoint contains an active native Browser in its inspector overlay and hides it on close', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.evaluate(() => {
    const w = /** @type {any} */ (window);
    w.__vantageEmbedPayloads = [];
    const nativeFetch = window.fetch.bind(window);
    window.fetch = async (input, init = {}) => {
      try {
        const requestUrl = String(input);
        if (requestUrl.includes('/api/ipc') && typeof init.body === 'string') {
          const body = JSON.parse(init.body);
          if (body?.method === 'browser_embed') {
            w.__vantageEmbedPayloads.push({ ...(body.params || {}) });
            return new Response(JSON.stringify({
              ok: true,
              result: { ok: true, visible: !!body.params?.visible },
            }), { status: 200, headers: { 'Content-Type': 'application/json' } });
          }
        }
      } catch (_) {}
      return nativeFetch(input, init);
    };
  });

  await page.click('#toggleRight');
  await page.click('#rpTabBrowser');
  await expect.poll(() => page.evaluate(() => {
    const payloads = /** @type {any} */ (window).__vantageEmbedPayloads || [];
    return payloads.some((/** @type {any} */ payload) => payload.visible === true);
  })).toBe(true);

  await page.setViewportSize({ width: 520, height: 700 });
  const compactDiagnostic = await page.evaluate(() => {
    const right = document.getElementById('rightPane');
    const hole = document.getElementById('browserLiveHole');
    const rightRect = right?.getBoundingClientRect();
    const holeRect = hole?.getBoundingClientRect();
    return {
      innerWidth: window.innerWidth,
      media: window.matchMedia('(max-width:560px)').matches,
      rightDisplay: right ? getComputedStyle(right).display : null,
      right: rightRect ? { left: rightRect.left, right: rightRect.right, width: rightRect.width } : null,
      hole: holeRect ? { left: holeRect.left, right: holeRect.right, width: holeRect.width, height: holeRect.height } : null,
    };
  });
  expect(compactDiagnostic.innerWidth).toBe(520);
  expect(compactDiagnostic.media).toBe(true);
  expect(compactDiagnostic.rightDisplay).toBe('flex');
  expect(compactDiagnostic.right?.left).toBeGreaterThanOrEqual(0);
  expect(compactDiagnostic.right?.right).toBeLessThanOrEqual(520);
  expect(compactDiagnostic.hole?.width).toBeGreaterThanOrEqual(32);
  expect(compactDiagnostic.hole?.height).toBeGreaterThanOrEqual(32);
  expect(compactDiagnostic.hole?.left).toBeGreaterThanOrEqual(0);
  expect(compactDiagnostic.hole?.right).toBeLessThanOrEqual(520);
  await expect.poll(() => page.evaluate(() => {
    const payloads = /** @type {any} */ (window).__vantageEmbedPayloads || [];
    const last = payloads[payloads.length - 1];
    return !!(last && last.visible && last.x >= 0 && last.w >= 32 && last.x + last.w <= window.innerWidth + 1);
  })).toBe(true);

  await page.click('#toggleRight');
  await expect.poll(() => page.evaluate(() => {
    const payloads = /** @type {any} */ (window).__vantageEmbedPayloads || [];
    return payloads.length ? payloads[payloads.length - 1].visible : null;
  })).toBe(false);
  await expect(page.locator('#rightPane')).toBeHidden();
});

test('resize paths are direct and reduced motion preserves state without transforms', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.click('#toggleRight');

  const handle = page.locator('#rightResize');
  const box = await handle.boundingBox();
  if (!box) throw new Error('right resize handle has no box');
  await handle.dispatchEvent('pointerdown', { bubbles: true, clientX: box.x + 3, clientY: box.y + 80 });
  await expect(page.locator('body')).toHaveClass(/is-resizing/);
  const during = await page.evaluate(() => {
    const row = document.getElementById('appRow');
    const pane = document.getElementById('rightPane');
    if (!row || !pane) throw new Error('workspace geometry nodes missing');
    return {
      row: getComputedStyle(row).transitionDuration,
      pane: getComputedStyle(pane).transitionDuration,
    };
  });
  expect(during.row.split(',').every((value) => parseFloat(value) === 0)).toBe(true);
  expect(during.pane.split(',').every((value) => parseFloat(value) === 0)).toBe(true);
  await page.evaluate(() => window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true })));
  await expect(page.locator('body')).not.toHaveClass(/is-resizing/);

  await page.emulateMedia({ reducedMotion: 'reduce' });
  const reduced = await page.evaluate(() => {
    const paneEl = document.getElementById('rightPane');
    const termEl = document.getElementById('termPane');
    if (!paneEl || !termEl) throw new Error('workspace motion nodes missing');
    const pane = getComputedStyle(paneEl);
    const term = getComputedStyle(termEl);
    return {
      paneDurations: pane.transitionDuration.split(',').map(parseFloat),
      termDurations: term.transitionDuration.split(',').map(parseFloat),
      paneTransform: pane.transform,
      termTransform: term.transform,
    };
  });
  expect(Math.max(...reduced.paneDurations)).toBeLessThanOrEqual(0.01);
  expect(Math.max(...reduced.termDurations)).toBeLessThanOrEqual(0.01);
  expect(reduced.paneTransform).toBe('none');
  expect(reduced.termTransform).toBe('none');
});
