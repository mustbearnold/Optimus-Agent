import { useEffect, useRef, useState } from 'react';
import type { RunStatus } from '../../ipc/contracts';
import { frameCoordinator } from '../../performance/frameCoordinator';
import { Icon } from '../chrome/Icon';

type ComposerSettings = {
  provider: 'offline' | 'codex' | 'openai_compat';
  model: string;
  thinking: string;
  access: string;
  fast: boolean;
};

type Props = {
  value: string;
  runStatus: RunStatus;
  disabled: boolean;
  isRunOwner: boolean;
  settings: ComposerSettings;
  onChange: (value: string) => void;
  onSettings: (settings: ComposerSettings) => void;
  onSend: () => void;
  onStop: () => void;
};

export function Composer({
  value,
  runStatus,
  disabled,
  isRunOwner,
  settings,
  onChange,
  onSettings,
  onSend,
  onStop,
}: Props) {
  const textarea = useRef<HTMLTextAreaElement>(null);
  const [composing, setComposing] = useState(false);
  const busy = ['submitting', 'working', 'awaiting_approval', 'cancelling'].includes(runStatus);

  useEffect(() => {
    frameCoordinator.schedule('layout-write', () => {
      const node = textarea.current;
      if (!node) return;
      node.style.height = '0px';
      const next = Math.max(44, Math.min(176, node.scrollHeight));
      if (node.offsetHeight !== next) node.style.height = `${next}px`;
    });
  }, [value]);

  return (
    <div className="composer-shell">
      {disabled && !isRunOwner ? (
        <div className="composer-lock" role="status">
          Another session is working. You can inspect it or stop the active run.
        </div>
      ) : null}
      <div className="composer-card">
        <textarea
          ref={textarea}
          value={value}
          aria-label="Message Optimus"
          placeholder="Ask anything, @ files/folders, or describe an outcome…"
          disabled={disabled && !isRunOwner}
          onChange={(event) => onChange(event.target.value)}
          onCompositionStart={() => setComposing(true)}
          onCompositionEnd={() => setComposing(false)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey && !composing && !event.nativeEvent.isComposing) {
              event.preventDefault();
              if (busy && isRunOwner) onStop();
              else if (!disabled) onSend();
            }
          }}
        />
        <div className="composer-controls">
          <div className="composer-selects">
            <label>
              <span className="sr-only">Provider</span>
              <select
                value={settings.provider}
                onChange={(event) =>
                  onSettings({
                    ...settings,
                    provider: event.target.value as ComposerSettings['provider'],
                  })
                }
              >
                <option value="offline">Offline</option>
                <option value="codex">Codex</option>
                <option value="openai_compat">OpenAI compatible</option>
              </select>
            </label>
            <label>
              <span className="sr-only">Model</span>
              <select
                value={settings.model}
                onChange={(event) => onSettings({ ...settings, model: event.target.value })}
              >
                {settings.provider === 'offline' ? (
                  <option value="offline-echo">offline-echo</option>
                ) : (
                  <>
                    <option value="gpt-5.6-terra">GPT-5.6 Terra</option>
                    <option value="gpt-5.6-sol">GPT-5.6 Sol</option>
                  </>
                )}
              </select>
            </label>
            <label>
              <span className="sr-only">Thinking level</span>
              <select
                value={settings.thinking}
                onChange={(event) => onSettings({ ...settings, thinking: event.target.value })}
              >
                <option value="low">Low effort</option>
                <option value="medium">Medium effort</option>
                <option value="high">High effort</option>
                <option value="xhigh">Extra high</option>
              </select>
            </label>
            <label>
              <span className="sr-only">Access</span>
              <select
                value={settings.access}
                onChange={(event) => onSettings({ ...settings, access: event.target.value })}
              >
                <option value="full">Full access</option>
                <option value="ask">Ask before effects</option>
                <option value="read">Read only</option>
              </select>
            </label>
            <button
              type="button"
              className={`fast-toggle${settings.fast ? ' is-active' : ''}`}
              aria-pressed={settings.fast}
              onClick={() => onSettings({ ...settings, fast: !settings.fast })}
            >
              Fast
            </button>
          </div>
          <button
            type="button"
            className={`send-button${busy && isRunOwner ? ' is-stop' : ''}`}
            disabled={busy ? !isRunOwner : disabled || !value.trim()}
            aria-label={busy && isRunOwner ? 'Stop current run' : 'Send message'}
            title={busy && isRunOwner ? 'Stop current run' : 'Send message'}
            onClick={busy && isRunOwner ? onStop : onSend}
          >
            <Icon name={busy && isRunOwner ? 'stop' : 'send'} />
          </button>
        </div>
      </div>
      <div className="composer-foot">
        <span>Local checkout</span>
        <span>{busy ? 'One active run' : 'Ready'}</span>
      </div>
    </div>
  );
}
