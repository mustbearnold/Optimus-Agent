import { useEffect, useMemo, useState } from 'react';
import type {
  DeveloperAccess,
  DeveloperCapabilities,
  DeveloperScope,
  DeveloperSupervisorStatus,
  OptimusTransport,
  Project,
} from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

const CONFIRMATION = 'I understand Developer Full Access risks';
const capabilityLabels: Record<keyof DeveloperCapabilities, { label: string; description: string }> = {
  workspace_files: { label: 'Workspace files', description: 'Read, create, modify, rename, and delete files in scope.' },
  terminal_execution: { label: 'Terminal execution', description: 'Run development commands through the scoped host boundary.' },
  process_management: { label: 'Process management', description: 'Start, stop, restart, and inspect local development processes.' },
  package_installation: { label: 'Package installation', description: 'Install dependencies and development tools locally.' },
  network_access: { label: 'Network access', description: 'Reach network resources needed for development.' },
  external_services: { label: 'External services', description: 'Use local or remote development services outside the workspace.' },
  production_systems: { label: 'Production systems', description: 'Intentionally unavailable in this local development mode.' },
  secrets: { label: 'Secrets', description: 'Read secret material only when explicitly enabled.' },
};
const capabilityKeys = Object.keys(capabilityLabels) as Array<keyof DeveloperCapabilities>;

type Props = {
  transport: OptimusTransport;
  projects: Project[];
  sessionId?: string | null;
  value?: DeveloperAccess;
  onValue: (value: DeveloperAccess) => void;
};

