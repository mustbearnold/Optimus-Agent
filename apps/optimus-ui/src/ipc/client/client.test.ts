/**
 * Client seam tests (spec-001 A6, ADR-0090): the client is tested across
 * the SAME seam its callers use — a fake `OptimusTransport`. The pinned
 * `ipc/*.test.ts` suites (transports + broker selection) are untouched;
 * this file pins the client's own contract: exactly-one-terminal, R4/R5
 * folding, R9 classification, error typing, envelope unwrapping, and
 * fresh projections.
 */
import { describe, expect, it, vi } from 'vitest';
import type {
  ChatHandle,
  ChatRequest,
  OptimusTransport,
  StreamEvent,
  ToolApprovalBinding,
} from '../contracts';
import { createOptimusClient } from './client';
import {
  IpcError,
  NoTransportError,
  TurnInFlightError,
  type TurnOutcome,
} from './types';

/* ------------------------------------------------------------------ */
/* Fake transport                                                     */
/* ------------------------------------------------------------------ */

class FakeTransport implements OptimusTransport {
  readonly kind = 'fixture' as const;
  readonly calls: Array<{ method: string; params?: Record<string, unknown> }> = [];
  private readonly handlers = new Map<string, (params?: Record<string, unknown>) => unknown>();
  private readonly failing = new Set<string>();
  streamFactory:
    | ((request: ChatRequest, onEvent: (event: StreamEvent) => void) => ChatHandle)
    | null = null;

  windowAction = vi.fn();
  pickFolder = vi.fn();
  openPath = vi.fn();

  on(method: string, handler: (params?: Record<string, unknown>) => unknown): void {
    this.handlers.set(method, handler);
  }

  fail(method: string): void {
    this.failing.add(method);
  }

  chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
    if (!this.streamFactory) throw new Error('no streamFactory configured');
    return this.streamFactory(request, onEvent);
  }

  chatApprovalResolve(
    request: Parameters<OptimusTransport['chatApprovalResolve']>[0],
    onEvent: (event: StreamEvent) => void
  ): ChatHandle {
    if (!this.streamFactory) throw new Error('no streamFactory configured');
    return this.streamFactory(request as unknown as ChatRequest, onEvent);
  }

  async invoke<T>(method: string, params?: Record<string, unknown>): Promise<T> {
    this.calls.push({ method, params });
    if (this.failing.has(method)) throw new Error(`invoke ${method} failed`);
    const handler = this.handlers.get(method);
    if (handler) return handler(params) as T;
    return undefined as T;
  }
}

function terminalHandle(
  terminal: StreamEvent,
  before?: (onEvent: (event: StreamEvent) => void) => void
): ChatHandle {
  return {
    streamId: 1,
    done: Promise.resolve(terminal),
    cancel: async () => {
      before?.((event) => {
        void event;
      });
      return { requested: true };
    },
  };
}

function rejectingHandle(error: Error, onEvent: (event: StreamEvent) => void): ChatHandle {
  return {
    streamId: 1,
    done: Promise.reject(error),
    cancel: async () => ({ requested: true }),
    // eslint-disable-next-line no-console
    ...{ __onEvent: onEvent },
  };
}

const binding: ToolApprovalBinding = {
  run_id: 'run-1',
  call_id: 'call-1',
  tool_id: 'tool-1',
  job_id: 'job-1',
  node_id: 'node-1',
  node_index: 3,
  effect_sha256: 'a'.repeat(64),
  summary: 'run a command',
  command_class: 'run_command',
};

async function outcomeOf(promise: Promise<TurnOutcome>): Promise<TurnOutcome> {
  return promise;
}

/* ------------------------------------------------------------------ */
/* Terminal classification                                             */
/* ------------------------------------------------------------------ */

