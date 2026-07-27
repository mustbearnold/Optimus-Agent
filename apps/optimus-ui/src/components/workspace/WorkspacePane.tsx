import type { OptimusTransport } from '../../ipc/contracts';
import type { WorkspaceTab } from '../../state/layoutStore';
import { ArtifactsSurface } from './ArtifactsSurface';
import { BrowserSurface } from './BrowserSurface';
import { FilesSurface } from './FilesSurface';

// Browser / Files / Artifacts are selected from inside the pane. The compact
// switcher that used to own this choice is display:none above 899px, so on a
// normal desktop window the pane could only ever show its default Browser tab
// and the Files and Artifacts surfaces were unreachable.
const TABS: { tab: WorkspaceTab; label: string }[] = [
  { tab: 'browser', label: 'Browser' },
  { tab: 'files', label: 'Files' },
  { tab: 'artifacts', label: 'Artifacts' },
];

export function WorkspacePane({
  tab,
  transport,
  suspended,
  onAddToPrompt,
  onSelectTab,
}: {
  tab: WorkspaceTab;
  transport: OptimusTransport;
  suspended: boolean;
  onAddToPrompt: (text: string) => void;
  onSelectTab: (tab: WorkspaceTab) => void;
}) {
  return (
    <aside className="workspace-pane" aria-label="Evidence workspace">
      <div className="workspace-tabs" role="tablist" aria-label="Evidence surface">
        {TABS.map((entry) => (
          <button
            type="button"
            role="tab"
            id={`workspace-tab-${entry.tab}`}
            key={entry.tab}
            className={tab === entry.tab ? 'is-active' : undefined}
            aria-selected={tab === entry.tab}
            aria-controls={`workspace-panel-${entry.tab}`}
            onClick={() => onSelectTab(entry.tab)}
          >
            {entry.label}
          </button>
        ))}
      </div>
      <div className="workspace-body">
        <div id="workspace-panel-browser" aria-label="Preview browser" hidden={tab !== 'browser'} className={tab === 'browser' ? 'workspace-panel is-active' : 'workspace-panel'} role="tabpanel">
          <BrowserSurface transport={transport} active={tab === 'browser' && !suspended} onAddToPrompt={onAddToPrompt} />
        </div>
        <div id="workspace-panel-files" aria-label="Files" hidden={tab !== 'files'} className={tab === 'files' ? 'workspace-panel is-active' : 'workspace-panel'} role="tabpanel">
          <FilesSurface transport={transport} active={tab === 'files'} />
        </div>
        <div id="workspace-panel-artifacts" aria-label="Artifacts" hidden={tab !== 'artifacts'} className={tab === 'artifacts' ? 'workspace-panel is-active' : 'workspace-panel'} role="tabpanel">
          <ArtifactsSurface transport={transport} active={tab === 'artifacts'} />
        </div>
      </div>
    </aside>
  );
}
