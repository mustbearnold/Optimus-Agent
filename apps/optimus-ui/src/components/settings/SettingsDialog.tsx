import { useEffect, useRef, useState } from 'react';
import type { CronJob, OptimusTransport, ProductSettings } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

const fallback: ProductSettings = {
  work_isolation: 'shared',
  allow_concurrent_projects: false,
  enforcement_active: false,
};

export function SettingsDialog({
  open,
  transport,
  theme,
  onTheme,
  onClose,
}: {
  open: boolean;
  transport: OptimusTransport;
  theme: 'dark' | 'light';
  onTheme: (theme: 'dark' | 'light') => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const [settings, setSettings] = useState(fallback);
  const [cron, setCron] = useState<CronJob[]>([]);
  const [auth, setAuth] = useState<Record<string, unknown>>({});
  const [saved, setSaved] = useState('');

  useEffect(() => {
    if (!open) return;
    previousFocus.current = document.activeElement as HTMLElement | null;
    Promise.all([
      transport.invoke<{ settings?: ProductSettings }>('settings_get'),
      transport.invoke<{ jobs?: CronJob[] }>('cron_list'),
      transport.invoke<Record<string, unknown>>('auth_status'),
    ]).then(([settingsResult, cronResult, authResult]) => {
      setSettings(settingsResult.settings || fallback);
      setCron(cronResult.jobs || []);
      setAuth(authResult);
    }).catch(() => undefined);
    requestAnimationFrame(() => dialog.current?.focus());
    return () => {
      previousFocus.current?.focus();
      previousFocus.current = null;
    };
  }, [open, transport]);

  if (!open) return null;
  const persist = async (next: ProductSettings) => {
    setSettings(next);
    await transport.invoke('settings_set', next as unknown as Record<string, unknown>);
    setSaved('Saved');
    window.setTimeout(() => setSaved(''), 1200);
  };

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          onClose();
          return;
        }
        if (event.key !== 'Tab' || !dialog.current) return;
        const focusable = Array.from(
          dialog.current.querySelectorAll<HTMLElement>(
            'button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])'
          )
        );
        if (!focusable.length) {
          event.preventDefault();
          dialog.current.focus();
          return;
        }
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }}
    >
      <div className="settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title" tabIndex={-1} ref={dialog}>
        <header>
          <div><span className="settings-mark"><Icon name="settings" /></span><div><h2 id="settings-title">Settings</h2><span>Product preferences and Rust-owned controls</span></div></div>
          <button type="button" aria-label="Close settings" onClick={onClose}><Icon name="close" /></button>
        </header>
        <div className="settings-content">
          <section>
            <h3>Appearance</h3>
            <div className="settings-row">
              <div><strong>Theme</strong><span>Choose the workbench color scheme.</span></div>
              <select value={theme} onChange={(event) => onTheme(event.target.value as 'dark' | 'light')}>
                <option value="dark">Dark</option>
                <option value="light">Light</option>
              </select>
            </div>
          </section>
          <section>
            <h3>Work controls</h3>
            <div className="settings-row">
              <div><strong>Work isolation</strong><span>Presentation intent; enforcement is reported separately.</span></div>
              <select
                value={settings.work_isolation}
                onChange={(event) => void persist({ ...settings, work_isolation: event.target.value as ProductSettings['work_isolation'] })}
              >
                <option value="shared">Shared workbench</option>
                <option value="project_bound">Project-bound intent</option>
                <option value="isolated_profiles">Isolated profiles intent</option>
              </select>
            </div>
            <label className="settings-row">
              <div><strong>Concurrent projects</strong><span>Allow more than one project to own work.</span></div>
              <input
                type="checkbox"
                checked={settings.allow_concurrent_projects}
                onChange={(event) => void persist({ ...settings, allow_concurrent_projects: event.target.checked })}
              />
            </label>
            <div className={`settings-callout${settings.enforcement_active ? ' is-ready' : ''}`}>
              <Icon name={settings.enforcement_active ? 'check' : 'warning'} />
              <span>{settings.enforcement_active ? 'Runtime enforcement reports active.' : 'Isolation is configured intent; enforcement is not confirmed.'}</span>
            </div>
          </section>
          <section>
            <h3>Authentication</h3>
            <div className="settings-row">
              <div><strong>Credential state</strong><span>{String(auth.mode || (auth.present ? 'Available' : 'Not configured'))}</span></div>
              <button type="button" onClick={() => void transport.invoke('auth_import_cli')}>Import CLI auth</button>
            </div>
            <div className="settings-row">
              <div><strong>Hermes import</strong><span>Imports compatible credentials only; Hermes files remain read-only.</span></div>
              <button type="button" onClick={() => void transport.invoke('auth_import_hermes')}>Import</button>
            </div>
          </section>
          <section>
            <h3>Schedules</h3>
            {cron.length ? cron.map((job) => (
              <div className="settings-row" key={job.id}>
                <div><strong>{job.name}</strong><span>Every {formatDuration(job.every_secs)} · {job.last_status || 'Not run'}</span></div>
                <span className={`state-chip${job.enabled ? ' is-ready' : ''}`}>{job.enabled ? 'Enabled' : 'Disabled'}</span>
              </div>
            )) : <p className="panel-muted">No cron schedules.</p>}
          </section>
        </div>
        <footer><span>{saved}</span><button type="button" onClick={onClose}>Done</button></footer>
      </div>
    </div>
  );
}

function formatDuration(seconds: number) {
  if (seconds % 86400 === 0) return `${seconds / 86400}d`;
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  return `${seconds}s`;
}
