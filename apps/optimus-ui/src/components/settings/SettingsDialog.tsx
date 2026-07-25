import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import type {
  CronJob,
  OptimusTransport,
  ProductSettings,
  Project,
} from '../../ipc/contracts';
import { Icon, type IconName } from '../chrome/Icon';
import { CronWorkbench } from '../cron/CronWorkbench';

const fallback: ProductSettings = {
  work_isolation: 'shared',
  allow_concurrent_projects: false,
  enforcement_active: false,
};

type SettingsSection =
  | 'general'
  | 'appearance'
  | 'agent'
  | 'work'
  | 'projects'
  | 'browser'
  | 'execution'
  | 'approvals'
  | 'memory'
  | 'automations'
  | 'authentication'
  | 'accessibility'
  | 'advanced';

const sections: Array<{
  id: SettingsSection;
  label: string;
  icon: IconName;
}> = [
  { id: 'general', label: 'General', icon: 'settings' },
  { id: 'appearance', label: 'Appearance', icon: 'appearance' },
  { id: 'agent', label: 'Agent & models', icon: 'agent' },
  { id: 'work', label: 'Work isolation', icon: 'shield' },
  { id: 'projects', label: 'Projects', icon: 'project' },
  { id: 'browser', label: 'Browser', icon: 'globe' },
  { id: 'execution', label: 'Terminal & execution', icon: 'terminal' },
  { id: 'approvals', label: 'Tools & approvals', icon: 'check' },
  { id: 'memory', label: 'Memory', icon: 'memory' },
  { id: 'automations', label: 'Automations', icon: 'automation' },
  { id: 'authentication', label: 'Authentication', icon: 'shield' },
  { id: 'accessibility', label: 'Accessibility', icon: 'accessibility' },
  { id: 'advanced', label: 'Advanced', icon: 'advanced' },
];

