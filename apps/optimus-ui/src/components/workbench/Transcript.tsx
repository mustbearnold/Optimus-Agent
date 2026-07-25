import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { Message, RunStatus } from '../../ipc/contracts';
import { frameCoordinator } from '../../performance/frameCoordinator';
import { Icon } from '../chrome/Icon';
import { ActivityTimeline, type ApprovalDecisionHandler } from './ActivityTimeline';

const MessageRow = memo(function MessageRow({
  message,
  onApprovalDecision,
}: {
  message: Message;
  onApprovalDecision?: ApprovalDecisionHandler;
}) {
  const visibleContent = useTypewriterContent(message);
  const revealing = visibleContent !== message.content;
  const activeAssistantStatus =
    message.role === 'assistant' && message.status && message.status !== 'completed';

  return (
    <article
      className={`message message-${message.role}`}
      data-status={message.status || 'completed'}
      data-message-id={message.id}
    >
      {activeAssistantStatus ? (
        <div className="message-meta">
          <span className="message-status">{statusLabel(message.status!)}</span>
        </div>
      ) : null}
      {message.role === 'assistant' && message.status === 'completed' && !revealing && typeof message.durationMs === 'number' ? (
        <div className="message-worked" aria-label={`Worked for ${formatDuration(message.durationMs)}`}>
          Worked for {formatDuration(message.durationMs)} <span aria-hidden="true">›</span>
        </div>
      ) : null}
      {message.role === 'assistant' && message.thinking ? (
        <details className="thinking-block">
          <summary>Thinking</summary>
          <pre className="thinking-body">{message.thinking}</pre>
        </details>
      ) : null}
      <div className="message-body">
        {visibleContent || (message.status === 'working' ? <span className="stream-caret">Working</span> : null)}
      </div>
      {message.role === 'assistant' && message.tools?.length ? (
        <ActivityTimeline tools={message.tools} onApprovalDecision={onApprovalDecision} />
      ) : null}
    </article>
  );
});

const typewriterTailLength = 180;
const liveStatuses = new Set<RunStatus>([
  'submitting',
  'working',
  'awaiting_approval',
  'cancelling',
]);

function useTypewriterContent(message: Message) {
  const liveAssistant =
    message.role === 'assistant' && Boolean(message.status && liveStatuses.has(message.status));
  const streamed = useRef(liveAssistant);
  const visibleRef = useRef(message.content);
  const visibleCharacterCount = useRef(Array.from(message.content).length);
  const targetCharacters = useRef(Array.from(message.content));
  const [visible, setVisible] = useState(message.content);
  const frameKey = `typewriter:${message.id}`;
  const revealNext = useRef<() => void>(() => undefined);

  if (liveAssistant) streamed.current = true;
  targetCharacters.current = Array.from(message.content);

  revealNext.current = () => {
    const target = targetCharacters.current;
    let nextCount = visibleCharacterCount.current;
    const backlog = target.length - nextCount;
    if (backlog <= 0) return;

    nextCount =
      backlog > typewriterTailLength
        ? target.length - typewriterTailLength
        : nextCount + 1;
    const next = target.slice(0, nextCount).join('');
    visibleCharacterCount.current = nextCount;
    visibleRef.current = next;
    setVisible(next);

    if (nextCount < target.length) {
      frameCoordinator.scheduleKeyed('content', frameKey, () => revealNext.current());
    }
  };

  useEffect(() => {
    const reducedMotion =
      typeof window !== 'undefined' &&
      window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
    const currentIsPrefix = message.content.startsWith(visibleRef.current);
    const animate =
      message.role === 'assistant' &&
      streamed.current &&
      !reducedMotion &&
      !document.hidden &&
      currentIsPrefix;

    if (!animate) {
      frameCoordinator.cancelKeyed('content', frameKey);
      visibleRef.current = message.content;
      visibleCharacterCount.current = targetCharacters.current.length;
      setVisible(message.content);
      return;
    }

    if (visibleRef.current !== message.content) {
      frameCoordinator.scheduleKeyed('content', frameKey, () => revealNext.current());
    }
  }, [frameKey, message.content, message.role, message.status]);

  useEffect(
    () => () => frameCoordinator.cancelKeyed('content', frameKey),
    [frameKey]
  );

  return visible;
}

