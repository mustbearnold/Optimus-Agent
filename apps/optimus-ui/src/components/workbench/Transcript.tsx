import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { Message, RunStatus, ToolActivity } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';
import { ActivityTimeline } from './ActivityTimeline';

const MessageRow = memo(function MessageRow({ message }: { message: Message }) {
  return (
    <article
      className={`message message-${message.role}`}
      data-status={message.status || 'completed'}
      data-message-id={message.id}
    >
      <div className="message-meta">
        <span>{message.role === 'user' ? 'You' : 'Optimus'}</span>
        {message.role === 'assistant' && message.status && message.status !== 'completed' ? (
          <span className="message-status">{statusLabel(message.status)}</span>
        ) : null}
      </div>
      <div className="message-body">
        {message.content || (message.status === 'working' ? <span className="stream-caret">Working</span> : null)}
      </div>
    </article>
  );
});

export function Transcript({
  messages,
  tools,
  status,
  statusText,
  onStarter,
}: {
  messages: Message[];
  tools: ToolActivity[];
  status: RunStatus;
  statusText: string;
  onStarter: (text: string) => void;
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
          <MessageRow message={message} key={message.id} />
        ))}
        {messages.length ? <ActivityTimeline tools={tools} statusText={statusText} /> : null}
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
    ['Audit current changes', 'Review the current changed files for regressions, unsafe behavior, and unnecessary complexity.'],
    ['Plan a feature', 'Inspect this workspace and create a decision-complete implementation plan for the feature I describe.'],
  ] as const;
  return (
    <section className="work-empty">
      <span className="empty-kicker">Optimus workbench</span>
      <h1>What should we build?</h1>
      <p>Start with a concrete outcome. Optimus will keep controls and evidence close to the work.</p>
      <div className="starter-list">
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
