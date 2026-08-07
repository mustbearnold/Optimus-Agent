// @ts-check
// The spec-015 A3 e2e fixture: Playwright drives the REACT workbench
// (apps/optimus-ui/dist) against a spawned `optimus serve`.
//
// The wire contract is WS JSON-RPC (PROTOCOL_VERSION=1). The fixture:
//  1. spawns `target/debug/optimus serve --port <port> --home <home>`
//  2. waits for the v2/ws record at <home>/host-runtime.json (port + ticket)
//  3. serves the built React workbench from a loopback origin (a tiny
//     static file server; the Origin allowlist admits loopback, R7)
//  4. injects the broker ticket via `page.addInitScript` into the
//     `__OPTIMUS_BROKER_TICKET__` global — never a URL
//  5. exposes `rpc()` / `chatStream()` helpers that speak the same wire
//     contract from Node (native WebSocket) for protocol-level assertions.
//
// The old HTTP transport (`/api/ipc`, `/api/chat/stream`) is gone from the
// serve surface (spec-015): only `GET /api/health` remains on HTTP, on the
// record port, Bearer-gated with the record token.
const { test: base, expect } = require('@playwright/test');
const { spawn } = require('child_process');
const http = require('http');
const path = require('path');
const fs = require('fs');
const os = require('os');
const net = require('net');

const ROOT = path.resolve(__dirname, '../../..');
// The live tier points the fixture at a real, credentialed home. That home
// belongs to the human: the fixture must never create, modify-check, or —
// above all — delete it. Deterministic runs leave this unset and get a
// throwaway tmp home per worker as before.
const HOME_OVERRIDE = process.env.OPTIMUS_E2E_HOME || '';
const TARGET_DIR = process.env.CARGO_TARGET_DIR || path.join(ROOT, 'target');
const EXE = path.join(
  TARGET_DIR,
  'debug',
  `optimus${process.platform === 'win32' ? '.exe' : ''}`
);
const UI_DIST = path.join(ROOT, 'apps', 'optimus-ui', 'dist');
const PROTOCOL_VERSION = 1;

let activeWorkbenchBaseUrl = '';

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function hasExited(server) {
  return server.exitCode !== null || server.signalCode !== null;
}

function reservePort() {
  return new Promise((resolve, reject) => {
    const socket = net.createServer();
    socket.unref();
    socket.once('error', reject);
    socket.listen(0, '127.0.0.1', () => {
      const address = socket.address();
      const port = typeof address === 'object' && address ? address.port : 0;
      socket.close((error) => (error ? reject(error) : resolve(port)));
    });
  });
}

// --- static workbench server (loopback origin) -----------------------------
// Serves apps/optimus-ui/dist with an SPA fallback to index.html. The
// origin is loopback, which the WS Origin allowlist admits (R7).
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
};

function startStaticServer(port) {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const url = new URL(req.url, 'http://127.0.0.1');
      let filePath = path.join(UI_DIST, decodeURIComponent(url.pathname));
      if (!filePath.startsWith(UI_DIST)) {
        res.writeHead(403);
        res.end('forbidden');
        return;
      }
      if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
        filePath = path.join(UI_DIST, 'index.html');
      }
      const ext = path.extname(filePath).toLowerCase();
      res.writeHead(200, { 'Content-Type': MIME[ext] || 'application/octet-stream' });
      fs.createReadStream(filePath).pipe(res);
    });
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => resolve(server));
  });
}

