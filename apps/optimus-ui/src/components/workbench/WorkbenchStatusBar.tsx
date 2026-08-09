import type { DeveloperAccess, Project, RunStatus } from '../../ipc/contracts';
import type { ComposerSettings } from '../../state/composerStore';
import { Icon } from '../chrome/Icon';

type Props = {
  status: RunStatus;
  statusText: string;
  settings: ComposerSettings;
  developerAccess?: DeveloperAccess;
  project: Project | null;
};

export function WorkbenchStatusBar({ status, statusText, settings, developerAccess, project }: Props) {
  const busy = status === 'submitting' || status === 'working' || status === 'cancelling';
  const attention = status === 'awaiting_approval';
  const failed = status === 'failed' || status === 'disconnected';
  const stateLabel = busy
    ? 'Working'
    : attention
      ? 'Approval needed'
      : failed
        ? 'Needs attention'
        : status === 'completed'
          ? 'Completed'
          : status === 'cancelled'
            ? 'Cancelled'
            : 'Ready';
  // A terminal run reports its own name as the detail ("Completed"), which the
  // state label already says. Rendering both produced "Completed Completed".
  const detail =
    statusText.trim().toLowerCase() === stateLabel.toLowerCase() ? '' : statusText;
  const model = settings.model || (settings.provider === 'offline' ? 'Offline' : 'Auto');
  const thinking = settings.thinking ? capitalize(settings.thinking) : 'Minimal';
  const access = developerAccess?.enabled
    ? `Developer · ${developerScopeLabel(developerAccess)}`
    : accessLabel(settings.access);

  return (
    <footer className="workbench-statusbar" aria-label="Session status">
      <span className={`workbench-status-state${busy ? ' is-working' : ''}${attention ? ' is-attention' : ''}${failed ? ' is-failed' : ''}`}>
        <span className="workbench-status-dot" aria-hidden="true" />
        <span>{stateLabel}</span>
        {detail ? <span className="workbench-status-detail" title={detail}>{detail}</span> : null}
      </span>
      <span className="workbench-status-spacer" />
      <span className="workbench-status-segment workbench-status-project" title={project?.primaryRoot || 'No project folder'}>
        <Icon name="folder" />
        <span>{project?.name || 'Local session'}</span>
      </span>
      <span className="workbench-status-segment workbench-status-primary" title={`Model · ${model}`}>
        <Icon name="agent" />
        <span>{model}</span>
      </span>
      <span className="workbench-status-segment workbench-status-secondary workbench-status-thinking" title={`Thinking · ${thinking}`}>
        <Icon name="source" />
        <span>{thinking}</span>
      </span>
      <span className="workbench-status-segment workbench-status-secondary workbench-status-access" title={`Access · ${access}`}>
        <Icon name={developerAccess?.enabled ? 'terminal' : 'shield'} />
        <span>{access}</span>
      </span>
    </footer>
  );
}

function accessLabel(value: string) {
  const labels: Record<string, string> = {
    standard: 'Standard',
    review_changes: 'Review changes',
    read_only: 'Read only',
    full_project: 'Full project',
    developer_full_access: 'Developer Full Access',
    unrestricted_host: 'Unrestricted host',
  };
  return labels[value] || 'Standard';
}

function developerScopeLabel(access: DeveloperAccess) {
  if (access.scope.kind === 'entire_local_machine') return 'machine';
  if (access.scope.kind === 'selected_repository') return 'repository';
  return `${access.scope.roots.length} dirs`;
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