export function DeveloperAccessPanel({ transport, projects, sessionId, value, onValue }: Props) {
  const [grant, setGrant] = useState<DeveloperAccess>(() => value || disabledAccess());
  const [supervisor, setSupervisor] = useState<DeveloperSupervisorStatus | null>(null);
  const [logs, setLogs] = useState('');
  const [actionLogs, setActionLogs] = useState('');
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [binary, setBinary] = useState('');
  const [workspace, setWorkspace] = useState('');
  const [port, setPort] = useState('17866');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [buildLogs, setBuildLogs] = useState('');

  useEffect(() => {
    if (value) setGrant(value);
  }, [value]);

  useEffect(() => {
    let live = true;
    void transport.invoke<{ developer_access?: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>('developer_access_get')
      .then((result) => {
        if (!live) return;
        if (result.developer_access) {
          setGrant(result.developer_access);
          const root = scopeRoots(result.developer_access.scope)[0] || '';
          setWorkspace(root);
          if (root) setBinary(`${root}/target/debug/optimus-desktop`);
        }
        if (result.supervisor) setSupervisor(result.supervisor);
      })
      .catch(() => undefined);
    return () => { live = false; };
  }, [transport]);

  const selectedRoots = useMemo(() => scopeRoots(grant.scope), [grant.scope]);
  const updateGrant = (patch: Partial<DeveloperAccess>) => {
    setGrant((current) => ({ ...current, ...patch }));
    setMessage('');
  };
  const updateCapabilities = (patch: Partial<DeveloperCapabilities>) => {
    setGrant((current) => ({ ...current, capabilities: { ...current.capabilities, ...patch } }));
    setMessage('');
  };
  const chooseFolder = async (multiple: boolean) => {
    const picked = await transport.pickFolder();
    if (!picked.ok || !picked.path) return;
    const next = multiple
      ? [...new Set([...scopeRoots(grant.scope), picked.path])]
      : [picked.path];
    const scope: DeveloperScope = multiple
      ? { kind: 'selected_directories', roots: next }
      : { kind: 'selected_repository', root: picked.path };
    updateGrant({ scope });
    setWorkspace(picked.path);
    if (!binary) setBinary(`${picked.path}/target/debug/optimus-desktop`);
  };
  const setScopeKind = (kind: DeveloperScope['kind']) => {
    if (kind === 'entire_local_machine') {
      updateGrant({ scope: { kind } });
      return;
    }
    if (kind === 'selected_repository') {
      updateGrant({ scope: { kind, root: selectedRoots[0] || projects[0]?.primaryRoot || '' } });
      return;
    }
    updateGrant({ scope: { kind, roots: selectedRoots } });
  };
  const removeRoot = (root: string) => {
    if (grant.scope.kind === 'selected_repository') {
      updateGrant({ scope: { kind: 'selected_repository', root: '' } });
    } else if (grant.scope.kind === 'selected_directories') {
      updateGrant({ scope: { kind: 'selected_directories', roots: grant.scope.roots.filter((item) => item !== root) } });
    }
  };
  const enable = async () => {
    setBusy(true);
    setMessage('');
    try {
      const result = await transport.invoke<{ developer_access: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>(
        'developer_access_enable',
        { confirmation: CONFIRMATION, grant: { ...grant, enabled: true } },
      );
      setGrant(result.developer_access);
      onValue(result.developer_access);
      if (result.supervisor) setSupervisor(result.supervisor);
      setConfirmOpen(false);
      setMessage('Developer Full Access is active for this scope.');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };
  const revoke = async () => {
    setBusy(true);
    try {
      const result = await transport.invoke<{ developer_access: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>('developer_access_revoke');
      setGrant(result.developer_access);
      onValue(result.developer_access);
      if (result.supervisor) setSupervisor(result.supervisor);
      setMessage('Access revoked. Any development instance was stopped.');
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };
  const supervisorAction = async (method: 'developer_supervisor_launch' | 'developer_supervisor_build_launch' | 'developer_supervisor_stop' | 'developer_supervisor_restart' | 'developer_supervisor_rollback' | 'developer_emergency_stop') => {
    setBusy(true);
    setMessage('');
    try {
      const params = method === 'developer_supervisor_launch'
        ? { binary, workspace: workspace || selectedRoots[0] || '', port: Number(port), ...(sessionId ? { session_id: sessionId } : {}) }
        : method === 'developer_supervisor_build_launch'
          ? { workspace: workspace || selectedRoots[0] || '', port: Number(port), surface: 'desktop', ...(sessionId ? { session_id: sessionId } : {}) }
          : {};
      const result = await transport.invoke<DeveloperSupervisorStatus>(method, params);
      setSupervisor(result);
      setMessage(method === 'developer_emergency_stop'
        ? 'Emergency stop sent to the development instance.'
        : method === 'developer_supervisor_build_launch'
          ? `Development desktop build completed${sessionId ? '; selected session handed off' : ''}; supervisor ${result.status}.`
          : `Supervisor ${result.status}.`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };
  const refreshLog = async () => {
    const result = await transport.invoke<{ lines?: string; actions?: string; build?: string }>('developer_supervisor_log', { lines: 120 });
    setLogs(result.lines || '');
    setActionLogs(result.actions || '');
    setBuildLogs(result.build || '');
  };

  return (
    <div className="developer-access-panel">
      <div className={`developer-access-banner${grant.enabled ? ' is-active' : ''}`} role="status">
        <Icon name={grant.enabled ? 'terminal' : 'shield'} />
        <div>
          <strong>{grant.enabled ? 'Developer Full Access active' : 'Developer Full Access is off'}</strong>
          <span>{grant.enabled ? `${grant.scope_label || scopeLabel(grant.scope)} · enforced by the capability broker` : 'A local, explicit grant is required before development authority is available.'}</span>
        </div>
        <span className={`state-chip${grant.enabled ? ' is-ready' : ''}`}>{grant.enabled ? 'Active' : 'Disabled'}</span>
      </div>

      <fieldset className="developer-access-fieldset">
        <legend>Scope</legend>
        <div className="developer-scope-options">
          <ScopeChoice checked={grant.scope.kind === 'selected_repository'} onChange={() => setScopeKind('selected_repository')} title="Selected repository" description="Normal choice: the agent can work inside one repository." />
          <ScopeChoice checked={grant.scope.kind === 'selected_directories'} onChange={() => setScopeKind('selected_directories')} title="Selected directories" description="Grant more than one local source without opening the whole machine." />
          <ScopeChoice checked={grant.scope.kind === 'entire_local_machine'} onChange={() => setScopeKind('entire_local_machine')} title="Entire local machine" description="Advanced opt-in. Every local path is in scope." />
        </div>
        {grant.scope.kind !== 'entire_local_machine' ? (
          <div className="developer-roots">
            {grant.scope.kind === 'selected_repository' ? (
              <label className="developer-path-field">
                <span>Repository root</span>
                <input value={grant.scope.root} onChange={(event) => {
                  if (grant.scope.kind !== 'selected_repository') return;
                  updateGrant({ scope: { kind: 'selected_repository', root: event.target.value, root_hash: grant.scope.root_hash } });
                }} placeholder="/projects/optimus-agent" />
                <button type="button" onClick={() => void chooseFolder(false)}>Browse</button>
              </label>
            ) : (
              <>
                <div className="developer-root-list">
                  {grant.scope.roots.map((root) => <span className="developer-root" key={root}><span title={root}>{root}</span><button type="button" aria-label={'Remove ' + root} onClick={() => removeRoot(root)}>×</button></span>)}
                  {!grant.scope.roots.length ? <span className="panel-muted">No directories selected.</span> : null}
                </div>
                <button type="button" className="developer-secondary-action" onClick={() => void chooseFolder(true)}><Icon name="plus" /> Add directory</button>
              </>
            )}
          </div>
        ) : (
          <div className="developer-warning"><Icon name="warning" /><span>Whole-machine access can expose private data and cause irreversible changes. Keep production systems disabled.</span></div>
        )}
      </fieldset>

      <fieldset className="developer-access-fieldset">
        <legend>Capabilities</legend>
        <div className="developer-capabilities">
          {capabilityKeys.map((key) => {
            const item = capabilityLabels[key];
            const locked = key === 'production_systems';
            return <label className={`developer-capability${locked ? ' is-locked' : ''}`} key={key}>
              <input type="checkbox" checked={grant.capabilities[key]} disabled={locked} onChange={(event) => updateCapabilities({ [key]: event.target.checked })} />
              <span><strong>{item.label}</strong><small>{item.description}</small></span>
              {locked ? <em>Disabled</em> : null}
            </label>;
          })}
        </div>
        <label className="developer-check-row"><input type="checkbox" checked={grant.pause_before_destructive} onChange={(event) => updateGrant({ pause_before_destructive: event.target.checked })} /><span><strong>Pause before destructive actions</strong><small>Keep a confirmation boundary for deletes, process stops, and other destructive effects.</small></span></label>
        <label className="developer-check-row"><input type="checkbox" checked={grant.checkpoint_on_mutation} onChange={(event) => updateGrant({ checkpoint_on_mutation: event.target.checked })} /><span><strong>Checkpoint mutations</strong><small>Record a checkpoint before file and development-instance changes when the repository supports it.</small></span></label>
      </fieldset>

      <div className="developer-access-actions">
        {grant.enabled ? <button type="button" className="developer-danger-action" disabled={busy} onClick={() => void revoke()}>Revoke access</button> : <button type="button" className="primary-action" disabled={busy} onClick={() => setConfirmOpen(true)}>Enable Developer Full Access</button>}
        {message ? <span className="developer-action-message" role="status">{message}</span> : null}
      </div>

      {grant.enabled ? (
        <section className="developer-supervisor" aria-labelledby="developer-supervisor-title">
          <div className="developer-supervisor-heading"><div><h5 id="developer-supervisor-title">Development supervisor</h5><p>Runs a separate Optimus instance and keeps this control channel alive while it is rebuilt.</p></div><span className={`state-chip${supervisor?.healthy ? ' is-ready' : ''}`}>{supervisor?.status || 'idle'}</span></div>
          <div className="developer-launch-fields">
            <label><span>Development binary</span><input value={binary} onChange={(event) => setBinary(event.target.value)} placeholder="/path/to/optimus" /></label>
            <label><span>Workspace</span><input value={workspace} onChange={(event) => setWorkspace(event.target.value)} placeholder={selectedRoots[0] || '/path/to/repository'} /></label>
            <label><span>Port</span><input value={port} inputMode="numeric" onChange={(event) => setPort(event.target.value)} /></label>
          </div>
          <p className="developer-handoff-note">{sessionId ? 'The selected session will be opened in the separate development window after its health check passes.' : 'Select a session in the workbench to carry it into the separate development window.'}</p>
          <div className="developer-supervisor-actions">
            <button type="button" className="primary-action" disabled={busy || !(workspace || selectedRoots[0])} onClick={() => void supervisorAction('developer_supervisor_build_launch')}>Build + launch development desktop</button>
            <button type="button" className="primary-action" disabled={busy || !binary || !(workspace || selectedRoots[0])} onClick={() => void supervisorAction('developer_supervisor_launch')}>Launch development copy</button>
            <button type="button" disabled={busy} onClick={() => void supervisorAction('developer_supervisor_restart')}>Restart</button>
            <button type="button" disabled={busy || !supervisor?.previous_available} onClick={() => void supervisorAction('developer_supervisor_rollback')}>Rollback</button>
            <button type="button" disabled={busy} onClick={() => void supervisorAction('developer_supervisor_stop')}>Stop</button>
            <button type="button" className="developer-danger-action" disabled={busy} onClick={() => void supervisorAction('developer_emergency_stop')}>Emergency stop</button>
            <button type="button" disabled={busy} onClick={() => void refreshLog()}>View live logs</button>
          </div>
          {supervisor?.last_error ? <div className="developer-warning"><Icon name="warning" /><span>{supervisor.last_error}</span></div> : null}
          {logs || actionLogs || buildLogs ? <div className="developer-log-stack">
            {actionLogs ? <div><span className="developer-log-label">Live action log</span><pre className="developer-log" aria-label="Developer Full Access action log">{actionLogs}</pre></div> : null}
            {buildLogs ? <div><span className="developer-log-label">Development build log</span><pre className="developer-log" aria-label="Development build log">{buildLogs}</pre></div> : null}
            {logs ? <div><span className="developer-log-label">Development instance log</span><pre className="developer-log" aria-label="Development supervisor log">{logs}</pre></div> : null}
          </div> : null}
        </section>
      ) : null}

      {confirmOpen ? (
        <div className="developer-confirm" role="dialog" aria-modal="true" aria-labelledby="developer-confirm-title">
          <div className="developer-confirm-card">
            <Icon name="warning" />
            <h5 id="developer-confirm-title">Enable Developer Full Access?</h5>
            <p>Optimus will be able to execute commands and modify content within the selected scope. It may delete files, install software, access network resources, expose private data, or cause data loss. Use a backup or disposable development environment.</p>
            <p className="developer-confirm-scope">Scope: <strong>{scopeLabel(grant.scope)}</strong></p>
            <div className="developer-supervisor-actions"><button type="button" onClick={() => setConfirmOpen(false)}>Cancel</button><button type="button" className="primary-action" disabled={busy} onClick={() => void enable()}>I understand and enable</button></div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function ScopeChoice({ checked, onChange, title, description }: { checked: boolean; onChange: () => void; title: string; description: string }) {
  return <label className={`developer-scope-choice${checked ? ' is-selected' : ''}`}><input type="radio" checked={checked} onChange={onChange} name="developer-scope" /><span><strong>{title}</strong><small>{description}</small></span></label>;
}

function disabledAccess(): DeveloperAccess {
  return {
    enabled: false,
    scope: { kind: 'selected_repository', root: '' },
    capabilities: { workspace_files: true, terminal_execution: true, process_management: true, package_installation: true, network_access: true, external_services: false, production_systems: false, secrets: false },
    pause_before_destructive: true,
    checkpoint_on_mutation: true,
  };
}

function scopeRoots(scope: DeveloperScope) {
  return scope.kind === 'selected_repository' ? (scope.root ? [scope.root] : []) : scope.kind === 'selected_directories' ? scope.roots : [];
}

function scopeLabel(scope: DeveloperScope) {
  if (scope.kind === 'entire_local_machine') return 'Entire local machine';
  const roots = scopeRoots(scope);
  return scope.kind === 'selected_repository' ? `Repository · ${roots[0] || 'not selected'}` : `${roots.length} selected director${roots.length === 1 ? 'y' : 'ies'}`;
}
