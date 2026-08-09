import { useSyncExternalStore } from 'react';
import type {
  Message,
  RunStatus,
  SessionDetail,
  StreamEvent,
  ToolActivity,
  ToolLifecyclePhase,
} from '../ipc/contracts';
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
  private readonly streamThinking = new Map<string, string>();
  private readonly toolEventIds = new Map<string, Set<string>>();
  private readonly sessionVersions = new Map<string, number>();
  private readonly indicatorSnapshots = new Map<string, SessionIndicatorState | null>();
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
    const eventIds = new Set<string>();
    let lastLifecycle: ToolLifecyclePhase | undefined;
    let lastSummary = '';
    const messages: Message[] = detail.messages.map((message, index) => {
      if (message.role === 'user') {
        lastLifecycle = undefined;
        lastSummary = '';
      }
      const tools = new Map<string, ToolActivity>();
      for (const event of message.tool_events || []) {
        if (eventIds.has(event.event_id)) continue;
        eventIds.add(event.event_id);
        lastLifecycle = event.phase;
        lastSummary = event.summary;
        const current = tools.get(event.call_id);
        tools.set(event.call_id, { ...current, ...toolActivityFromEvent(event) });
      }
      const persistedTools = [...tools.values()];
      return {
        id: `${detail.id}:persisted:${index}`,
        role: message.role,
        content: message.content,
        status: persistedMessageStatus(persistedTools),
        ...(persistedTools.length ? { tools: persistedTools } : {}),
      };
    });
    const status: RunStatus =
      lastLifecycle === 'approval_required'
        ? 'awaiting_approval'
        : lastLifecycle === 'failed' ||
            lastLifecycle === 'ambiguous' ||
            detail.run_status === 'failed'
          ? 'failed'
          : lastLifecycle === 'cancelled' || detail.run_status === 'cancelled'
            ? 'cancelled'
            : detail.run_status === 'running'
            ? 'working'
            : 'idle';
    const statusText =
      status === 'idle'
        ? ''
        : status === 'failed' && lastLifecycle !== 'failed' && lastLifecycle !== 'ambiguous'
          ? 'Run failed'
          : status === 'cancelled' && lastLifecycle !== 'cancelled'
            ? 'Run cancelled'
            : lastSummary;
    // R4: a failed continuation carries its specific error text on the done
    // payload; a reload must not downgrade it to the generic "Run failed".
    const current = this.sessions.get(detail.id);
    const preservedFailure =
      status === 'failed' &&
      current?.status === 'failed' &&
      current.statusText &&
      current.statusText !== 'Run failed'
        ? current.statusText
        : statusText;
    this.sessions.set(detail.id, {
      messages,
      status,
      statusText: preservedFailure,
      loaded: true,
    });
    this.toolEventIds.set(detail.id, eventIds);
    this.emit(detail.id);
  }

  /** R11: timing offsets that arrived before their tool lifecycle event
   *  (sessionId -> callId -> { startedAtMs?, finishedAtMs? }). */
  private pendingToolTimes = new Map<
    string,
    Map<string, { startedAtMs?: number; finishedAtMs?: number }>
  >();

  private attachToolTime(
    sessionId: string,
    callId: string,
    kind: 'tool_started' | 'tool_finished',
    elapsedMs: number,
  ) {
    const stashed =
      this.pendingToolTimes.get(sessionId) ||
      new Map<string, { startedAtMs?: number; finishedAtMs?: number }>();
    const entry = stashed.get(callId) || {};
    entry[kind === 'tool_started' ? 'startedAtMs' : 'finishedAtMs'] = elapsedMs;
    stashed.set(callId, entry);
    this.pendingToolTimes.set(sessionId, stashed);
    const session = this.sessions.get(sessionId);
    if (!session) return;
    const messages = session.messages.slice();
    const messageIndex = findLastAssistantIndex(messages);
    if (messageIndex < 0) return;
    const message = messages[messageIndex]!;
    const tools = [...(message.tools || [])];
    const toolIndex = tools.findIndex((tool) => tool.callId === callId);
    if (toolIndex >= 0) {
      tools[toolIndex] = { ...tools[toolIndex]!, ...entry };
      messages[messageIndex] = { ...message, tools };
      this.sessions.set(sessionId, { ...session, messages });
    }
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
    this.streamThinking.set(sessionId, '');
    this.toolEventIds.set(sessionId, new Set());
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
    if (event.type === 'thinking') {
      this.streamThinking.set(
        sessionId,
        (this.streamThinking.get(sessionId) || '') + event.text
      );
      frameCoordinator.scheduleKeyed('content', `thinking:${sessionId}`, () =>
        this.flushThinking(sessionId)
      );
      return;
    }
    if (event.type === 'tool') {
      if (!this.acceptToolEvent(sessionId, event.event_id)) return;
      const messages = current.messages.slice();
      const messageIndex = findLastAssistantIndex(messages);
      if (messageIndex < 0) return;
      const message = messages[messageIndex]!;
      const tools = [...(message.tools || [])];
      const toolIndex = tools.findIndex((tool) => tool.callId === event.call_id);
      const activity = toolActivityFromEvent(event);
      if (toolIndex >= 0) {
        tools[toolIndex] = { ...tools[toolIndex]!, ...activity };
      } else {
        tools.push(activity);
      }
      // R11 back-fill: the kernel sinks timing BEFORE the lifecycle event, so
      // start/finish offsets stashed by the timing handler are applied when
      // the tool activity finally appears.
      const stashed = this.pendingToolTimes.get(sessionId)?.get(event.call_id);
      if (stashed && Object.keys(stashed).length > 0) {
        const target = toolIndex >= 0 ? toolIndex : tools.length - 1;
        tools[target] = { ...tools[target]!, ...stashed };
      }
      messages[messageIndex] = {
        ...message,
        tools: tools.slice(-200),
      };
      this.sessions.set(sessionId, {
        ...current,
        status: event.phase === 'approval_required' ? 'awaiting_approval' : 'working',
        statusText: event.summary || `${event.tool_id}…`,
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
      const next = {
        ...current,
        durationMs:
          typeof event.elapsed_ms === 'number' ? event.elapsed_ms : current.durationMs,
      };
      this.sessions.set(sessionId, next);
      // R11: tool-to-tool gap breakdown — attach start/finish run offsets to
      // the owning tool activity. The kernel sinks the timing event BEFORE
      // the tool lifecycle event (turn_loop.rs), so attach is
      // order-independent: offsets are stashed per call and back-filled when
      // the tool activity appears.
      if (
        typeof event.elapsed_ms === 'number' &&
        event.call_id &&
        (event.kind === 'tool_started' || event.kind === 'tool_finished')
      ) {
        this.attachToolTime(sessionId, event.call_id, event.kind, event.elapsed_ms);
      }
    } else if (event.type === 'done') {
      this.flushText(sessionId);
      this.flushThinking(sessionId);
      const result = event.result;
      // R4: the resolve/chat terminal payload decides the state. A failed
      // continuation must surface as the failure it was, with the error text
      // preserved across the post-resolve reload; a re-parked approval (R5)
      // stays in an explicit awaiting state — the second card renders from the
      // record after the reload.
      if (result?.resume_error) {
        this.setTerminal(sessionId, 'failed', String(result.resume_error));
        return;
      }
      if (result?.still_pending === true) {
        const current = this.sessions.get(sessionId) || emptyProjection();
        this.sessions.set(sessionId, {
          ...current,
          status: 'awaiting_approval',
          statusText: 'Waiting for the next approval',
        });
        this.emit(sessionId);
        return;
      }
      this.setTerminal(sessionId, 'completed', 'Completed');
      return;
    } else if (event.type === 'cancelled') {
      this.flushText(sessionId);
      this.flushThinking(sessionId);
      this.setTerminal(sessionId, 'cancelled', 'Cancelled · partial response retained');
      return;
    } else if (event.type === 'error') {
      this.flushText(sessionId);
      this.flushThinking(sessionId);
      // R4: no swallow. Any error during an approval pause fails the session
      // truthfully; a stale-card click surfaces "missing or already resolved"
      // instead of freezing the transcript on a card that can never settle.
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

  private flushThinking(sessionId: string) {
    if (!this.streamThinking.has(sessionId)) return;
    const current = this.sessions.get(sessionId);
    if (!current) return;
    const thinking = this.streamThinking.get(sessionId) || '';
    const messages = current.messages.slice();
    const index = findLastAssistantIndex(messages);
    if (index < 0) return;
    if (messages[index]?.thinking === thinking) return;
    messages[index] = { ...messages[index]!, thinking, status: current.status };
    this.sessions.set(sessionId, {
      ...current,
      messages,
      status: current.status === 'submitting' ? 'working' : current.status,
    });
    this.emit(sessionId);
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
          tool.status === 'running' || tool.status === 'awaiting_approval'
            ? {
                ...tool,
                status:
                  status === 'failed' || status === 'disconnected'
                    ? ('failed' as const)
                    : status === 'cancelled'
                      ? ('cancelled' as const)
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
    this.streamThinking.delete(sessionId);
    this.emit(sessionId);
  }

  private acceptToolEvent(sessionId: string, eventId: string) {
    const ids = this.toolEventIds.get(sessionId) || new Set<string>();
    if (ids.has(eventId)) return false;
    ids.add(eventId);
    if (ids.size > 512) {
      const oldest = ids.values().next().value;
      if (oldest) ids.delete(oldest);
    }
    this.toolEventIds.set(sessionId, ids);
    return true;
  }

  private emit(sessionId: string) {
    this.sessionVersions.set(sessionId, this.version(sessionId) + 1);
    this.listeners.get(sessionId)?.forEach((listener) => listener());
    const nextIndicator = this.indicator(sessionId);
    const indicatorChanged =
      !this.indicatorSnapshots.has(sessionId) ||
      this.indicatorSnapshots.get(sessionId) !== nextIndicator;
    this.indicatorSnapshots.set(sessionId, nextIndicator);
    if (!indicatorChanged) return;
    this.allVersion += 1;
    this.allListeners.forEach((listener) => listener());
  }
}

function findLastAssistantIndex(messages: Message[]) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (messages[index]?.role === 'assistant') return index;
  }
  return -1;
}

function toolActivityStatus(phase: ToolLifecyclePhase): ToolActivity['status'] {
  switch (phase) {
    case 'started':
      return 'running';
    case 'approval_required':
      return 'awaiting_approval';
    case 'succeeded':
      return 'completed';
    default:
      return phase;
  }
}

function toolActivityFromEvent(
  event: Extract<StreamEvent, { type: 'tool' }>
): ToolActivity {
  return {
    id: event.call_id,
    runId: event.run_id,
    callId: event.call_id,
    name: event.tool_id,
    detail: event.summary,
    status: toolActivityStatus(event.phase),
    ...(typeof event.duration_ms === 'number' ? { durationMs: event.duration_ms } : {}),
    ...(event.outcome ? { outcome: event.outcome } : {}),
    ...(event.approval ? { approval: event.approval } : {}),
  };
}

function persistedMessageStatus(tools: ToolActivity[]): RunStatus {
  if (tools.some((tool) => tool.status === 'awaiting_approval')) return 'awaiting_approval';
  if (tools.some((tool) => tool.status === 'failed' || tool.status === 'ambiguous')) return 'failed';
  if (tools.some((tool) => tool.status === 'running')) return 'working';
  return 'completed';
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
