import type { LayoutState } from './layoutStore';

export type AppState = {
  selectedSessionId: string | null;
  activeRunSessionId: string | null;
  layout: LayoutState;
  settingsOpen: boolean;
  taskPanelOpen: boolean;
  theme: 'dark' | 'light';
};

export type AppAction =
  | { type: 'select-session'; id: string | null }
  | { type: 'set-active-run'; id: string | null }
  | { type: 'set-layout'; layout: LayoutState }
  | { type: 'patch-layout'; patch: Partial<LayoutState> }
  | { type: 'settings'; open: boolean }
  | { type: 'tasks'; open: boolean }
  | { type: 'theme'; theme: 'dark' | 'light' };

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case 'select-session':
      return { ...state, selectedSessionId: action.id };
    case 'set-active-run':
      return { ...state, activeRunSessionId: action.id };
    case 'set-layout':
      return { ...state, layout: action.layout };
    case 'patch-layout':
      return { ...state, layout: { ...state.layout, ...action.patch } };
    case 'settings':
      return { ...state, settingsOpen: action.open };
    case 'tasks':
      return { ...state, taskPanelOpen: action.open };
    case 'theme':
      return { ...state, theme: action.theme };
    default:
      return state;
  }
}
