import { useCallback, useEffect, useState } from 'react';
import { useAlive } from '../../hooks/useAlive';
import type { Approval, Campaign, Doctor, OptimusTransport } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

type ProviderRow = {
  id: string;
  connect?: string;
  connect_detail?: string;
  supports_tools?: boolean;
  supports_vision?: boolean;
  supports_streaming?: boolean;
  default_model?: { 0?: string } | string;
  remote?: boolean;
};

export function CapabilitiesPage({
  doctor,
  approvals,
  campaigns,
  onOpenExecution,
  transport,
}: {
  doctor: Doctor | null;
  approvals: Approval[];
  campaigns: Campaign[];
  onOpenExecution: () => void;
  transport?: OptimusTransport;
}) {
  const packs = doctor?.pack_catalog || [];
  const toolCount = packs.reduce((count, pack) => count + (pack.tools?.length || 0), 0);
  const activeCampaigns = campaigns.filter((campaign) => /run/i.test(campaign.status || '')).length;
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [mcpTools, setMcpTools] = useState<Array<Record<string, unknown>>>([]);
  const [routePreview, setRoutePreview] = useState('');
  const [error, setError] = useState('');
  const alive = useAlive();

  const loadExt = useCallback(async () => {
    if (!transport) return;
    setError('');
    try {
      const cat = await transport.invoke<{ providers?: ProviderRow[] }>('providers_catalog');
      if (!alive()) return;
      setProviders(cat.providers || []);
      const mcp = await transport.invoke<{ tools?: Array<Record<string, unknown>> }>('mcp_tools', {
        transport: 'stdio',
      });
      if (!alive()) return;
      setMcpTools(mcp.tools || []);
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [alive, transport]);

  useEffect(() => {
    void loadExt();
  }, [loadExt]);

  const previewFailover = async () => {
    if (!transport) return;
    setError('');
    try {
      const r = await transport.invoke<{
        ok?: boolean;
        decision?: { provider?: string; model?: string | { 0?: string }; fallback_from?: string };
        error?: string;
      }>('providers_route_preview', {
        provider: 'codex',
        model: 'not-a-codex-model',
        allow_fallback: true,
        fallback_order: ['offline'],
      });
      if (!alive()) return;
      if (r.ok && r.decision) {
        const model =
          typeof r.decision.model === 'string'
            ? r.decision.model
            : r.decision.model?.[0] || JSON.stringify(r.decision.model);
        setRoutePreview(
          `${r.decision.provider} / ${model}` +
            (r.decision.fallback_from ? ` (fallback from ${r.decision.fallback_from})` : '')
        );
      } else {
        setRoutePreview(r.error || 'no route');
      }
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const modelLabel = (row: ProviderRow) => {
    if (!row.default_model) return '—';
    if (typeof row.default_model === 'string') return row.default_model;
    return row.default_model[0] || '—';
  };

  return (
    <main className="route-page capabilities-page" aria-label="Capabilities">
      <header className="route-heading">
        <span className="route-kicker">Models, tools, and packs</span>
        <h1>Runtime capabilities</h1>
        <p>
          See which model providers are connected, which tools packs expose, and whether optional
          MCP tools are available. One tool catalog only — nothing is silently duplicated.
        </p>
      </header>
      <section className="capability-overview" aria-label="Runtime summary">
        <dl>
          <div>
            <dt>Capability packs</dt>
            <dd>{packs.length}</dd>
          </div>
          <div>
            <dt>Canonical tools</dt>
            <dd>{toolCount}</dd>
          </div>
          <div>
            <dt>Active campaigns</dt>
            <dd>{activeCampaigns}</dd>
          </div>
          <div>
            <dt>Providers</dt>
            <dd>{providers.length || '—'}</dd>
          </div>
        </dl>
        <button type="button" className="approval-summary" onClick={onOpenExecution}>
          <span>Pending approvals</span>
          <strong>{approvals.length}</strong>
          <Icon name="forward" />
        </button>
      </section>

      <section className="capability-registry" aria-labelledby="providers-title">
        <header className="capability-section-heading">
          <div>
            <h2 id="providers-title">Model providers</h2>
            <p>Connection status and what each provider supports.</p>
          </div>
          <button type="button" onClick={() => void loadExt()} aria-label="Refresh providers">
            <Icon name="refresh" />
          </button>
        </header>
        <div className="tool-list">
          {providers.map((p) => (
            <div className="tool-row" key={String(p.id)}>
              <span
                className={`status-dot${p.connect === 'connected' ? ' is-ready' : ''}`}
                title={p.connect_detail || p.connect}
              />
              <div>
                <strong>{String(p.id)}</strong>
                <span>
                  {p.connect === 'connected' ? 'Connected' : p.connect || 'Unknown'} ·{' '}
                  {modelLabel(p)}
                  {p.supports_tools ? ' · tools' : ''}
                  {p.supports_vision ? ' · vision' : ''}
                  {p.supports_streaming ? ' · streaming' : ''}
                </span>
              </div>
              <code>{p.remote ? 'remote' : 'local'}</code>
            </div>
          ))}
          {!providers.length ? (
            <p className="panel-muted">No providers reported yet. Open this page after the host is running.</p>
          ) : null}
        </div>
        <div className="console-recall-form" style={{ marginTop: 12 }}>
          <button type="button" onClick={() => void previewFailover()}>
            Preview failover (codex → offline)
          </button>
          {routePreview ? <span className="state-chip">{routePreview}</span> : null}
        </div>
      </section>

      <section className="capability-registry" aria-labelledby="mcp-title">
        <header className="capability-section-heading">
          <div>
            <h2 id="mcp-title">Optional MCP tools</h2>
            <p>External tools exposed through configured packs (when available).</p>
          </div>
        </header>
        <div className="tool-list">
          {mcpTools.map((t) => (
            <div className="tool-row" key={String(t.id)}>
              <span className="status-dot" />
              <div>
                <strong>{String(t.id)}</strong>
                <span>{String(t.description || '')}</span>
              </div>
              <code>{String(t.available ? 'available' : 'unavailable')}</code>
            </div>
          ))}
          {!mcpTools.length ? <p className="panel-muted">No optional MCP tools available.</p> : null}
        </div>
      </section>

      <section className="capability-registry" aria-labelledby="runtime-tools-title">
        <header className="capability-section-heading">
          <div>
            <h2 id="runtime-tools-title">Built-in tools</h2>
            <p>
              {toolCount} {toolCount === 1 ? 'tool' : 'tools'} across {packs.length} enabled{' '}
              {packs.length === 1 ? 'pack' : 'packs'}.
            </p>
          </div>
          <span className="state-chip is-ready">Built-in</span>
        </header>
        {packs.map((pack) => (
          <section className="capability-pack" key={pack.id}>
            <header>
              <div>
                <h3>{pack.id}</h3>
                <p>{pack.description || 'Backend capability pack'}</p>
              </div>
              <span>
                {pack.tools?.length || 0} {(pack.tools?.length || 0) === 1 ? 'tool' : 'tools'}
              </span>
            </header>
            <div className="tool-list">
              {(pack.tools || []).map((tool) => (
                <div className="tool-row" key={tool.id}>
                  <span className="status-dot is-ready" />
                  <div>
                    <strong>{tool.id}</strong>
                    <span>{tool.description || tool.policy || 'Built-in tool'}</span>
                  </div>
                  <code>{tool.policy || 'runtime'}</code>
                </div>
              ))}
            </div>
          </section>
        ))}
      </section>
      <section className="capability-boundary" aria-labelledby="capability-boundary-title">
        <header>
          <Icon name="warning" />
          <div>
            <h2 id="capability-boundary-title">Boundaries</h2>
            <p>Configured intent is not presented as working behavior when evidence is missing.</p>
          </div>
        </header>
        <ul>
          <li>
            <strong>Specialist agents</strong>
            <span>Unavailable</span>
          </li>
          <li>
            <strong>Parallel child orchestration</strong>
            <span>Unavailable</span>
          </li>
          <li>
            <strong>Project isolation enforcement</strong>
            <span>Configured intent only</span>
          </li>
          <li>
            <strong>Unsigned packs</strong>
            <span>Rejected by default</span>
          </li>
        </ul>
      </section>
      {error ? <div className="surface-error">{error}</div> : null}
    </main>
  );
}
