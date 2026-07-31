// @ts-check
const { test, expect, url, waitForReady } = require('./support');

test('layout locks: Vantage radii, compact tools, rail cannot blow sidebar', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const layout = await page.evaluate(() => {
    const app = document.querySelector('.app') || document.getElementById('appRow');
    const sidebar = document.querySelector('.sidebar') || document.getElementById('leftRail');
    const tool = document.createElement('details');
    tool.className = 'tool-card';
    tool.innerHTML = '<summary><span class="tool-dot ok"></span><span>web_search ×5</span></summary><pre>hits</pre>';
    document.body.appendChild(tool);
    const cs = getComputedStyle(tool);
    const radiusGlobal = getComputedStyle(document.querySelector('.composer')).borderRadius;
    const sendR = getComputedStyle(document.querySelector('#send')).borderRadius;
    const sideH = sidebar.getBoundingClientRect().height;
    const shell = document.querySelector('.shell') || app;
    const appH = shell.getBoundingClientRect().height;
    const foot = document.querySelector('.side-foot').getBoundingClientRect();
    const shellBottom = shell.getBoundingClientRect().bottom;
    const toolW = tool.getBoundingClientRect().width;
    const romeInlay = getComputedStyle(document.documentElement).getPropertyValue('--rome-inlay').trim();
    const rail = document.getElementById('railSplit');
    tool.remove();
    return {
      appOverflow: getComputedStyle(document.querySelector('.shell') || app).overflow,
      sideOverflow: getComputedStyle(sidebar).overflow,
      radiusComposer: radiusGlobal,
      radiusSend: sendR,
      sideFitsApp: sideH <= appH + 2,
      footVisible: foot.bottom <= shellBottom + 2 && foot.height > 0,
      toolCompact: toolW <= 720,
      toolGlass: (cs.backdropFilter || '').includes('blur') || (cs.webkitBackdropFilter || '').includes('blur'),
      romeInlay,
      hasRailSplit: !!rail,
      signalGone: !document.getElementById('signalPanel') || getComputedStyle(document.getElementById('signalPanel')).display === 'none',
    };
  });
  expect(layout.appOverflow).toMatch(/hidden/);
  expect(layout.sideOverflow).toMatch(/hidden/);
  expect(layout.radiusComposer).toBe('12px');
  expect(parseFloat(layout.radiusSend)).toBeGreaterThanOrEqual(8);
  expect(layout.sideFitsApp).toBe(true);
  expect(layout.footVisible).toBe(true);
  expect(layout.toolCompact).toBe(true);
  expect(layout.romeInlay).toBeTruthy();
  expect(layout.hasRailSplit).toBe(true);
  expect(layout.signalGone).toBe(true);
});

