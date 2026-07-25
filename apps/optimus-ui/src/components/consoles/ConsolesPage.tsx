import { useCallback, useEffect, useState } from 'react';
import type { OptimusTransport } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

export type ConsoleTab = 'skills' | 'memory' | 'packs' | 'logs';

export function ConsolesPage({
  transport,
  initialTab = 'skills',
}: {
  transport: OptimusTransport;
  initialTab?: ConsoleTab;
}) {
  const [tab, setTab] = useState<ConsoleTab>(initialTab);
  return (
    <main className="route-page consoles-page" aria-label="Consoles">
      <header className="route-heading">
        <span className="route-kicker">Program P26 · surface existing backends</span>
        <h1>Consoles</h1>
        <p>
          Skills, memory, packs, and redacted diagnostics. Memory is evidence data only — never
          ActionAuthorize. Pack activate uses the same CapabilitySession APIs as CLI.
        </p>
      </header>
      <div className="console-tabs" role="tablist" aria-label="Console sections">
        {(['skills', 'memory', 'packs', 'logs'] as ConsoleTab[]).map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? 'is-active' : ''}
            onClick={() => setTab(id)}
          >
            {id}
          </button>
        ))}
      </div>
      {tab === 'skills' ? <SkillsConsole transport={transport} /> : null}
      {tab === 'memory' ? <MemoryConsole transport={transport} /> : null}
      {tab === 'packs' ? <PacksConsole transport={transport} /> : null}
      {tab === 'logs' ? <LogsConsole transport={transport} /> : null}
    </main>
  );
}

