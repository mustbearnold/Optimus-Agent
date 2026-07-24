import { useSyncExternalStore } from 'react';
import type { Message, RunStatus, SessionDetail, StreamEvent, ToolActivity } from '../ipc/contracts';
import { frameCoordinator } from '../performance/frameCoordinator';

type SessionProjection = {
  messages: Message[];
  status: RunStatus;
  statusText: string;
  durationMs?: number;
  loaded: boolean;
};

export type SessionIndicatorState = 'working' | 'attention' | 'error';

const emptyProjection = (): SessionProjection => ({
  messages: [],
  status: 'idle',
  statusText: '',
  loaded: false,
});

class ConversationStore {
  private readonly sessions = new Map<string, SessionProjection>();
  private readonly listeners = new Map<string, Set<() => void>>();
  private readonly allListeners = new Set<() => void>();
  private readonly streamText = new Map<string, string>();
  private readonly sessionVersions = new Map<string, number>();
  private allVersion = 0;

  get(sessionId: string | null): SessionProjection {
    if (!sessionId) return emptyProjection();
    return this.sessions.get(sessionId) || emptyProjection();
  }

  version(sessionId: string | null) {
    if (!sessionId) return 0;
    return this.sessionVersions.get(sessionId) || 0;
  }

  subscribe(sessionId: string | null, listener: () => void) {
    if (!sessionId) return () => undefined;
    const listeners = this.listeners.get(sessionId) || new Set();
    listeners.add(listener);
    this.listeners.set(sessionId, listeners);
    return () => {
      listeners.delete(listener);
      if (!listeners.size) this.listeners.delete(sessionId);
    };
  }

  subscribeAll(listener: () => void) {
    this.allListeners.add(listener);
    return () => {
      this.allListeners.delete(listener);
    };
  }

  versionAll() {
    return this.allVersion;
  }

  indicator(sessionId: string): SessionIndicatorState | null {
    const status = this.sessions.get(sessionId)?.status;
    if (status === 'submitting' || status === 'working') return 'working';
    if (status === 'awaiting_approval') return 'attention';
    if (status === 'failed' || status === 'disconnected') return 'error';
    return null;
  }

  load(detail: SessionDetail) {
    const messages: Message[] = detail.messages.map((message, index) => ({
      id: `${detail.id}:persisted:${index}`,
      role: message.role,
      content: message.content,
      status: 'completed',
    }));
    this.sessions.set(detail.id, {
      messages,
      status: 'idle',
      statusText: '',
      loaded: true,
    });
    this.emit(detail.id);
  }

  begin(sessionId: string, userText: string) {
    const current = this.sessions.get(sessionId) || emptyProjection();
    const stamp = `${Date.now()}:${Math.random().toString(36).slice(2)}`;
    this.sessions.set(sessionId, {
      ...current,
      loaded: true,
      status: 'submitting',
      statusText: 'Submitting…',
      messages: [
        ...current.messages,
        {
          id: `${sessionId}:user:${stamp}`,
          role: 'user',
          content: userText,
          status: 'completed',
        },
        {
          id: `${sessionId}:assistant:${stamp}`,
          role: 'assistant',
          content: '',
          status: 'working',
          tools: [],
        },
      ],
    });
    this.streamText.set(sessionId, '');
    this.emit(sessionId);
  }

  apply(sessionId: string, event: StreamEvent) {
    const current = this.sessions.get(sessionId);
    if (!current) return;
    if (event.type === 'delta') {
      this.streamText.set(sessionId, (this.streamText.get(sessionId) || '') + event.text);
      frameCoordinator.scheduleKeyed('content', `stream:${sessionId}`, () =>
        this.flushText(sessionId)
      );
      return;
    }
    if (event.type === 'tool') {
      const messages = current.messages.slice();
      const messageIndex = findLastAssistantIndex(messages);
      if (messageIndex < 0) return;
      const message = messages[messageIndex]!;
      const tools = [...(message.tools || [])];
      const openToolIndex = findLastOpenToolIndex(tools, event.name);
      if (openToolIndex >= 0 && event.detail !== 'running') {
        const tool = tools[openToolIndex]!;
        tools[openToolIndex] = {
          ...tool,
          detail: event.detail,
          status: toolResultStatus(event.detail),
        };
      } else if (openToolIndex >= 0) {
        tools[openToolIndex] = { ...tools[openToolIndex]!, detail: event.detail };
      } else {
        const priorToolCount = current.messages.reduce(
          (count, candidate) => count + (candidate.tools?.length || 0),
          0
        );
        tools.push({
          id: `${message.id}:tool:${priorToolCount}`,
          name: event.name,
          detail: event.detail,
          status: 'running',
        });
      }
      messages[messageIndex] = {
        ...message,
        tools: tools.slice(-200),
      };
      this.sessions.set(sessionId, {
        ...current,
        status: 'working',
        statusText: event.detail || `${event.name}…`,
        messages,
      });
    } else if (event.type === 'status') {
      const needsAttention =
        /\b(?:approval|permission|question|confirmation)\b|\b(?:input|answer|choice)\s+(?:required|needed|requested)\b|\bawaiting\s+(?:input|answer|choice)\b/i.test(
          event.text
        );
      this.sessions.set(sessionId, {
        ...current,
        status: needsAttention ? 'awaiting_approval' : 'working',
        statusText: event.text,
      });
    } else if (event.type === 'timing') {
      this.sessions.set(sessionId, {
        ...current,
        durationMs:
          typeof event.elapsed_ms === 'number' ? event.elapsed_ms : current.durationMs,
      });
    } else if (event.type === 'done') {
      this.flushText(sessionId);
      this.setTerminal(sessionId, 'completed', 'Completed');
      return;
    } else if (event.type === 'cancelled') {
      this.flushText(sessionId);
      this.setTerminal(sessionId, 'cancelled', 'Cancelled · partial response retained');
      return;
    } else if (event.type === 'error') {
      this.flushText(sessionId);
      const status: RunStatus = /abort|cancel/i.test(event.error) ? 'cancelled' : 'failed';
      this.setTerminal(
        sessionId,
        status,
        status === 'cancelled' ? 'Cancelled · partial response retained' : event.error
      );
      return;
    }
    this.emit(sessionId);
  }

