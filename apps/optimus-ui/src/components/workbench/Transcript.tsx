import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { Message, RunStatus } from '../../ipc/contracts';
import { frameCoordinator } from '../../performance/frameCoordinator';
import { Icon } from '../chrome/Icon';
import { ActivityTimeline, type ApprovalDecisionHandler } from './ActivityTimeline';
import { RichText } from './RichText';

const MessageRow = memo(function MessageRow({
  message,
  onApprovalDecision,
}: {
  message: Message;
  onApprovalDecision?: ApprovalDecisionHandler;
}) {
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
      {message.role === 'assistant' && message.status === 'completed' && typeof message.durationMs === 'number' ? (
        <div className="message-worked" aria-label={`Worked for ${formatDuration(message.durationMs)}`}>
          Worked for {formatDuration(message.durationMs)} <span aria-hidden="true">›</span>
        </div>
      ) : null}
      {message.role === 'assistant' && message.thinking ? (
        <details
          className="thinking-block"
          open={message.status === 'working' ? true : undefined}
        >
          <summary>Thinking</summary>
          <pre className="thinking-body">{message.thinking}</pre>
        </details>
      ) : null}
      <div className="message-body">
        {message.content
          ? <RichText content={message.content} />
          : message.status === 'working'
            ? <span className="stream-caret">Working</span>
            : null}
      </div>
      {message.role === 'assistant' && message.tools?.length ? (
        <ActivityTimeline tools={message.tools} onApprovalDecision={onApprovalDecision} />
      ) : null}
    </article>
  );
});

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
      stripForAnnouncement(
        [...messages].reverse().find((message) => message.role === 'assistant')?.content || ''
      );
    if (['completed', 'cancelled', 'failed', 'disconnected'].includes(status)) {
      return assistantText ? `${assistantText.slice(-320)}. ${statusText}` : statusText;
    }
    const completeSentences = assistantText.match(/[^.!?]*[.!?](?:\s|$)/g);
    return completeSentences?.at(-1)?.trim() || statusText;
  }, [messages, status, statusText]);

  // Streaming changes `statusText` many times a second, and each change wrote
  // `scrollTop` straight from an effect — a layout write landing in the middle of
  // the frame the compositor was already scrolling. Batched into the frame's
  // scroll lane it happens once per frame, after the reads.
  useEffect(() => {
    if (!following) return;
    frameCoordinator.scheduleKeyed('scroll', 'transcript-follow', () => {
      const node = scroller.current;
      if (!node) return;
      node.scrollTop = node.scrollHeight;
    });
  }, [following, messages.length, statusText]);

  useLayoutEffect(() => {
    const node = scroller.current;
    if (!node || prependHeight.current === null) return;
    node.scrollTop += node.scrollHeight - prependHeight.current;
    prependHeight.current = null;
  }, [visibleCount]);

  // `scroll` fires far more often than once a frame, and reading `scrollHeight`
  // forces the browser to lay the whole transcript out before it can answer.
  // Doing that per event — then setting state, which schedules the write above,
  // which invalidates the layout the next event reads — is read/write thrash on
  // the one interaction that has to stay at frame rate. One coalesced read per
  // frame, and state is only touched when the answer actually changes.
  const onScroll = () => {
    frameCoordinator.scheduleKeyed('layout-read', 'transcript-following', () => {
      const node = scroller.current;
      if (!node) return;
      const distance = node.scrollHeight - node.scrollTop - node.clientHeight;
      setFollowing((current) => (current === distance < 72 ? current : distance < 72));
    });
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

function stripForAnnouncement(value: string) {
  return value
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/\[([^\]\n]+)\]\(https?:\/\/[^\s)]+\)/g, '$1')
    .replace(/^\s{0,3}(?:[-+*]|\d+[.)])\s+/gm, '')
    .replace(/[\*_`#>]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function EmptyWorkbench({ onStarter }: { onStarter: (text: string) => void }) {
  const starters = [
    ['Fix a failing test', 'Trace the failing test to its root cause, implement the smallest fix, and verify it.'],
    ['Review current changes', 'Review the current changed files for regressions, unsafe behavior, and unnecessary complexity.'],
    ['Plan a scoped feature', 'Inspect this workspace and create a decision-complete implementation plan for the feature I describe.'],
  ] as const;
  return (
    <section className="work-empty" aria-labelledby="empty-work-title">
      <span className="empty-kicker">New session</span>
      <h1 id="empty-work-title">What should Optimus do?</h1>
      <p>
        Write a concrete task below, or pick a starter. Tools, files, and approvals stay on this
        session.
      </p>
      <div className="starter-list" aria-label="Common local tasks">
        {starters.map(([label, prompt], index) => (
          <button type="button" onClick={() => onStarter(prompt)} key={label}>
            <span className="starter-index" aria-hidden="true">
              {index + 1}
            </span>
            <span>
              <strong>{label}</strong>
              <small>
                {index === 0
                  ? 'Find the failure, patch, re-run'
                  : index === 1
                    ? 'Scan the diff for risks'
                    : 'Scope first, then plan'}
              </small>
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