export function Transcript({
  messages,
  status,
  statusText,
  onStarter,
  onApprovalDecision,
}: {
  messages: Message[];
  status: RunStatus;
  statusText: string;
  onStarter: (text: string) => void;
  onApprovalDecision?: ApprovalDecisionHandler;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [visibleCount, setVisibleCount] = useState(200);
  const [following, setFollowing] = useState(true);
  const prependHeight = useRef<number | null>(null);
  const visible = useMemo(
    () => messages.slice(Math.max(0, messages.length - visibleCount)),
    [messages, visibleCount]
  );
  const announcement = useMemo(() => {
    const assistantText =
      [...messages].reverse().find((message) => message.role === 'assistant')?.content.trim() || '';
    if (['completed', 'cancelled', 'failed', 'disconnected'].includes(status)) {
      return assistantText ? `${assistantText.slice(-320)}. ${statusText}` : statusText;
    }
    const completeSentences = assistantText.match(/[^.!?]*[.!?](?:\s|$)/g);
    return completeSentences?.at(-1)?.trim() || statusText;
  }, [messages, status, statusText]);

  useEffect(() => {
    const node = scroller.current;
    if (!node || !following) return;
    node.scrollTop = node.scrollHeight;
  }, [following, messages.length, statusText]);

  useLayoutEffect(() => {
    const node = scroller.current;
    if (!node || prependHeight.current === null) return;
    node.scrollTop += node.scrollHeight - prependHeight.current;
    prependHeight.current = null;
  }, [visibleCount]);

  const onScroll = () => {
    const node = scroller.current;
    if (!node) return;
    const distance = node.scrollHeight - node.scrollTop - node.clientHeight;
    setFollowing(distance < 72);
  };

  return (
    <div
      className="transcript"
      ref={scroller}
      onScroll={onScroll}
      role="log"
      aria-label="Conversation"
      aria-live="off"
    >
      <span className="sr-only" role="status" aria-live="polite">{announcement}</span>
      <div className="transcript-inner">
        {messages.length > visibleCount ? (
          <button
            type="button"
            className="show-earlier"
            onClick={() => {
              prependHeight.current = scroller.current?.scrollHeight ?? null;
              setVisibleCount((count) => count + 100);
            }}
          >
            Show 100 earlier messages
          </button>
        ) : null}
        {!messages.length ? <EmptyWorkbench onStarter={onStarter} /> : null}
        {visible.map((message) => (
          <MessageRow
            message={message}
            key={message.id}
            onApprovalDecision={onApprovalDecision}
          />
        ))}
        {status === 'disconnected' || status === 'failed' ? (
          <div className="inline-notice is-error" role="status">
            <Icon name="warning" />
            <span>{statusText}</span>
          </div>
        ) : null}
      </div>
      {!following ? (
        <button
          type="button"
          className="jump-latest"
          onClick={() => {
            setFollowing(true);
            const node = scroller.current;
            if (node) node.scrollTop = node.scrollHeight;
          }}
        >
          Jump to latest
        </button>
      ) : null}
    </div>
  );
}

function EmptyWorkbench({ onStarter }: { onStarter: (text: string) => void }) {
  const starters = [
    ['Fix a failing test', 'Trace the failing test to its root cause, implement the smallest fix, and verify it.'],
    ['Review current changes', 'Review the current changed files for regressions, unsafe behavior, and unnecessary complexity.'],
    ['Plan a scoped feature', 'Inspect this workspace and create a decision-complete implementation plan for the feature I describe.'],
  ] as const;
  return (
    <section className="work-empty" aria-labelledby="empty-work-title">
      <span className="empty-kicker">New local session</span>
      <h1 id="empty-work-title">Start local work</h1>
      <p>Describe the outcome and constraints. Commands, files, approvals, and artifacts stay attached to this session.</p>
      <div className="starter-list" aria-label="Common local tasks">
        {starters.map(([label, prompt], index) => (
          <button type="button" onClick={() => onStarter(prompt)} key={label}>
            <span className="starter-index">0{index + 1}</span>
            <span>
              <strong>{label}</strong>
              <small>{index === 0 ? 'trace → patch → verify' : index === 1 ? 'diff → risks → proof' : 'scope → decide → build'}</small>
            </span>
            <Icon name="forward" />
          </button>
        ))}
      </div>
    </section>
  );
}

function statusLabel(status: RunStatus) {
  switch (status) {
    case 'working':
    case 'submitting':
      return 'Working';
    case 'cancelling':
      return 'Stopping';
    case 'cancelled':
      return 'Cancelled';
    case 'failed':
      return 'Failed';
    case 'disconnected':
      return 'Disconnected';
    default:
      return '';
  }
}

function formatDuration(durationMs: number) {
  const seconds = Math.max(0, Math.round(durationMs / 1000));
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;
}
