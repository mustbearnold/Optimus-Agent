/**
 * Dev launcher: ensure Rust host binary exists, then start Electron.
 * Optional React Vite: set OPTIMUS_ELECTRON_UI=react and run Vite separately
 * or set OPTIMUS_UI_AUTOSTART=1.
 */
const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const net = require('net');

const ROOT = path.resolve(__dirname, '../../..');
const ELECTRON_DIR = path.resolve(__dirname, '..');
const UI_DIR = path.resolve(ROOT, 'apps/optimus-ui');
const UI_MODE = process.env.OPTIMUS_ELECTRON_UI || 'legacy';
const UI_PORT = Number(process.env.OPTIMUS_UI_PORT || 5173);

function cargoDesktop() {
  const targetDir = process.env.CARGO_TARGET_DIR || path.join(ROOT, 'target');
  const debug = path.join(targetDir, 'debug', 'optimus-desktop');
  const release = path.join(targetDir, 'release', 'optimus-desktop');
  return fs.existsSync(debug) ? debug : release;
}

function waitPort(port, host = '127.0.0.1', timeoutMs = 60000) {
  const start = Date.now();
  return new Promise((resolve, reject) => {
    const tryOnce = () => {
      const socket = net.connect({ port, host }, () => {
        socket.end();
        resolve();
      });
      socket.on('error', () => {
        socket.destroy();
        if (Date.now() - start > timeoutMs) reject(new Error(`port ${port} timeout`));
        else setTimeout(tryOnce, 200);
      });
    };
    tryOnce();
  });
}

async function main() {
  const bin = cargoDesktop();
  if (!fs.existsSync(bin)) {
    console.error('Building optimus-desktop (debug)…');
    const build = spawn('cargo', ['build', '-p', 'optimus-desktop'], {
      cwd: ROOT,
      stdio: 'inherit',
    });
    await new Promise((resolve, reject) => {
      build.on('exit', (c) => (c === 0 ? resolve() : reject(new Error(`cargo build exit ${c}`))));
    });
  }

  const children = [];
  if (UI_MODE === 'react' && process.env.OPTIMUS_UI_AUTOSTART !== '0') {
    if (fs.existsSync(path.join(UI_DIR, 'package.json'))) {
      console.error(`[dev] starting Vite UI on :${UI_PORT}`);
      const vite = spawn(
        process.platform === 'win32' ? 'npm.cmd' : 'npm',
        ['run', 'dev', '--', '--host', '127.0.0.1', '--port', String(UI_PORT)],
        { cwd: UI_DIR, stdio: 'inherit', env: { ...process.env } }
      );
      children.push(vite);
      await waitPort(UI_PORT).catch(() => {
        console.warn('[dev] Vite port not ready yet; Electron will retry load');
      });
    }
  }

  const electronBin = require('electron');
  const electron = spawn(electronBin, ['.'], {
    cwd: ELECTRON_DIR,
    stdio: 'inherit',
    env: {
      ...process.env,
      OPTIMUS_ELECTRON_UI: UI_MODE,
      OPTIMUS_UI_PORT: String(UI_PORT),
    },
  });
  children.push(electron);

  const shutdown = () => {
    for (const c of children) {
      if (c && !c.killed) c.kill('SIGTERM');
    }
    process.exit(0);
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
  electron.on('exit', shutdown);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
