import type { Approval, Job, RunStatus, SessionMeta } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

export function TaskPanel({
  open,
  jobs,
  approvals,
  runSession,
  runStatus,
  onClose,
  onStop,
}: {
  open: boolean;
  jobs: Job[];
  approvals: Approval[];
  runSession: SessionMeta | null;
  runStatus: RunStatus;
  onClose: () => void;
  onStop: () => void;
}) {
  if (!open) return null;
  const busy = ['submitting', 'working', 'awaiting_approval', 'cancelling'].includes(runStatus);
  return (
    <div className="floating-panel task-panel" role="dialog" aria-modal="false" aria-label="Active tasks">
      <header>
        <div><Icon name="tasks" /><strong>Tasks</strong></div>
        <button type="button" aria-label="Close tasks" onClick={onClose}><Icon name="close" /></button>
      </header>
      {runSession ? (
        <article className="foreground-task">
          <span className={`task-pulse${busy ? ' is-active' : ''}`} />
          <div>
            <strong>{runSession.title || runSession.id}</strong>
            <span>{statusLabel(runStatus)}</span>
          </div>
          {busy ? <button type="button" onClick={onStop}>Stop</button> : null}
        </article>
      ) : <p className="panel-muted">No foreground run.</p>}
      <div className="task-summary">
        <span><strong>{jobs.length}</strong> durable jobs</span>
        <span><strong>{approvals.length}</strong> approvals</span>
      </div>
      {jobs.slice(0, 5).map((job) => (
        <article className="task-row" key={job.job_id}>
          <span className="status-dot" />
          <div><strong>{job.label || job.job_id}</strong><span>{job.status || 'Unknown'}</span></div>
        </article>
      ))}
    </div>
  );
}

function statusLabel(status: RunStatus) {
  return status === 'awaiting_approval'
    ? 'Awaiting approval'
    : status === 'cancelling'
      ? 'Cancellation requested'
      : status === 'disconnected'
        ? 'Connection lost'
        : status[0]?.toUpperCase() + status.slice(1);
}
