// @ts-check
// spec-015 A3: the desktop e2e suite drives the REACT workbench against a
// spawned `optimus serve`. This spec pins the boot contract:
//   - the serve health surface (HTTP GET /api/health, Bearer-gated with the
//     record token — the ONLY HTTP surface serve keeps, serve.rs:8)
//   - the React workbench boots over the WS transport (ticket injected via
//     addInitScript into __OPTIMUS_BROKER_TICKET__, never a URL)
//   - a chat round-trip emits EXACTLY ONE terminal event (the A3 headline)
//   - WS cancel is one-shot; a second cancel is a no-op
//   - held streams do not block the health surface or a new hello
const { test, expect, url, waitForReady, rpc, chatStream } = require('./support');

test('serve exposes only the Bearer-gated health surface on HTTP', async ({ serverInfo }) => {
  // The record token IS the Bearer (serve.rs:8, ws.rs:55-66).
  const ok = await fetch(`http://127.0.0.1:${serverInfo.port}/api/health`, {
    headers: { Authorization: `Bearer ${serverInfo.ticket}` },
  }).then((r) => r.json());
  expect(ok).toEqual({ ok: true, streaming: true, transport: 'ws' });

  // Without the token the health probe is 401.
  const denied = await fetch(`http://127.0.0.1:${serverInfo.port}/api/health`);
  expect(denied.status).toBe(401);

  // Anything else on HTTP is 404 — no /api/ipc, no /api/chat/stream.
  const unknown = await fetch(`http://127.0.0.1:${serverInfo.port}/api/ipc`);
  expect(unknown.status).toBe(404);
});

test('react workbench boots on the WS transport from a loopback origin', async ({ page }) => {
  await page.goto('/');
  await waitForReady(page);
  // The work surface rendered (composer visible) — the transport resolved.
  await expect(page.getByLabel('Message Optimus')).toBeVisible();
  // The status bar shows the workbench state.
  await expect(page.getByLabel('Session status')).toBeVisible();
});

test('hello carries the record ticket and answers the protocol version', async ({ serverInfo }) => {
  const { WebSocket } = globalThis;
  const ws = new WebSocket(`ws://127.0.0.1:${serverInfo.port}/ws`);
  const replies = [];
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });
  ws.addEventListener('message', (event) => {
    replies.push(JSON.parse(String(event.data)));
    if (replies.length >= 2) ws.close();
  });
  ws.send(JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    method: 'hello',
    params: { protocol_version: 1, client_kind: 'renderer', ticket: serverInfo.ticket },
  }));
  await new Promise((resolve) => ws.addEventListener('close', resolve, { once: true }));
  const hello = replies.find((r) => r.id === 1);
  expect(hello.result.protocol_version).toBe(1);
  expect(hello.result.capabilities.streaming).toBe(true);
  expect(hello.result.capabilities.carriers).toEqual(['stdio', 'ws']);
  const ready = replies.find((r) => r.method === 'host.ready');
  expect(ready).toBeTruthy();
});

test('chat round-trip emits exactly one terminal event (A3 headline)', async ({ serverInfo }) => {
  const { events, terminal, terminalCount } = await chatStream(serverInfo, {
    session: '',
    message: 'hello from playwright',
    provider: 'offline',
  });
  // The offline provider echoes the message; deltas stream before the terminal.
  const text = events.map((e) => e.text || '').join('');
  expect(text).toContain('offline echo: hello from playwright');
  expect(terminalCount).toBe(1);
  expect(terminal.type).toBe('done');
  expect(terminal.result).toBeTruthy();
});

test('chat_cancel is one-shot: a second cancel is a no-op', async ({ serverInfo }) => {
  const ws = await (async () => {
    const { WebSocket } = globalThis;
    const ws = new WebSocket(`ws://127.0.0.1:${serverInfo.port}/ws`);
    const pending = new Map();
    ws.addEventListener('message', (event) => {
      const value = JSON.parse(String(event.data));
      if (value.id !== undefined && pending.has(value.id)) {
        const { resolve } = pending.get(value.id);
        pending.delete(value.id);
        resolve(value);
      }
    });
    await new Promise((resolve, reject) => {
      ws.addEventListener('open', resolve, { once: true });
      ws.addEventListener('error', reject, { once: true });
    });
    ws.invoke = (id, method, params) =>
      new Promise((resolve) => {
        pending.set(id, { resolve });
        ws.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
      });
    return ws;
  })();
  try {
    await ws.invoke(1, 'hello', {
      protocol_version: 1,
      client_kind: 'renderer',
      ticket: serverInfo.ticket,
    });
    const ack = await ws.invoke(2, 'chat_start', {
      stream_id: 7,
      request: { session: '', message: 'cancel me', provider: 'offline' },
    });
    expect(ack.result.stream_id).toBe(7);
    // The first cancel requests the cancellation.
    const first = await ws.invoke(3, 'chat_cancel', { stream_id: 7 });
    expect(first.result.requested).toBe(true);
    // A second cancel on the same stream is a no-op (R6).
    const second = await ws.invoke(4, 'chat_cancel', { stream_id: 7 });
    expect(second.result.requested).toBe(false);
  } finally {
    ws.close();
  }
});

test('held streams leave the health surface and a new hello responsive', async ({ serverInfo }) => {
  // Saturate the pool with paced offline turns, then prove the control
  // plane stays responsive: health answers and a fresh hello completes.
  const { WebSocket } = globalThis;
  const ws = new WebSocket(`ws://127.0.0.1:${serverInfo.port}/ws`);
  const pending = new Map();
  ws.addEventListener('message', (event) => {
    const value = JSON.parse(String(event.data));
    if (value.id !== undefined && pending.has(value.id)) {
      const { resolve } = pending.get(value.id);
      pending.delete(value.id);
      resolve(value);
    }
  });
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });
  ws.invoke = (id, method, params) =>
    new Promise((resolve) => {
      pending.set(id, { resolve });
      ws.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    });
  try {
    await ws.invoke(1, 'hello', {
      protocol_version: 1,
      client_kind: 'renderer',
      ticket: serverInfo.ticket,
    });
    for (let i = 0; i < 4; i += 1) {
      await ws.invoke(10 + i, 'chat_start', {
        stream_id: 20 + i,
        request: { session: '', message: `held turn ${i}`, provider: 'offline' },
      });
    }
    // Health still answers under load.
    const health = await fetch(`http://127.0.0.1:${serverInfo.port}/api/health`, {
      headers: { Authorization: `Bearer ${serverInfo.ticket}` },
    }).then((r) => r.json());
    expect(health.ok).toBe(true);
    // A fresh connection's hello still completes (control-plane bypass).
    const again = await rpc(serverInfo, 'ping');
    expect(again.pong).toBe(true);
  } finally {
    ws.close();
  }
});
