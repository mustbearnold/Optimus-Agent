import type { KeyboardEvent } from 'react';
import type { OptimusTransport } from '../../ipc/contracts';
import type { WorkspaceTab } from '../../state/layoutStore';
import { Icon } from '../chrome/Icon';
import { ArtifactsSurface } from './ArtifactsSurface';
import { BrowserSurface } from './BrowserSurface';
import { FilesSurface } from './FilesSurface';

export function WorkspacePane({
  tab,
  transport,
  onTab,
  onClose,
  onAnnotation,
}: {
  tab: WorkspaceTab;
  transport: OptimusTransport;
  onTab: (tab: WorkspaceTab) => void;
  onClose: () => void;
  onAnnotation: (text: string) => void;
}) {
  const tabs: Array<{ id: WorkspaceTab; label: string; icon: 'browser' | 'files' | 'artifact' }> = [
    { id: 'browser', label: 'Browser', icon: 'browser' },
    { id: 'files', label: 'Files', icon: 'files' },
    { id: 'artifacts', label: 'Artifacts', icon: 'artifact' },
  ];
  const moveTab = (event: KeyboardEvent<HTMLButtonElement>, current: WorkspaceTab) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const index = tabs.findIndex((candidate) => candidate.id === current);
    const nextIndex =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? tabs.length - 1
          : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    const next = tabs[nextIndex];
    onTab(next.id);
    requestAnimationFrame(() => {
      document.getElementById(`workspace-tab-${next.id}`)?.focus();
    });
  };
  return (
    <aside className="workspace-pane" aria-label="Evidence workspace">
      <div className="workspace-tabs" role="tablist" aria-label="Evidence surfaces">
        {tabs.map((item) => (
          <button
            type="button"
            id={`workspace-tab-${item.id}`}
            role="tab"
            aria-selected={tab === item.id}
            aria-controls={`workspace-panel-${item.id}`}
            tabIndex={tab === item.id ? 0 : -1}
            className={tab === item.id ? 'is-active' : ''}
            onClick={() => onTab(item.id)}
            onKeyDown={(event) => moveTab(event, item.id)}
            key={item.id}
          >
            <Icon name={item.icon} />
            <span>{item.label}</span>
          </button>
        ))}
        <span className="workspace-tabs-spacer" />
        <button type="button" aria-label="Close workspace" title="Close workspace" onClick={onClose}>
          <Icon name="close" />
        </button>
      </div>
      <div className="workspace-body">
        <div id="workspace-panel-browser" aria-labelledby="workspace-tab-browser" hidden={tab !== 'browser'} className={tab === 'browser' ? 'workspace-panel is-active' : 'workspace-panel'} role="tabpanel">
          <BrowserSurface transport={transport} active={tab === 'browser'} onAnnotation={onAnnotation} />
        </div>
        <div id="workspace-panel-files" aria-labelledby="workspace-tab-files" hidden={tab !== 'files'} className={tab === 'files' ? 'workspace-panel is-active' : 'workspace-panel'} role="tabpanel">
          <FilesSurface transport={transport} active={tab === 'files'} />
        </div>
        <div id="workspace-panel-artifacts" aria-labelledby="workspace-tab-artifacts" hidden={tab !== 'artifacts'} className={tab === 'artifacts' ? 'workspace-panel is-active' : 'workspace-panel'} role="tabpanel">
          <ArtifactsSurface transport={transport} active={tab === 'artifacts'} />
        </div>
      </div>
    </aside>
  );
}