export function SettingsDialog({
  open,
  transport,
  theme,
  projects,
  onTheme,
  onManageProject,
  onClose,
}: {
  open: boolean;
  transport: OptimusTransport;
  theme: 'dark' | 'light';
  projects: Project[];
  onTheme: (theme: 'dark' | 'light') => void;
  onManageProject: (project: Project) => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const [active, setActive] = useState<SettingsSection>('general');
  const [settings, setSettings] = useState(fallback);
  const [cron, setCron] = useState<CronJob[]>([]);
  const [auth, setAuth] = useState<Record<string, unknown>>({});
  const [saved, setSaved] = useState('');
  const [density, setDensity] = useState<'comfortable' | 'compact'>(() =>
    localStorage.getItem('optimus.react.density') === 'compact' ? 'compact' : 'comfortable'
  );

  useEffect(() => {
    document.documentElement.dataset.density = density;
    localStorage.setItem('optimus.react.density', density);
  }, [density]);

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
  const section = sections.find((item) => item.id === active) || sections[0];

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          onClose();
          return;
        }
        trapFocus(event, dialog.current);
      }}
    >
      <div
        className="settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        tabIndex={-1}
        ref={dialog}
      >
        <header>
          <div>
            <span className="settings-mark"><Icon name="settings" /></span>
            <div>
              <h2 id="settings-title">Settings</h2>
              <span>Optimus Agent</span>
            </div>
          </div>
          <button type="button" aria-label="Close settings" onClick={onClose}>
            <Icon name="close" />
          </button>
        </header>

        <div className="settings-layout">
          <nav aria-label="Settings categories" className="settings-nav">
            {sections.map((item) => (
              <button
                type="button"
                className={item.id === active ? 'is-active' : ''}
                aria-current={item.id === active ? 'page' : undefined}
                onClick={() => setActive(item.id)}
                key={item.id}
              >
                <Icon name={item.icon} />
                <span>{item.label}</span>
              </button>
            ))}
          </nav>

          <main className="settings-content" aria-label={`${section.label} settings`}>
            <div className="settings-heading">
              <h3>{section.label}</h3>
              <p>{sectionDescription(active)}</p>
            </div>

            {active === 'general' ? (
              <SettingsGroup title="Startup">
                <SettingRow title="Open previous layout" description="Restore panel widths, the selected evidence surface, and terminal height.">
                  <span className="state-chip is-ready">On</span>
                </SettingRow>
                <SettingRow title="Project catalog" description="Local multi-folder projects are restored before session assignment.">
                  <span className="state-chip is-ready">Local</span>
                </SettingRow>
              </SettingsGroup>
            ) : null}

            {active === 'appearance' ? (
              <>
                <SettingsGroup title="Theme">
                  <SettingRow title="Color theme" description="Start with Codex-compatible neutral geometry; Optimus theme tokens remain customizable.">
                    <select value={theme} onChange={(event) => onTheme(event.target.value as 'dark' | 'light')}>
                      <option value="light">Light</option>
                      <option value="dark">Dark</option>
                    </select>
                  </SettingRow>
                  <SettingRow title="Interface density" description="Compact keeps the same hierarchy with shorter navigation rows.">
                    <select value={density} onChange={(event) => setDensity(event.target.value as 'comfortable' | 'compact')}>
                      <option value="comfortable">Comfortable</option>
                      <option value="compact">Compact</option>
                    </select>
                  </SettingRow>
                </SettingsGroup>
                <SettingsCallout>
                  Optimus uses the native system UI stack and neutral tokens derived from the captured Codex layout. No proprietary fonts or icons are bundled.
                </SettingsCallout>
              </>
            ) : null}

            {active === 'agent' ? (
              <SettingsGroup title="Defaults">
                <SettingRow title="Provider and model" description="Choose per task in the composer so the active runtime contract remains visible.">
                  <button type="button" disabled>Per composer</button>
                </SettingRow>
                <SettingRow title="Thinking level" description="Kept beside the message action instead of hidden in a global preference.">
                  <button type="button" disabled>Per composer</button>
                </SettingRow>
              </SettingsGroup>
            ) : null}

            {active === 'work' ? (
              <SettingsGroup title="Project boundaries">
                <SettingRow title="Work isolation" description="Presentation intent; enforcement is reported separately.">
                  <select
                    value={settings.work_isolation}
                    onChange={(event) => void persist({
                      ...settings,
                      work_isolation: event.target.value as ProductSettings['work_isolation'],
                    })}
                  >
                    <option value="shared">Shared workbench</option>
                    <option value="project_bound">Project-bound intent</option>
                    <option value="isolated_profiles">Isolated profiles intent</option>
                  </select>
                </SettingRow>
                <label className="settings-row">
                  <div>
                    <strong>Concurrent projects</strong>
                    <span>Allow more than one project to own work.</span>
                  </div>
                  <input
                    type="checkbox"
                    checked={settings.allow_concurrent_projects}
                    onChange={(event) => void persist({
                      ...settings,
                      allow_concurrent_projects: event.target.checked,
                    })}
                  />
                </label>
                <div className={`settings-callout${settings.enforcement_active ? ' is-ready' : ''}`}>
                  <Icon name={settings.enforcement_active ? 'check' : 'warning'} />
                  <span>
                    {settings.enforcement_active
                      ? 'Runtime enforcement reports active.'
                      : 'Isolation is configured intent; runtime enforcement is not confirmed.'}
                  </span>
                </div>
              </SettingsGroup>
            ) : null}

            {active === 'projects' ? (
              <SettingsGroup title="Multi-folder projects">
                {projects.map((project) => (
                  <SettingRow
                    key={project.id}
                    title={project.name}
                    description={`${project.rootPaths.length} source${project.rootPaths.length === 1 ? '' : 's'} · ${project.primaryRoot || 'No primary source'}`}
                  >
                    <button type="button" onClick={() => onManageProject(project)}>Manage</button>
                  </SettingRow>
                ))}
                {!projects.length ? <p className="panel-muted">No local projects.</p> : null}
              </SettingsGroup>
            ) : null}

            {active === 'browser' ? (
              <>
                <SettingsGroup title="Preview">
                  <SettingRow title="Native preview panel" description="Sandboxed Electron child view with permission and popup denial.">
                    <span className="state-chip is-ready">Enabled</span>
                  </SettingRow>
                  <SettingRow title="Page annotations" description="Select one element and attach only its bounded role, label, URL, and geometry.">
                    <span className="state-chip is-ready">Explicit</span>
                  </SettingRow>
                </SettingsGroup>
                <SettingsCallout>
                  Remote pages do not receive Node access. Annotation capture is user-triggered, cancellable, and never copies page HTML.
                </SettingsCallout>
              </>
            ) : null}

            {active === 'execution' ? (
              <SettingsGroup title="Terminal">
                <SettingRow title="Durable commands" description="Commands travel through the Rust-owned term_run path.">
                  <span className="state-chip is-ready">Protected</span>
                </SettingRow>
                <SettingRow title="Panel location" description="Resizable bottom panel; compact windows promote it to a dedicated surface.">
                  <span className="state-chip">Bottom</span>
                </SettingRow>
              </SettingsGroup>
            ) : null}

            {active === 'approvals' ? (
              <SettingsGroup title="Effects">
                <SettingRow title="High-risk actions" description="Exact durable effects require explicit approval before execution.">
                  <span className="state-chip is-ready">Ask</span>
                </SettingRow>
                <SettingRow title="Tool policies" description="Runtime pack contracts remain authoritative.">
                  <button type="button" disabled>Runtime-owned</button>
                </SettingRow>
              </SettingsGroup>
            ) : null}

            {active === 'memory' ? (
              <SettingsGroup title="Memory systems">
                <SettingRow title="Runtime memory" description="Separate from sessions, skills, project knowledge, and Engineering Memory.">
                  <button type="button" disabled>Runtime-owned</button>
                </SettingRow>
                <SettingRow title="Engineering Memory" description="Generated repository evidence for the implementation—not chat memory.">
                  <span className="state-chip">Repository</span>
                </SettingRow>
              </SettingsGroup>
            ) : null}

            {active === 'automations' ? (
              <SettingsGroup title="Schedules">
                <CronWorkbench transport={transport} active={open && active === 'automations'} />
              </SettingsGroup>
            ) : null}

            {active === 'authentication' ? (
              <SettingsGroup title="Credentials">
                <SettingRow title="Credential state" description={String(auth.mode || (auth.present ? 'Available' : 'Not configured'))}>
                  <button type="button" onClick={() => void transport.invoke('auth_import_cli')}>Import CLI auth</button>
                </SettingRow>
                <SettingRow title="Hermes import" description="Imports compatible credentials only; Hermes files remain read-only.">
                  <button type="button" onClick={() => void transport.invoke('auth_import_hermes')}>Import</button>
                </SettingRow>
              </SettingsGroup>
            ) : null}

            {active === 'accessibility' ? (
              <SettingsGroup title="Display and motion">
                <SettingRow title="Reduced motion" description="System preference disables non-essential interface motion.">
                  <span className="state-chip is-ready">System</span>
                </SettingRow>
                <SettingRow title="Keyboard navigation" description="Roving tabs, visible focus, and modal focus containment are enabled.">
                  <span className="state-chip is-ready">Enabled</span>
                </SettingRow>
                <SettingRow title="Zoom" description="Native Electron zoom controls are not exposed in this build.">
                  <button type="button" disabled>Not available</button>
                </SettingRow>
              </SettingsGroup>
            ) : null}

            {active === 'advanced' ? (
              <SettingsGroup title="Runtime surface">
                <SettingRow title="Transport" description="The active UI transport is reported here without exposing credentials.">
                  <span className="state-chip">{transport.kind}</span>
                </SettingRow>
                <SettingRow title="Developer tools" description="No hidden production toggle is implemented.">
                  <button type="button" disabled>Not available</button>
                </SettingRow>
              </SettingsGroup>
            ) : null}
          </main>
        </div>

        <footer>
          <span role="status">{saved}</span>
          <button type="button" onClick={onClose}>Done</button>
        </footer>
      </div>
    </div>
  );
}

function SettingsGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-group">
      <h4>{title}</h4>
      <div>{children}</div>
    </section>
  );
}

function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div>
        <strong>{title}</strong>
        <span>{description}</span>
      </div>
      {children}
    </div>
  );
}

function SettingsCallout({ children }: { children: ReactNode }) {
  return (
    <div className="settings-callout">
      <Icon name="info" />
      <span>{children}</span>
    </div>
  );
}

function sectionDescription(section: SettingsSection) {
  const descriptions: Record<SettingsSection, string> = {
    general: 'Startup behavior and workbench persistence.',
    appearance: 'Codex-aligned geometry with Optimus-owned theme tokens.',
    agent: 'Visible model and reasoning controls for each task.',
    work: 'How projects declare ownership and isolation intent.',
    projects: 'Group several folder sources under one durable project identity.',
    browser: 'Native preview, navigation, and bounded page context.',
    execution: 'Terminal placement, output, and durable command flow.',
    approvals: 'Permission boundaries for tools and side effects.',
    memory: 'Distinct runtime, project, session, and engineering memory systems.',
    automations: 'Long-running schedules owned by the runtime.',
    authentication: 'Credential imports and provider state.',
    accessibility: 'Keyboard, motion, focus, and reflow behavior.',
    advanced: 'Runtime diagnostics and intentionally unavailable controls.',
  };
  return descriptions[section];
}

function formatDuration(seconds: number) {
  if (seconds % 86400 === 0) return `${seconds / 86400}d`;
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  return `${seconds}s`;
}

function trapFocus(event: KeyboardEvent, container: HTMLElement | null) {
  if (event.key !== 'Tab' || !container) return;
  const focusable = Array.from(
    container.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])'
    )
  );
  if (!focusable.length) {
    event.preventDefault();
    container.focus();
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
}
