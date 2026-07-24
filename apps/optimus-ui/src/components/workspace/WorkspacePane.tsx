import type { OptimusTransport } from '../../ipc/contracts';
import type { WorkspaceTab } from '../../state/layoutStore';
import { ArtifactsSurface } from './ArtifactsSurface';
import { BrowserSurface } from './BrowserSurface';
import { FilesSurface } from './FilesSurface';

export function WorkspacePane({
  tab,
  transport,
  suspended,
  onAnnotation,
}: {
  tab: WorkspaceTab;
  transport: OptimusTransport;
  suspended: boolean;
  onAnnotation: (text: string) => void;
}) {
  return (
    <aside className="workspace-pane" aria-label="Evidence workspace">
      <div className="workspace-body">
        <div id="workspace-panel-browser" aria-label="Preview browser" hidden={tab !== 'browser'} className={tab === 'browser' ? 'workspace-panel is-active' : 'workspace-panel'} role="tabpanel">
          <BrowserSurface transport={transport} active={tab === 'browser' && !suspended} onAnnotation={onAnnotation} />
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
