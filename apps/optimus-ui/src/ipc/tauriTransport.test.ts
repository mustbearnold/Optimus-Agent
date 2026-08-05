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

  it('streams approval resolution and cancels by the same stream identity', async () => {
    const received: string[] = [];
    const transport = createTauriTransport();
    const request = {
      session_id: 'session-a',
      run_id: 'run-1',
      call_id: 'call-1',
      job_id: 'job-1',
      node_id: 'node-1',
      node_index: 0,
      effect_sha256: 'a'.repeat(64),
      decision: 'approve' as const,
    };
    const handle = transport.chatApprovalResolve(request, (event) =>
      received.push(event.type)
    );

    expect(mocks.invoke).toHaveBeenCalledWith(
      'chat_approval_resolve_start',
      expect.objectContaining({
        streamId: handle.streamId,
        params: request,
        events: mocks.channels[0],
      })
    );
    mocks.channels[0].onmessage({ type: 'status', text: 'Resolving approval…' });
    mocks.channels[0].onmessage({ type: 'done' });
    await handle.done;
    expect(received).toEqual(['status', 'done']);

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
