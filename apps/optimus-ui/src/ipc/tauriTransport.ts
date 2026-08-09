import { Channel, invoke } from '@tauri-apps/api/core';

import type {
  ApprovalResolveRequest,
  ChatHandle,
  ChatRequest,
  DesktopMethod,
  OptimusTransport,
  ProjectRootSelection,
  StreamEvent,
} from './contracts';

let nextStreamId = 1;

/** Open a Tauri event-channel stream and settle `done` on its terminal event. */
function openStream(
  method: 'chat_start' | 'chat_approval_resolve_start',
  payload: Record<string, unknown>,
  onEvent: (event: StreamEvent) => void
): ChatHandle {
  const streamId = nextStreamId++;
  const events = new Channel<StreamEvent>();
  let terminal = false;
  let resolveDone: (event?: StreamEvent) => void = () => undefined;
  let rejectDone: (reason?: unknown) => void = () => undefined;
  const done = new Promise<StreamEvent | undefined>((resolve, reject) => {
    resolveDone = resolve;
    rejectDone = reject;
  });
  events.onmessage = (event) => {
    onEvent(event);
    if (event.type === 'done' || event.type === 'error' || event.type === 'cancelled') {
      terminal = true;
      // R4: resolve with the terminal payload so callers can branch on
      // `result.resume_error` / `result.still_pending`.
      resolveDone(event);
    }
  };
  void invoke(method, { streamId, ...payload, events }).catch((error) => {
    if (!terminal) rejectDone(error);
  });
  return {
    streamId,
    done,
    cancel: async () => invoke<{ requested: boolean }>('chat_cancel', { streamId }),
  };
}

export function createTauriTransport(): OptimusTransport {
  return {
    kind: 'tauri',
    invoke<T>(method: DesktopMethod, params: Record<string, unknown> = {}) {
      return invoke<T>('host_invoke', { method, params });
    },
    chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
      return openStream('chat_start', { request }, onEvent);
    },
    // Settling an approval resumes the paused turn (ADR-0046), so this is a
    // streaming turn like chat — the continuation must reach the workbench as
    // it happens, and the handle must be cancellable, or the button stays on
    // "Approving…" with no feedback for the whole continuation.
    chatApprovalResolve(
      request: ApprovalResolveRequest,
      onEvent: (event: StreamEvent) => void
    ): ChatHandle {
      return openStream('chat_approval_resolve_start', { params: request }, onEvent);
    },
    windowAction(action) {
      return invoke('window_action', { action });
    },
    async pickFolder() {
      const result = await invoke<{
        ok: boolean;
        cancelled?: boolean;
        path?: string;
        grant_token?: string;
        grant_expires_unix?: number;
      }>('pick_folder');
      return {
        ok: result.ok,
        cancelled: result.cancelled,
        path: result.path,
        grantToken: result.grant_token,
        grantExpiresUnix: result.grant_expires_unix,
      } satisfies ProjectRootSelection;
    },
    openPath(path) {
      return invoke('host_invoke', { method: 'open_path', params: { path } });
    },
  };
}
