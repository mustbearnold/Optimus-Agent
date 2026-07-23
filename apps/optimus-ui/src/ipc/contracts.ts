export type DesktopMethod =
  | 'ping'
  | 'doctor'
  | 'auth_status'
  | 'auth_import_hermes'
  | 'auth_import_cli'
  | 'settings_get'
  | 'settings_set'
  | 'sessions'
  | 'new_session'
  | 'get_session'
  | 'rename_session'
  | 'delete_session'
  | 'cron_list'
  | 'cron_add'
  | 'cron_tick'
  | 'approvals_list'
  | 'approvals_grant'
  | 'jobs_list'
  | 'campaign_list'
  | 'campaign_create'
  | 'campaign_run'
  | 'campaign_status'
  | 'term_run'
  | 'browser_navigate'
  | 'browser_click'
  | 'browser_reload'
  | 'fs_roots'
  | 'fs_list'
  | 'fs_read'
  | 'artifacts_list'
  | 'artifacts_put_text'
  | 'artifacts_get'
  | 'artifacts_delete'
  | 'artifacts_delete_many';

export type RunStatus =
  | 'idle'
  | 'submitting'
  | 'working'
  | 'awaiting_approval'
  | 'cancelling'
  | 'completed'
  | 'cancelled'
  | 'failed'
  | 'disconnected';

export type Message = {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  status?: RunStatus;
  createdAt?: string;
};

export type SessionMeta = {
  id: string;
  title?: string;
  message_count?: number;
  packs?: string[];
  updated_at?: string;
  created_at?: string;
};

export type SessionDetail = SessionMeta & {
  messages: Array<{ role: 'user' | 'assistant'; content: string }>;
};

export type ProductSettings = {
  work_isolation: 'shared' | 'project_bound' | 'isolated_profiles';
  work_isolation_label?: string;
  allow_concurrent_projects: boolean;
  enforcement_active?: boolean;
  note?: string;
};

export type PackTool = {
  id: string;
  description?: string;
  policy?: string;
  invocation?: string;
};

export type PackDescriptor = {
  id: string;
  description?: string;
  tools?: PackTool[];
};

export type Doctor = {
  version?: string;
  home?: string;
  phase?: string;
  browser?: string;
  preview_browser?: boolean;
  streaming?: boolean;
  files?: boolean;
  approvals?: boolean;
  campaigns?: boolean;
  cron?: boolean;
  gateway?: boolean;
  cron_jobs?: number;
  campaigns_active?: number;
  approvals_pending?: number;
  core_schema_tokens?: number;
  max_budget?: number;
  work_isolation?: string;
  work_isolation_label?: string;
  allow_concurrent_projects?: boolean;
  isolation_enforcement_active?: boolean;
  pack_catalog?: PackDescriptor[];
  settings?: ProductSettings;
};

export type Approval = {
  job_id: string;
  job_label?: string;
  job_status?: string;
  node_label?: string;
  node_index?: number;
  has_grant?: boolean;
  effect_json?: string;
};

export type Job = {
  job_id: string;
  label?: string;
  status?: string;
  steps_executed?: number;
  max_steps?: number;
};

export type Campaign = {
  id: string;
  name?: string;
  status?: string;
  current_step?: number;
  steps?: unknown[];
};

export type CronJob = {
  id: string;
  name: string;
  every_secs: number;
  enabled: boolean;
  next_run_unix?: number;
  last_status?: string;
  provider?: string;
  prompt?: string;
};

export type FsEntry = {
  name: string;
  path: string;
  kind?: string;
  is_dir?: boolean;
  size?: number;
  modified?: string;
};

export type ArtifactRecord = {
  sha256: string;
  label?: string;
  source?: string;
  media_type?: string;
  size_bytes?: number;
  created_at?: string;
};

export type ArtifactDetail = {
  artifact: ArtifactRecord;
  kind: 'image' | 'text' | 'binary';
  data_url?: string;
  text?: string;
  truncated?: boolean;
  hex_preview?: string;
  size_bytes?: number;
};

export type ToolActivity = {
  id: string;
  name: string;
  detail: string;
  status: 'running' | 'completed' | 'failed';
};

export type TimingEvent = {
  type: 'timing';
  phase?: string;
  elapsed_ms?: number;
  [key: string]: unknown;
};

export type StreamEvent =
  | { type: 'delta'; text: string }
  | { type: 'tool'; name: string; detail: string }
  | { type: 'status'; text: string }
  | TimingEvent
  | { type: 'done'; result?: Record<string, unknown> }
  | { type: 'cancelled'; error?: string }
  | { type: 'error'; error: string };

export type ChatRequest = {
  session: string;
  message: string;
  provider: 'offline' | 'codex' | 'openai_compat';
  model?: string;
  thinking_level?: string;
  fast?: boolean;
  access?: string;
};

export type ChatEnvelope = {
  streamId: number;
  sessionId: string;
  event: StreamEvent;
};

export type ChatHandle = {
  streamId: number;
  done: Promise<void>;
  cancel: () => Promise<{ requested: boolean }>;
};

export type BrowserBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type BrowserState = {
  url: string;
  title: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  visible: boolean;
  error?: string;
  native: boolean;
};

export type Project = {
  id: string;
  name: string;
  path: string;
  pinned?: boolean;
};

export interface OptimusTransport {
  readonly kind: 'electron' | 'http' | 'fixture';
  invoke<T>(method: DesktopMethod, params?: Record<string, unknown>): Promise<T>;
  chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle;
  windowAction(action: 'minimize' | 'maximize' | 'close'): Promise<unknown>;
  pickFolder(): Promise<{ ok: boolean; cancelled?: boolean; path?: string }>;
  openPath(path: string): Promise<unknown>;
  browser?: {
    setBounds(bounds: BrowserBounds): void;
    setVisible(visible: boolean): void;
    navigate(url: string): Promise<BrowserState>;
    back(): Promise<BrowserState>;
    forward(): Promise<BrowserState>;
    reload(): Promise<BrowserState>;
    state(): Promise<BrowserState>;
    subscribe(listener: (state: BrowserState) => void): () => void;
  };
}

export type OptimusElectronBridge = {
  isElectron: true;
  hostInfo: () => Promise<{ baseUrl: string; token?: string; uiMode?: string }>;
  invoke: <T>(method: DesktopMethod, params?: Record<string, unknown>) => Promise<T>;
  chat: {
    start: (request: ChatRequest) => Promise<{ streamId: number }>;
    cancel: (streamId: number) => Promise<{ requested: boolean }>;
    subscribe: (listener: (event: ChatEnvelope) => void) => () => void;
  };
  browser: {
    setBounds: (bounds: BrowserBounds) => void;
    setVisible: (visible: boolean) => void;
    navigate: (url: string) => Promise<BrowserState>;
    back: () => Promise<BrowserState>;
    forward: () => Promise<BrowserState>;
    reload: () => Promise<BrowserState>;
    state: () => Promise<BrowserState>;
    subscribe: (listener: (state: BrowserState) => void) => () => void;
  };
  windowAction: (action: string) => Promise<unknown>;
  pickFolder: () => Promise<{ ok: boolean; cancelled?: boolean; path?: string }>;
  openPath: (path: string) => Promise<unknown>;
  openUrl: (url: string) => Promise<unknown>;
};

declare global {
  interface Window {
    optimusElectron?: OptimusElectronBridge;
  }
}