describe('turn terminal classification (R4/R9)', () => {
  it('classifies done as completed', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) => terminalHandle({ type: 'done' });
    const client = createOptimusClient(transport);
    const chat = client.chat('s1');
    const { outcome } = chat.send(
      { message: 'hi', provider: 'offline' },
      () => undefined
    );
    await expect(outcomeOf(outcome)).resolves.toEqual({ kind: 'completed' });
  });

  it('classifies cancelled with the error text', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) =>
      terminalHandle({ type: 'cancelled', error: 'cancelled by user' });
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined);
    await expect(outcomeOf(outcome)).resolves.toEqual({ kind: 'cancelled', error: 'cancelled by user' });
  });

  it('R4: resume_error is a failed continuation, not a transport loss', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) =>
      terminalHandle({ type: 'error', error: 'resume_error: continuation failed' });
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined);
    await expect(outcomeOf(outcome)).resolves.toEqual({
      kind: 'failed',
      message: 'resume_error: continuation failed',
    });
  });

  it('R4: still_pending is a re-parked approval', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) =>
      terminalHandle({ type: 'error', error: 'still_pending: approval re-parked' });
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined);
    await expect(outcomeOf(outcome)).resolves.toEqual({ kind: 'awaiting-approval' });
  });

  it('R9: connection-lost terminal is disconnected, not failed', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) =>
      terminalHandle({ type: 'error', error: 'connection lost' });
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined);
    await expect(outcomeOf(outcome)).resolves.toEqual({ kind: 'disconnected' });
  });

  it('R9: web socket closed unexpectedly is disconnected', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) =>
      terminalHandle({ type: 'error', error: 'web socket closed unexpectedly' });
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined);
    await expect(outcomeOf(outcome)).resolves.toEqual({ kind: 'disconnected' });
  });

  it('a rejected start is failed with the REAL cause and mirrored to the event stream', async () => {
    const transport = new FakeTransport();
    const events: StreamEvent[] = [];
    transport.streamFactory = (_request, onEvent) =>
      rejectingHandle(new Error('No DeepSeek API key configured'), onEvent);
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send(
      { message: 'hi', provider: 'deepseek' },
      (event) => events.push(event)
    );
    await expect(outcomeOf(outcome)).resolves.toEqual({
      kind: 'failed',
      message: 'No DeepSeek API key configured',
    });
    expect(events).toEqual([{ type: 'error', error: 'No DeepSeek API key configured' }]);
  });

  it('a turn without any terminal event is failed, not hung', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) =>
      ({ streamId: 1, done: Promise.resolve(undefined), cancel: async () => ({ requested: true }) });
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined);
    await expect(outcomeOf(outcome)).resolves.toMatchObject({ kind: 'failed' });
  });

  it('stream events flow to the caller exactly as delivered', async () => {
    const transport = new FakeTransport();
    const events: StreamEvent[] = [];
    transport.streamFactory = (_request, onEvent) => {
      onEvent({ type: 'delta', text: 'hel' });
      onEvent({ type: 'delta', text: 'lo' });
      onEvent({ type: 'status', text: 'working' });
      return terminalHandle({ type: 'done' });
    };
    const client = createOptimusClient(transport);
    const { outcome } = client.chat('s1').send(
      { message: 'hi', provider: 'offline' },
      (event) => events.push(event)
    );
    await outcomeOf(outcome);
    expect(events).toEqual([
      { type: 'delta', text: 'hel' },
      { type: 'delta', text: 'lo' },
      { type: 'status', text: 'working' },
    ]);
  });
});

/* ------------------------------------------------------------------ */
/* Send / approve / cancel lifecycle                                   */
/* ------------------------------------------------------------------ */

