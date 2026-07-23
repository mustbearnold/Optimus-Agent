/**
 * Optimus Electron main process.
 * Spawns the Rust host (`optimus-desktop --host-only`) and loads the UI origin.
 */
const { app, BrowserWindow, ipcMain, dialog, shell } = require('electron');
const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const net = require('net');
const http = require('http');

const ROOT = path.resolve(__dirname, '../..');
const HOST_PORT = Number(process.env.OPTIMUS_HOST_PORT || 17865);
const UI_MODE = process.env.OPTIMUS_ELECTRON_UI || 'legacy'; // legacy | react
const REACT_DEV_PORT = Number(process.env.OPTIMUS_UI_PORT || 5173);

let hostProc = null;
let mainWindow = null;
let httpToken = process.env.OPTIMUS_HTTP_TOKEN || '';
let hostBase = `http://127.0.0.1:${HOST_PORT}`;

function cargoTargetDesktop() {
  const targetDir = process.env.CARGO_TARGET_DIR || path.join(ROOT, 'target');
  const release = path.join(targetDir, 'release', 'optimus-desktop');
  const debug = path.join(targetDir, 'debug', 'optimus-desktop');
  if (fs.existsSync(release)) return release;
  if (fs.existsSync(debug)) return debug;
  return debug;
}

function waitForHealth(base, token, timeoutMs = 45000) {
  const start = Date.now();
  return new Promise((resolve, reject) => {
    const tick = () => {
      if (Date.now() - start > timeoutMs) {
        reject(new Error(`host health timeout at ${base}`));
        return;
      }
      // Health may require bearer depending on host version; try with token first.
      const req = http.get(
        `${base}/api/health`,
        {
          headers: token
            ? {
                Authorization: `Bearer ${token}`,
                Origin: base,
              }
            : {},
        },
        (res) => {
          let body = '';
          res.on('data', (c) => (body += c));
          res.on('end', () => {
            try {
              const j = JSON.parse(body || '{}');
              if (res.statusCode === 200 && (j.ok === true || j.streaming === true)) resolve(j);
              else setTimeout(tick, 200);
            } catch {
              setTimeout(tick, 200);
            }
          });
        }
      );
      req.on('error', () => setTimeout(tick, 200));
      req.setTimeout(2000, () => {
        req.destroy();
        setTimeout(tick, 200);
      });
    };
    tick();
  });
}

function startHost() {
  if (process.env.OPTIMUS_HOST_EXTERNAL === '1') {
    httpToken = process.env.OPTIMUS_HTTP_TOKEN || httpToken;
    hostBase = process.env.OPTIMUS_HOST_URL || hostBase;
    return Promise.resolve();
  }

  const bin = cargoTargetDesktop();
  if (!fs.existsSync(bin)) {
    return Promise.reject(
      new Error(
        `optimus-desktop binary missing at ${bin}. Run: cargo build -p optimus-desktop`
      )
    );
  }

  const env = { ...process.env };
  if (!env.OPTIMUS_HTTP_TOKEN || env.OPTIMUS_HTTP_TOKEN.length < 32) {
    env.OPTIMUS_HTTP_TOKEN = `optimus-electron-${process.pid}-${Date.now()}-0123456789ab`;
  }
  httpToken = env.OPTIMUS_HTTP_TOKEN;
  hostBase = `http://127.0.0.1:${HOST_PORT}`;

  const args = ['--host-only', '--host-port', String(HOST_PORT)];
  if (process.env.OPTIMUS_HOME) {
    args.push('--home', process.env.OPTIMUS_HOME);
  }

  hostProc = spawn(bin, args, {
    env,
    cwd: ROOT,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  const onData = (buf) => {
    const text = buf.toString();
    process.stderr.write(text);
    const m = text.match(/OPTIMUS_HTTP_TOKEN=(\S+)/);
    if (m) httpToken = m[1];
  };
  hostProc.stdout.on('data', onData);
  hostProc.stderr.on('data', onData);
  hostProc.on('exit', (code, signal) => {
    console.error(`[optimus-electron] host exited code=${code} signal=${signal}`);
  });

  return waitForHealth(hostBase, httpToken);
}

function uiUrl() {
  if (UI_MODE === 'react') {
    return `http://127.0.0.1:${REACT_DEV_PORT}/?host=${encodeURIComponent(hostBase)}&token=${encodeURIComponent(httpToken)}`;
  }
  return hostBase + '/';
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 840,
    minWidth: 420,
    minHeight: 320,
    title: 'Optimus Agent',
    backgroundColor: '#080b10',
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  mainWindow.webContents.on('did-finish-load', () => {
    // Inject host pairing for legacy HTML (bridge already has HTTP mode from host).
    // For React, query params carry host+token.
    if (UI_MODE === 'legacy' && httpToken) {
      mainWindow.webContents.executeJavaScript(
        `window.__OPTIMUS_HTTP_MODE__=true;window.__OPTIMUS_HTTP_TOKEN__=${JSON.stringify(httpToken)};`
      ).catch(() => {});
    }
  });

  mainWindow.loadURL(uiUrl());
}

function setupIpc() {
  ipcMain.handle('optimus:host-info', () => ({
    baseUrl: hostBase,
    token: httpToken,
    uiMode: UI_MODE,
  }));

  ipcMain.handle('optimus:window', async (_e, action) => {
    if (!mainWindow) return { ok: false };
    switch (action) {
      case 'minimize':
        mainWindow.minimize();
        break;
      case 'maximize':
        if (mainWindow.isMaximized()) mainWindow.unmaximize();
        else mainWindow.maximize();
        break;
      case 'close':
        mainWindow.close();
        break;
      default:
        return { ok: false, error: 'unknown window action' };
    }
    return { ok: true };
  });

  ipcMain.handle('optimus:pick-folder', async () => {
    const r = await dialog.showOpenDialog(mainWindow, {
      properties: ['openDirectory', 'createDirectory'],
    });
    if (r.canceled || !r.filePaths[0]) return { ok: false, cancelled: true };
    return { ok: true, path: r.filePaths[0] };
  });

  ipcMain.handle('optimus:open-path', async (_e, p) => {
    if (!p || typeof p !== 'string') return { ok: false };
    const err = await shell.openPath(p);
    return err ? { ok: false, error: err } : { ok: true };
  });

  ipcMain.handle('optimus:open-url', async (_e, url) => {
    if (!url || typeof url !== 'string') return { ok: false };
    await shell.openExternal(url);
    return { ok: true };
  });
}

app.whenReady().then(async () => {
  setupIpc();
  try {
    await startHost();
  } catch (e) {
    console.error('[optimus-electron] failed to start host:', e);
    app.quit();
    return;
  }
  createWindow();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

app.on('before-quit', () => {
  if (hostProc && !hostProc.killed) {
    hostProc.kill('SIGTERM');
    hostProc = null;
  }
});
