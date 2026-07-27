const { expect } = require('@playwright/test');
const fs = require('fs');
const net = require('net');
const path = require('path');

// One offline acceptance flow, two launch modes.
//
// `compiled-workbench.spec.cjs` runs it against the repository shell on every
// `just ui`, so a renamed control or moved popover fails a gate immediately.
// `installed-shell.spec.cjs` runs the same assertions against an installed
// candidate to produce the PF-00 paint/a11y baseline. Keeping one flow is the
// point: the installed spec previously drifted nine assertions behind the UI
// because nothing executed it.

function reservePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      server.close((error) => (error ? reject(error) : resolve(port)));
    });
  });
}

function launchEnvironment() {
  const { ELECTRON_RUN_AS_NODE: _electronRunAsNode, ...rest } = process.env;
  return rest;
}

async function workbenchWindow(application) {
  await expect
    .poll(() => application.windows().map((candidate) => candidate.url()))
    .toContain('optimus-app://ui/index.html');
  const page = application
    .windows()
    .find((candidate) => candidate.url().startsWith('optimus-app://ui/'));
  expect(page).toBeTruthy();
  await application.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()[0].setContentSize(1600, 1000);
  });
  return page;
}

async function offlineWorkbenchFlow(page, options) {
  const evidenceDir = options.evidenceDir || '';
  const prompt = options.prompt || 'installed Electron offline proof';
  const response = `offline echo: ${prompt}`;

  await expect(page.getByRole('complementary', { name: 'Projects and sessions' })).toBeVisible();
  await expect(page).toHaveURL('optimus-app://ui/index.html');
  expect(page.url()).not.toContain('token=');

  // Chat-first workbench: the evidence workspace starts closed and is reachable
  // from the topbar toggle. Proving the toggle keeps the surface covered without
  // asserting a pane the redesign deliberately hides on first paint.
  const workspaceToggle = page.getByRole('button', { name: 'Workspace', exact: true });
  await expect(workspaceToggle).toHaveAttribute('aria-pressed', 'false');
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toHaveCount(0);
  await workspaceToggle.click();
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeVisible();
  await workspaceToggle.click();
  await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toHaveCount(0);

  await page.getByRole('button', { name: 'New thread' }).click();
  await expect(page.locator('.session-row.is-active')).toHaveCount(1);

  const runSettings = page.getByRole('button', { name: 'Model and run settings' });
  await runSettings.click();
  const popover = page.getByRole('dialog', { name: 'Model and run settings' });
  await popover.getByLabel('Provider').selectOption('offline');
  await expect(popover.getByLabel('Model')).toHaveValue('offline-echo');
  await popover.getByLabel('Thinking level').selectOption('medium');
  await page.keyboard.press('Escape');
  await expect(popover).toHaveCount(0);

  const accessTrigger = page.getByRole('button', { name: /^Access: / });
  const accessLabel = await accessTrigger.getAttribute('aria-label');
  await accessTrigger.click();
  await page
    .getByRole('listbox', { name: 'Access' })
    .getByRole('option', { name: 'Ask before effects' })
    .click();
  await expect(page.getByRole('button', { name: 'Access: Ask before effects' })).toBeVisible();

  const composer = page.getByLabel('Message Optimus');
  await composer.fill(prompt);
  await expect(composer).toHaveValue(prompt);
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect(page.locator('.message-body').getByText(response, { exact: true })).toBeVisible();

  const completedTitle = (
    await page.locator('.session-row.is-active .session-title').innerText()
  ).trim();
  expect(completedTitle).not.toEqual('');

  // Durable proof for the completed run, taken while it is still the active session.
  // The sessions domain answers { sessions: [...] } (crates/optimus-host/src/sessions.rs).
  const listed = await page.evaluate(() => window.optimusElectron.invoke('sessions', {}));
  const sessions = Array.isArray(listed) ? listed : listed.sessions;
  expect(Array.isArray(sessions)).toBe(true);
  const session = sessions.find((candidate) => Number(candidate.message_count) >= 2);
  expect(session).toBeTruthy();
  const detail = await page.evaluate(
    (id) => window.optimusElectron.invoke('get_session', { id }),
    session.id
  );
  expect(detail.messages).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ role: 'user', content: prompt }),
      expect.objectContaining({ role: 'assistant', content: response }),
    ])
  );

  // New thread is empty and carries no transient state from the completed run.
  await page.getByRole('button', { name: 'New thread' }).click();
  await expect(page.getByRole('log', { name: 'Conversation' }).locator('.message')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'What should Optimus do?' })).toBeVisible();

  // Reopening the completed session restores its durable transcript. Target the
  // inactive row so a same-titled fresh thread cannot satisfy the assertion.
  const completedRow = page
    .locator('.session-row:not(.is-active)')
    .filter({ has: page.locator('.session-title', { hasText: detail.title || completedTitle }) })
    .first();
  await expect(completedRow).toBeVisible();
  await completedRow.locator('.session-select').click();
  await expect(page.locator('.message-body').getByText(response, { exact: true })).toBeVisible();

  const record = {
    mode: options.mode,
    uiUrl: page.url(),
    provider: 'offline',
    model: 'offline-echo',
    thinking: 'medium',
    access: 'ask',
    accessLabelBeforeSelect: accessLabel,
    prompt,
    response,
    sessionId: session.id,
    sessionTitle: completedTitle,
    ...(options.record || {}),
  };

  if (evidenceDir) {
    fs.mkdirSync(evidenceDir, { recursive: true });
    const screenshot = path.join(evidenceDir, `${options.mode}-workbench.png`);
    await page.screenshot({ path: screenshot });
    record.screenshot = screenshot;
    fs.writeFileSync(
      path.join(evidenceDir, 'acceptance.json'),
      `${JSON.stringify(record, null, 2)}\n`,
      'utf8'
    );
  }

  await page.locator('.window-controls').getByRole('button', { name: 'Close' }).click();
  return record;
}

module.exports = {
  launchEnvironment,
  offlineWorkbenchFlow,
  reservePort,
  workbenchWindow,
};
