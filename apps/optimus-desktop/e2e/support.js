// @ts-check
const { test: base, expect } = require('@playwright/test');
const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');
const net = require('net');

const ROOT = path.resolve(__dirname, '../../..');
const TARGET_DIR = process.env.CARGO_TARGET_DIR || path.join(ROOT, 'target');
const EXE = path.join(
  TARGET_DIR,
  'debug',
  `optimus-desktop${process.platform === 'win32' ? '.exe' : ''}`
);
let activeBaseUrl = '';
const HTTP_TOKEN = `optimus-e2e-token-${process.pid}-0123456789abcdef`;
const nativeFetch = global.fetch.bind(global);
global.fetch = (input, init = {}) => {
  const target = typeof input === 'string' ? input : String(input?.url || '');
  if (activeBaseUrl && target.startsWith(`${activeBaseUrl}/api/`)) {
    const headers = new Headers(init.headers || {});
    headers.set('Authorization', `Bearer ${HTTP_TOKEN}`);
    headers.set('X-Optimus-CSRF', '1');
    headers.set('Origin', activeBaseUrl);
    return nativeFetch(input, { ...init, headers });
  }
  return nativeFetch(input, init);
};

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function hasExited(server) {
  return server.exitCode !== null || server.signalCode !== null;
}

async function waitForHealth(server, timeoutMs = 30000) {
  const start = Date.now();
  let bootLog = '';
  const appendLog = (chunk) => {
    bootLog = (bootLog + chunk.toString()).slice(-65536);
  };
  server.stdout.on('data', appendLog);
  server.stderr.on('data', appendLog);
  try {
    while (Date.now() - start < timeoutMs) {
      if (server.optimusSpawnError) throw server.optimusSpawnError;
      if (hasExited(server)) {
        throw new Error(
          `Optimus HTTP server exited code=${server.exitCode} signal=${server.signalCode}\n${bootLog}`
        );
      }
      try {
        const response = await fetch(`${activeBaseUrl}/api/health`);
        const health = response.ok ? await response.json() : null;
        if (health?.ok === true && health?.streaming === true) return;
      } catch {
        // Retry until the bounded startup deadline.
      }
      await wait(200);
    }
    throw new Error(`HTTP server health timeout\n--- server log ---\n${bootLog}`);
  } finally {
    server.stdout.off('data', appendLog);
    server.stderr.off('data', appendLog);
  }
}

function reservePort() {
  return new Promise((resolve, reject) => {
    const socket = net.createServer();
    socket.unref();
    socket.once('error', reject);
    socket.listen(0, '127.0.0.1', () => {
      const address = socket.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      socket.close((error) => error ? reject(error) : resolve(port));
    });
  });
}

function waitForExit(server, timeoutMs) {
  if (hasExited(server)) return Promise.resolve(true);
  return new Promise((resolve) => {
    let settled = false;
    const finish = (exited) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      server.off('exit', onExit);
      resolve(exited);
    };
    const onExit = () => finish(true);
    const timer = setTimeout(() => finish(hasExited(server)), timeoutMs);
    server.once('exit', onExit);
    if (hasExited(server)) finish(true);
  });
}

async function stopServer(server) {
  if (hasExited(server)) return;
  server.kill('SIGTERM');
  if (await waitForExit(server, 2000)) return;
  server.kill('SIGKILL');
  if (!(await waitForExit(server, 2000))) {
    throw new Error(`Optimus HTTP server did not exit: pid=${server.pid}`);
  }
}

async function waitForPortRelease(timeoutMs = 3000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      await fetch(`${activeBaseUrl}/api/health`);
    } catch {
      return;
    }
    await wait(50);
  }
  throw new Error(`Optimus HTTP port was not released: ${activeBaseUrl}`);
}

function url(pathname = '') {
  if (!activeBaseUrl) throw new Error('Optimus worker server is not ready');
  return `${activeBaseUrl}${pathname}`;
}

async function waitForReady(page) {
  await expect(page.locator('html')).toHaveAttribute('data-boot-state', 'ready', {
    timeout: 20000,
  });
}

const test = base.extend({
  serverInfo: [async ({}, use, workerInfo) => {
    if (!fs.existsSync(EXE)) {
      throw new Error(`Missing binary: ${EXE} — run cargo build -p optimus-desktop`);
    }
    const port = await reservePort();
    const baseURL = `http://127.0.0.1:${port}`;
    const home = path.join(
      os.tmpdir(),
      `optimus-e2e-${process.pid}-${workerInfo.workerIndex}-${Date.now()}`
    );
    fs.mkdirSync(home, { recursive: true });
    activeBaseUrl = baseURL;
    let server = null;
    const failures = [];
    try {
      server = spawn(EXE, ['--http', String(port), '--development-http', '--home', home], {
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
        env: { ...process.env, OPTIMUS_HTTP_TOKEN: HTTP_TOKEN },
      });
      server.optimusHome = home;
      server.once('error', (error) => { server.optimusSpawnError = error; });
      await waitForHealth(server);
      await use({ home, port, baseURL: activeBaseUrl });
    } catch (error) {
      failures.push(error);
    } finally {
      try {
        if (server) await stopServer(server);
      } catch (error) {
        failures.push(error);
      }
      try {
        if (server) await waitForPortRelease();
      } catch (error) {
        failures.push(error);
      }
      try {
        fs.rmSync(home, { recursive: true, force: true });
      } catch (error) {
        failures.push(error);
      } finally {
        activeBaseUrl = '';
      }
    }
    if (failures.length === 1) throw failures[0];
    if (failures.length > 1) {
      throw new AggregateError(failures, 'Optimus worker fixture failed and cleanup was incomplete');
    }
  }, { scope: 'worker', auto: true }],
  baseURL: async ({ serverInfo }, use) => {
    await use(serverInfo.baseURL);
  },
});

module.exports = { test, expect, url, waitForReady };
