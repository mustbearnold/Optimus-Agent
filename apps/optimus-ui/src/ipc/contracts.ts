export type DesktopMethod =
  | 'ping'
  | 'doctor'
  | 'auth_status'
  | 'auth_import_hermes'
  | 'auth_import_cli'
  | 'provider_keys_status'
  | 'provider_key_set'
  | 'provider_key_clear'
  | 'settings_get'
  | 'settings_set'
  | 'developer_access_get'
  | 'developer_access_enable'
  | 'developer_access_revoke'
  | 'developer_supervisor_status'
  | 'developer_supervisor_launch'
  | 'developer_supervisor_build_launch'
  | 'developer_supervisor_stop'
  | 'developer_supervisor_restart'
  | 'developer_supervisor_rollback'
  | 'developer_supervisor_log'
  | 'developer_emergency_stop'
  | 'startup_context'
  | 'sessions'
  | 'new_session'
  | 'get_session'
  | 'rename_session'
  | 'delete_session'
  | 'session_search'
  | 'archive_session'
  | 'pin_session'
  | 'chat_start'
  | 'chat_cancel'
  | 'chat_approval_resolve_start'
  | 'cron_list'
  | 'cron_add'
  | 'cron_tick'
  | 'cron_set_enabled'
  | 'cron_remove'
  | 'cron_history'
  | 'approvals_list'
  | 'approvals_grant'
  | 'approvals_release_yolo'
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
  | 'memory_search'
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
  /** Total model-phase duration for this message's step (ms), from the
   *  kernel's model_finished timing event. Drives the "thought for Xs" label. */
  thinkingMs?: number;
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
  developer_access?: DeveloperAccess;
};

export type DeveloperScope =
  | { kind: 'selected_repository'; root: string; root_hash?: string | null }
  | { kind: 'selected_directories'; roots: string[] }
  | { kind: 'entire_local_machine' };

export type DeveloperCapabilities = {
  workspace_files: boolean;
  terminal_execution: boolean;
  process_management: boolean;
  package_installation: boolean;
  network_access: boolean;
  external_services: boolean;
  production_systems: boolean;
  secrets: boolean;
};

export type DeveloperAccess = {
  enabled: boolean;
  scope: DeveloperScope;
  scope_label?: string;
  roots?: string[];
  capabilities: DeveloperCapabilities;
  pause_before_destructive: boolean;
  checkpoint_on_mutation: boolean;
  confirmation_version?: number;
};

export type DeveloperSupervisorStatus = {
  status: string;
  healthy: boolean;
  pid?: number | null;
  port?: number | null;
  binary?: string | null;
  surface?: string | null;
  workspace?: string | null;
  child_home?: string | null;
  handoff_session_id?: string | null;
  log_path?: string;
  started_unix?: number | null;
  last_error?: string | null;
  emergency_stopped?: boolean;
  previous_available?: boolean;
  build?: {
    binary?: string;
    surface?: string;
    workspace?: string;
    profile?: string;
    log_path?: string;
  };
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
  developer_access?: DeveloperAccess;
  developer_supervisor?: DeveloperSupervisorStatus;
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
  /**
   * Wall-clock offsets (ms) of tool start/finish within the run, from the
   * kernel's timing events — feed the R11 tool-to-tool gap breakdown.
   * Absent for persisted runs without timing data.
   */
  startedAtMs?: number;
  finishedAtMs?: number;
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
  /** The wire timing payload (spec-015): the kernel's timing fields. */
  kind?: string | null;
  status?: string | null;
  step?: number | null;
  call_id?: string | null;
  name?: string | null;
  duration_ms?: number | null;
  suppressed?: boolean;
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
  provider: 'auto' | 'offline' | 'codex' | 'deepseek' | 'open-ai-compat';
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

/** The exact persisted approval binding a surface resolves (ADR-0046). */
export type ApprovalResolveRequest = {
  session_id: string;
  run_id: string;
  call_id: string;
  job_id: string;
  node_id: string;
  node_index: number;
  effect_sha256: string;
  decision: 'approve' | 'deny';
  project_id?: string;
};

export type ChatHandle = {
  streamId: number;
  /**
   * Resolves with the stream's terminal event (`done` / `cancelled` /
   * `error`). The `done` payload carries the resolve/chat result, including
   * `resume_error` and `still_pending` (spec-014 R4/R5), so callers can
   * branch on a failed continuation or a re-parked approval.
   */
  done: Promise<StreamEvent | undefined>;
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
  readonly kind: 'tauri' | 'http' | 'fixture' | 'ws';
  invoke<T>(method: DesktopMethod, params?: Record<string, unknown>): Promise<T>;
  chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle;
  /** Resolve a parked approval as a streaming turn; events arrive as they happen. */
  chatApprovalResolve(
    request: ApprovalResolveRequest,
    onEvent: (event: StreamEvent) => void
  ): ChatHandle;
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

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __TAURI__?: unknown;
    /**
     * The broker-owned ticket global (spec-015 A3): set by the shell
     * broker command in the packaged app (re-issued on reload) and by
     * dev-mode injection for tests. Presence selects the WS transport;
     * confirmed absence (bridge present, broker answered no ticket)
     * selects NO transport.
     */
    __OPTIMUS_BROKER_TICKET__?: { port: number; ticket: string } | null;
  }
}