test('Vantage workspace design tokens are defined', async ({ page }) => {
  await page.goto('/');
  const tokens = await page.evaluate(() => {
    const cs = getComputedStyle(document.documentElement);
    return {
      inlay: cs.getPropertyValue('--rome-inlay').trim(),
      void: cs.getPropertyValue('--rome-void').trim(),
      stone: cs.getPropertyValue('--rome-stone').trim(),
      ink: cs.getPropertyValue('--rome-ink').trim(),
      radiusImportant: getComputedStyle(document.querySelector('.composer')).borderRadius,
    };
  });
  expect(tokens.inlay).toMatch(/#7c8cff/i);
  expect(tokens.void).toBeTruthy();
  expect(tokens.stone).toBeTruthy();
  expect(tokens.ink).toBeTruthy();
  expect(tokens.radiusImportant).toBe('12px');
});

test('turn and session timers expose live and terminal timing evidence', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.evaluate(() => {
    window.optimus.chatStream = (_text, _opts, onEvent) => {
      let resolve;
      const task = new Promise((done) => { resolve = done; });
      task.cancel = () => true;
      window.__finishTimedTurn = () => {
        onEvent({ type: 'timing', kind: 'first_response', elapsed_ms: 34 });
        onEvent({ type: 'timing', kind: 'tool_started', name: 'web_search', call_id: 'search-1', elapsed_ms: 34, suppressed: false });
        onEvent({ type: 'tool', name: 'web_search', detail: 'running' });
        onEvent({ type: 'timing', kind: 'tool_finished', name: 'web_search', call_id: 'search-1', duration_ms: 21, elapsed_ms: 55, suppressed: false });
        onEvent({ type: 'delta', text: 'Latest AI news.' });
        onEvent({ type: 'timing', kind: 'turn_finished', duration_ms: 89, elapsed_ms: 89, status: 'succeeded' });
        resolve({
          assistant_text: 'Latest AI news.', session_id: 'timed-session', title: 'timed',
          provider: 'offline', steps: 1, schema_tokens_final: 1, tool_trace: [],
          timings: { total_ms: 89, first_response_ms: 34, model_ms: 68, tool_ms: 21 },
        });
      };
      return task;
    };
  });

  await page.fill('#input', 'timed request');
  await page.click('#send');
  await expect(page.locator('#turnTimer')).toHaveAttribute('data-active', 'true');
  await expect(page.locator('#turnTimer')).toContainText('turn');
  await expect(page.locator('#sessionTimer')).toContainText('session');
  await page.evaluate(() => window.__finishTimedTurn());
  await expect(page.locator('#send')).toHaveAttribute('aria-label', 'Send');
  await expect(page.locator('#turnTimer')).toHaveAttribute('data-active', 'false');
  await expect(page.locator('.msg.assistant .status-strip').last()).toContainText('total 89 ms');
  await expect(page.locator('.msg.assistant .status-strip').last()).toContainText('first 34 ms');
  await expect(page.locator('.msg.assistant .status-strip').last()).toContainText('model 68 ms');
  await expect(page.locator('.msg.assistant .status-strip').last()).toContainText('tools 21 ms');
  await expect(page.locator('#taskBody .task-item')).toHaveCount(1);
  await expect(page.locator('#taskBody .task-item')).toContainText('web_search');
  await expect(page.locator('#taskBody .task-item')).toContainText('21 ms');
  await expect(page.locator('#taskBody .task-item')).not.toContainText('×2');
});

test('approval-required turn is honest and session navigation clears transient state', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.evaluate(() => {
    window.optimus.chatStream = (_text, _opts, onEvent) => {
      onEvent({ type: 'timing', kind: 'tool_started', name: 'terminal', call_id: 'terminal-approval' });
      onEvent({ type: 'tool', name: 'terminal', detail: 'running' });
      onEvent({ type: 'timing', kind: 'turn_finished', duration_ms: 12, elapsed_ms: 12, status: 'failed' });
      const task = Promise.reject(new Error('runtime: needs approval for job 11111111-1111-1111-1111-111111111111 node 0'));
      task.cancel = () => true;
      return task;
    };
  });

  await page.fill('#input', 'approval test');
  await page.click('#send');
  await expect(page.locator('.msg.assistant').last()).toContainText('Approval required');
  await expect(page.locator('.msg.assistant .status-strip').last()).toContainText('approval required');
  await expect(page.locator('#taskBody .task-item')).toContainText('approval required');
  await expect(page.locator('#taskBody .task-item')).not.toContainText('running');

  await page.click('#newThread');
  await expect(page.locator('#taskCount')).toHaveText('0');
  await expect(page.locator('#taskPanel')).toBeHidden();
  await expect(page.locator('#turnTimer')).toContainText('turn —');
});

