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
  | 'session_search'
  | 'archive_session'
  | 'pin_session'
  | 'chat_approval_resolve'
  | 'cron_list'
  | 'cron_add'
  | 'cron_tick'
  | 'cron_set_enabled'
  | 'cron_remove'
  | 'cron_history'
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
  | 'project_scopes_list'
  | 'project_scopes_authorize'
  | 'artifacts_list'
  | 'artifacts_put_text'
  | 'artifacts_get'
  | 'artifacts_delete'
  | 'artifacts_delete_many'
  | 'artifacts_export'
  | 'artifacts_export_zip'
  | 'skills_list'
  | 'skills_pin'
  | 'skills_deprecate'
  | 'memory_list'
  | 'memory_recall'
  | 'memory_correct'
  | 'memory_forget'
  | 'packs_state'
  | 'packs_activate'
  | 'packs_deactivate'
  | 'logs_tail'
  | 'commands_list'
  | 'gateway_status'
  | 'gateway_inbox'
  | 'gateway_outbox'
  | 'gateway_enqueue'
  | 'gateway_ambiguous'
  | 'gateway_ack_delivery'
  | 'gateway_telegram_status'
  | 'providers_catalog'
  | 'providers_route_preview'
  | 'mcp_status'
  | 'mcp_tools'
  | 'packs_verify_signed';

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
  /** Reasoning/thinking block — never mixed into `content` (program P24). */
  thinking?: string;
  status?: RunStatus;
  durationMs?: number;
  createdAt?: string;
  tools?: ToolActivity[];
};

export type SessionMeta = {
  id: string;
  title?: string;
  message_count?: number;
  packs?: string[];
  updated_at?: string;
  created_at?: string;
  pinned?: boolean;
  archived?: boolean;
};

export type SessionDetail = SessionMeta & {
  run_status?: 'running' | 'succeeded' | 'failed' | 'cancelled';
  messages: Array<{
    role: 'user' | 'assistant';
    content: string;
    tool_events?: ToolLifecycleEvent[];
  }>;
};

export type ProductSettings = {
  work_isolation: 'shared' | 'project_bound' | 'isolated_profiles';
  work_isolation_label?: string;
  allow_concurrent_projects: boolean;
  enforcement_active?: boolean;
  product_fs_enforced?: boolean;
  configured_mode?: string;
  enforced_mode?: string;
  command_envelope_enforced?: boolean;
  command_fs_envelope?: string;
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
  gateway_inbox_pending?: number;
  gateway_outbox_total?: number;
  gateway_ambiguous_sends?: number;
  gateway_note?: string;
  shell_mode?: string;
  shell_default?: boolean;
  shell_label?: string;
  updater_channel?: string;
  updater_note?: string;
  install_present?: boolean;
  install_shell?: string | null;
  install_version?: string | null;
  packs_loaded?: number;
  packs_on_demand?: number;
  packs_tool_count?: number;
  program_phase?: string;
  cron_jobs?: number;
  campaigns_active?: number;
  approvals_pending?: number;
  core_schema_tokens?: number;
  max_budget?: number;
  work_isolation?: string;
  work_isolation_label?: string;
  allow_concurrent_projects?: boolean;
  isolation_enforcement_active?: boolean;
  product_fs_enforced?: boolean;
  configured_mode?: string;
  enforced_mode?: string;
  command_envelope_enforced?: boolean;
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
  runId: string;
  callId: string;
  name: string;
  detail: string;
  status:
    | 'running'
    | 'awaiting_approval'
    | 'completed'
    | 'failed'
    | 'cancelled'
    | 'suppressed'
    | 'ambiguous';
  durationMs?: number;
  outcome?: ToolOutcome;
  /**
   * The durable runtime identity for a pending high-risk effect. This is
   * intentionally absent for ordinary tool activity: the UI must never turn
   * natural-language text into an approval request.
   */
  approval?: ToolApprovalBinding;
};

export type ToolApprovalBinding = {
  run_id: string;
  call_id: string;
  tool_id: string;
  job_id: string;
  node_id: string;
  node_index: number;
  effect_sha256: string;
  summary: string;
};

export type ToolLifecyclePhase =
  | 'started'
  | 'approval_required'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'suppressed'
  | 'ambiguous';

export type ToolOutcome = {
  version: number;
  call_id: string;
  tool_id: string;
  kind: 'succeeded' | 'failed' | 'cancelled' | 'ambiguous';
  summary: string;
  data: unknown;
  artifacts: Array<Record<string, unknown>>;
  error?: { code: string; message: string; retryable: boolean } | null;
  replay: string;
  provenance?: Record<string, unknown> | null;
};

export type ToolLifecycleEvent = {
  type: 'tool';
  schema_version: 1;
  event_id: string;
  run_id: string;
  call_id: string;
  tool_id: string;
  phase: ToolLifecyclePhase;
  summary: string;
  duration_ms?: number;
  outcome?: ToolOutcome;
  /** Present only when the runtime is awaiting approval of this exact effect. */
  approval?: ToolApprovalBinding;
};

export type TimingEvent = {
  type: 'timing';
  phase?: string;
  elapsed_ms?: number;
  [key: string]: unknown;
};

export type StreamEvent =
  | { type: 'delta'; text: string }
  | { type: 'thinking'; text: string }
  | ToolLifecycleEvent
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
  project_id?: string;
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

export type BrowserAnnotation = {
  cancelled?: boolean;
  url?: string;
  pageTitle?: string;
  tag?: string;
  role?: string;
  label?: string;
  text?: string;
  rect?: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
};

export type Project = {
  id: string;
  name: string;
  rootPaths: string[];
  primaryRoot?: string;
  pinned?: boolean;
  createdAt?: string;
  updatedAt?: string;
};

export type ProjectRootSelection = {
  ok: boolean;
  cancelled?: boolean;
  path?: string;
  grantToken?: string;
  grantExpiresUnix?: number;
};

export type ProjectRuntimeScope = {
  project_id: string;
  roots: string[];
  primary_root: string;
  updated_unix: number;
};

export interface OptimusTransport {
  readonly kind: 'electron' | 'http' | 'fixture';
  invoke<T>(method: DesktopMethod, params?: Record<string, unknown>): Promise<T>;
  chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle;
  windowAction(action: 'minimize' | 'maximize' | 'close'): Promise<unknown>;
  pickFolder(): Promise<ProjectRootSelection>;
  openPath(path: string): Promise<unknown>;
  browser?: {
    setBounds(bounds: BrowserBounds): void;
    setVisible(visible: boolean): void;
    navigate(url: string): Promise<BrowserState>;
    back(): Promise<BrowserState>;
    forward(): Promise<BrowserState>;
    reload(): Promise<BrowserState>;
    state(): Promise<BrowserState>;
    annotate(): Promise<BrowserAnnotation>;
    cancelAnnotation(): Promise<{ cancelled: boolean }>;
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
    annotate: () => Promise<BrowserAnnotation>;
    cancelAnnotation: () => Promise<{ cancelled: boolean }>;
    subscribe: (listener: (state: BrowserState) => void) => () => void;
  };
  windowAction: (action: string) => Promise<unknown>;
  pickFolder: () => Promise<ProjectRootSelection>;
  openPath: (path: string) => Promise<unknown>;
  openUrl: (url: string) => Promise<unknown>;
};

declare global {
  interface Window {
    optimusElectron?: OptimusElectronBridge;
  }
}
