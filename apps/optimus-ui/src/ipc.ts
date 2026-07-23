/**
 * Typed-ish IPC client for the Rust host HTTP surface.
 * Contract: docs/contracts/desktop-ipc-methods.md
 */

export type HostInfo = { baseUrl: string; token: string };

export type StreamEvent = {
  type: string;
  [key: string]: unknown;
};

function parseQueryHost(): Partial<HostInfo> {
  const q = new URLSearchParams(window.location.search);
  const baseUrl = q.get('host') || '';
  const token = q.get('token') || '';
  return {
    baseUrl: baseUrl || undefined,
    token: token || undefined,
  };
}

async function resolveHost(): Promise<HostInfo> {
  const q = parseQueryHost();
  if (q.baseUrl && q.token) {
    return { baseUrl: q.baseUrl, token: q.token };
  }
  const electron = (window as unknown as { optimusElectron?: { hostInfo: () => Promise<HostInfo> } })
    .optimusElectron;
  if (electron?.hostInfo) {
    const info = await electron.hostInfo();
    return { baseUrl: info.baseUrl, token: info.token };
  }
  // Dev fallback: assume host-only default
  const token =
    (window as unknown as { __OPTIMUS_HTTP_TOKEN__?: string }).__OPTIMUS_HTTP_TOKEN__ ||
    '';
  return {
    baseUrl: 'http://127.0.0.1:17865',
    token,
  };
}

let cached: HostInfo | null = null;
let idSeq = 1;

export async function getHost(): Promise<HostInfo> {
  if (!cached) cached = await resolveHost();
  return cached;
}

export async function invoke<T = unknown>(
  method: string,
  params: Record<string, unknown> = {}
): Promise<T> {
  const host = await getHost();
  const id = idSeq++;
  const res = await fetch(`${host.baseUrl}/api/ipc`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${host.token}`,
      'X-Optimus-CSRF': '1',
      Origin: host.baseUrl,
    },
    body: JSON.stringify({ id, method, params }),
  });
  const body = (await res.json()) as {
    ok?: boolean;
    result?: T;
    error?: string;
  };
  if (!res.ok || body.ok === false) {
    throw new Error(body.error || `ipc ${method} failed (${res.status})`);
  }
  return body.result as T;
}

export async function chatStream(
  params: Record<string, unknown>,
  onEvent: (ev: StreamEvent) => void
): Promise<void> {
  const host = await getHost();
  const res = await fetch(`${host.baseUrl}/api/chat/stream`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${host.token}`,
      'X-Optimus-CSRF': '1',
      Origin: host.baseUrl,
      Accept: 'text/event-stream',
    },
    body: JSON.stringify(params),
  });
  if (!res.ok || !res.body) {
    throw new Error(`chat stream failed (${res.status})`);
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = '';
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const chunks = buf.split('\n\n');
    buf = chunks.pop() || '';
    for (const chunk of chunks) {
      const line = chunk
        .split('\n')
        .filter((l) => l.startsWith('data:'))
        .map((l) => l.slice(5).trim())
        .join('');
      if (!line) continue;
      try {
        onEvent(JSON.parse(line) as StreamEvent);
      } catch {
        /* ignore malformed */
      }
    }
  }
}

export async function windowAction(action: 'minimize' | 'maximize' | 'close') {
  const el = (window as unknown as {
    optimusElectron?: { windowAction: (a: string) => Promise<unknown> };
  }).optimusElectron;
  if (el?.windowAction) return el.windowAction(action);
  return invoke(`window_${action}`, {});
}
