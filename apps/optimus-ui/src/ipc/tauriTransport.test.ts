import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { StreamEvent } from './contracts';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  channels: [] as Array<{ onmessage: (event: StreamEvent) => void }>,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
  Channel: class MockChannel {
    onmessage: (event: StreamEvent) => void = () => undefined;
    constructor() {
      mocks.channels.push(this);
    }
  },
}));

import { createTauriTransport } from './tauriTransport';

beforeEach(() => {
  mocks.invoke.mockReset().mockResolvedValue({});
  mocks.channels.length = 0;
});

describe('tauri transport', () => {
  it('streams chat over a bounded Tauri channel and settles on done', async () => {
    const received: string[] = [];
    const transport = createTauriTransport();
    const handle = transport.chat(
      { session: 'session-a', message: 'go', provider: 'offline' },
      (event) => received.push(event.type)
    );

    expect(transport.kind).toBe('tauri');
    expect(mocks.invoke).toHaveBeenCalledWith(
      'chat_start',
      expect.objectContaining({ streamId: handle.streamId, events: mocks.channels[0] })
    );
    mocks.channels[0].onmessage({ type: 'delta', text: 'hello' });
    mocks.channels[0].onmessage({ type: 'done' });
    await handle.done;
    expect(received).toEqual(['delta', 'done']);
  });

  it('routes cancellation using the exact stream identity', async () => {
    const handle = createTauriTransport().chat(
      { session: 'session-b', message: 'stop', provider: 'offline' },
      () => undefined
    );
    await handle.cancel();
    expect(mocks.invoke).toHaveBeenLastCalledWith('chat_cancel', {
      streamId: handle.streamId,
    });
  });

  it('normalizes Rust folder grants to the UI contract', async () => {
    mocks.invoke.mockResolvedValueOnce({
      ok: true,
      path: '/workspace',
      grant_token: 'grant-1',
      grant_expires_unix: 42,
    });
    await expect(createTauriTransport().pickFolder()).resolves.toEqual({
      ok: true,
      cancelled: undefined,
      path: '/workspace',
      grantToken: 'grant-1',
      grantExpiresUnix: 42,
    });
  });
});
