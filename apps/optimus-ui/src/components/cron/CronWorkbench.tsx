import { useCallback, useEffect, useState } from 'react';
import type { CronJob, OptimusTransport } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

export type CronAttempt = {
  attempt_id: string;
  job_id: string;
  status: string;
  started_unix: number;
  completed_unix?: number | null;
  detail?: string | null;
};

export function CronWorkbench({
  transport,
  active,
}: {
  transport: OptimusTransport;
  active: boolean;
}) {
  const [jobs, setJobs] = useState<CronJob[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [history, setHistory] = useState<CronAttempt[]>([]);
  const [error, setError] = useState('');
  const [name, setName] = useState('');
  const [everySecs, setEverySecs] = useState('3600');
  const [prompt, setPrompt] = useState('');
  const [provider, setProvider] = useState('offline');
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    setError('');
    try {
      const result = await transport.invoke<{ jobs?: CronJob[] }>('cron_list');
      setJobs(result.jobs || []);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [transport]);

  const loadHistory = useCallback(
    async (id: string) => {
      try {
        const result = await transport.invoke<{ attempts?: CronAttempt[] }>('cron_history', {
          id,
          limit: 20,
        });
        setHistory(result.attempts || []);
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      }
    },
    [transport]
  );

  useEffect(() => {
    if (active) void load();
  }, [active, load]);

  useEffect(() => {
    if (selected) void loadHistory(selected);
    else setHistory([]);
  }, [selected, loadHistory]);

  const create = async () => {
    setBusy(true);
    setError('');
    try {
      const every = Math.max(5, Number(everySecs) || 3600);
      await transport.invoke('cron_add', {
        name: name.trim() || 'schedule',
        every_secs: every,
        prompt: prompt.trim() || 'tick',
        provider,
      });
      setName('');
      setPrompt('');
      await load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  const toggleEnabled = async (job: CronJob) => {
    setError('');
    try {
      await transport.invoke('cron_set_enabled', { id: job.id, enabled: !job.enabled });
      await load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const remove = async (job: CronJob) => {
    if (!window.confirm(`Remove schedule “${job.name}”?`)) return;
    setError('');
    try {
      await transport.invoke('cron_remove', { id: job.id });
      if (selected === job.id) setSelected(null);
      await load();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  return (
    <section className="cron-workbench" aria-label="Cron schedules">
      <div className="surface-toolbar">
        <div><Icon name="automation" /><strong>Schedules</strong></div>
        <button type="button" aria-label="Refresh schedules" onClick={() => void load()}>
          <Icon name="refresh" />
        </button>
      </div>
      <div className="cron-layout">
        <form
          className="cron-create"
          onSubmit={(event) => {
            event.preventDefault();
            void create();
          }}
        >
          <h3>Create schedule</h3>
          <label>
            Name
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={120}
              required
              placeholder="Nightly status"
            />
          </label>
          <label>
            Every (seconds)
            <input
              type="number"
              min={5}
              value={everySecs}
              onChange={(e) => setEverySecs(e.target.value)}
              required
            />
          </label>
          <label>
            Provider
            <select value={provider} onChange={(e) => setProvider(e.target.value)}>
              <option value="offline">offline</option>
              <option value="codex">codex</option>
              <option value="openai_compat">openai_compat</option>
            </select>
          </label>
          <label>
            Prompt
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={3}
              required
              placeholder="What should Optimus do on this schedule?"
            />
          </label>
          <button type="submit" disabled={busy}>
            {busy ? 'Creating…' : 'Create'}
          </button>
          <p className="panel-muted">
            Create only — pause/remove never mint leases. Tick/claim stays on the host runtime.
          </p>
        </form>
        <div className="cron-list">
          {jobs.map((job) => (
            <div
              key={job.id}
              className={`cron-row${selected === job.id ? ' is-active' : ''}${job.enabled ? '' : ' is-paused'}`}
            >
              <button type="button" className="cron-select" onClick={() => setSelected(job.id)}>
                <strong>{job.name}</strong>
                <small>
                  Every {formatDuration(job.every_secs)} · {job.last_status || 'Not run'} ·{' '}
                  {job.provider || 'offline'}
                </small>
              </button>
              <div className="cron-row-actions">
                <button type="button" onClick={() => void toggleEnabled(job)}>
                  {job.enabled ? 'Pause' : 'Resume'}
                </button>
                <button type="button" className="danger-text" onClick={() => void remove(job)}>
                  Remove
                </button>
              </div>
            </div>
          ))}
          {!jobs.length ? <div className="surface-empty">No schedules yet.</div> : null}
        </div>
        <div className="cron-history" aria-label="Schedule history">
          <h3>History</h3>
          {selected ? (
            history.length ? (
              <ul>
                {history.map((attempt) => (
                  <li key={attempt.attempt_id}>
                    <strong>{attempt.status}</strong>
                    <small>
                      {formatUnix(attempt.started_unix)}
                      {attempt.detail ? ` · ${attempt.detail}` : ''}
                    </small>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="panel-muted">No attempts recorded for this schedule.</p>
            )
          ) : (
            <p className="panel-muted">Select a schedule to view attempt history.</p>
          )}
        </div>
      </div>
      {error ? (
        <div className="surface-error" role="alert">
          <Icon name="warning" />
          {error}
        </div>
      ) : null}
    </section>
  );
}

function formatDuration(seconds: number) {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h`;
  return `${Math.round(seconds / 86400)}d`;
}

function formatUnix(unix: number) {
  try {
    return new Date(unix * 1000).toLocaleString();
  } catch {
    return String(unix);
  }
}
