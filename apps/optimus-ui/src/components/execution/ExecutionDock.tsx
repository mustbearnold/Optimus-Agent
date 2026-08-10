import { useCallback, useEffect, useState } from 'react';
import { useAlive } from '../../hooks/useAlive';
import type { Approval, Job } from '../../ipc/contracts';
import type { OptimusClient } from '../../ipc/client';
import { Icon } from '../chrome/Icon';

type TerminalResult = {
  job_id?: string;
  status?: string;
  stdout?: string;
  stderr?: string;
};

export function ExecutionDock({
  client,
  open,
  onClose,
  onState,
}: {
  client: OptimusClient;
  open: boolean;
  onClose: () => void;
  onState: (approvals: Approval[], jobs: Job[]) => void;
}) {
  const [tab, setTab] = useState<'terminal' | 'approvals' | 'jobs'>('terminal');
  const [command, setCommand] = useState('bun run --cwd apps/optimus-ui test');
  const [terminal, setTerminal] = useState<TerminalResult | null>(null);
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const alive = useAlive();

  const refresh = useCallback(async () => {
    try {
      const [nextApprovals, nextJobs] = await Promise.all([
        client.approvals.list(),
        client.jobs.list(),
      ]);
      if (!alive()) return;
      setApprovals(nextApprovals);
      setJobs(nextJobs);
      onState(nextApprovals, nextJobs);
    } catch (nextError) {
      if (!alive()) return;
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [alive, onState, client]);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  const submit = async () => {
    const value = command.trim();
    if (!value || busy) return;
    setBusy(true);
    setError('');
    try {
      const result = await client.shell.run(value);
      if (!alive()) return;
      setTerminal(result);
      if (/approval/i.test(result.status || '')) setTab('approvals');
      await refresh();
    } catch (nextError) {
      if (!alive()) return;
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      if (alive()) setBusy(false);
    }
  };

  return (
    <aside className={`execution-dock${open ? ' is-open' : ''}`} aria-label="Execution dock">
      <div className="execution-tabs" role="tablist" aria-label="Execution views">
        <button type="button" role="tab" aria-selected={tab === 'terminal'} onClick={() => setTab('terminal')}>
          <Icon name="terminal" />
          <span>Terminal</span>
        </button>
        <button type="button" role="tab" aria-selected={tab === 'approvals'} onClick={() => setTab('approvals')}>
          <Icon name="warning" />
          <span>Approvals</span>
          {approvals.length ? <span className="count-badge">{approvals.length}</span> : null}
        </button>
        <button type="button" role="tab" aria-selected={tab === 'jobs'} onClick={() => setTab('jobs')}>
          <Icon name="tasks" />
          <span>Jobs</span>
        </button>
        <span />
        <button type="button" aria-label="Refresh execution state" title="Refresh" onClick={() => void refresh()}>
          <Icon name="refresh" />
        </button>
        <button type="button" aria-label="Close execution dock" title="Close" onClick={onClose}>
          <Icon name="close" />
        </button>
      </div>

      <div className="execution-body">
        {error ? <div className="inline-notice is-error"><Icon name="warning" />{error}</div> : null}
        {tab === 'terminal' ? (
          <section className="terminal-panel" aria-label="Terminal command">
            <div className="terminal-output" aria-live="polite">
              {terminal ? (
                <>
                  <div><span className="terminal-prompt">$</span> {command}</div>
                  <div className={`terminal-state state-${(terminal.status || 'unknown').toLowerCase()}`}>
                    {terminal.status || 'Unknown'}
                  </div>
                  {terminal.stdout ? <pre>{terminal.stdout}</pre> : null}
                  {terminal.stderr ? <pre className="terminal-error">{terminal.stderr}</pre> : null}
                  {/approval/i.test(terminal.status || '') ? (
                    <p>This command is waiting for an explicit durable approval.</p>
                  ) : null}
                </>
              ) : (
                <div className="terminal-placeholder">Type a command below and press Run. High-risk commands still need approval.</div>
              )}
            </div>
            <form
              className="terminal-command"
              onSubmit={(event) => {
                event.preventDefault();
                void submit();
              }}
            >
              <span aria-hidden="true">$</span>
              <input aria-label="Terminal command" value={command} onChange={(event) => setCommand(event.target.value)} />
              <button type="submit" disabled={!command.trim() || busy}>{busy ? 'Submitting…' : 'Run'}</button>
            </form>
          </section>
        ) : null}
        {tab === 'approvals' ? (
          <section className="approval-list" aria-label="Pending approvals">
            {approvals.length ? approvals.map((approval) => (
              <article className="approval-card" key={`${approval.job_id}:${approval.node_index || 0}`}>
                <header>
                  <span className="approval-mark"><Icon name="warning" /></span>
                  <div>
                    <strong>{approval.node_label || approval.job_label || 'Durable effect'}</strong>
                    <span>{approval.job_label || approval.job_id}</span>
                  </div>
                  <span className="state-chip">Awaiting approval</span>
                </header>
                <pre>{formatEffect(approval.effect_json)}</pre>
                <div className="approval-actions">
                  <span>Review the exact effect before granting.</span>
                  <button
                    type="button"
                    onClick={async () => {
                      setBusy(true);
                      try {
                        await client.approvals.grant({
                          job_id: approval.job_id,
                          node_index: approval.node_index,
                        });
                        await refresh();
                      } finally {
                        if (alive()) setBusy(false);
                      }
                    }}
                    disabled={busy}
                  >
                    Approve command
                  </button>
                </div>
              </article>
            )) : <EmptyState label="No pending approvals" />}
          </section>
        ) : null}
        {tab === 'jobs' ? (
          <section className="job-list" aria-label="Jobs">
            {jobs.length ? jobs.map((job) => (
              <article key={job.job_id}>
                <span className={`status-dot status-${(job.status || '').toLowerCase()}`} />
                <div><strong>{job.label || job.job_id}</strong><span>{job.status || 'Unknown'}</span></div>
                {typeof job.steps_executed === 'number' ? <small>{job.steps_executed}/{job.max_steps || '?'}</small> : null}
              </article>
            )) : <EmptyState label="No durable jobs" />}
          </section>
        ) : null}
      </div>
    </aside>
  );
}

function EmptyState({ label }: { label: string }) {
  return <div className="execution-empty"><Icon name="check" /><span>{label}</span></div>;
}

function formatEffect(value?: string) {
  if (!value) return 'Effect details unavailable';
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