  markCancelling(sessionId: string) {
    const current = this.sessions.get(sessionId);
    if (
      !current ||
      ['completed', 'cancelled', 'failed', 'disconnected'].includes(current.status)
    ) {
      return;
    }
    this.sessions.set(sessionId, {
      ...current,
      status: 'cancelling',
      statusText: 'Cancellation requested…',
    });
    this.emit(sessionId);
  }

  markDisconnected(sessionId: string) {
    this.flushText(sessionId);
    this.setTerminal(sessionId, 'disconnected', 'Connection lost · cancellation requested');
  }

  private flushText(sessionId: string) {
    // A terminal event deletes the active buffer. A previously scheduled frame
    // must never replay an empty historical snapshot over immutable final text.
    if (!this.streamText.has(sessionId)) return;
    const current = this.sessions.get(sessionId);
    if (!current) return;
    const content = this.streamText.get(sessionId) || '';
    const messages = current.messages.slice();
    let index = -1;
    for (let cursor = messages.length - 1; cursor >= 0; cursor -= 1) {
      if (messages[cursor]?.role === 'assistant') {
        index = cursor;
        break;
      }
    }
    if (index >= 0 && messages[index]?.content !== content) {
      messages[index] = { ...messages[index], content, status: current.status };
      this.sessions.set(sessionId, {
        ...current,
        messages,
        status: current.status === 'submitting' ? 'working' : current.status,
        statusText: current.statusText || 'Optimus is working…',
      });
      this.emit(sessionId);
    }
  }

  private setTerminal(sessionId: string, status: RunStatus, statusText: string) {
    const current = this.sessions.get(sessionId);
    if (
      !current ||
      ['completed', 'cancelled', 'failed', 'disconnected'].includes(current.status)
    ) {
      return;
    }
    const messages = current.messages.map((message, index, all) => {
      if (index !== all.length - 1 || message.role !== 'assistant') return message;
      return {
        ...message,
        status,
        tools: message.tools?.map((tool) =>
          tool.status === 'running'
            ? {
                ...tool,
                status:
                  status === 'failed' || status === 'disconnected'
                    ? ('failed' as const)
                    : ('completed' as const),
              }
            : tool
        ),
        ...(typeof current.durationMs === 'number' ? { durationMs: current.durationMs } : {}),
      };
    });
    this.sessions.set(sessionId, {
      ...current,
      messages,
      status,
      statusText,
    });
    this.streamText.delete(sessionId);
    this.emit(sessionId);
  }

  private emit(sessionId: string) {
    this.sessionVersions.set(sessionId, this.version(sessionId) + 1);
    this.allVersion += 1;
    this.listeners.get(sessionId)?.forEach((listener) => listener());
    this.allListeners.forEach((listener) => listener());
  }
}

function findLastAssistantIndex(messages: Message[]) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (messages[index]?.role === 'assistant') return index;
  }
  return -1;
}

function findLastOpenToolIndex(tools: ToolActivity[], name: string) {
  for (let index = tools.length - 1; index >= 0; index -= 1) {
    const tool = tools[index];
    if (tool?.name === name && tool.status === 'running') return index;
  }
  return -1;
}

function toolResultStatus(detail: string): ToolActivity['status'] {
  return /\b(?:fail(?:ed|ure)?|error|denied|suppressed)\b/i.test(detail)
    ? 'failed'
    : 'completed';
}

export const conversationStore = new ConversationStore();

export function useConversation(sessionId: string | null) {
  useSyncExternalStore(
    (listener) => conversationStore.subscribe(sessionId, listener),
    () => conversationStore.version(sessionId),
    () => conversationStore.version(sessionId)
  );
  return conversationStore.get(sessionId);
}

export function useConversationIndicators(sessionIds: string[]) {
  useSyncExternalStore(
    (listener) => conversationStore.subscribeAll(listener),
    () => conversationStore.versionAll(),
    () => conversationStore.versionAll()
  );

  return Object.fromEntries(
    sessionIds.flatMap((sessionId) => {
      const indicator = conversationStore.indicator(sessionId);
      return indicator ? [[sessionId, indicator] as const] : [];
    })
  ) as Record<string, SessionIndicatorState>;
}