test('shell multi-pane IA: leftRail main statusBar; right/term hidden by default', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await expect(page.locator('#leftRail')).toBeVisible();
  await expect(page.locator('#main')).toBeVisible();
  await expect(page.locator('#statusBar')).toBeVisible();
  await expect(page.locator('#chat')).toBeVisible();
  await expect(page.locator('#composerWrap')).toBeVisible();

  const defaults = await page.evaluate(() => {
    const right = document.getElementById('rightPane');
    const term = document.getElementById('termPane');
    const st = document.getElementById('statusBar');
    const isHidden = (el) => {
      if (!el) return true;
      if (el.hasAttribute('hidden')) return true;
      const cs = getComputedStyle(el);
      return cs.display === 'none' || cs.visibility === 'hidden' || el.classList.contains('pane-hidden');
    };
    return {
      rightHidden: isHidden(right),
      termHidden: isHidden(term),
      statusH: st ? st.getBoundingClientRect().height : 0,
      shellH: document.querySelector('.shell').getBoundingClientRect().height,
      vh: window.innerHeight,
      hasToggles: !!(document.getElementById('toggleRight') && document.getElementById('toggleTerm')),
      statusIds: ['stGateway', 'stAgents', 'stCron', 'stTokens', 'stModel', 'stHome', 'stVer'].every(
        (id) => !!document.getElementById(id)
      ),
    };
  });
  expect(defaults.rightHidden).toBe(true);
  expect(defaults.termHidden).toBe(true);
  expect(defaults.statusH).toBeGreaterThan(0);
  expect(defaults.statusH).toBeLessThanOrEqual(32);
  expect(defaults.shellH).toBeLessThanOrEqual(defaults.vh + 1);
  expect(defaults.hasToggles).toBe(true);
  expect(defaults.statusIds).toBe(true);

  await page.click('#toggleRight');
  await expect(page.locator('#rightPane')).toBeVisible();
  await expect(page.locator('#filesTree')).toBeAttached();

  await page.click('#toggleTerm');
  await expect(page.locator('#termPane')).toBeVisible();
  await expect(page.locator('#termOut')).toBeAttached();
  await expect(page.locator('#termIn')).toBeAttached();

  // toggle closed again
  await page.click('#toggleRight');
  await page.click('#toggleTerm');
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const isHidden = (el) =>
          !el ||
          el.hasAttribute('hidden') ||
          getComputedStyle(el).display === 'none' ||
          el.classList.contains('pane-hidden');
        return isHidden(document.getElementById('rightPane')) && isHidden(document.getElementById('termPane'));
      })
    )
    .toBe(true);
});

test('nav routes: Capabilities shows page-capabilities; New session stays on chat', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  await expect(page.locator('#page-chat')).toBeVisible();
  await page.click('#navCapabilities');
  await expect(page.locator('#page-capabilities')).toBeVisible();
  await expect(page.locator('#page-chat')).toBeHidden();

  await page.click('#navMessaging');
  await expect(page.locator('#page-messaging')).toBeVisible();

  await page.click('#navArtifacts');
  await expect(page.locator('#page-artifacts')).toBeVisible();

  await page.click('#newThread');
  await expect(page.locator('#page-chat')).toBeVisible();
  await expect(page.locator('#page-capabilities')).toBeHidden();
});

test('single branded window header — no duplicate topbar', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const chrome = await page.evaluate(() => {
    const titlebar = document.querySelector('.titlebar');
    const heading = document.getElementById('heading');
    const task = document.getElementById('taskChip');
    const copy = document.getElementById('copySession');
    const topbar = document.querySelector('.main > .topbar');
    const titlebarBrand = document.querySelector('.titlebar .tb-brand');
    const brand = document.querySelector('.sidebar .brand');
    return {
      headingInTitlebar: !!(heading && titlebar && titlebar.contains(heading)),
      tasksInTitlebar: !!(task && titlebar && titlebar.contains(task)),
      copyInTitlebar: !!(copy && titlebar && titlebar.contains(copy)),
      noSecondTopbar: !topbar || getComputedStyle(topbar).display === 'none',
      brandInTitlebar: !!titlebarBrand,
      brandHidden: !brand || brand.hasAttribute('hidden') || getComputedStyle(brand).display === 'none',
      onlyOneHeader: document.querySelectorAll('.titlebar').length === 1,
    };
  });
  expect(chrome.headingInTitlebar).toBe(true);
  expect(chrome.tasksInTitlebar).toBe(true);
  expect(chrome.copyInTitlebar).toBe(true);
  expect(chrome.noSecondTopbar).toBe(true);
  expect(chrome.brandInTitlebar).toBe(true);
  expect(chrome.brandHidden).toBe(true);
  expect(chrome.onlyOneHeader).toBe(true);
});

