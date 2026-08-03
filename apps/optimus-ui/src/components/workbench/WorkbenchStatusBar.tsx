import type { Project, RunStatus } from '../../ipc/contracts';
import type { ComposerSettings } from '../../state/composerStore';
import { Icon } from '../chrome/Icon';

type Props = {
  status: RunStatus;
  statusText: string;
  settings: ComposerSettings;
  project: Project | null;
};

export function WorkbenchStatusBar({ status, statusText, settings, project }: Props) {
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
  const model = settings.model || (settings.provider === 'offline' ? 'Offline' : 'Auto');
  const thinking = settings.thinking ? capitalize(settings.thinking) : 'High';
  const access = accessLabel(settings.access);

  return (
    <footer className="workbench-statusbar" aria-label="Session status">
      <span className={`workbench-status-state${busy ? ' is-working' : ''}${attention ? ' is-attention' : ''}${failed ? ' is-failed' : ''}`}>
        <span className="workbench-status-dot" aria-hidden="true" />
        <span>{stateLabel}</span>
        {statusText ? <span className="workbench-status-detail" title={statusText}>{statusText}</span> : null}
      </span>
      <span className="workbench-status-spacer" />
      <span className="workbench-status-segment" title={project?.primaryRoot || 'No project folder'}>
        <Icon name="folder" />
        <span>{project?.name || 'Local session'}</span>
      </span>
      <span className="workbench-status-segment" title={`Model · ${model}`}>
        <Icon name="agent" />
        <span>{model}</span>
      </span>
      <span className="workbench-status-segment workbench-status-secondary" title={`Thinking · ${thinking}`}>
        <Icon name="source" />
        <span>{thinking}</span>
      </span>
      <span className="workbench-status-segment workbench-status-secondary" title={`Access · ${access}`}>
        <Icon name="shield" />
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
    unrestricted_host: 'Unrestricted host',
  };
  return labels[value] || 'Standard';
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}