describe('ChatSession lifecycle', () => {
  it('maps SendOptions to the wire ChatRequest (snake_case, one place)', async () => {
    const transport = new FakeTransport();
    let seen: ChatRequest | null = null;
    transport.streamFactory = (request, _onEvent) => {
      seen = request;
      return terminalHandle({ type: 'done' });
    };
    const client = createOptimusClient(transport);
    await client.chat('s1').send(
      {
        message: 'hi',
        provider: 'codex',
        model: 'gpt-5',
        thinkingLevel: 'high',
        fast: true,
        access: 'public',
        projectId: 'p1',
      },
      () => undefined
    ).outcome;
    expect(seen).toEqual({
      session: 's1',
      message: 'hi',
      provider: 'codex',
      model: 'gpt-5',
      thinking_level: 'high',
      fast: true,
      access: 'public',
      project_id: 'p1',
    });
  });

  it('approve resolves through chatApprovalResolve with the binding mapped to the wire', async () => {
    const transport = new FakeTransport();
    let resolveRequest: Parameters<OptimusTransport['chatApprovalResolve']>[0] | null = null;
    transport.streamFactory = ((request: ChatRequest, _onEvent: (e: StreamEvent) => void) => {
      resolveRequest = request as unknown as Parameters<OptimusTransport['chatApprovalResolve']>[0];
      return terminalHandle({ type: 'done' });
    }) as FakeTransport['streamFactory'];
    const client = createOptimusClient(transport);
    await client.chat('s1').approve(binding, 'approve', 'p1').outcome;
    expect(resolveRequest).toEqual({
      session_id: 's1',
      run_id: 'run-1',
      call_id: 'call-1',
      job_id: 'job-1',
      node_id: 'node-1',
      node_index: 3,
      effect_sha256: 'a'.repeat(64),
      decision: 'approve',
      project_id: 'p1',
    });
  });

  it('a second send while one is live throws TurnInFlightError; next send works after terminal', async () => {
    const transport = new FakeTransport();
    const deferred: { resolve: ((event: StreamEvent) => void) | null } = { resolve: null };
    transport.streamFactory = (_request, _onEvent) => ({
      streamId: 1,
      done: new Promise((resolve) => {
        deferred.resolve = (event) => resolve(event);
      }),
      cancel: async () => ({ requested: true }),
    });
    const client = createOptimusClient(transport);
    const chat = client.chat('s1');
    const first = chat.send({ message: 'one', provider: 'offline' }, () => undefined);
    expect(() => chat.send({ message: 'two', provider: 'offline' }, () => undefined)).toThrow(
      TurnInFlightError
    );
    deferred.resolve?.({ type: 'done' });
    await expect(outcomeOf(first.outcome)).resolves.toEqual({ kind: 'completed' });
    expect(chat.busy).toBe(false);
    const second = chat.send({ message: 'three', provider: 'offline' }, () => undefined);
    deferred.resolve?.({ type: 'done' });
    await expect(outcomeOf(second.outcome)).resolves.toEqual({ kind: 'completed' });
  });

  it('cancel passes through while a turn is live and no-ops once the terminal lands', async () => {
    const transport = new FakeTransport();
    const cancel = vi.fn(async () => ({ requested: true }));
    const deferred: { resolve: ((event: StreamEvent) => void) | null } = { resolve: null };
    transport.streamFactory = (_request, _onEvent) => ({
      streamId: 1,
      done: new Promise((resolve) => {
        deferred.resolve = (event) => resolve(event);
      }),
      cancel,
    });
    const client = createOptimusClient(transport);
    const chat = client.chat('s1');
    const { outcome } = chat.send({ message: 'hi', provider: 'offline' }, () => undefined);
    await chat.cancel();
    await chat.cancel();
    expect(cancel).toHaveBeenCalledTimes(2);
    deferred.resolve?.({ type: 'cancelled', error: 'cancelled by user' });
    await expect(outcomeOf(outcome)).resolves.toEqual({ kind: 'cancelled', error: 'cancelled by user' });
    await chat.cancel();
    expect(cancel).toHaveBeenCalledTimes(2);
    expect(chat.busy).toBe(false);
  });
});

/* ------------------------------------------------------------------ */
/* Error typing and no-transport affordance                            */
/* ------------------------------------------------------------------ */