test('custom titlebar shell and send absolute position', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const geom = await page.evaluate(() => {
    const shell = document.querySelector('.shell');
    const send = document.querySelector('#send');
    const composer = document.querySelector('.composer');
    const titlebar = document.querySelector('.titlebar');
    const foot = document.querySelector('.side-foot');
    if (!shell || !send || !composer || !titlebar || !foot) {
      return { missing: true };
    }
    const s = send.getBoundingClientRect();
    const c = composer.getBoundingClientRect();
    const shellBottom = shell.getBoundingClientRect().bottom;
    return {
      missing: false,
      hasShell: true,
      hasTitlebar: true,
      bodyClass: document.body.className,
      sendInComposer:
        s.right <= c.right + 1 &&
        s.bottom <= c.bottom + 1 &&
        s.left >= c.left - 1 &&
        Math.abs(s.right - (c.right - 10)) < 10,
      sendBottomPinned: Math.abs(s.bottom - (c.bottom - 10)) < 8,
      premiumRadius: parseFloat(getComputedStyle(composer).borderRadius) >= 12,
      footVisible: foot.getBoundingClientRect().bottom <= shellBottom + 2,
      hasBrand: !!document.querySelector('.titlebar .tb-brand'),
      toggleFiles: !!document.getElementById('toggleRight'),
      toggleTerm: !!document.getElementById('toggleTerm'),
      hermesRail: !!document.getElementById('railSplit') && !!document.getElementById('modeProjects'),
    };
  });
  expect(geom.missing).toBe(false);
  expect(geom.hasShell).toBe(true);
  expect(geom.hasTitlebar).toBe(true);
  expect(geom.bodyClass).toMatch(/http-mode|native-chrome/);
  expect(geom.sendInComposer).toBe(true);
  expect(geom.sendBottomPinned).toBe(true);
  expect(geom.premiumRadius).toBe(true);
  expect(geom.footVisible).toBe(true);
  expect(geom.hasBrand).toBe(true);
  expect(geom.toggleFiles).toBe(true);
  expect(geom.toggleTerm).toBe(true);
  expect(geom.hermesRail).toBe(true);
});

test('send button stays on composer bar baseline', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const geom = await page.evaluate(() => {
    const composer = document.querySelector('.composer');
    const send = document.querySelector('#send');
    if (!composer || !send) return null;
    const c = composer.getBoundingClientRect();
    const s = send.getBoundingClientRect();
    return {
      pinnedBottomRight:
        Math.abs(s.right - (c.right - 10)) < 10 &&
        Math.abs(s.bottom - (c.bottom - 10)) < 10,
      height: s.height,
    };
  });
  expect(geom).toBeTruthy();
  if (!geom) throw new Error('send geometry missing');
  expect(geom.pinnedBottomRight).toBe(true);
  expect(Math.abs(geom.height - 32)).toBeLessThanOrEqual(2);
});

