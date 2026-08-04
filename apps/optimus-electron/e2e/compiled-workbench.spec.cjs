const { test, _electron: electron } = require('@playwright/test');
const fs = require('fs');
const path = require('path');
const {
  electronLaunchArgs,
  launchEnvironment,
  offlineWorkbenchFlow,
  reservePort,
  workbenchWindow,
} = require('./support/workbench-flow.cjs');

const ROOT = path.resolve(__dirname, '../../..');
const ELECTRON_DIR = path.join(ROOT, 'apps', 'optimus-electron');
const UI_DIST = path.join(ROOT, 'apps', 'optimus-ui', 'dist', 'index.html');
const EVIDENCE_DIR = path.join(ROOT, 'local', 'tmp', 'compiled-workbench');

// Gate twin of installed-shell.spec.cjs: same offline acceptance flow against the
// repository shell, so control renames fail here long before an installed capture.
test('compiled Electron workbench completes and restores an offline session', async () => {
  test.skip(!fs.existsSync(UI_DIST), 'bun run --cwd apps/optimus-ui build');

  const home = path.join(EVIDENCE_DIR, `optimus-home-${process.pid}`);
  fs.mkdirSync(home, { recursive: true });
  const hostPort = await reservePort();

  let application;
  try {
    application = await electron.launch({
      args: electronLaunchArgs(ELECTRON_DIR),
      cwd: ELECTRON_DIR,
      env: {
        ...launchEnvironment(),
        OPTIMUS_ELECTRON_UI: 'react',
        OPTIMUS_HOST_PORT: String(hostPort),
        OPTIMUS_HTTP_TOKEN: `optimus-compiled-e2e-${process.pid}-0123456789abcdef`,
        OPTIMUS_HOME: home,
        OPTIMUS_ELECTRON_USER_DATA: path.join(home, 'electron-user-data'),
      },
    });

    const page = await workbenchWindow(application);
    await offlineWorkbenchFlow(page, {
      mode: 'compiled-electron',
      evidenceDir: EVIDENCE_DIR,
      record: { optimusHome: home },
    });

    await application.waitForEvent('close');
    application = null;
  } finally {
    if (application) await application.close().catch(() => undefined);
  }
});
