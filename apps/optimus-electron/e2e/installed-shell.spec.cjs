const { test, expect, _electron: electron } = require('@playwright/test');
const fs = require('fs');
const net = require('net');
const path = require('path');

const INSTALLED_ELECTRON = process.env.OPTIMUS_INSTALLED_ELECTRON || '';
const INSTALLED_HOST = process.env.OPTIMUS_INSTALLED_HOST || '';
const INSTALL_ROOT = process.env.OPTIMUS_INSTALLED_ROOT || '';
const EVIDENCE_DIR = process.env.OPTIMUS_INSTALLED_EVIDENCE_DIR || '';

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

test('installed Electron shell completes and restores an offline session', async () => {
  test.skip(
    !INSTALLED_ELECTRON || !INSTALLED_HOST || !INSTALL_ROOT || !EVIDENCE_DIR,
    'installed application paths are required'
  );
  for (const candidate of [INSTALLED_ELECTRON, INSTALLED_HOST, INSTALL_ROOT]) {
    expect(path.isAbsolute(candidate)).toBe(true);
  }

  fs.mkdirSync(EVIDENCE_DIR, { recursive: true });
  const home = path.join(EVIDENCE_DIR, 'optimus-home');
  const userData = path.join(EVIDENCE_DIR, 'electron-user-data');
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(userData, { recursive: true });
  const hostPort = await reservePort();
  const prompt = 'installed Electron offline proof';
  const response = `offline echo: ${prompt}`;

  const { ELECTRON_RUN_AS_NODE: _electronRunAsNode, ...launchEnvironment } = process.env;
  let application;
  try {
    application = await electron.launch({
      executablePath: INSTALLED_ELECTRON,
      cwd: INSTALL_ROOT,
      env: {
        ...launchEnvironment,
        OPTIMUS_APP_ROOT: INSTALL_ROOT,
        OPTIMUS_DESKTOP_BIN: INSTALLED_HOST,
        OPTIMUS_ELECTRON_UI: 'react',
        OPTIMUS_HOST_PORT: String(hostPort),
        OPTIMUS_HTTP_TOKEN: `optimus-installed-e2e-${process.pid}-0123456789abcdef`,
        OPTIMUS_HOME: home,
        OPTIMUS_ELECTRON_USER_DATA: userData,
      },
    });

    expect(fs.realpathSync(`/proc/${application.process().pid}/exe`)).toBe(
      fs.realpathSync(INSTALLED_ELECTRON)
    );
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

    await expect(page.getByRole('complementary', { name: 'Projects and sessions' })).toBeVisible();
    await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeVisible();
    await expect(page).toHaveURL('optimus-app://ui/index.html');
    expect(page.url()).not.toContain('token=');

    await page.getByRole('button', { name: 'New session', exact: true }).click();
    await expect(page.locator('.session-row.is-active')).toHaveCount(1);
    await page.getByLabel('Provider').selectOption('offline');
    await expect(page.getByLabel('Model')).toHaveValue('offline-echo');
    await page.getByLabel('Thinking level').selectOption('medium');
    await page.getByLabel('Access').selectOption('ask');
    const composer = page.getByLabel('Message Optimus');
    await composer.fill(prompt);
    await expect(composer).toHaveValue(prompt);
    await page.getByRole('button', { name: 'Send message' }).click();
    await expect(page.locator('.message-body').getByText(response, { exact: true })).toBeVisible();
    await expect(page.locator('.session-row.is-active')).toContainText('2 messages');

    await page.getByRole('button', { name: 'New session', exact: true }).click();
    await expect(page.locator('.session-row.is-active')).toContainText('0 messages');
    await expect(page.getByRole('log', { name: 'Conversation' }).locator('.message')).toHaveCount(0);
    await expect(page.getByText('Start with an outcome')).toBeVisible();

    const completedSession = page.locator('.session-row').filter({ hasText: '2 messages' }).first();
    await completedSession.locator('.session-select').click();
    await expect(page.locator('.message-body').getByText(response, { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Tasks' })).toContainText('0');

    const sessions = await page.evaluate(() => window.optimusElectron.invoke('sessions', {}));
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

    const screenshot = path.join(EVIDENCE_DIR, 'installed-electron-workbench.png');
    await page.screenshot({ path: screenshot });
    fs.writeFileSync(
      path.join(EVIDENCE_DIR, 'acceptance.json'),
      `${JSON.stringify(
        {
          installedElectron: fs.realpathSync(INSTALLED_ELECTRON),
          installedHost: fs.realpathSync(INSTALLED_HOST),
          uiUrl: page.url(),
          provider: 'offline',
          model: 'offline-echo',
          thinking: 'medium',
          access: 'ask',
          prompt,
          response,
          sessionId: session.id,
          optimusHome: home,
          screenshot,
        },
        null,
        2
      )}\n`,
      'utf8'
    );

    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await application.waitForEvent('close');
    application = null;
  } finally {
    if (application) await application.close().catch(() => undefined);
  }
});