test('chat pane scrolls and composer controls exist', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const chat = page.locator('#chat');
  await expect(chat).toBeVisible();
  const overflow = await chat.evaluate((el) => getComputedStyle(el).overflowY);
  expect(['auto', 'scroll', 'overlay']).toContain(overflow);
  const minH = await chat.evaluate((el) => getComputedStyle(el).minHeight);
  // flex scroll fix: min-height must not force growth past parent
  expect(minH === '0px' || minH === 'auto').toBeTruthy();

  // Compact single-line composer menus
  await expect(page.locator('#provBtn')).toBeVisible();
  await expect(page.locator('#modelBtn')).toBeVisible();
  await expect(page.locator('#thinkBtn')).toBeVisible();
  await expect(page.locator('#accessBtn')).toBeVisible();
  // Canonical selects still present (sr-only)
  await expect(page.locator('#provider')).toBeAttached();
  await expect(page.locator('#model')).toBeAttached();
  await expect(page.locator('#thinkingLevel')).toBeAttached();
  const levels = await page.locator('#thinkingLevel option').allTextContents();
  for (const need of ['low', 'medium', 'high', 'xhigh', 'max', 'ultra']) {
    expect(levels.join(' ')).toContain(need);
  }
  const models = await page.locator('#model option').allTextContents();
  expect(models.join(' ')).toMatch(/GPT-5\.6 Sol/);
  expect(models.join(' ')).toMatch(/Terra/);
  expect(models.join(' ')).toMatch(/Luna/);
  expect(models.join(' ')).toContain('GPT-5.5');
  // Thinking/Fast live inside think menu (body portal above chip)
  await page.click('#thinkBtn');
  await expect(page.locator('#cddPortal.open')).toBeVisible();
  await expect(page.locator('#cddPortal button[data-kind="think-on"]')).toBeVisible();
  await expect(page.locator('#cddPortal button[data-kind="think-fast"]')).toBeVisible();
  await expect(page.locator('#cddPortal button[data-kind="think-level"]')).toHaveCount(8);
  // portal is above the think chip
  const above = await page.evaluate(() => {
    const p = document.getElementById('cddPortal').getBoundingClientRect();
    const b = document.getElementById('thinkBtn').getBoundingClientRect();
    return p.bottom <= b.top + 2 && p.height > 40;
  });
  expect(above).toBe(true);
  await expect(page.locator('#thinkingToggle')).toBeAttached();
  await expect(page.locator('#fastToggle')).toBeAttached();
  // open model menu too
  await page.click('#modelBtn');
  await expect(page.locator('#cddPortal.open')).toBeVisible();
  await expect(page.locator('#cddPortal button[data-kind="model"]').first()).toBeVisible();
  // Access keeps broader authority behind visible Advanced/Expert boundaries.
  await page.click('#accessBtn');
  const tiers = page.locator('#cddPortal .cdd-access-tier');
  await expect(tiers).toHaveCount(3);
  await expect(tiers.nth(0)).toHaveAttribute('aria-label', 'Recommended');
  await expect(tiers.nth(1)).toHaveAttribute('aria-label', 'Advanced');
  await expect(tiers.nth(2)).toHaveAttribute('aria-label', 'Expert');
  await expect(tiers.nth(1).locator('.cdd-sec')).toHaveText('Advanced');
  await expect(tiers.nth(1).locator('button')).toHaveAttribute('data-v', 'full_project');
  await expect(tiers.nth(2).locator('.cdd-sec')).toHaveText('Expert');
  const unrestricted = page.locator('#cddPortal button[data-v="unrestricted_host"]');
  await expect(tiers.nth(2).locator('button')).toHaveCount(1);
  await expect(tiers.nth(2).locator('button')).toHaveAttribute('data-v', 'unrestricted_host');
  await expect(page.locator('#cddPortal button[role="option"]')).toHaveCount(5);
  expect(
    await page.locator('#cddPortal button[role="option"]').evaluateAll((buttons) =>
      buttons.map((button) => button.getAttribute('data-v'))
    )
  ).toEqual(['standard', 'review_changes', 'read_only', 'full_project', 'unrestricted_host']);
  const accessibleOptions = [
    ['standard', 'Standard. Ordinary project work runs; anything else asks'],
    ['review_changes', 'Review changes. Reads run; writes and commands ask first'],
    ['read_only', 'Read only. Nothing is changed'],
    ['full_project', 'Full project. Wider autonomy inside the project; credentials and your system still ask'],
    ['unrestricted_host', 'Unrestricted host. Break-glass: no pauses, and the whole machine is in reach'],
  ];
  for (const [value, name] of accessibleOptions) {
    await expect(page.locator(`#cddPortal button[data-v="${value}"]`)).toHaveAccessibleName(name);
  }
  await expect(page.locator('#cddPortal button.access-warning')).toHaveCount(1);
  await expect(unrestricted).toHaveClass(/access-warning/);
  await expect(unrestricted).toHaveAttribute('aria-selected', 'false');
  await expect(unrestricted.locator('.access-risk')).toHaveText('!');
  await expect(unrestricted.locator('.access-hint')).toHaveText(
    'Break-glass: no pauses, and the whole machine is in reach'
  );
  // single line: composer-bar does not wrap; chips never overlap
  const bar = await page.locator('.composer-bar').evaluate((el) => {
    const cs = getComputedStyle(el);
    return { wrap: cs.flexWrap, h: el.getBoundingClientRect().height };
  });
  expect(bar.wrap).toBe('nowrap');
  expect(bar.h).toBeLessThanOrEqual(32);
  // shrink viewport / force narrow composer: no horizontal overlap between chips
  const noOverlap = await page.evaluate(() => {
    const btns = [...document.querySelectorAll('.composer-controls .cdd-btn')];
    const rects = btns.map((b) => b.getBoundingClientRect()).filter((r) => r.width > 0);
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i], b = rects[j];
        const overlap = !(a.right <= b.left + 0.5 || b.right <= a.left + 0.5 || a.bottom <= b.top + 0.5 || b.bottom <= a.top + 0.5);
        if (overlap) return false;
      }
    }
    return rects.length >= 3;
  });
  expect(noOverlap).toBe(true);
  await expect(page.locator('#taskChip')).toBeVisible();
  await expect(page.locator('#taskPanel')).toBeAttached();
});

