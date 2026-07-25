import { useEffect, useRef, useState } from 'react';
import type { RunStatus } from '../../ipc/contracts';
import { frameCoordinator } from '../../performance/frameCoordinator';
import { Icon } from '../chrome/Icon';

const accessOptions = [
  { value: 'full', label: 'Full access', icon: 'terminal' },
  { value: 'ask', label: 'Ask before effects', icon: 'shield' },
  { value: 'read', label: 'Read only', icon: 'files' },
] as const;

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
  const settingsMenu = useRef<HTMLDivElement>(null);
  const settingsTrigger = useRef<HTMLButtonElement>(null);
  const accessMenu = useRef<HTMLDivElement>(null);
  const accessTrigger = useRef<HTMLButtonElement>(null);
  const providerSelect = useRef<HTMLSelectElement>(null);
  const [composing, setComposing] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [accessOpen, setAccessOpen] = useState(false);
  const busy = ['submitting', 'working', 'awaiting_approval', 'cancelling'].includes(runStatus);
  const thinkingLabel = {
    low: 'Low',
    medium: 'Medium',
    high: 'High',
    xhigh: 'Extra high',
  }[settings.thinking] || settings.thinking;
  const modelLabel = visibleModelLabel(settings.provider, settings.model);

  useEffect(() => {
    frameCoordinator.schedule('layout-write', () => {
      const node = textarea.current;
      if (!node) return;
      node.style.height = '0px';
      const next = Math.max(44, Math.min(176, node.scrollHeight));
      if (node.offsetHeight !== next) node.style.height = `${next}px`;
    });
  }, [value]);

  useEffect(() => {
    if (!settingsOpen) return;
    const focusFrame = requestAnimationFrame(() => providerSelect.current?.focus());
    const onPointerDown = (event: PointerEvent) => {
      if (!settingsMenu.current?.contains(event.target as Node)) setSettingsOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      setSettingsOpen(false);
      requestAnimationFrame(() => settingsTrigger.current?.focus());
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      cancelAnimationFrame(focusFrame);
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [settingsOpen]);

  useEffect(() => {
    if (!accessOpen) return;
    const focusFrame = requestAnimationFrame(() =>
      accessMenu.current?.querySelector<HTMLButtonElement>('[aria-selected="true"]')?.focus()
    );
    const onPointerDown = (event: PointerEvent) => {
      if (
        !accessMenu.current?.contains(event.target as Node) &&
        !accessTrigger.current?.contains(event.target as Node)
      ) {
        setAccessOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      setAccessOpen(false);
      requestAnimationFrame(() => accessTrigger.current?.focus());
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      cancelAnimationFrame(focusFrame);
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [accessOpen]);

  const selectedAccess = accessOptions.find((option) => option.value === settings.access) || accessOptions[1];
  const selectAccess = (access: string) => {
    onSettings({ ...settings, access });
    setAccessOpen(false);
    requestAnimationFrame(() => accessTrigger.current?.focus());
  };

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
          placeholder="Describe the task, @ files or folders, or paste a failing command…"
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
            <div className="composer-access">
              <button
                ref={accessTrigger}
                type="button"
                className={`composer-access-trigger${settings.access === 'full' ? ' is-full-access' : ''}`}
                aria-label={`Access: ${selectedAccess.label}`}
                aria-haspopup="listbox"
                aria-expanded={accessOpen}
                onClick={() => setAccessOpen((open) => !open)}
                onKeyDown={(event) => {
                  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
                    event.preventDefault();
                    setAccessOpen(true);
                  }
                }}
              >
                <Icon name={selectedAccess.icon} />
                <span>{selectedAccess.label}</span>
              </button>
              {accessOpen ? (
                <div className="composer-access-menu" ref={accessMenu} role="listbox" aria-label="Access">
                  {accessOptions.map((option) => (
                    <button
                      type="button"
                      role="option"
                      aria-selected={option.value === settings.access}
                      key={option.value}
                      onClick={() => selectAccess(option.value)}
                      onKeyDown={(event) => {
                        if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
                        event.preventDefault();
                        const choices = Array.from(accessMenu.current?.querySelectorAll<HTMLButtonElement>('[role="option"]') || []);
                        const index = choices.indexOf(event.currentTarget);
                        choices[(index + (event.key === 'ArrowDown' ? 1 : -1) + choices.length) % choices.length]?.focus();
                      }}
                    >
                      <Icon name={option.icon} />
                      <span>{option.label}</span>
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
            <div className="composer-settings-control" ref={settingsMenu}>
              <button
                ref={settingsTrigger}
                type="button"
                className={`composer-settings-trigger${settingsOpen ? ' is-open' : ''}`}
                aria-label="Model and run settings"
                aria-haspopup="dialog"
                aria-expanded={settingsOpen}
                onClick={() => setSettingsOpen((open) => !open)}
              >
                <span className="composer-settings-model">{modelLabel}</span>
                <span className="composer-settings-summary">
                  {thinkingLabel}{settings.fast ? ' · Fast' : ''}
                </span>
                <Icon name="chevron" />
              </button>
              {settingsOpen ? (
                <div
                  className="composer-settings-popover"
                  role="dialog"
                  aria-label="Model and run settings"
                >
                  <div className="composer-settings-grid">
                    <label>
                      <span>Provider</span>
                      <select
                        ref={providerSelect}
                        value={settings.provider}
                        onChange={(event) => {
                          const provider = event.target.value as ComposerSettings['provider'];
                          onSettings({
                            ...settings,
                            provider,
                            model:
                              provider === 'offline'
                                ? 'offline-echo'
                                : settings.model === 'offline-echo'
                                  ? 'gpt-5.6-terra'
                                  : settings.model,
                          });
                        }}
                      >
                        <option value="offline">Offline</option>
                        <option value="codex">Codex</option>
                        <option value="openai_compat">OpenAI compatible</option>
                      </select>
                    </label>
                    <label>
                      <span>Model</span>
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
                      <span>Thinking level</span>
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
                    <button
                      type="button"
                      className={`fast-mode-toggle${settings.fast ? ' is-active' : ''}`}
                      role="switch"
                      aria-checked={settings.fast}
                      aria-label="Fast mode"
                      onClick={() => onSettings({ ...settings, fast: !settings.fast })}
                    >
                      <span>
                        <strong>Fast mode</strong>
                        <small>Prefer lower latency when available.</small>
                      </span>
                      <span className="fast-mode-indicator" aria-hidden="true" />
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
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
    </div>
  );
}

function visibleModelLabel(provider: ComposerSettings['provider'], model: string) {
  if (provider === 'offline') return model;
  const match = /^gpt-(\d+(?:\.\d+)*)(?:-([a-z0-9]+))?$/i.exec(model);
  if (!match) return model;
  const [, version, name] = match;
  return [version, name ? `${name[0]!.toUpperCase()}${name.slice(1).toLowerCase()}` : ''].filter(Boolean).join(' ');
}
