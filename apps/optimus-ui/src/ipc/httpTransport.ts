import type {
  ApprovalResolveRequest,
  ChatHandle,
  ChatRequest,
  DesktopMethod,
  OptimusTransport,
  StreamEvent,
} from './contracts';

type HttpConfig = { baseUrl: string; token: string };

function configFromQuery(): HttpConfig | null {
  const query = new URLSearchParams(location.search);
  const baseUrl = query.get('host');
  const token = query.get('token');
  if (!baseUrl || !token) return null;
  return { baseUrl: baseUrl.replace(/\/$/, ''), token };
}

export function hasHttpConfig() {
  return configFromQuery() !== null;
}

export function createHttpTransport(): OptimusTransport & {
  legacyInvoke<T>(method: string, params?: Record<string, unknown>): Promise<T>;
} {
  const config = configFromQuery();
  if (!config) throw new Error('HTTP host pairing unavailable');
  let requestId = 1;
  let streamId = 1;
  const headers = () => ({
    Authorization: `Bearer ${config.token}`,
    'X-Optimus-CSRF': '1',
    'Content-Type': 'application/json',
  });

  return {
    kind: 'http',
    async invoke<T>(method: DesktopMethod, params: Record<string, unknown> = {}) {
      return this.legacyInvoke<T>(method as string, params);
    },
    /**
     * Named typed legacy shim (spec-015 A3): a STRING-typed invoke path for
     * the superseded `chat_approval_resolve` member only (removed from the
     * DesktopMethod union; exempted in the surface-contract gate as the
     * HTTP-legacy bucket). Everything else dispatches through the typed
     * union. The HTTP host is dev-only and has no streaming resolve
     * endpoint, so this is the blocking IPC round trip.
     */
    async legacyInvoke<T>(
      method: string,
      params: Record<string, unknown> = {}
    ): Promise<T> {
      const response = await fetch(`${config.baseUrl}/api/ipc`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify({ id: requestId++, method, params }),
      });
      const body = (await response.json()) as {
        ok?: boolean;
        result?: T;
        error?: string;
      };
      if (!response.ok || body.ok === false) {
        throw new Error(body.error || `IPC ${method} failed (${response.status})`);
      }
      return body.result as T;
    },
    chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
      const id = streamId++;
      const controller = new AbortController();
      const done = (async (): Promise<StreamEvent | undefined> => {
        const response = await fetch(`${config.baseUrl}/api/chat/stream`, {
          method: 'POST',
          headers: { ...headers(), Accept: 'text/event-stream' },
          body: JSON.stringify(request),
          signal: controller.signal,
        });
        if (!response.ok || !response.body) {
          throw new Error(`Chat stream failed (${response.status})`);
        }
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let terminal: StreamEvent | undefined;
        while (true) {
          const { done: ended, value } = await reader.read();
          if (ended) break;
          buffer += decoder.decode(value, { stream: true });
          const blocks = buffer.split('\n\n');
          buffer = blocks.pop() || '';
          for (const block of blocks) {
            const data = block
              .split('\n')
              .filter((line) => line.startsWith('data:'))
              .map((line) => line.slice(5).trim())
              .join('');
            if (!data) continue;
            try {
              const event = JSON.parse(data) as StreamEvent;
              onEvent(event);
              if (event.type === 'done' || event.type === 'cancelled' || event.type === 'error') {
                terminal = event;
              }
            } catch {
              onEvent({ type: 'error', error: 'Malformed stream event' });
            }
          }
        }
        return terminal;
      })();
      return {
        streamId: id,
        done,
        cancel: async () => {
          if (!controller.signal.aborted) controller.abort();
          return { requested: true };
        },
      };
    },
    // The HTTP host has no streaming resolve endpoint (the vanilla UI it serves
    // renders no approval cards), so this is the blocking IPC round trip. The
    // product desktop shell uses the Tauri streaming path instead. The
    // superseded member is reached through the named legacy shim (A3).
    chatApprovalResolve(
      request: ApprovalResolveRequest,
      onEvent: (event: StreamEvent) => void
    ): ChatHandle {
      const id = streamId++;
      const done = (async (): Promise<StreamEvent | undefined> => {
        const result = await this.legacyInvoke<{ status: string }>(
          'chat_approval_resolve',
          { ...request }
        );
        const event: StreamEvent = { type: 'done', result };
        onEvent(event);
        return event;
      })();
      return {
        streamId: id,
        done,
        cancel: async () => ({ requested: false }),
      };
    },
    windowAction: async () => ({ ok: true }),
    pickFolder: async () => ({ ok: false, cancelled: true }),
    openPath: async () => ({ ok: false }),
  };
}