describe('error typing and the no-transport affordance', () => {
  it('every call rejects NoTransportError when the transport is null', async () => {
    const client = createOptimusClient(null);
    await expect(client.sessions.list()).rejects.toBeInstanceOf(NoTransportError);
    await expect(client.system.doctor()).rejects.toBeInstanceOf(NoTransportError);
    await expect(client.cron.list()).rejects.toBeInstanceOf(NoTransportError);
    expect(() => client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined)).toThrow(
      NoTransportError
    );
  });

  it('invoke failures surface as IpcError with the real message', async () => {
    const transport = new FakeTransport();
    transport.fail('cron_list');
    const client = createOptimusClient(transport);
    await expect(client.cron.list()).rejects.toBeInstanceOf(IpcError);
    await expect(client.cron.list()).rejects.toThrow('invoke cron_list failed');
  });
});

/* ------------------------------------------------------------------ */
/* Envelope unwrapping and fresh projections                           */
/* ------------------------------------------------------------------ */

describe('envelope unwrapping and fresh projections', () => {
  it('sessions.list unwraps boxed, array, and absent envelopes', async () => {
    const transport = new FakeTransport();
    const client = createOptimusClient(transport);
    transport.on('sessions', () => ({ sessions: [{ id: 'a' }] }));
    await expect(client.sessions.list()).resolves.toEqual([{ id: 'a' }]);
    transport.on('sessions', () => [{ id: 'b' }]);
    await expect(client.sessions.list()).resolves.toEqual([{ id: 'b' }]);
    transport.on('sessions', () => null);
    await expect(client.sessions.list()).resolves.toEqual([]);
  });

  it('cron.add uses the fresh projection when the host returns one', async () => {
    const transport = new FakeTransport();
    const client = createOptimusClient(transport);
    transport.on('cron_add', () => ({ jobs: [{ id: 'j1' }] }));
    await expect(client.cron.add({ name: 'x', every_secs: 60, prompt: 'p' })).resolves.toEqual([
      { id: 'j1' },
    ]);
    expect(transport.calls.filter((c) => c.method === 'cron_list')).toHaveLength(0);
  });

  it('cron.add re-fetches the list when the host returns no projection', async () => {
    const transport = new FakeTransport();
    const client = createOptimusClient(transport);
    transport.on('cron_add', () => undefined);
    transport.on('cron_list', () => ({ jobs: [{ id: 'j2' }] }));
    await expect(client.cron.add({ name: 'x', every_secs: 60, prompt: 'p' })).resolves.toEqual([
      { id: 'j2' },
    ]);
    const methods = transport.calls.map((c) => c.method);
    expect(methods).toEqual(['cron_add', 'cron_list']);
  });

  it('approvals.list unwraps pending', async () => {
    const transport = new FakeTransport();
    const client = createOptimusClient(transport);
    transport.on('approvals_list', () => ({ pending: [{ run_id: 'r1' }] }));
    await expect(client.approvals.list()).resolves.toEqual([{ run_id: 'r1' }]);
  });
});

/* ------------------------------------------------------------------ */
/* Observability (law 11)                                              */
/* ------------------------------------------------------------------ */

describe('RuntimeObserver (law 11)', () => {
  it('records invokes and streams in arrival order', async () => {
    const transport = new FakeTransport();
    transport.streamFactory = (_request, _onEvent) => terminalHandle({ type: 'done' });
    const client = createOptimusClient(transport);
    transport.on('sessions', () => ({ sessions: [] }));
    await client.sessions.list();
    await client.chat('s1').send({ message: 'hi', provider: 'offline' }, () => undefined).outcome;
    const events = client.observer.tail(10);
    expect(events.map((e) => (e.type === 'invoke' ? e.method : `stream:${e.method}`))).toEqual([
      'sessions',
      'stream:chat_start',
    ]);
    const live: string[] = [];
    const unsubscribe = client.observer.subscribe((event) => live.push(event.type));
    await client.jobs.list();
    expect(live).toEqual(['invoke']);
    unsubscribe();
  });
});
