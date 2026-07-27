const { test, expect, _electron: electron } = require('@playwright/test');
const fs = require('fs');
const http = require('http');
const net = require('net');
const path = require('path');
const { electronLaunchArgs } = require('./support/workbench-flow.cjs');

const ROOT = path.resolve(__dirname, '../../..');
const ELECTRON_DIR = path.join(ROOT, 'apps', 'optimus-electron');
const EVIDENCE_DIR = path.join(ROOT, 'local', 'tmp');

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

function fixtureServer() {
  const server = http.createServer((_request, response) => {
    response.writeHead(200, {
      'Content-Type': 'text/html; charset=utf-8',
      'Cache-Control': 'no-store',
    });
    response.end(`<!doctype html>
      <html>
        <body style="margin:0;display:grid;place-items:center;height:100vh;background:#10231f;color:#f2fff9;font:16px system-ui">
          <button id="target" style="width:240px;height:90px">Native preview target <span id="count">0</span></button>
          <script>
            window.nativeClicks = 0;
            document.querySelector('#target').addEventListener('click', () => {
              window.nativeClicks += 1;
              document.querySelector('#count').textContent = String(window.nativeClicks);
            });
          </script>
        </body>
      </html>`);
  });
  return server;
}

test('compiled Electron shell secures Rust transport and aligns native preview', async () => {
  fs.mkdirSync(EVIDENCE_DIR, { recursive: true });
  const hostPort = await reservePort();
  const previewPort = await reservePort();
  const home = path.join(EVIDENCE_DIR, `electron-home-${process.pid}-${Date.now()}`);
  fs.mkdirSync(home, { recursive: true });
  const server = fixtureServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(previewPort, '127.0.0.1', resolve);
  });

  let application;
  try {
    const { ELECTRON_RUN_AS_NODE: _electronRunAsNode, ...launchEnvironment } = process.env;
    application = await electron.launch({
      args: electronLaunchArgs(ELECTRON_DIR),
      cwd: ELECTRON_DIR,
      env: {
        ...launchEnvironment,
        OPTIMUS_ELECTRON_UI: 'react',
        OPTIMUS_HOST_PORT: String(hostPort),
        OPTIMUS_HTTP_TOKEN: `optimus-electron-e2e-${process.pid}-0123456789abcdef`,
        OPTIMUS_HOME: home,
        OPTIMUS_ELECTRON_USER_DATA: path.join(home, 'electron-user-data'),
      },
    });
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
    await expect(page).toHaveURL(/^optimus-app:\/\/ui\//);
    await expect(page.getByRole('complementary', { name: 'Projects and sessions' })).toBeVisible();
    // The evidence workspace is chat-first and starts closed (workspace-redesign
    // Slice 1 removed the phantom column). Open it for the Files/Artifacts/Browser
    // tab assertions below, which live inside that pane.
    await page.getByRole('button', { name: 'Workspace', exact: true }).click();
    await expect(page.getByRole('complementary', { name: 'Evidence workspace' })).toBeVisible();

    const hostInfo = await page.evaluate(() => window.optimusElectron.hostInfo());
    expect(hostInfo.uiMode).toBe('react');
    expect(hostInfo.token).toBeUndefined();
    const productionLocation = await page.evaluate(() => location.href);
    expect(productionLocation).not.toContain('token=');

    await page.getByRole('button', { name: 'New thread' }).click();
    await expect(page.locator('.session-row.is-active')).toHaveCount(1);
    await page.waitForTimeout(50);
    const composer = page.getByLabel('Message Optimus');
    await composer.fill('compiled shell offline proof');
    await page.getByRole('button', { name: 'Send message' }).click();
    await expect(
      page.locator('.message-body').getByText('offline echo: compiled shell offline proof', {
        exact: true,
      })
    ).toBeVisible();

    const cancellation = await page.evaluate(async () => {
      const session = await window.optimusElectron.invoke('new_session', {});
      return new Promise(async (resolve) => {
        const unsubscribe = window.optimusElectron.chat.subscribe((envelope) => {
          if (envelope.sessionId !== session.id) return;
          if (['done', 'error', 'cancelled'].includes(envelope.event.type)) {
            unsubscribe();
            resolve({ event: envelope.event.type });
          }
        });
        const started = await window.optimusElectron.chat.start({
          session: session.id,
          message: 'cancel this offline run',
          provider: 'offline',
        });
        const result = await window.optimusElectron.chat.cancel(started.streamId);
        setTimeout(() => resolve({ event: 'timeout', result }), 3000);
      });
    });
    expect(cancellation.event).toBe('cancelled');

    await page.getByRole('tab', { name: 'Files' }).click();
    await expect(page.getByRole('tree', { name: 'Directory contents' })).toBeVisible();
    await page.getByRole('tab', { name: 'Artifacts' }).click();
    await expect(page.getByText('No matching artifacts.')).toBeVisible();
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await expect(page.getByRole('dialog', { name: 'Settings' })).toBeVisible();
    await page.getByRole('button', { name: 'Done' }).click();

    await page.getByRole('tab', { name: 'Browser' }).click();
    const address = page.getByLabel('Browser address');
    await address.fill(`http://127.0.0.1:${previewPort}/fixture`);
    await address.press('Enter');
    // The redesign dropped the "Live · host" chrome badge. Liveness is proven
    // below against the real native view (url, visibility, bounds), which is
    // stronger evidence than the removed label ever was.
    await expect(address).toHaveValue(`http://127.0.0.1:${previewPort}/fixture`);

    await page.waitForTimeout(200);
    const nativeBefore = await nativeViewState(application);
    expect(nativeBefore.url).toBe(`http://127.0.0.1:${previewPort}/fixture`);
    expect(nativeBefore.visible).toBe(true);
    const holeBefore = await page.getByTestId('browser-hole').boundingBox();
    expect(holeBefore).not.toBeNull();
    expect(nativeBefore.bounds).toEqual(roundBox(holeBefore));

    const nativeCapture = await application.evaluate(async ({ BrowserWindow }) => {
      const window = BrowserWindow.getAllWindows()[0];
      const view = window.contentView.children.find((child) => child.webContents);
      const image = await view.webContents.capturePage();
      return image.toPNG().toString('base64');
    });
    fs.writeFileSync(
      path.join(EVIDENCE_DIR, 'compiled-electron-native-browser.png'),
      Buffer.from(nativeCapture, 'base64')
    );

    await application.evaluate(async ({ BrowserWindow }) => {
      const window = BrowserWindow.getAllWindows()[0];
      const view = window.contentView.children.find((child) => child.webContents);
      const bounds = view.getBounds();
      view.webContents.sendInputEvent({ type: 'mouseMove', x: bounds.width / 2, y: bounds.height / 2 });
      view.webContents.sendInputEvent({ type: 'mouseDown', x: bounds.width / 2, y: bounds.height / 2, button: 'left', clickCount: 1 });
      view.webContents.sendInputEvent({ type: 'mouseUp', x: bounds.width / 2, y: bounds.height / 2, button: 'left', clickCount: 1 });
    });
    await expect.poll(() =>
      application.evaluate(async ({ BrowserWindow }) => {
        const window = BrowserWindow.getAllWindows()[0];
        const view = window.contentView.children.find((child) => child.webContents);
        return view.webContents.executeJavaScript('window.nativeClicks');
      })
    ).toBe(1);

    await page.getByRole('button', { name: 'Annotate preview' }).click();
    await page.waitForTimeout(50);
    await application.evaluate(async ({ BrowserWindow }) => {
      const window = BrowserWindow.getAllWindows()[0];
      const view = window.contentView.children.find((child) => child.webContents);
      const bounds = view.getBounds();
      view.webContents.sendInputEvent({ type: 'mouseMove', x: bounds.width / 2, y: bounds.height / 2 });
      view.webContents.sendInputEvent({ type: 'mouseDown', x: bounds.width / 2, y: bounds.height / 2, button: 'left', clickCount: 1 });
      view.webContents.sendInputEvent({ type: 'mouseUp', x: bounds.width / 2, y: bounds.height / 2, button: 'left', clickCount: 1 });
    });
    // A preview click lands in the annotation gallery only. Reaching the composer
    // requires the explicit Add to prompt action (program P23 / ADR-0040), so the
    // untrusted preview can never inject straight into the prompt.
    const annotationText = `Preview context (untrusted): button “Native preview target 1” on 127.0.0.1:${previewPort}, 240 × 90px.`;
    const gallery = page.getByLabel('Preview annotation gallery');
    await expect(gallery.getByText(annotationText)).toBeVisible();
    await expect(composer).toHaveValue('');
    await gallery.getByRole('button', { name: 'Add to prompt' }).first().click();
    await expect(composer).toHaveValue(annotationText);
    await expect.poll(() =>
      application.evaluate(async ({ BrowserWindow }) => {
        const window = BrowserWindow.getAllWindows()[0];
        const view = window.contentView.children.find((child) => child.webContents);
        return view.webContents.executeJavaScript('window.nativeClicks');
      })
    ).toBe(1);

    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await expect(page.getByRole('dialog', { name: 'Settings' })).toBeVisible();
    await expect.poll(async () => (await nativeViewState(application)).visible).toBe(false);
    await page.screenshot({
      path: path.join(EVIDENCE_DIR, 'compiled-electron-settings-overlay.png'),
    });
    await page.getByRole('button', { name: 'Done' }).click();
    await expect.poll(async () => (await nativeViewState(application)).visible).toBe(true);

    const divider = page.getByRole('separator', { name: 'Resize evidence workspace' });
    const dividerBox = await divider.boundingBox();
    expect(dividerBox).not.toBeNull();
    await page.mouse.move(dividerBox.x + 1, dividerBox.y + dividerBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(dividerBox.x + 97, dividerBox.y + dividerBox.height / 2);
    await page.mouse.up();
    await expect.poll(async () => {
      const native = await nativeViewState(application);
      const hole = await page.getByTestId('browser-hole').boundingBox();
      return JSON.stringify(native.bounds) === JSON.stringify(roundBox(hole));
    }).toBe(true);
    const holeAfter = await page.getByTestId('browser-hole').boundingBox();
    const nativeAfter = await nativeViewState(application);
    expect(nativeAfter.bounds).toEqual(roundBox(holeAfter));
    expect(nativeAfter.bounds.width).toBeLessThan(nativeBefore.bounds.width - 80);

    await page.screenshot({
      path: path.join(EVIDENCE_DIR, 'compiled-electron-workbench.png'),
    });
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await application.waitForEvent('close');
    application = null;
  } finally {
    if (application) await application.close().catch(() => undefined);
    await new Promise((resolve) => server.close(resolve));
    if (home.startsWith(`${EVIDENCE_DIR}${path.sep}`)) {
      fs.rmSync(home, { recursive: true, force: true });
    }
  }
});

async function nativeViewState(application) {
  return application.evaluate(({ BrowserWindow }) => {
    const window = BrowserWindow.getAllWindows()[0];
    const view = window.contentView.children.find((child) => child.webContents);
    return {
      url: view.webContents.getURL(),
      visible: view.getVisible(),
      bounds: view.getBounds(),
    };
  });
}

function roundBox(box) {
  return {
    x: Math.round(box.x),
    y: Math.round(box.y),
    width: Math.round(box.width),
    height: Math.round(box.height),
  };
}
