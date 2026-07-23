import { useSyncExternalStore } from 'react';
import type { Message, RunStatus, SessionDetail, StreamEvent, ToolActivity } from '../ipc/contracts';
import { frameCoordinator } from '../performance/frameCoordinator';

type SessionProjection = {
  messages: Message[];
  tools: ToolActivity[];
  status: RunStatus;
  statusText: string;
  durationMs?: number;
  loaded: boolean;
};

const emptyProjection = (): SessionProjection => ({
  messages: [],
  tools: [],
  status: 'idle',
  statusText: '',
  loaded: false,
});

class ConversationStore {
  private readonly sessions = new Map<string, SessionProjection>();
  private readonly listeners = new Map<string, Set<() => void>>();
  private readonly streamText = new Map<string, string>();
  private readonly sessionVersions = new Map<string, number>();

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

  load(detail: SessionDetail) {
    const messages: Message[] = detail.messages.map((message, index) => ({
      id: `${detail.id}:persisted:${index}`,
      role: message.role,
      content: message.content,
      status: 'completed',
    }));
    this.sessions.set(detail.id, {
      messages,
      tools: [],
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
      tools: [],
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
      frameCoordinator.schedule('content', () => this.flushText(sessionId));
      return;
    }
    if (event.type === 'tool') {
      const id = `${event.name}:${current.tools.length}`;
      this.sessions.set(sessionId, {
        ...current,
        status: 'working',
        statusText: event.detail || `${event.name}…`,
        tools: [
          ...current.tools.slice(-199),
          { id, name: event.name, detail: event.detail, status: 'running' },
        ],
      });
    } else if (event.type === 'status') {
      this.sessions.set(sessionId, {
        ...current,
        status: /approval/i.test(event.text) ? 'awaiting_approval' : 'working',
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
    const messages = current.messages.map((message, index, all) =>
      index === all.length - 1 && message.role === 'assistant'
        ? { ...message, status }
        : message
    );
    this.sessions.set(sessionId, {
      ...current,
      messages,
      tools: current.tools.map((tool) =>
        tool.status === 'running' ? { ...tool, status: 'completed' } : tool
      ),
      status,
      statusText,
    });
    this.streamText.delete(sessionId);
    this.emit(sessionId);
  }

  private emit(sessionId: string) {
    this.sessionVersions.set(sessionId, this.version(sessionId) + 1);
    this.listeners.get(sessionId)?.forEach((listener) => listener());
  }
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
