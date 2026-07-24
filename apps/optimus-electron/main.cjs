/**
 * Optimus Electron main process.
 * Rust remains authoritative; the React renderer receives a bounded command
 * bridge and never receives the production bearer token.
 */
const {
  app,
  BrowserWindow,
  WebContentsView,
  dialog,
  ipcMain,
  protocol,
  shell,
} = require('electron');
const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const http = require('http');
const { assertPreviewUrl } = require('./browser-policy.cjs');

protocol.registerSchemesAsPrivileged([
  {
    scheme: 'optimus-app',
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      corsEnabled: false,
    },
  },
]);

const ROOT = path.resolve(__dirname, '../..');
const UI_DIST = path.join(ROOT, 'apps', 'optimus-ui', 'dist');
const EXPLICIT_USER_DATA = process.env.OPTIMUS_ELECTRON_USER_DATA || '';
if (EXPLICIT_USER_DATA) app.setPath('userData', path.resolve(EXPLICIT_USER_DATA));
const HOST_PORT = Number(process.env.OPTIMUS_HOST_PORT || 17865);
const UI_MODE = process.env.OPTIMUS_ELECTRON_UI || 'react'; // react | legacy
const REACT_DEV_URL = process.env.OPTIMUS_UI_DEV_URL || '';
const MAX_IPC_BYTES = 1024 * 1024;
const MAX_ACTIVE_STREAMS = 1;

const DESKTOP_METHODS = new Set([
  'ping',
  'doctor',
  'auth_status',
  'auth_import_hermes',
  'auth_import_cli',
  'settings_get',
  'settings_set',
  'sessions',
  'new_session',
  'get_session',
  'rename_session',
  'delete_session',
  'cron_list',
  'cron_add',
  'cron_tick',
  'approvals_list',
  'approvals_grant',
  'jobs_list',
  'campaign_list',
  'campaign_create',
  'campaign_run',
  'campaign_status',
  'term_run',
  'browser_navigate',
  'browser_click',
  'browser_reload',
  'fs_roots',
  'fs_list',
  'fs_read',
  'artifacts_list',
  'artifacts_put_text',
  'artifacts_get',
  'artifacts_delete',
  'artifacts_delete_many',
]);

