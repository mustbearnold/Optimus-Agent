import type { Message } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

type Props = {
  messages: Message[];
  activePromptId: string | null;
  onSelect: (messageId: string) => void;
};

/**
 * A compact, in-session index of user prompts. It is deliberately separate
 * from Recent Chats: this rail navigates the current transcript, while the
 * project rail changes the current session.
 */
export function PromptHistoryRail({ messages, activePromptId, onSelect }: Props) {
  const prompts = messages.filter((message) => message.role === 'user');
  return (
    <nav className="prompt-history-rail" aria-label="Prompt history" data-testid="prompt-history">
      <div className="prompt-history-heading">
        <Icon name="chat" />
        <span>Prompts</span>
        <small>{prompts.length}</small>
      </div>
      <div className="prompt-history-list" role="list">
        {prompts.length ? prompts.map((prompt, index) => {
          const label = compactPrompt(prompt.content);
          const active = prompt.id === activePromptId;
          return (
            <button
              type="button"
              role="listitem"
              className={`prompt-history-item${active ? ' is-active' : ''}`}
              data-prompt-id={prompt.id}
              aria-current={active ? 'true' : undefined}
              title={prompt.content}
              onClick={() => onSelect(prompt.id)}
              key={prompt.id}
            >
              <span className="prompt-history-index">{String(index + 1).padStart(2, '0')}</span>
              <span className="prompt-history-label">{label || 'Empty prompt'}</span>
            </button>
          );
        }) : (
          <p className="prompt-history-empty">Your prompts appear here.</p>
        )}
      </div>
    </nav>
  );
}

function compactPrompt(value: string) {
  return value.replace(/\s+/g, ' ').trim();
}
