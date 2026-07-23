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
  return (
    <main className="route-page capabilities-page" aria-label="Capabilities">
      <header className="route-heading">
        <span className="route-kicker">Rust-owned capability catalog</span>
        <h1>Capabilities</h1>
        <p>What Optimus can do now, what requires approval, and what remains unavailable.</p>
      </header>
      <div className="capability-summary">
        <Metric value={packs.length} label="Enabled packs" />
        <Metric value={packs.reduce((count, pack) => count + (pack.tools?.length || 0), 0)} label="Canonical tools" />
        <Metric value={approvals.length} label="Pending approvals" action={onOpenExecution} />
        <Metric value={campaigns.filter((campaign) => /run/i.test(campaign.status || '')).length} label="Active campaigns" />
      </div>
      <section className="capability-grid">
        {packs.map((pack) => (
          <article className="capability-card" key={pack.id}>
            <header>
              <span className="capability-icon"><Icon name="capabilities" /></span>
              <div><strong>{pack.id}</strong><span>{pack.description || 'Backend capability pack'}</span></div>
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
          </article>
        ))}
        <article className="capability-card is-unavailable">
          <header>
            <span className="capability-icon"><Icon name="warning" /></span>
            <div><strong>Configured intent</strong><span>Visible boundaries prevent accidental overclaiming.</span></div>
          </header>
          <ul>
            <li>Messaging — unavailable</li>
            <li>Specialist agents — unavailable</li>
            <li>Parallel child orchestration — unavailable</li>
            <li>Project isolation enforcement — configured intent only</li>
          </ul>
        </article>
      </section>
    </main>
  );
}

function Metric({ value, label, action }: { value: number; label: string; action?: () => void }) {
  const content = <><strong>{value}</strong><span>{label}</span></>;
  return action ? <button type="button" className="metric-card" onClick={action}>{content}</button> : <div className="metric-card">{content}</div>;
}