test('stored access words migrate without reviving break-glass', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);

  const migrations = [
    ['smart_deny', 'review_changes', 'Review changes'],
    ['full', 'standard', 'Standard'],
    ['unrestricted_host', 'standard', 'Standard'],
    ['constructor', 'standard', 'Standard'],
    ['full_project', 'full_project', 'Full project'],
  ];
  for (const [stored, expected, label] of migrations) {
    await page.evaluate((access) => {
      localStorage.setItem('optimus.ui.composer', JSON.stringify({ access }));
    }, stored);
    await page.reload();
    await waitForReady(page);
    await expect(page.locator('#access')).toHaveValue(expected);
    await expect(page.locator('#accessVal')).toHaveText(label);
  }
});

test('theme toggle switches data-theme', async ({ page }) => {
  await page.goto('/');
  await page.waitForSelector('#themeToggle');
  const html = page.locator('html');
  const before = await html.getAttribute('data-theme');
  await page.click('#themeToggle');
  await expect
    .poll(async () => html.getAttribute('data-theme'))
    .not.toBe(before);
  await page.click('#themeToggle');
  await expect(html).toHaveAttribute('data-theme', before || 'dark');
});

test('active composer turn exposes Stop and preserves partial text on cancellation', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  await page.evaluate(() => {
    window.optimus.chatStream = function (_message, _opts, onEvent) {
      let rejectTask;
      let open = true;
      const task = new Promise((_resolve, reject) => { rejectTask = reject; });
      task.cancel = function () {
        if (!open) return false;
        open = false;
        const error = new Error('turn cancelled');
        error.name = 'AbortError';
        rejectTask(error);
        return true;
      };
      setTimeout(() => onEvent({ type: 'delta', text: 'partial answer' }), 0);
      return task;
    };
  });

  await page.locator('#input').fill('cancel this turn');
  await page.locator('#send').click();
  await expect(page.locator('#send')).toHaveAttribute('aria-label', 'Stop');
  await expect(page.locator('.msg.assistant').last()).toContainText('partial answer');
  await page.locator('#send').click();

  await expect(page.locator('#send')).toHaveAttribute('aria-label', 'Send');
  await expect(page.locator('.msg.assistant').last()).toContainText('partial answer');
  await expect(page.locator('.msg.assistant').last()).toContainText('cancelled');
  await expect(page.locator('.msg.assistant').last()).not.toContainText('Error:');
});

test('fresh home boots at Auto while route resolution remains offline without credentials', async ({ page }) => {
  // R30.8 keeps one product-facing Auto intent. A fresh home has no Codex
  // credentials, so the host still resolves a sent turn deterministically to
  // offline; the composer itself must not persist that one-turn resolution.
  await page.goto('/');
  await page.waitForFunction(() => window.__optimusBridgeInstalled === true);
  await waitForReady(page);
  await expect(page.locator('#provider')).toHaveValue('auto');
  await expect(page.locator('#model')).toHaveValue('');
  await expect(page.locator('#provVal')).toContainText('Auto');
});
