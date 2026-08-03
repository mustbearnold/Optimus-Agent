import { useEffect, useRef, useState } from 'react';
import type { RunStatus } from '../../ipc/contracts';
import { frameCoordinator } from '../../performance/frameCoordinator';
import {
  PROVIDER_MODELS,
  type ComposerProvider,
  type ComposerSettings,
} from '../../state/composerStore';
import { Icon } from '../chrome/Icon';

// The ADR-0044 autonomy profiles, in the order that decision 7 states them:
// Standard first because it is the recommended default, unrestricted host last
// and behind an Expert heading because it is break-glass, not a routine
// choice. `value` is the wire string the host parses into an
// `AutonomyProfile`; `scripts/check-autonomy-profiles.py` fails the build if
// this list and the Rust vocabulary ever drift apart.
const accessOptions = [
  {
    value: 'standard',
    label: 'Standard',
    hint: 'Ordinary project work runs; anything else asks',
    icon: 'shield',
    tier: 'primary',
  },
  {
    value: 'review_changes',
    label: 'Review changes',
    hint: 'Reads run; writes and commands ask first',
    icon: 'check',
    tier: 'primary',
  },
  {
    value: 'read_only',
    label: 'Read only',
    hint: 'Nothing is changed',
    icon: 'files',
    tier: 'primary',
  },
  {
    value: 'full_project',
    label: 'Full project',
    hint: 'Wider autonomy inside the project; credentials and your system still ask',
    icon: 'project',
    tier: 'advanced',
  },
  {
    value: 'developer_full_access',
    label: 'Developer Full Access',
    hint: 'Edit, execute, install, and rebuild inside an explicit local scope',
    icon: 'terminal',
    tier: 'developer',
  },
  {
    value: 'unrestricted_host',
    label: 'Unrestricted host',
    hint: 'Break-glass: no pauses, and the whole machine is in reach',
    icon: 'warning',
    tier: 'expert',
  },
] as const;

const accessTiers = [
  { tier: 'primary', heading: '' },
  { tier: 'advanced', heading: 'Advanced' },
  { tier: 'developer', heading: 'Developer' },
  { tier: 'expert', heading: 'Expert' },
] as const;

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
  const modelLabel = visibleModelLabel(settings.model);

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

  // An unknown stored value falls back to Standard, which is what the host
  // does with it too — the label never claims more authority than the run has.
  const selectedAccess = accessOptions.find((option) => option.value === settings.access) || accessOptions[0];
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
                className={`composer-access-trigger${settings.access === 'unrestricted_host' ? ' is-unrestricted-host' : ''}`}
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
                  {accessTiers.map(({ tier, heading }) => (
                    <div
                      className={`composer-access-tier is-${tier}`}
                      key={tier}
                      role="group"
                      aria-label={heading || 'Recommended'}
                    >
                      {/* The group already carries this word as its accessible
                          name; showing it again would read it twice. */}
                      {heading ? (
                        <p className="composer-access-heading" aria-hidden="true">
                          {heading}
                        </p>
                      ) : null}
                      {accessOptions
                        .filter((option) => option.tier === tier)
                        .map((option) => (
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
                              // Clamped, not wrapped: wrapping put ArrowUp from
                              // the default one keystroke away from break-glass,
                              // which is what the tiers exist to prevent. The
                              // ARIA listbox pattern does not wrap either.
                              const next = index + (event.key === 'ArrowDown' ? 1 : -1);
                              choices[Math.min(choices.length - 1, Math.max(0, next))]?.focus();
                            }}
                          >
                            <Icon name={option.icon} />
                            <span className="composer-access-option">
                              <span>{option.label}</span>
                              <small>{option.hint}</small>
                            </span>
                          </button>
                        ))}
                    </div>
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
                          const provider = event.target.value as ComposerProvider;
                          onSettings({
                            ...settings,
                            provider,
                            // Provider defaults differ. Reset to model Auto
                            // rather than carrying an incompatible explicit id.
                            model: provider === settings.provider ? settings.model : '',
                          });
                        }}
                      >
                        <option value="auto">Auto</option>
                        <option value="offline">Offline</option>
                        <option value="codex">Codex</option>
                        <option value="open-ai-compat">OpenAI compatible</option>
                      </select>
                    </label>
                    <label>
                      <span>Model</span>
                      <select
                        value={settings.model}
                        onChange={(event) => onSettings({ ...settings, model: event.target.value })}
                      >
                        <option value="">Auto</option>
                        {PROVIDER_MODELS[settings.provider].map((model) => (
                          <option value={model} key={model}>
                            {visibleModelLabel(model)}
                          </option>
                        ))}
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

function visibleModelLabel(model: string) {
  if (!model) return 'Auto';
  const match = /^gpt-(\d+(?:\.\d+)*)(?:-([a-z0-9]+))?$/i.exec(model);
  if (!match) return model;
  const [, version, name] = match;
  return [version, name ? `${name[0]!.toUpperCase()}${name.slice(1).toLowerCase()}` : ''].filter(Boolean).join(' ');
}
