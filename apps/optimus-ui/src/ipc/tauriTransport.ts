import { Channel, invoke } from '@tauri-apps/api/core';

import type {
  ChatHandle,
  ChatRequest,
  DesktopMethod,
  OptimusTransport,
  ProjectRootSelection,
  StreamEvent,
} from './contracts';

let nextStreamId = 1;

export function createTauriTransport(): OptimusTransport {
  return {
    kind: 'tauri',
    invoke<T>(method: DesktopMethod, params: Record<string, unknown> = {}) {
      return invoke<T>('host_invoke', { method, params });
    },
    chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
      const streamId = nextStreamId++;
      const events = new Channel<StreamEvent>();
      let terminal = false;
      let resolveDone: () => void = () => undefined;
      let rejectDone: (reason?: unknown) => void = () => undefined;
      const done = new Promise<void>((resolve, reject) => {
        resolveDone = resolve;
        rejectDone = reject;
      });
      events.onmessage = (event) => {
        onEvent(event);
        if (event.type === 'done' || event.type === 'error' || event.type === 'cancelled') {
          terminal = true;
          resolveDone();
        }
      };
      void invoke('chat_start', { streamId, request, events }).catch((error) => {
        if (!terminal) rejectDone(error);
      });
      return {
        streamId,
        done,
        cancel: async () => invoke<{ requested: boolean }>('chat_cancel', { streamId }),
      };
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
