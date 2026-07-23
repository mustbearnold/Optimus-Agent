import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ChatEnvelope, OptimusElectronBridge } from './contracts';
import { createElectronTransport } from './electronTransport';

afterEach(() => {
  delete window.optimusElectron;
});

describe('electron transport', () => {
  it('buffers stream events that arrive before chat.start resolves', async () => {
    let listener: (envelope: ChatEnvelope) => void = () => undefined;
    let resolveStart: (value: { streamId: number }) => void = () => undefined;
    const start = new Promise<{ streamId: number }>((resolve) => {
      resolveStart = resolve;
    });
    window.optimusElectron = bridge({
      start: () => start,
      subscribe: (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const events: string[] = [];
    const handle = createElectronTransport().chat(
      { session: 'session-a', message: 'go', provider: 'offline' },
      (event) => events.push(event.type === 'delta' ? event.text : event.type)
    );

    listener({ streamId: 42, sessionId: 'session-a', event: { type: 'delta', text: 'early' } });
    listener({ streamId: 42, sessionId: 'session-a', event: { type: 'done' } });
    resolveStart({ streamId: 42 });
    await handle.done;

    expect(events).toEqual(['early', 'done']);
  });

  it('routes only matching session and stream events', async () => {
    let listener: (envelope: ChatEnvelope) => void = () => undefined;
    window.optimusElectron = bridge({
      start: async () => ({ streamId: 7 }),
      subscribe: (next) => {
        listener = next;
        return () => undefined;
      },
    });
    const events: string[] = [];
    const handle = createElectronTransport().chat(
      { session: 'owner', message: 'go', provider: 'offline' },
      (event) => events.push(event.type)
    );
    await Promise.resolve();
    listener({ streamId: 7, sessionId: 'other', event: { type: 'delta', text: 'wrong' } });
    listener({ streamId: 8, sessionId: 'owner', event: { type: 'done' } });
    listener({ streamId: 7, sessionId: 'owner', event: { type: 'done' } });
    await handle.done;
    expect(events).toEqual(['done']);
  });

  it('honors cancellation requested before the stream id arrives', async () => {
    let resolveStart: (value: { streamId: number }) => void = () => undefined;
    const cancel = vi.fn(async () => ({ requested: true }));
    window.optimusElectron = bridge({
      start: () =>
        new Promise((resolve) => {
          resolveStart = resolve;
        }),
      cancel,
    });
    const handle = createElectronTransport().chat(
      { session: 'owner', message: 'go', provider: 'offline' },
      () => undefined
    );
    await handle.cancel();
    resolveStart({ streamId: 19 });
    await vi.waitFor(() => expect(cancel).toHaveBeenCalledWith(19));
  });
});

function bridge(
  chat: Partial<OptimusElectronBridge['chat']>
): OptimusElectronBridge {
  return {
    isElectron: true,
    hostInfo: async () => ({ baseUrl: 'http://127.0.0.1:1' }),
    invoke: async () => ({}) as never,
    chat: {
      start: async () => ({ streamId: 1 }),
      cancel: async () => ({ requested: true }),
      subscribe: () => () => undefined,
      ...chat,
    },
    browser: {
      setBounds: () => undefined,
      setVisible: () => undefined,
      navigate: async () => browserState,
      back: async () => browserState,
      forward: async () => browserState,
      reload: async () => browserState,
      state: async () => browserState,
      subscribe: () => () => undefined,
    },
    windowAction: async () => ({}),
    pickFolder: async () => ({ ok: false }),
    openPath: async () => ({}),
    openUrl: async () => ({}),
  };
}

const browserState = {
  url: 'about:blank',
  title: 'Preview',
  loading: false,
  canGoBack: false,
  canGoForward: false,
  visible: false,
  native: true,
};