let hostProc = null;
let mainWindow = null;
let previewView = null;
let previewVisible = false;
let previewBounds = null;
let previewError = '';
let previewAnnotationActive = false;
let previewSession = null;
let previewDownloadHandler = null;
let httpToken = process.env.OPTIMUS_HTTP_TOKEN || '';
let hostBase = `http://127.0.0.1:${HOST_PORT}`;
let requestId = 1;
let streamId = 1;
const activeStreams = new Map();

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
          res.on('data', (chunk) => (body += chunk));
          res.on('end', () => {
            try {
              const payload = JSON.parse(body || '{}');
              if (res.statusCode === 200 && (payload.ok === true || payload.streaming === true)) {
                resolve(payload);
              } else {
                setTimeout(tick, 200);
              }
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
    return waitForHealth(hostBase, httpToken);
  }

  const bin = cargoTargetDesktop();
  if (!fs.existsSync(bin)) {
    return Promise.reject(
      new Error(`optimus-desktop binary missing at ${bin}. Run: cargo build -p optimus-desktop`)
    );
  }

  const env = { ...process.env };
  if (!env.OPTIMUS_HTTP_TOKEN || env.OPTIMUS_HTTP_TOKEN.length < 32) {
    env.OPTIMUS_HTTP_TOKEN = `optimus-electron-${process.pid}-${Date.now()}-0123456789ab`;
  }
  httpToken = env.OPTIMUS_HTTP_TOKEN;
  hostBase = `http://127.0.0.1:${HOST_PORT}`;

  const args = ['--host-only', '--host-port', String(HOST_PORT)];
  if (process.env.OPTIMUS_HOME) args.push('--home', process.env.OPTIMUS_HOME);
  hostProc = spawn(bin, args, {
    env,
    cwd: ROOT,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  const onData = (buffer) => {
    const text = buffer.toString();
    process.stderr.write(text);
    const match = text.match(/OPTIMUS_HTTP_TOKEN=(\S+)/);
    if (match) httpToken = match[1];
  };
  hostProc.stdout.on('data', onData);
  hostProc.stderr.on('data', onData);
  hostProc.on('exit', (code, signal) => {
    console.error(`[optimus-electron] host exited code=${code} signal=${signal}`);
  });
  return waitForHealth(hostBase, httpToken);
}

function registerUiProtocol() {
  protocol.handle('optimus-app', async (request) => {
    let relative = 'index.html';
    try {
      const url = new URL(request.url);
      relative = decodeURIComponent(url.pathname).replace(/^\/+/, '') || 'index.html';
    } catch {
      return new Response('bad request', { status: 400 });
    }
    let target = path.resolve(UI_DIST, relative);
    const rootPrefix = `${path.resolve(UI_DIST)}${path.sep}`;
    if (target !== path.resolve(UI_DIST) && !target.startsWith(rootPrefix)) {
      return new Response('forbidden', { status: 403 });
    }
    if (!fs.existsSync(target) || fs.statSync(target).isDirectory()) {
      if (path.extname(relative)) return new Response('not found', { status: 404 });
      target = path.join(UI_DIST, 'index.html');
    }
    if (!fs.existsSync(target)) {
      return new Response('React assets missing. Run npm --prefix apps/optimus-ui run build.', {
        status: 503,
        headers: { 'Content-Type': 'text/plain; charset=utf-8' },
      });
    }
    return new Response(fs.readFileSync(target), {
      status: 200,
      headers: {
        'Content-Type': mimeType(target),
        'Cache-Control': target.endsWith('index.html') ? 'no-store' : 'public, max-age=31536000, immutable',
        ...(target.endsWith('index.html')
          ? {
              'Content-Security-Policy':
                "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
            }
          : {}),
      },
    });
  });
}

function mimeType(file) {
  switch (path.extname(file).toLowerCase()) {
    case '.html':
      return 'text/html; charset=utf-8';
    case '.js':
      return 'text/javascript; charset=utf-8';
    case '.css':
      return 'text/css; charset=utf-8';
    case '.svg':
      return 'image/svg+xml';
    case '.png':
      return 'image/png';
    case '.woff2':
      return 'font/woff2';
    default:
      return 'application/octet-stream';
  }
}

function uiUrl() {
  if (UI_MODE === 'legacy') return `${hostBase}/`;
  if (REACT_DEV_URL) return REACT_DEV_URL;
  return 'optimus-app://ui/index.html';
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 320,
    minHeight: 480,
    title: 'Optimus Agent',
    frame: false,
    backgroundColor: '#07090d',
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });
  mainWindow.setMenu(null);

  if (UI_MODE === 'react') createPreviewView();

  mainWindow.webContents.on('did-finish-load', () => {
    if (UI_MODE === 'legacy' && httpToken) {
      mainWindow.webContents
        .executeJavaScript(
          `window.__OPTIMUS_HTTP_MODE__=true;window.__OPTIMUS_HTTP_TOKEN__=${JSON.stringify(httpToken)};`
        )
        .catch(() => undefined);
    }
  });

  mainWindow.on('closed', () => {
    destroyPreviewView();
    mainWindow = null;
  });
  mainWindow.loadURL(uiUrl());
}

function createPreviewView() {
  if (!mainWindow || previewView) return;
  previewView = new WebContentsView({
    webPreferences: {
      partition: 'persist:optimus-preview',
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
      webSecurity: true,
    },
  });
  mainWindow.contentView.addChildView(previewView);
  previewView.setVisible(false);
  const contents = previewView.webContents;
  previewSession = contents.session;
  previewDownloadHandler = (event, item) => {
    event.preventDefault();
    item.cancel();
  };
  previewSession.on('will-download', previewDownloadHandler);
  previewSession.setPermissionRequestHandler((_webContents, _permission, callback) => callback(false));
  previewSession.setPermissionCheckHandler(() => false);
  contents.setWindowOpenHandler(() => ({ action: 'deny' }));
  contents.on('will-navigate', (event, url) => {
    try {
      assertPreviewUrl(url);
    } catch {
      event.preventDefault();
      previewError = 'Navigation blocked by the Optimus preview policy.';
      emitBrowserState();
    }
  });
  contents.on('did-start-loading', () => {
    previewError = '';
    emitBrowserState();
  });
  contents.on('did-stop-loading', emitBrowserState);
  contents.on('page-title-updated', emitBrowserState);
  contents.on('did-navigate', emitBrowserState);
  contents.on('did-navigate-in-page', emitBrowserState);
  contents.on('did-fail-load', (_event, code, description, validatedUrl, isMainFrame) => {
    if (!isMainFrame || code === -3) return;
    previewError = `${description} (${validatedUrl})`;
    emitBrowserState();
  });
  contents.loadURL('about:blank').catch(() => undefined);
}

function destroyPreviewView() {
  if (!previewView) return;
  if (previewSession && previewDownloadHandler) {
    previewSession.removeListener('will-download', previewDownloadHandler);
  }
  const contents = previewView.webContents;
  if (!contents.isDestroyed()) contents.close();
  previewView = null;
  previewSession = null;
  previewDownloadHandler = null;
  previewBounds = null;
  previewVisible = false;
}

function browserState() {
  const contents = previewView?.webContents;
  if (!contents || contents.isDestroyed()) {
    return {
      url: '',
      title: 'Preview',
      loading: false,
      canGoBack: false,
      canGoForward: false,
      visible: false,
      error: 'Native preview unavailable',
      native: true,
    };
  }
  const history = contents.navigationHistory;
  return {
    url: contents.getURL(),
    title: contents.getTitle() || 'Preview',
    loading: contents.isLoading(),
    canGoBack: history.canGoBack(),
    canGoForward: history.canGoForward(),
    visible: previewVisible,
    ...(previewError ? { error: previewError } : {}),
    native: true,
  };
}

function emitBrowserState() {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  mainWindow.webContents.send('optimus:browser-state', browserState());
}

function setPreviewBounds(value) {
  if (!previewView || !value || typeof value !== 'object') return;
  const next = {
    x: boundedInteger(value.x, 0, 10000),
    y: boundedInteger(value.y, 0, 10000),
    width: boundedInteger(value.width, 0, 10000),
    height: boundedInteger(value.height, 0, 10000),
  };
  if (
    previewBounds &&
    previewBounds.x === next.x &&
    previewBounds.y === next.y &&
    previewBounds.width === next.width &&
    previewBounds.height === next.height
  ) {
    return;
  }
  previewBounds = next;
  previewView.setBounds(next);
}

async function cancelPreviewAnnotation() {
  const contents = previewView?.webContents;
  if (!contents || contents.isDestroyed() || !previewAnnotationActive) {
    return { cancelled: false };
  }
  try {
    await contents.executeJavaScript(
      'typeof window.__optimusCancelPreviewAnnotation === "function" && window.__optimusCancelPreviewAnnotation()',
      true
    );
  } catch {
    // Navigation can destroy the captured page context. That is a cancellation.
  }
  return { cancelled: true };
}

async function capturePreviewAnnotation() {
  const contents = previewView?.webContents;
  if (!contents || contents.isDestroyed()) {
    throw new Error('Native preview unavailable');
  }
  if (previewAnnotationActive) await cancelPreviewAnnotation();
  previewAnnotationActive = true;
  const script = `(() => new Promise((resolve) => {
    const prior = window.__optimusCancelPreviewAnnotation;
    if (typeof prior === "function") prior();
    const marker = document.createElement("div");
    marker.setAttribute("data-optimus-preview-annotation", "");
    Object.assign(marker.style, {
      position: "fixed",
      zIndex: "2147483647",
      pointerEvents: "none",
      border: "2px solid #2f6feb",
      borderRadius: "4px",
      background: "rgba(47,111,235,.08)",
      boxSizing: "border-box",
      display: "none"
    });
    document.documentElement.appendChild(marker);
    let settled = false;
    let timeout = 0;
    const clean = () => {
      document.removeEventListener("pointermove", move, true);
      document.removeEventListener("click", click, true);
      document.removeEventListener("keydown", keydown, true);
      marker.remove();
      window.clearTimeout(timeout);
      delete window.__optimusCancelPreviewAnnotation;
    };
    const finish = (value) => {
      if (settled) return;
      settled = true;
      clean();
      resolve(value);
    };
    const targetAt = (event) => {
      const candidate = document.elementFromPoint(event.clientX, event.clientY);
      return candidate instanceof HTMLElement && candidate !== marker ? candidate : null;
    };
    const move = (event) => {
      const target = targetAt(event);
      if (!target) {
        marker.style.display = "none";
        return;
      }
      const rect = target.getBoundingClientRect();
      marker.style.display = "block";
      marker.style.left = rect.left + "px";
      marker.style.top = rect.top + "px";
      marker.style.width = rect.width + "px";
      marker.style.height = rect.height + "px";
    };
    const click = (event) => {
      const target = targetAt(event);
      if (!target) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const rect = target.getBoundingClientRect();
      const label = target.getAttribute("aria-label") || target.getAttribute("alt") || target.getAttribute("title") || "";
      finish({
        cancelled: false,
        url: String(location.href).slice(0, 2048),
        pageTitle: String(document.title || "").slice(0, 240),
        tag: String(target.tagName || "").toLowerCase().slice(0, 32),
        role: String(target.getAttribute("role") || "").slice(0, 64),
        label: String(label).replace(/\\s+/g, " ").trim().slice(0, 240),
        text: String(target.innerText || target.textContent || "").replace(/\\s+/g, " ").trim().slice(0, 240),
        rect: {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.round(rect.width),
          height: Math.round(rect.height)
        }
      });
    };
    const keydown = (event) => {
      if (event.key === "Escape") finish({ cancelled: true });
    };
    window.__optimusCancelPreviewAnnotation = () => finish({ cancelled: true });
    document.addEventListener("pointermove", move, true);
    document.addEventListener("click", click, true);
    document.addEventListener("keydown", keydown, true);
    timeout = window.setTimeout(() => finish({ cancelled: true }), 120000);
  }))()`;
  try {
    const result = await contents.executeJavaScript(script, true);
    if (!result || typeof result !== 'object') return { cancelled: true };
    return {
      cancelled: Boolean(result.cancelled),
      url: String(result.url || '').slice(0, 2048),
      pageTitle: String(result.pageTitle || '').slice(0, 240),
      tag: String(result.tag || '').slice(0, 32),
      role: String(result.role || '').slice(0, 64),
      label: String(result.label || '').slice(0, 240),
      text: String(result.text || '').slice(0, 240),
      rect: {
        x: boundedInteger(result.rect?.x, -10000, 10000),
        y: boundedInteger(result.rect?.y, -10000, 10000),
        width: boundedInteger(result.rect?.width, 0, 10000),
        height: boundedInteger(result.rect?.height, 0, 10000),
      },
    };
  } catch {
    return { cancelled: true };
  } finally {
    previewAnnotationActive = false;
  }
}

function boundedInteger(value, minimum, maximum) {
  const number = Number(value);
  if (!Number.isFinite(number)) return minimum;
  return Math.max(minimum, Math.min(maximum, Math.round(number)));
}

function assertMainSender(event) {
  if (
    !mainWindow ||
    event.sender !== mainWindow.webContents ||
    event.senderFrame !== mainWindow.webContents.mainFrame
  ) {
    throw new Error('Rejected IPC from a non-primary renderer');
  }
}

function assertBounded(value, label) {
  let json;
  try {
    json = JSON.stringify(value ?? {});
  } catch {
    throw new Error(`${label} must be JSON serializable`);
  }
  if (Buffer.byteLength(json, 'utf8') > MAX_IPC_BYTES) {
    throw new Error(`${label} exceeds ${MAX_IPC_BYTES} bytes`);
  }
}

async function invokeHost(method, params = {}) {
  if (!DESKTOP_METHODS.has(method)) throw new Error(`Unsupported desktop method: ${method}`);
  assertBounded(params, 'IPC params');
  const response = await fetch(`${hostBase}/api/ipc`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${httpToken}`,
      Origin: hostBase,
      'X-Optimus-CSRF': '1',
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ id: requestId++, method, params }),
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok || body.ok === false) {
    throw new Error(body.error || `IPC ${method} failed (${response.status})`);
  }
  return body.result;
}

function startChat(request) {
  assertBounded(request, 'Chat request');
  if (!request || typeof request.session !== 'string' || typeof request.message !== 'string') {
    throw new Error('Chat requires session and message');
  }
  if (activeStreams.size >= MAX_ACTIVE_STREAMS) throw new Error('One foreground stream is already active');
  const id = streamId++;
  const controller = new AbortController();
  activeStreams.set(id, {
    controller,
    sessionId: request.session,
    terminal: false,
  });
  setImmediate(() => void pumpChat(id, request));
  return { streamId: id };
}

async function pumpChat(id, request) {
  const active = activeStreams.get(id);
  if (!active) return;
  try {
    const response = await fetch(`${hostBase}/api/chat/stream`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${httpToken}`,
        Origin: hostBase,
        'X-Optimus-CSRF': '1',
        'Content-Type': 'application/json',
        Accept: 'text/event-stream',
      },
      body: JSON.stringify(request),
      signal: active.controller.signal,
    });
    if (!response.ok || !response.body) throw new Error(`Chat stream failed (${response.status})`);
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let splitAt;
      while ((splitAt = buffer.indexOf('\n\n')) >= 0) {
        const block = buffer.slice(0, splitAt);
        buffer = buffer.slice(splitAt + 2);
        for (const line of block.split('\n')) {
          if (!line.startsWith('data:')) continue;
          const payload = line.slice(5).trim();
          if (!payload || payload.startsWith(':')) continue;
          let event;
          try {
            event = JSON.parse(payload);
          } catch {
            sendChatEvent(id, { type: 'error', error: 'Malformed stream event' });
            return;
          }
          sendChatEvent(id, event);
          if (event.type === 'done' || event.type === 'error' || event.type === 'cancelled') return;
        }
      }
    }
    const current = activeStreams.get(id);
    if (current && !current.terminal) {
      sendChatEvent(id, { type: 'error', error: 'Stream ended without a terminal event' });
    }
  } catch (error) {
    const current = activeStreams.get(id);
    if (!current || current.terminal) return;
    if (error && error.name === 'AbortError') {
      sendChatEvent(id, { type: 'cancelled', error: 'cancelled by user' });
    } else {
      sendChatEvent(id, {
        type: 'error',
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}

function sendChatEvent(id, event) {
  const active = activeStreams.get(id);
  if (!active || active.terminal) return;
  const terminal = event.type === 'done' || event.type === 'error' || event.type === 'cancelled';
  if (terminal) active.terminal = true;
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('optimus:chat-event', {
      streamId: id,
      sessionId: active.sessionId,
      event,
    });
  }
  if (terminal) activeStreams.delete(id);
}

function setupIpc() {
  ipcMain.handle('optimus:host-info', (event) => {
    assertMainSender(event);
    return {
      baseUrl: hostBase,
      ...(UI_MODE === 'legacy' ? { token: httpToken } : {}),
      uiMode: UI_MODE,
    };
  });

  ipcMain.handle('optimus:invoke', async (event, method, params) => {
    assertMainSender(event);
    return invokeHost(method, params);
  });

  ipcMain.handle('optimus:chat-start', (event, request) => {
    assertMainSender(event);
    return startChat(request);
  });

  ipcMain.handle('optimus:chat-cancel', (event, id) => {
    assertMainSender(event);
    const active = activeStreams.get(Number(id));
    if (!active || active.terminal) return { requested: false };
    if (!active.controller.signal.aborted) active.controller.abort();
    return { requested: true };
  });

  ipcMain.handle('optimus:window', async (event, action) => {
    assertMainSender(event);
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

  ipcMain.handle('optimus:pick-folder', async (event) => {
    assertMainSender(event);
    const result = await dialog.showOpenDialog(mainWindow, {
      properties: ['openDirectory', 'createDirectory'],
    });
    if (result.canceled || !result.filePaths[0]) return { ok: false, cancelled: true };
    return { ok: true, path: result.filePaths[0] };
  });

  ipcMain.handle('optimus:open-path', async (event, targetPath) => {
    assertMainSender(event);
    if (!targetPath || typeof targetPath !== 'string') return { ok: false };
    const error = await shell.openPath(targetPath);
    return error ? { ok: false, error } : { ok: true };
  });

  ipcMain.handle('optimus:open-url', async (event, input) => {
    assertMainSender(event);
    const url = assertPreviewUrl(input);
    await shell.openExternal(url);
    return { ok: true };
  });

  ipcMain.on('optimus:browser-bounds', (event, bounds) => {
    try {
      assertMainSender(event);
      setPreviewBounds(bounds);
    } catch {
      // Fire-and-forget geometry is deliberately fail-closed.
    }
  });

  ipcMain.on('optimus:browser-visible', (event, visible) => {
    try {
      assertMainSender(event);
      previewVisible = Boolean(visible);
      if (!previewVisible) void cancelPreviewAnnotation();
      if (previewVisible && previewBounds) previewView?.setBounds(previewBounds);
      previewView?.setVisible(previewVisible);
      emitBrowserState();
    } catch {
      // Fire-and-forget visibility is deliberately fail-closed.
    }
  });

  ipcMain.handle('optimus:browser-state', (event) => {
    assertMainSender(event);
    return browserState();
  });

  ipcMain.handle('optimus:browser-navigate', async (event, input) => {
    assertMainSender(event);
    const url = assertPreviewUrl(input);
    previewError = '';
    await previewView.webContents.loadURL(url);
    return browserState();
  });

  ipcMain.handle('optimus:browser-back', (event) => {
    assertMainSender(event);
    const history = previewView?.webContents.navigationHistory;
    if (history?.canGoBack()) history.goBack();
    return browserState();
  });

  ipcMain.handle('optimus:browser-forward', (event) => {
    assertMainSender(event);
    const history = previewView?.webContents.navigationHistory;
    if (history?.canGoForward()) history.goForward();
    return browserState();
  });

  ipcMain.handle('optimus:browser-reload', (event) => {
    assertMainSender(event);
    previewView?.webContents.reload();
    return browserState();
  });

  ipcMain.handle('optimus:browser-annotate', async (event) => {
    assertMainSender(event);
    return capturePreviewAnnotation();
  });

  ipcMain.handle('optimus:browser-annotation-cancel', async (event) => {
    assertMainSender(event);
    return cancelPreviewAnnotation();
  });
}

app.whenReady().then(async () => {
  setupIpc();
  registerUiProtocol();
  try {
    await startHost();
  } catch (error) {
    console.error('[optimus-electron] failed to start host:', error);
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
  for (const active of activeStreams.values()) active.controller.abort();
  activeStreams.clear();
  destroyPreviewView();
  if (hostProc && !hostProc.killed) {
    hostProc.kill('SIGTERM');
    hostProc = null;
  }
});

module.exports = {
  boundedInteger,
};
