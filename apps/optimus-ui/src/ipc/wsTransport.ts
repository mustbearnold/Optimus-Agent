import type {
  ApprovalResolveRequest,
  ChatHandle,
  ChatRequest,
  DesktopMethod,
  OptimusTransport,
  ProjectRootSelection,
  StreamEvent,
} from './contracts';

/**
 * WebSocket carrier transport (spec-015 A3): the renderer speaks the
 * surface protocol directly to `optimus serve` (JSON-RPC 2.0 over
 * loopback WS) instead of the in-process Tauri bridge. Selected only when
 * a broker ticket global is present (transport auto-detect, spec-001 R8).
 *
 * Behavior contract (spec-014 coupling): reproduces `tauriTransport`'s
 * terminal-event handling seams — exactly one terminal per stream,
 * `done` settles only on the terminal event, the cancel handle is the
 * control-plane `chat_cancel` round trip, and an unexpected close
 * synthesizes a terminal `error` for every open stream and rejects every
 * pending invoke (R9).
 */

export type BrokerTicket = { port: number; ticket: string };

const PROTOCOL_VERSION = 1;

export function createWsTransport(broker: BrokerTicket): OptimusTransport {
  return new WsTransport(broker);
}

class WsTransport {
  readonly kind = 'ws' as const;
  private readonly url: string;
  private readonly ticket: string;
  private socket: WebSocket | null = null;
  // id 1 belongs to the hello handshake (the readiness reply); every
  // invoke and stream starts at 2 so replies can never collide with it.
  private nextRequestId = 2;
  private nextStreamId = 1;
  private pending = new Map<number, { resolve: (value: unknown) => void; reject: (reason: Error) => void }>();
  private streams = new Map<number, (event: StreamEvent) => void>();
  private ready: Promise<void>;
  private closed = false;

  constructor(broker: BrokerTicket) {
    this.url = `ws://127.0.0.1:${broker.port}/ws`;
    this.ticket = broker.ticket;
    this.ready = this.connect();
    // The handshake failure surfaces to every await of `ready` (invokes and
    // streams) — the sink here only keeps an early failure from being an
    // unhandled rejection while nothing has awaited yet.
    this.ready.catch(() => undefined);
  }

  private connect(): Promise<void> {
    return new Promise((resolveReady, rejectReady) => {
      const socket = new WebSocket(this.url);
      this.socket = socket;
      socket.onopen = () => {
        this.send({
          jsonrpc: '2.0',
          id: 1,
          method: 'hello',
          params: {
            protocol_version: PROTOCOL_VERSION,
            client_kind: 'renderer',
            ticket: this.ticket,
          },
        });
      };
      socket.onmessage = (message) => {
        let value: unknown;
        try {
          value = JSON.parse(String(message.data));
        } catch {
          return;
        }
        this.handleFrame(value as Record<string, unknown>);
      };
      socket.onerror = () => {
        if (!this.closed) {
          this.failEverything(new Error('web socket connection failed'));
          rejectReady(new Error('web socket connection failed'));
        }
      };
      socket.onclose = () => {
        if (!this.closed) {
          this.failEverything(new Error('web socket closed unexpectedly'));
        }
      };
      // The hello result (id 1) resolves readiness; the host.ready
      // notification may arrive before or after it — readiness means the
      // handshake completed, which the hello reply proves.
      this.helloResolve = resolveReady;
      this.helloReject = rejectReady;
    });
  }

  private helloResolve: (() => void) | null = null;
  private helloReject: ((reason: Error) => void) | null = null;

  private handleFrame(frame: Record<string, unknown>) {
    if (frame.method === 'event') {
      const params = (frame.params ?? {}) as Record<string, unknown>;
      const streamId = Number(params.stream_id);
      const event = params.event as StreamEvent;
      const onEvent = this.streams.get(streamId);
      if (onEvent && event) onEvent(event);
      return;
    }
    const id = Number(frame.id);
    if (id === 1) {
      if (frame.error) {
        this.helloReject?.(new Error(String(frame.error)));
      } else {
        this.helloResolve?.();
      }
      return;
    }
    const pending = this.pending.get(id);
    if (!pending) return;
    this.pending.delete(id);
    if (frame.error) {
      const error = (frame.error ?? {}) as Record<string, unknown>;
      pending.reject(new Error(String(error.message ?? 'ipc error')));
    } else {
      pending.resolve(frame.result);
    }
  }

  private send(payload: unknown) {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      throw new Error('web socket not connected');
    }
    socket.send(JSON.stringify(payload));
  }

  private failEverything(error: Error) {
    if (this.closed) return;
    this.closed = true;
    for (const { reject } of this.pending.values()) reject(error);
    this.pending.clear();
    // Synthetic terminal error for every open stream (R9): a consumer
    // that never sees a terminal would hang its run state forever.
    for (const [streamId, onEvent] of this.streams) {
      onEvent({ type: 'error', error: 'connection lost' });
      this.streams.delete(streamId);
    }
  }

  private async invokeRaw<T>(method: string, params: Record<string, unknown>): Promise<T> {
    await this.ready;
    const id = this.nextRequestId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
      });
      try {
        this.send({ jsonrpc: '2.0', id, method, params });
      } catch (error) {
        this.pending.delete(id);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  invoke<T>(method: DesktopMethod, params: Record<string, unknown> = {}): Promise<T> {
    return this.invokeRaw<T>(method, params);
  }

  private openStream(
    method: 'chat_start' | 'chat_approval_resolve_start',
    payload: Record<string, unknown>,
    onEvent: (event: StreamEvent) => void
  ): ChatHandle {
    const streamId = this.nextStreamId++;
    let terminal = false;
    let resolveDone: () => void = () => undefined;
    let rejectDone: (reason?: Error) => void = () => undefined;
    const done = new Promise<void>((resolve, reject) => {
      resolveDone = resolve;
      rejectDone = reject;
    });
    this.streams.set(streamId, (event) => {
      onEvent(event);
      if (event.type === 'done' || event.type === 'error' || event.type === 'cancelled') {
        terminal = true;
        this.streams.delete(streamId);
        resolveDone();
      }
    });
    this.invokeRaw<{ stream_id: number }>(method, { stream_id: streamId, ...payload }).catch(
      (error: Error) => {
        if (!terminal) {
          this.streams.delete(streamId);
          rejectDone(error);
        }
      }
    );
    return {
      streamId,
      done,
      cancel: async () => {
        const result = await this.invokeRaw<{ requested: boolean }>('chat_cancel', {
          stream_id: streamId,
        });
        return result;
      },
    };
  }

  chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
    return this.openStream('chat_start', { request }, onEvent);
  }

  chatApprovalResolve(
    request: ApprovalResolveRequest,
    onEvent: (event: StreamEvent) => void
  ): ChatHandle {
    return this.openStream('chat_approval_resolve_start', { params: request }, onEvent);
  }

  // OS affordances are Tauri-bridge-only; over the wire they do not exist
  // (mirrors the HTTP transport's graceful degradation).
  windowAction(): Promise<unknown> {
    return Promise.resolve({ ok: false });
  }

  pickFolder(): Promise<ProjectRootSelection> {
    return Promise.resolve({ ok: false, cancelled: true });
  }

  openPath(): Promise<unknown> {
    return Promise.resolve({ ok: false });
  }
}