function SkillsConsole({ transport }: { transport: OptimusTransport }) {
  const [skills, setSkills] = useState<Array<Record<string, unknown>>>([]);
  const [error, setError] = useState('');
  const load = useCallback(async () => {
    setError('');
    try {
      const r = await transport.invoke<{ skills?: Array<Record<string, unknown>> }>('skills_list', {
        include_deprecated: true,
      });
      setSkills(r.skills || []);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [transport]);
  useEffect(() => {
    void load();
  }, [load]);

  return (
    <section className="console-panel" aria-label="Skills console">
      <header className="console-panel-head">
        <strong>Skills</strong>
        <button type="button" onClick={() => void load()}>
          <Icon name="refresh" />
        </button>
      </header>
      <ul className="console-list">
        {skills.map((s) => (
          <li key={String(s.id)}>
            <div>
              <strong>
                {String(s.name)} v{String(s.version)}
              </strong>
              <small>
                {String(s.status)} · uses {String(s.uses)} · rate{' '}
                {Number(s.success_rate || 0).toFixed(2)}
              </small>
              <p>{String(s.body_preview || '')}</p>
            </div>
            <div className="console-row-actions">
              <button
                type="button"
                onClick={() => void transport.invoke('skills_pin', { id: s.id }).then(load)}
              >
                Pin
              </button>
              <button
                type="button"
                className="danger-text"
                onClick={() => void transport.invoke('skills_deprecate', { id: s.id }).then(load)}
              >
                Deprecate
              </button>
            </div>
          </li>
        ))}
        {!skills.length ? <li className="surface-empty">No skills registered.</li> : null}
      </ul>
      {error ? <div className="surface-error">{error}</div> : null}
    </section>
  );
}

function MemoryConsole({ transport }: { transport: OptimusTransport }) {
  const [claims, setClaims] = useState<Array<Record<string, unknown>>>([]);
  const [fence, setFence] = useState('');
  const [error, setError] = useState('');
  const [subject, setSubject] = useState('');
  const [predicate, setPredicate] = useState('');
  const [recall, setRecall] = useState<Record<string, unknown> | null>(null);

  const load = useCallback(async () => {
    setError('');
    try {
      const r = await transport.invoke<{
        claims?: Array<Record<string, unknown>>;
        fence?: string;
      }>('memory_list', { limit: 50 });
      setClaims(r.claims || []);
      setFence(String(r.fence || ''));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [transport]);

  useEffect(() => {
    void load();
  }, [load]);

  const runRecall = async () => {
    setError('');
    try {
      const r = await transport.invoke<Record<string, unknown>>('memory_recall', {
        purpose: 'inform',
        subject: subject || undefined,
        predicate: predicate || undefined,
        limit: 20,
      });
      setRecall(r);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <section className="console-panel" aria-label="Memory explorer">
      <header className="console-panel-head">
        <strong>Memory</strong>
        <span className="state-chip">{fence || 'data only'}</span>
        <button type="button" onClick={() => void load()}>
          <Icon name="refresh" />
        </button>
      </header>
      <p className="panel-muted">
        Recall is evidence data — never ActionAuthorize. Correct/forget stay scope-gated writes.
      </p>
      <div className="console-recall-form">
        <input
          placeholder="subject"
          value={subject}
          onChange={(e) => setSubject(e.target.value)}
          aria-label="Recall subject"
        />
        <input
          placeholder="predicate"
          value={predicate}
          onChange={(e) => setPredicate(e.target.value)}
          aria-label="Recall predicate"
        />
        <button type="button" onClick={() => void runRecall()}>
          Recall (inform)
        </button>
      </div>
      <ul className="console-list">
        {claims.map((c) => (
          <li key={String(c.id)}>
            <div>
              <strong>
                {String(c.subject)} · {String(c.predicate)}
              </strong>
              <small>{String(c.object)}</small>
            </div>
            <div className="console-row-actions">
              <button
                type="button"
                onClick={async () => {
                  const next = window.prompt('Correct object to', String(c.object || ''));
                  if (!next?.trim()) return;
                  await transport.invoke('memory_correct', { id: c.id, object: next.trim() });
                  await load();
                }}
              >
                Correct
              </button>
              <button
                type="button"
                className="danger-text"
                onClick={async () => {
                  if (!window.confirm('Forget (tombstone) this claim?')) return;
                  await transport.invoke('memory_forget', { id: c.id });
                  await load();
                }}
              >
                Forget
              </button>
            </div>
          </li>
        ))}
        {!claims.length ? (
          <li className="surface-empty">No claims in default kernel memory scope.</li>
        ) : null}
      </ul>
      {recall ? (
        <pre className="console-json" aria-label="Recall packet">
          {JSON.stringify(recall, null, 2)}
        </pre>
      ) : null}
      {error ? <div className="surface-error">{error}</div> : null}
    </section>
  );
}

function PacksConsole({ transport }: { transport: OptimusTransport }) {
  const [state, setState] = useState<Record<string, unknown> | null>(null);
  const [error, setError] = useState('');
  const load = useCallback(async () => {
    setError('');
    try {
      setState(await transport.invoke('packs_state'));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [transport]);
  useEffect(() => {
    void load();
  }, [load]);

  const catalog = (state?.catalog as Array<Record<string, unknown>>) || [];
  const loaded = new Set((state?.loaded as string[]) || []);

  return (
    <section className="console-panel" aria-label="Packs console">
      <header className="console-panel-head">
        <strong>Packs</strong>
        <small>
          tokens {String(state?.schema_tokens ?? '—')} / {String(state?.max_schema_tokens ?? '—')}
        </small>
        <button type="button" onClick={() => void load()}>
          <Icon name="refresh" />
        </button>
      </header>
      <p className="panel-muted">
        Activate/deactivate use CapabilitySession (same pack budget rules as CLI). Not a second tool
        list.
      </p>
      <ul className="console-list">
        {catalog.map((pack) => {
          const id = String(pack.id);
          const isLoaded = loaded.has(id);
          const isCore = id === 'core';
          return (
            <li key={id}>
              <div>
                <strong>{id}</strong>
                <small>
                  {String(pack.summary || '')} · {String(pack.schema_tokens)} tokens ·{' '}
                  {((pack.tools as unknown[]) || []).length} tools
                </small>
              </div>
              <div className="console-row-actions">
                {isLoaded ? (
                  <button
                    type="button"
                    disabled={isCore}
                    onClick={() => void transport.invoke('packs_deactivate', { name: id }).then(load)}
                  >
                    Deactivate
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => void transport.invoke('packs_activate', { name: id }).then(load)}
                  >
                    Activate
                  </button>
                )}
                <span className={`state-chip${isLoaded ? ' is-ready' : ''}`}>
                  {isLoaded ? 'loaded' : 'available'}
                </span>
              </div>
            </li>
          );
        })}
      </ul>
      {error ? <div className="surface-error">{error}</div> : null}
    </section>
  );
}

function LogsConsole({ transport }: { transport: OptimusTransport }) {
  const [lines, setLines] = useState<string[]>([]);
  const [error, setError] = useState('');
  const load = useCallback(async () => {
    setError('');
    try {
      const r = await transport.invoke<{ lines?: string[] }>('logs_tail', { limit: 100 });
      setLines(r.lines || []);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [transport]);
  useEffect(() => {
    void load();
  }, [load]);

  return (
    <section className="console-panel" aria-label="Logs drawer">
      <header className="console-panel-head">
        <strong>Logs</strong>
        <span className="state-chip is-ready">redacted</span>
        <button type="button" onClick={() => void load()}>
          <Icon name="refresh" />
        </button>
      </header>
      <pre className="console-logs" aria-label="Redacted log lines">
        {lines.join('\n') || 'No diagnostic lines.'}
      </pre>
      {error ? <div className="surface-error">{error}</div> : null}
    </section>
  );
}
