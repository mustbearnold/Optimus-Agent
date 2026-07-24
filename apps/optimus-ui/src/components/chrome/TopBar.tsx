import { Icon, type IconName } from './Icon';
import type { AppRoute } from '../../state/layoutStore';

type Props = {
  title: string;
  projectName?: string;
  projectSourceCount?: number;
  route: AppRoute;
  activeTasks: number;
  workspaceOpen: boolean;
  executionOpen: boolean;
  theme: 'dark' | 'light';
  onToggleRail: () => void;
  onToggleWorkspace: () => void;
  onToggleExecution: () => void;
  onToggleTasks: () => void;
  onRoute: (route: AppRoute) => void;
  onTheme: () => void;
  onWindow: (action: 'minimize' | 'maximize' | 'close') => void;
};

function ToolbarButton({
  icon,
  label,
  pressed,
  count,
  onClick,
}: {
  icon: IconName;
  label: string;
  pressed?: boolean;
  count?: number;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="toolbar-button"
      aria-label={label}
      aria-pressed={pressed}
      title={label}
      onClick={onClick}
    >
      <Icon name={icon} />
      <span className="toolbar-label">{label}</span>
      {typeof count === 'number' ? <span className="count-badge">{count}</span> : null}
    </button>
  );
}
export function TopBar({
  title,
  projectName,
  projectSourceCount,
  route,
  activeTasks,
  workspaceOpen,
  executionOpen,
  theme,
  onToggleRail,
  onToggleWorkspace,
  onToggleExecution,
  onToggleTasks,
  onRoute,
  onTheme,
  onWindow,
}: Props) {
  return (
    <header className="topbar">
      <div className="topbar-left">
        <ToolbarButton icon="sidebar" label="Toggle project rail" onClick={onToggleRail} />
        <button type="button" className="product-mark" onClick={() => onRoute('work')}>
          <span className="optimus-glyph">O</span>
          <span>Optimus</span>
        </button>
        <span className="topbar-divider" />
        <div className="breadcrumb" title={title}>
          <span>{projectName || 'Optimus Agent'}</span>
          {projectSourceCount && projectSourceCount > 1 ? (
            <span className="source-count" title={`${projectSourceCount} project sources`}>
              {projectSourceCount}
            </span>
          ) : null}
          <span aria-hidden="true">/</span>
          <strong>{route === 'work' ? title : route}</strong>
        </div>
      </div>
      <nav className="topbar-actions" aria-label="Workbench controls">
        <ToolbarButton icon="tasks" label="Tasks" count={activeTasks} onClick={onToggleTasks} />
        <ToolbarButton
          icon="panel"
          label="Workspace"
          pressed={workspaceOpen}
          onClick={onToggleWorkspace}
        />
        <ToolbarButton
          icon="terminal"
          label="Terminal"
          pressed={executionOpen}
          onClick={onToggleExecution}
        />
        <ToolbarButton
          icon={theme === 'dark' ? 'sun' : 'moon'}
          label={`Use ${theme === 'dark' ? 'light' : 'dark'} theme`}
          onClick={onTheme}
        />
      </nav>
      <div className="window-controls" aria-label="Window controls">
        <button type="button" aria-label="Minimize" onClick={() => onWindow('minimize')}>
          <Icon name="minimize" />
        </button>
        <button type="button" aria-label="Maximize" onClick={() => onWindow('maximize')}>
          <Icon name="maximize" />
        </button>
        <button
          type="button"
          className="window-close"
          aria-label="Close"
          onClick={() => onWindow('close')}
        >
          <Icon name="close" />
        </button>
      </div>
    </header>
  );
}
