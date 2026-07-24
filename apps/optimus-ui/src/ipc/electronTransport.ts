import type {
  BrowserState,
  ChatEnvelope,
  ChatHandle,
  ChatRequest,
  DesktopMethod,
  OptimusTransport,
  StreamEvent,
} from './contracts';

export function createElectronTransport(): OptimusTransport {
  const bridge = window.optimusElectron;
  if (!bridge) throw new Error('Electron bridge unavailable');
  return {
    kind: 'electron',
    invoke<T>(method: DesktopMethod, params: Record<string, unknown> = {}) {
      return bridge.invoke<T>(method, params);
    },
    chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
      let streamId = 0;
      let unsubscribe: () => void = () => undefined;
      let cancelRequested = false;
      let resolveDone: () => void = () => undefined;
      let rejectDone: (reason?: unknown) => void = () => undefined;
      const pending: ChatEnvelope[] = [];
      const done = new Promise<void>((resolve, reject) => {
        resolveDone = resolve;
        rejectDone = reject;
      });
      const project = (envelope: ChatEnvelope) => {
        if (envelope.streamId !== streamId || envelope.sessionId !== request.session) return;
        onEvent(envelope.event);
        if (['done', 'error', 'cancelled'].includes(envelope.event.type)) {
          unsubscribe();
          resolveDone();
        }
      };
      unsubscribe = bridge.chat.subscribe((envelope) => {
        if (!streamId) {
          pending.push(envelope);
          return;
        }
        project(envelope);
      });
      void bridge.chat.start(request).then(async (started) => {
        streamId = started.streamId;
        for (const envelope of pending.splice(0)) project(envelope);
        if (cancelRequested) await bridge.chat.cancel(streamId);
      }).catch((error) => {
        unsubscribe();
        rejectDone(error);
      });
      return {
        get streamId() {
          return streamId;
        },
        done,
        cancel: async () => {
          cancelRequested = true;
          if (!streamId) {
            return { requested: true };
          }
          return bridge.chat.cancel(streamId);
        },
      };
    },
    windowAction(action) {
      return bridge.windowAction(action);
    },
    pickFolder() {
      return bridge.pickFolder();
    },
    openPath(path) {
      return bridge.openPath(path);
    },
    browser: {
      setBounds: (bounds) => bridge.browser.setBounds(bounds),
      setVisible: (visible) => bridge.browser.setVisible(visible),
      navigate: (url): Promise<BrowserState> => bridge.browser.navigate(url),
      back: (): Promise<BrowserState> => bridge.browser.back(),
      forward: (): Promise<BrowserState> => bridge.browser.forward(),
      reload: (): Promise<BrowserState> => bridge.browser.reload(),
      state: (): Promise<BrowserState> => bridge.browser.state(),
      annotate: () => bridge.browser.annotate(),
      cancelAnnotation: () => bridge.browser.cancelAnnotation(),
      subscribe: (listener) => bridge.browser.subscribe(listener),
    },
  };
}
