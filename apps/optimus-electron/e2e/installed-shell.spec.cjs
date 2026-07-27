const { test, expect, _electron: electron } = require('@playwright/test');
const fs = require('fs');
const path = require('path');
const {
  electronLaunchArgs,
  launchEnvironment,
  offlineWorkbenchFlow,
  reservePort,
  workbenchWindow,
} = require('./support/workbench-flow.cjs');

const INSTALLED_ELECTRON = process.env.OPTIMUS_INSTALLED_ELECTRON || '';
const INSTALLED_HOST = process.env.OPTIMUS_INSTALLED_HOST || '';
const INSTALL_ROOT = process.env.OPTIMUS_INSTALLED_ROOT || '';
const EVIDENCE_DIR = process.env.OPTIMUS_INSTALLED_EVIDENCE_DIR || '';

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

  let application;
  try {
    application = await electron.launch({
      args: electronLaunchArgs(),
      executablePath: INSTALLED_ELECTRON,
      cwd: INSTALL_ROOT,
      env: {
        ...launchEnvironment(),
        OPTIMUS_APP_ROOT: INSTALL_ROOT,
        OPTIMUS_DESKTOP_BIN: INSTALLED_HOST,
        OPTIMUS_ELECTRON_UI: 'react',
        OPTIMUS_HOST_PORT: String(hostPort),
        OPTIMUS_HTTP_TOKEN: `optimus-installed-e2e-${process.pid}-0123456789abcdef`,
        OPTIMUS_HOME: home,
        OPTIMUS_ELECTRON_USER_DATA: userData,
      },
    });

    // The installed candidate must be the process under test, not a stale target build.
    expect(fs.realpathSync(`/proc/${application.process().pid}/exe`)).toBe(
      fs.realpathSync(INSTALLED_ELECTRON)
    );

    const page = await workbenchWindow(application);
    await offlineWorkbenchFlow(page, {
      mode: 'installed-electron',
      evidenceDir: EVIDENCE_DIR,
      record: {
        installedElectron: fs.realpathSync(INSTALLED_ELECTRON),
        installedHost: fs.realpathSync(INSTALLED_HOST),
        installRoot: INSTALL_ROOT,
        optimusHome: home,
      },
    });

    await application.waitForEvent('close');
    application = null;
  } finally {
    if (application) await application.close().catch(() => undefined);
  }
});
