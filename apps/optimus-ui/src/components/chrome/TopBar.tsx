import { Icon, type IconName } from './Icon';
import type { WorkspaceTab } from '../../state/layoutStore';

type Props = {
  activeTasks: number;
  canGoBack: boolean;
  canGoForward: boolean;
  workspaceOpen: boolean;
  executionOpen: boolean;
  workspaceTab: WorkspaceTab;
  theme: 'dark' | 'light';
  onBack: () => void;
  onForward: () => void;
  onToggleRail: () => void;
  onToggleWorkspace: () => void;
  onToggleExecution: () => void;
  onWorkspaceTab: (tab: WorkspaceTab) => void;
  onToggleTasks: () => void;
  onTheme: () => void;
  onWindow: (action: 'minimize' | 'maximize' | 'close') => void;
};

function ToolbarButton({
  icon,
  label,
  pressed,
  count,
  iconOnly = false,
  disabled = false,
  onClick,
}: {
  icon: IconName;
  label: string;
  pressed?: boolean;
  count?: number;
  iconOnly?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  const isChromeControl = icon === 'sidebar' || icon === 'back' || icon === 'forward' || icon === 'terminal';
  return (
    <button
      type="button"
      className={`toolbar-button${isChromeControl ? ' chrome-icon-button' : ''}${iconOnly ? ' toolbar-button-icon-only' : ''}${typeof count === 'number' ? ' toolbar-button-with-count' : ''}`}
      aria-label={label}
      aria-pressed={pressed}
      title={label}
      disabled={disabled}
      onClick={onClick}
    >
      <Icon name={icon} />
      {!iconOnly ? <span className="toolbar-label">{label}</span> : null}
      {typeof count === 'number' ? <span className="count-badge">{count}</span> : null}
    </button>
  );
}

function WindowControlIcon({ action }: { action: 'minimize' | 'maximize' | 'close' }) {
  return (
    <svg
      className="window-control-icon"
      data-window-icon={action}
      viewBox="0 0 12 12"
      fill="none"
      aria-hidden="true"
    >
      {action === 'minimize' ? <path d="M2 9.5h8" /> : null}
      {action === 'maximize' ? <rect x="2.25" y="2.25" width="7.5" height="7.5" /> : null}
      {action === 'close' ? <path d="m2.5 2.5 7 7m0-7-7 7" /> : null}
    </svg>
  );
}

export function TopBar({
  activeTasks,
  canGoBack,
  canGoForward,
  workspaceOpen,
  executionOpen,
  workspaceTab,
  theme,
  onBack,
  onForward,
  onToggleRail,
  onToggleWorkspace,
  onToggleExecution,
  onWorkspaceTab,
  onToggleTasks,
  onTheme,
  onWindow,
}: Props) {
  return (
    <header className="topbar">
      <div className="topbar-left">
        <ToolbarButton icon="sidebar" label="Toggle project rail" iconOnly onClick={onToggleRail} />
        <ToolbarButton icon="back" label="Back" iconOnly disabled={!canGoBack} onClick={onBack} />
        <ToolbarButton
          icon="forward"
          label="Forward"
          iconOnly
          disabled={!canGoForward}
          onClick={onForward}
        />
      </div>
      <nav className="topbar-actions" aria-label="Workbench controls">
        <ToolbarButton icon="tasks" label="Tasks" count={activeTasks} iconOnly onClick={onToggleTasks} />
        <ToolbarButton
          icon={theme === 'dark' ? 'sun' : 'moon'}
          label={`Use ${theme === 'dark' ? 'light' : 'dark'} theme`}
          iconOnly
          onClick={onTheme}
        />
        <ToolbarButton
          icon="terminal"
          label="Terminal"
          iconOnly
          pressed={executionOpen}
          onClick={onToggleExecution}
        />
        <ToolbarButton
          icon="browser"
          label="Browser"
          iconOnly
          pressed={workspaceOpen && workspaceTab === 'browser'}
          onClick={() => workspaceOpen && workspaceTab === 'browser' ? onToggleWorkspace() : onWorkspaceTab('browser')}
        />
        <ToolbarButton
          icon="files"
          label="Files"
          iconOnly
          pressed={workspaceOpen && workspaceTab === 'files'}
          onClick={() => workspaceOpen && workspaceTab === 'files' ? onToggleWorkspace() : onWorkspaceTab('files')}
        />
        <ToolbarButton
          icon="artifact"
          label="Artifacts"
          iconOnly
          pressed={workspaceOpen && workspaceTab === 'artifacts'}
          onClick={() => workspaceOpen && workspaceTab === 'artifacts' ? onToggleWorkspace() : onWorkspaceTab('artifacts')}
        />
      </nav>
      <div className="window-controls" aria-label="Window controls">
        <button
          type="button"
          className="window-workspace-control"
          aria-label="Workspace"
          aria-pressed={workspaceOpen}
          title="Workspace"
          onClick={onToggleWorkspace}
        >
          <Icon name="sidebar" />
        </button>
        <button type="button" aria-label="Minimize" title="Minimize" onClick={() => onWindow('minimize')}>
          <WindowControlIcon action="minimize" />
        </button>
        <button type="button" aria-label="Maximize" title="Maximize" onClick={() => onWindow('maximize')}>
          <WindowControlIcon action="maximize" />
        </button>
        <button
          type="button"
          className="window-close"
          aria-label="Close"
          title="Close"
          onClick={() => onWindow('close')}
        >
          <WindowControlIcon action="close" />
        </button>
      </div>
    </header>
  );
}