// --- WS JSON-RPC client (Node native WebSocket) ----------------------------
// One connection per call cluster. `hello` carries the record ticket; every
// invoke waits for its matching reply id (replies can arrive in any order).
async function openWs(record) {
  const ws = new WebSocket(`ws://127.0.0.1:${record.port}/ws`);
  const pending = new Map();
  const listeners = new Set();
  let nextId = 2;
  ws.addEventListener('message', (event) => {
    let value;
    try {
      value = JSON.parse(String(event.data));
    } catch {
      return;
    }
    if (value.id !== undefined && value.id !== null && pending.has(value.id)) {
      const { resolve, reject } = pending.get(value.id);
      pending.delete(value.id);
      if (value.error) reject(new Error(`${value.error.code}: ${value.error.message}`));
      else resolve(value.result);
      return;
    }
    for (const listener of listeners) listener(value);
  });
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', () => reject(new Error('ws open failed')), { once: true });
  });
  ws.invoke = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    });
  ws.onEvent = (listener) => listeners.add(listener);
  const hello = await ws.invoke('hello', {
    protocol_version: PROTOCOL_VERSION,
    client_kind: 'renderer',
    ticket: record.ticket,
  });
  if (hello.protocol_version !== PROTOCOL_VERSION) {
    throw new Error(`unexpected protocol version: ${hello.protocol_version}`);
  }
  return ws;
}

// A helper that issues one hello + invoke and closes the socket. Results
// are returned as `{ ok: true, ...result }`; a JSON-RPC error becomes
// `{ ok: false, error }`. The `ok` field mirrors the HTTP IPC envelope the
// pre-A3 suite asserted, so protocol specs keep their shape.
async function rpc(record, method, params = {}) {
  const ws = await openWs(record);
  try {
    const result = await ws.invoke(method, params);
    return { ok: true, ...result };
  } catch (error) {
    return { ok: false, error: String(error.message || error) };
  } finally {
    ws.close();
  }
}

// Start an offline chat stream and drain events until the terminal one.
// The offline provider paces its answer by OPTIMUS_OFFLINE_LATENCY_MS,
// which the fixture sets on the serve process at spawn time (the worker
// reads the env at model construction, so a parent-side change cannot
// affect an already-running serve). Resolves with all events plus the
// terminal event; the caller asserts exactly one terminal event via the
// returned terminalCount.
async function chatStream(record, request) {
  const ws = await openWs(record);
  const streamId = Date.now() % 100000;
  const events = [];
  const terminalKinds = new Set(['done', 'error', 'cancelled']);
  let terminalCount = 0;
  let terminalEvent = null;
  ws.onEvent((value) => {
    if (value.method !== 'event') return;
    const event = value.params?.event;
    if (!event) return;
    events.push(event);
    if (terminalKinds.has(event.type)) {
      terminalCount += 1;
      terminalEvent = event;
    }
  });
  const send = (params) => ws.invoke('chat_start', params);
  const ack = await send({ stream_id: streamId, request });
  if (String(ack.stream_id) !== String(streamId)) {
    throw new Error(`chat_start ack stream_id mismatch: ${ack.stream_id}`);
  }
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline && terminalEvent === null) {
    await wait(50);
  }
  ws.close();
  if (!terminalEvent) throw new Error('chat stream did not reach a terminal event');
  return { events, terminal: terminalEvent, terminalCount };
}

// --- fixture ---------------------------------------------------------------
async function waitForHealth(record, timeoutMs = 30000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const response = await fetch(`http://127.0.0.1:${record.port}/api/health`, {
        headers: { Authorization: `Bearer ${record.token}` },
      });
      const health = response.ok ? await response.json() : null;
      if (health?.ok === true && health?.streaming === true) return;
    } catch {
      // Retry until the bounded startup deadline.
    }
    await wait(200);
  }
  throw new Error(`serve health timeout on port ${record.port}`);
}

async function waitForRecord(home, timeoutMs = 30000) {
  const start = Date.now();
  const recordPath = path.join(home, 'host-runtime.json');
  while (Date.now() - start < timeoutMs) {
    try {
      const record = JSON.parse(fs.readFileSync(recordPath, 'utf8'));
      if (record.version === 2 && record.transport === 'ws' && record.port && record.token) {
        return record;
      }
    } catch {
      // Record not written yet; retry.
    }
    await wait(200);
  }
  throw new Error(`serve record not found at ${recordPath}`);
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
    throw new Error(`optimus serve did not exit: pid=${server.pid}`);
  }
}

