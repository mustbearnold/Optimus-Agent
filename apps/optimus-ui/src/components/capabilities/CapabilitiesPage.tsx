import type { Approval, Campaign, Doctor } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

export function CapabilitiesPage({
  doctor,
  approvals,
  campaigns,
  onOpenExecution,
}: {
  doctor: Doctor | null;
  approvals: Approval[];
  campaigns: Campaign[];
  onOpenExecution: () => void;
}) {
  const packs = doctor?.pack_catalog || [];
  const toolCount = packs.reduce((count, pack) => count + (pack.tools?.length || 0), 0);
  const activeCampaigns = campaigns.filter((campaign) => /run/i.test(campaign.status || '')).length;
  return (
    <main className="route-page capabilities-page" aria-label="Capabilities">
      <header className="route-heading">
        <span className="route-kicker">Rust-owned runtime inventory</span>
        <h1>Runtime capabilities</h1>
        <p>Inspect the tools Optimus can use, the effects waiting for approval, and the boundaries this build does not cross.</p>
      </header>
      <section className="capability-overview" aria-label="Runtime summary">
        <dl>
          <div><dt>Capability packs</dt><dd>{packs.length}</dd></div>
          <div><dt>Canonical tools</dt><dd>{toolCount}</dd></div>
          <div><dt>Active campaigns</dt><dd>{activeCampaigns}</dd></div>
        </dl>
        <button type="button" className="approval-summary" onClick={onOpenExecution}>
          <span>Pending approvals</span>
          <strong>{approvals.length}</strong>
          <Icon name="forward" />
        </button>
      </section>
      <section className="capability-registry" aria-labelledby="runtime-tools-title">
        <header className="capability-section-heading">
          <div>
            <h2 id="runtime-tools-title">Available through Rust authority</h2>
            <p>{toolCount} canonical {toolCount === 1 ? 'tool' : 'tools'} across {packs.length} enabled {packs.length === 1 ? 'pack' : 'packs'}.</p>
          </div>
          <span className="state-chip is-ready">Runtime owned</span>
        </header>
        {packs.map((pack) => (
          <section className="capability-pack" key={pack.id}>
            <header>
              <div>
                <h3>{pack.id}</h3>
                <p>{pack.description || 'Backend capability pack'}</p>
              </div>
              <span>{pack.tools?.length || 0} {(pack.tools?.length || 0) === 1 ? 'tool' : 'tools'}</span>
            </header>
            <div className="tool-list">
              {(pack.tools || []).map((tool) => (
                <div className="tool-row" key={tool.id}>
                  <span className="status-dot is-ready" />
                  <div><strong>{tool.id}</strong><span>{tool.description || tool.policy || 'Available through Rust authority'}</span></div>
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
            <h2 id="capability-boundary-title">Unavailable in this build</h2>
            <p>These visible boundaries prevent configured intent from being presented as working behavior.</p>
          </div>
        </header>
        <ul>
          <li><strong>External messaging</strong><span>Unavailable</span></li>
          <li><strong>Specialist agents</strong><span>Unavailable</span></li>
          <li><strong>Parallel child orchestration</strong><span>Unavailable</span></li>
          <li><strong>Project isolation enforcement</strong><span>Configured intent only</span></li>
        </ul>
      </section>
    </main>
  );
}
