import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { createWsTransport } from './wsTransport';

/**
 * A scripted WebSocket fake: records sent frames, lets the test push
 * frames in, and exposes close/error triggers. The transport's contract
 * (spec-014 coupling with tauriTransport) is what is under test: hello
 * handshake, JSON-RPC invoke round trips, stream terminal handling,
 * cancel, and the synthetic terminal error on unexpected close (R9).
 */
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 3;
  readyState = FakeWebSocket.CONNECTING;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(readonly url: string) {
    FakeWebSocket.instances.push(this);
  }

  send(payload: string) {
    this.sent.push(payload);
  }

  open() {
    this.readyState = 1;
    this.onopen?.();
  }

  push(frame: unknown) {
    this.onmessage?.({ data: JSON.stringify(frame) });
  }

  closeUnexpectedly() {
    this.readyState = 3;
    this.onclose?.();
  }

  fail() {
    this.onerror?.();
  }
}

const realWebSocket = globalThis.WebSocket;

beforeEach(() => {
  FakeWebSocket.instances = [];
  globalThis.WebSocket = FakeWebSocket as unknown as typeof WebSocket;
});

afterEach(() => {
  globalThis.WebSocket = realWebSocket;
});

function lastSocket(): FakeWebSocket {
  const socket = FakeWebSocket.instances.at(-1);
  if (!socket) throw new Error('no socket created');
  return socket;
}

function sentJson(socket: FakeWebSocket) {
  return socket.sent.map((line) => JSON.parse(line));
}

/** Wait for a sent frame matching the predicate (sends are microtask-timed). */
async function waitForFrame(socket: FakeWebSocket, predicate: (frame: any) => boolean): Promise<any> {
  const deadline = Date.now() + 2000;
  for (;;) {
    const frame = sentJson(socket).find(predicate);
    if (frame) return frame;
    if (Date.now() > deadline) throw new Error('frame never sent');
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
}

describe('wsTransport (surface protocol carrier)', () => {
  it('performs the hello handshake with the broker ticket before anything else', async () => {
    const transport = createWsTransport({ port: 17865, ticket: 'ticket-abc' });
    const socket = lastSocket();
    expect(socket.url).toBe('ws://127.0.0.1:17865/ws');
    socket.open();
    const [hello] = sentJson(socket);
    expect(hello).toMatchObject({
      jsonrpc: '2.0',
      id: 1,
      method: 'hello',
      params: { protocol_version: 1, client_kind: 'renderer', ticket: 'ticket-abc' },
    });
    // The hello result settles readiness; a later invoke works.
    socket.push({ jsonrpc: '2.0', id: 1, result: { protocol_version: 1, capabilities: {} } });
    const pending = transport.invoke<{ ok: boolean }>('ping', {});
    const pingFrame = await waitForFrame(socket, (frame) => frame.method === 'ping');
    expect(pingFrame).toMatchObject({ jsonrpc: '2.0', method: 'ping' });
    socket.push({ jsonrpc: '2.0', id: pingFrame.id, result: { ok: true } });
    await expect(pending).resolves.toEqual({ ok: true });
  });

  it('round-trips a JSON-RPC invoke and maps errors', async () => {
    const transport = createWsTransport({ port: 17865, ticket: 't' });
    const socket = lastSocket();
    socket.open();
    socket.push({ jsonrpc: '2.0', id: 1, result: { protocol_version: 1 } });
    const pending = transport.invoke<{ ok: boolean }>('doctor', {});
    const frame = await waitForFrame(socket, (sent) => sent.method === 'doctor');
    socket.push({ jsonrpc: '2.0', id: frame.id, result: { ok: true } });
    await expect(pending).resolves.toEqual({ ok: true });

    const failing = transport.invoke<unknown>('frobnicate' as never, {});
    const failingFrame = await waitForFrame(socket, (sent) => sent.method === 'frobnicate');
    socket.push({ jsonrpc: '2.0', id: failingFrame.id, error: { code: -32601, message: 'unknown method' } });
    await expect(failing).rejects.toThrow('unknown method');
  });

  it('streams chat events with exactly one terminal and a cancellable handle', async () => {
    const transport = createWsTransport({ port: 17865, ticket: 't' });
    const socket = lastSocket();
    socket.open();
    socket.push({ jsonrpc: '2.0', id: 1, result: { protocol_version: 1 } });

    const events: string[] = [];
    const handle = transport.chat(
      { session: 's1', message: 'hi', provider: 'offline' },
      (event) => events.push(event.type)
    );
    const start = await waitForFrame(socket, (frame) => frame.method === 'chat_start');
    expect(start.params.stream_id).toBe(1);
    expect(start.params.request.message).toBe('hi');

    const streamId = start.params.stream_id;
    socket.push({ jsonrpc: '2.0', method: 'event', params: { stream_id: streamId, event: { type: 'delta', text: 'hi' } } });
    socket.push({ jsonrpc: '2.0', method: 'event', params: { stream_id: streamId, event: { type: 'done', result: {} } } });
    await handle.done;
    expect(events).toEqual(['delta', 'done']);

    const cancelPromise = handle.cancel();
    const cancel = await waitForFrame(socket, (frame) => frame.method === 'chat_cancel');
    expect(cancel.params.stream_id).toBe(1);
    socket.push({ jsonrpc: '2.0', id: cancel.id, result: { requested: true } });
    await cancelPromise;
  });

  it('synthesizes a terminal error for open streams on unexpected close (R9)', async () => {
    const transport = createWsTransport({ port: 17865, ticket: 't' });
    const socket = lastSocket();
    socket.open();
    socket.push({ jsonrpc: '2.0', id: 1, result: { protocol_version: 1 } });

    const events: string[] = [];
    const handle = transport.chat(
      { session: 's1', message: 'hi', provider: 'offline' },
      (event) => events.push(event.type)
    );
    const start = await waitForFrame(socket, (frame) => frame.method === 'chat_start');
    socket.push({ jsonrpc: '2.0', method: 'event', params: { stream_id: start.params.stream_id, event: { type: 'delta', text: 'partial' } } });

    socket.closeUnexpectedly();
    await handle.done;
    expect(events).toEqual(['delta', 'error']);
    expect(events.filter((type) => type === 'error').length).toBe(1);
  });

  it('rejects pending invokes on unexpected close', async () => {
    const transport = createWsTransport({ port: 17865, ticket: 't' });
    const socket = lastSocket();
    socket.open();
    socket.push({ jsonrpc: '2.0', id: 1, result: { protocol_version: 1 } });

    const pending = transport.invoke<unknown>('doctor', {});
    await waitForFrame(socket, (sent) => sent.method === 'doctor');
    socket.closeUnexpectedly();
    await expect(pending).rejects.toThrow('web socket closed unexpectedly');
  });
});
