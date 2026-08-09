import { Icon, type IconName } from './Icon';

type Props = {
  railCollapsed: boolean;
  workspaceOpen: boolean;
  workspaceMaximized: boolean;
  executionOpen: boolean;
  onHome: () => void;
  onToggleRail: () => void;
  onToggleWorkspace: () => void;
  onToggleWorkspaceMaximized: () => void;
  onToggleExecution: () => void;
  onWindow: (action: 'minimize' | 'maximize' | 'close') => void;
};

function ToolbarButton({
  icon,
  label,
  pressed,
  iconOnly = false,
  onClick,
}: {
  icon: IconName;
  label: string;
  pressed?: boolean;
  iconOnly?: boolean;
  onClick: () => void;
}) {
  const isChromeControl = icon === 'sidebar' || icon === 'terminal' || icon === 'maximize' || icon === 'minimize';
  return (
    <button
      type="button"
      className={`toolbar-button${isChromeControl ? ' chrome-icon-button' : ''}${iconOnly ? ' toolbar-button-icon-only' : ''}`}
      aria-label={label}
      aria-pressed={pressed}
      title={label}
      onClick={onClick}
    >
      <Icon name={icon} />
      {!iconOnly ? <span className="toolbar-label">{label}</span> : null}
    </button>
  );
}

// Window controls use the same Nerd Font glyphs as every other icon so the
// chrome stays uniform; `data-window-icon` is preserved for the layout tests.
const WINDOW_GLYPHS: Record<'minimize' | 'maximize' | 'close', string> = {
  minimize: '\uEABA', // cod-chrome_minimize
  maximize: '\uEAB9', // cod-chrome_maximize
  close: '\uEAB8', // cod-chrome_close
};

function WindowControlIcon({ action }: { action: 'minimize' | 'maximize' | 'close' }) {
  return (
    <span
      className="window-control-icon"
      data-window-icon={action}
      aria-hidden="true"
    >
      {WINDOW_GLYPHS[action]}
    </span>
  );
}

export function TopBar({
  railCollapsed,
  workspaceOpen,
  workspaceMaximized,
  executionOpen,
  onHome,
  onToggleRail,
  onToggleWorkspace,
  onToggleWorkspaceMaximized,
  onToggleExecution,
  onWindow,
}: Props) {
  return (
    <header className="topbar">
      <div className="topbar-left">
        <ToolbarButton
          icon="sidebar"
          label={railCollapsed ? 'Open project rail' : 'Close project rail'}
          pressed={!railCollapsed}
          iconOnly
          onClick={onToggleRail}
        />
        <button
          type="button"
          className="topbar-product-mark"
          onClick={onHome}
          aria-label="Optimus"
          title="Optimus"
        >
          <span>Optimus</span>
          <Icon name="chevron" />
        </button>
        <span className="topbar-drag-fill" data-tauri-drag-region aria-hidden="true" />
      </div>
      <nav className="topbar-actions" aria-label="Workbench controls">
        <ToolbarButton
          icon={workspaceMaximized ? 'minimize' : 'maximize'}
          label={workspaceMaximized ? 'Restore workspace' : 'Maximize workspace'}
          iconOnly
          pressed={workspaceMaximized}
          onClick={onToggleWorkspaceMaximized}
        />
        <ToolbarButton
          icon="terminal"
          label="Terminal"
          iconOnly
          pressed={executionOpen}
          onClick={onToggleExecution}
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
