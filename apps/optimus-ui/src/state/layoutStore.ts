export type WorkspaceTab = 'browser' | 'files' | 'artifacts';
export type AppRoute = 'work' | 'mail' | 'capabilities' | 'artifacts' | 'consoles';
export type CompactSurface = 'work' | WorkspaceTab | 'execution';

export type LayoutState = {
  version: 1;
  leftWidth: number;
  leftCollapsed: boolean;
  workspaceWidth: number;
  workspaceOpen: boolean;
  workspaceTab: WorkspaceTab;
  executionOpen: boolean;
  executionHeight: number;
  route: AppRoute;
  compactSurface: CompactSurface;
};

const KEY = 'optimus.react.layout.v1';

export const defaultLayout: LayoutState = {
  version: 1,
  leftWidth: 240,
  leftCollapsed: false,
  workspaceWidth: 720,
  // Chat-first: evidence workspace opens when the user chooses Browser/Files/Artifacts.
  workspaceOpen: false,
  workspaceTab: 'browser',
  executionOpen: false,
  executionHeight: 190,
  route: 'work',
  compactSurface: 'work',
};

const clamp = (value: unknown, min: number, max: number, fallback: number) => {
  const number = typeof value === 'number' && Number.isFinite(value) ? value : fallback;
  return Math.round(Math.min(max, Math.max(min, number)));
};

export function loadLayout(): LayoutState {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) || '{}') as Partial<LayoutState>;
    const rawRoute = String(value.route || '');
    const storedRoute = rawRoute === 'messaging' ? 'mail' : rawRoute;
    return {
      version: 1,
      leftWidth: clamp(value.leftWidth, 200, 400, defaultLayout.leftWidth),
      leftCollapsed: Boolean(value.leftCollapsed),
      workspaceWidth: clamp(value.workspaceWidth, 360, 1200, defaultLayout.workspaceWidth),
      workspaceOpen:
        typeof value.workspaceOpen === 'boolean'
          ? value.workspaceOpen
          : defaultLayout.workspaceOpen,
      workspaceTab: ['browser', 'files', 'artifacts'].includes(String(value.workspaceTab))
        ? (value.workspaceTab as WorkspaceTab)
        : defaultLayout.workspaceTab,
      executionOpen: Boolean(value.executionOpen),
      executionHeight: clamp(value.executionHeight, 120, 520, defaultLayout.executionHeight),
      route: ['work', 'mail', 'capabilities', 'artifacts', 'consoles'].includes(String(storedRoute))
        ? (storedRoute as AppRoute)
        : defaultLayout.route,
      compactSurface: ['work', 'browser', 'files', 'artifacts', 'execution'].includes(
        String(value.compactSurface)
      )
        ? (value.compactSurface as CompactSurface)
        : defaultLayout.compactSurface,
    };
  } catch {
    return defaultLayout;
  }
}

export function saveLayout(layout: LayoutState) {
  try {
    localStorage.setItem(KEY, JSON.stringify(layout));
  } catch {
    // Presentation persistence is best-effort.
  }
}
