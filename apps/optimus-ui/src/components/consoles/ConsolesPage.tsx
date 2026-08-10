import { useCallback, useEffect, useState } from 'react';
import { useAlive } from '../../hooks/useAlive';
import type { OptimusClient } from '../../ipc/client';
import { Icon } from '../chrome/Icon';
import { TextPromptDialog } from '../chrome/TextPromptDialog';

export type ConsoleTab = 'skills' | 'memory' | 'packs' | 'logs';

export function ConsolesPage({
  client,
  initialTab = 'skills',
}: {
  client: OptimusClient;
  initialTab?: ConsoleTab;
}) {
  const [tab, setTab] = useState<ConsoleTab>(initialTab);
  return (
    <main className="route-page consoles-page" aria-label="Resources">
      <header className="route-heading">
        <span className="route-kicker">Local operator tools</span>
        <h1>Resources</h1>
      </header>
      <div className="console-tabs" role="tablist" aria-label="Resource sections">
        {(['skills', 'memory', 'packs', 'logs'] as ConsoleTab[]).map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? 'is-active' : ''}
            onClick={() => setTab(id)}
          >
            {id[0]!.toUpperCase() + id.slice(1)}
          </button>
        ))}
      </div>
      {tab === 'skills' ? <SkillsConsole client={client} /> : null}
      {tab === 'memory' ? <MemoryConsole client={client} /> : null}
      {tab === 'packs' ? <PacksConsole client={client} /> : null}
      {tab === 'logs' ? <LogsConsole client={client} /> : null}
    </main>
  );
}

function SkillsConsole({ client }: { client: OptimusClient }) {
  const [skills, setSkills] = useState<Array<Record<string, unknown>>>([]);
  const [error, setError] = useState('');
  const alive = useAlive();
  const load = useCallback(async () => {
    setError('');
    try {
      const r = await client.skills.list({ include_deprecated: true });
      if (!alive()) return;
      setSkills(r);
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [alive, client]);
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
                onClick={() => void client.skills.pin(s.id).then(load)}
              >
                Pin
              </button>
              <button
                type="button"
                className="danger-text"
                onClick={() => void client.skills.deprecate(s.id).then(load)}
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

function MemoryConsole({ client }: { client: OptimusClient }) {
  const [claims, setClaims] = useState<Array<Record<string, unknown>>>([]);
  const [fence, setFence] = useState('');
  const [error, setError] = useState('');
  const [subject, setSubject] = useState('');
  const [predicate, setPredicate] = useState('');
  const [recall, setRecall] = useState<Record<string, unknown> | null>(null);
  const [correcting, setCorrecting] = useState<{ id: string; object: string } | null>(null);
  const alive = useAlive();

  const load = useCallback(async () => {
    setError('');
    try {
      const r = await client.memory.list({ limit: 50 });
      if (!alive()) return;
      setClaims(r.claims || []);
      setFence(String(r.fence || ''));
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [alive, client]);

  useEffect(() => {
    void load();
  }, [load]);

  const runRecall = async () => {
    setError('');
    try {
      const r = await client.memory.recall({
        purpose: 'inform',
        subject: subject || undefined,
        predicate: predicate || undefined,
        limit: 20,
      });
      if (!alive()) return;
      setRecall(r);
    } catch (e) {
      if (!alive()) return;
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
        Memory stores reference claims. Correct or forget only changes stored data — it does not
        approve tools or file writes.
      </p>
      <div className="console-recall-form">
        <input
          placeholder="Who or what (subject)"
          value={subject}
          onChange={(e) => setSubject(e.target.value)}
          aria-label="Recall subject"
        />
        <input
          placeholder="Relation (predicate)"
          value={predicate}
          onChange={(e) => setPredicate(e.target.value)}
          aria-label="Recall predicate"
        />
        <button type="button" onClick={() => void runRecall()}>
          Search memory
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
                onClick={() =>
                  setCorrecting({ id: String(c.id), object: String(c.object || '') })
                }
              >
                Correct
              </button>
              <button
                type="button"
                className="danger-text"
                onClick={async () => {
                  if (!window.confirm('Forget this claim? It will be tombstoned, not hard-deleted.')) {
                    return;
                  }
                  await client.memory.forget(c.id);
                  await load();
                }}
              >
                Forget
              </button>
            </div>
          </li>
        ))}
        {!claims.length ? (
          <li className="surface-empty">No claims stored yet.</li>
        ) : null}
      </ul>
      {recall ? (
        <pre className="console-json" aria-label="Recall packet">
          {JSON.stringify(recall, null, 2)}
        </pre>
      ) : null}
      {error ? <div className="surface-error">{error}</div> : null}
      <TextPromptDialog
        open={Boolean(correcting)}
        title="Correct claim"
        label="New value"
        initialValue={correcting?.object || ''}
        confirmLabel="Save correction"
        onCancel={() => setCorrecting(null)}
        onConfirm={async (next) => {
          if (!correcting) return;
          await client.memory.correct(correcting.id, next);
          if (!alive()) return;
          setCorrecting(null);
          await load();
        }}
      />
    </section>
  );
}

function PacksConsole({ client }: { client: OptimusClient }) {
  const [state, setState] = useState<Record<string, unknown> | null>(null);
  const [error, setError] = useState('');
  const alive = useAlive();
  const load = useCallback(async () => {
    setError('');
    try {
      const next = await client.packs.state();
      if (!alive()) return;
      setState(next);
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [alive, client]);
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
                    onClick={() => void client.packs.deactivate(id).then(load)}
                  >
                    Deactivate
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => void client.packs.activate(id).then(load)}
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

function LogsConsole({ client }: { client: OptimusClient }) {
  const [lines, setLines] = useState<string[]>([]);
  const [error, setError] = useState('');
  const alive = useAlive();
  const load = useCallback(async () => {
    setError('');
    try {
      const r = await client.system.logsTail({ limit: 100 });
      if (!alive()) return;
      setLines(r.lines || []);
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [alive, client]);
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