function url(pathname = '') {
  if (!activeWorkbenchBaseUrl) throw new Error('Optimus worker server is not ready');
  return `${activeWorkbenchBaseUrl}${pathname}`;
}

// The React workbench has no data-boot-state attribute. Ready means: the
// work surface rendered (transport resolved non-null) and no boot error
// banner is present. The composer textarea only renders inside the work
// surface, so its presence is the boot signal.
async function waitForReady(page) {
  await expect(page.getByLabel('Message Optimus')).toBeVisible({ timeout: 20000 });
  await expect(page.locator('.boot-error')).toHaveCount(0, { timeout: 10000 });
}

const test = base.extend({
  serverInfo: [
    async ({}, use, workerInfo) => {
      if (!fs.existsSync(EXE)) {
        throw new Error(`Missing binary: ${EXE} — run cargo build -p optimus-cli`);
      }
      if (!fs.existsSync(path.join(UI_DIST, 'index.html'))) {
        throw new Error(`Missing workbench dist: ${UI_DIST} — run bun run --cwd apps/optimus-ui build`);
      }
      const port = await reservePort();
      const workbenchPort = await reservePort();
      const home = HOME_OVERRIDE || path.join(
        os.tmpdir(),
        `optimus-e2e-${process.pid}-${workerInfo.workerIndex}-${Date.now()}`
      );
      if (!HOME_OVERRIDE) fs.mkdirSync(home, { recursive: true });
      let server = null;
      let staticServer = null;
      const failures = [];
      try {
        server = spawn(EXE, ['serve', '--port', String(port), '--home', home], {
          stdio: ['ignore', 'pipe', 'pipe'],
          windowsHide: true,
          // Paced offline turns give streaming and cancel tests a window
          // to aim at (spec-015 R6 pacing affordance).
          env: { ...process.env, OPTIMUS_OFFLINE_LATENCY_MS: '300' },
        });
        server.optimusHome = home;
        server.once('error', (error) => { server.optimusSpawnError = error; });
        const record = await waitForRecord(home);
        if (server.optimusSpawnError) throw server.optimusSpawnError;
        if (record.port !== port) throw new Error(`serve bound ${record.port}, expected ${port}`);
        await waitForHealth(record);
        staticServer = await startStaticServer(workbenchPort);
        activeWorkbenchBaseUrl = `http://127.0.0.1:${workbenchPort}`;
        await use({ home, port, ticket: record.token, baseURL: activeWorkbenchBaseUrl });
      } catch (error) {
        failures.push(error);
      } finally {
        activeWorkbenchBaseUrl = '';
        try {
          if (staticServer) staticServer.close();
        } catch (error) {
          failures.push(error);
        }
        try {
          if (server) await stopServer(server);
        } catch (error) {
          failures.push(error);
        }
        try {
          if (!HOME_OVERRIDE) fs.rmSync(home, { recursive: true, force: true });
        } catch (error) {
          failures.push(error);
        }
      }
      if (failures.length === 1) throw failures[0];
      if (failures.length > 1) {
        throw new AggregateError(failures, 'Optimus worker fixture failed and cleanup was incomplete');
      }
    },
    { scope: 'worker', auto: true },
  ],
  // Inject the broker ticket BEFORE any page script runs, per spec-015 A3:
  // the ticket reaches the broker global via addInitScript, never a URL.
  page: async ({ page, serverInfo }, use) => {
    await page.addInitScript(({ port, ticket }) => {
      window.__OPTIMUS_BROKER_TICKET__ = { port, ticket };
    }, { port: serverInfo.port, ticket: serverInfo.ticket });
    await use(page);
  },
  baseURL: async ({ serverInfo }, use) => {
    await use(serverInfo.baseURL);
  },
});

module.exports = { test, expect, url, waitForReady, rpc, chatStream, PROTOCOL_VERSION };
