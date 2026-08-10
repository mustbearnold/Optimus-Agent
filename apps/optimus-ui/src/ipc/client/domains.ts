/**
 * Domain APIs of the renderer client (ADR-0090): typed objects over the
 * frozen wire (spec-015). Each domain owns the envelope unwrapping
 * (`{ items?: T[] }` → `T[]`) and the snake_case param mapping for its
 * methods, so components never see wire names.
 */
import type {
  Approval,
  ArtifactDetail,
  ArtifactRecord,
  BrowserAnnotation,
  BrowserBounds,
  BrowserState,
  Campaign,
  CronJob,
  DesktopMethod,
  DeveloperAccess,
  DeveloperSupervisorStatus,
  Doctor,
  FsEntry,
  Job,
  OptimusTransport,
  ProductSettings,
  ProjectRootSelection,
  ProjectRuntimeScope,
  SessionDetail,
  SessionMeta,
} from '../contracts';
import type { RuntimeObserver } from './runtime';
import { IpcError, NoTransportError } from './types';
import { messageOf } from './turn';

/* ------------------------------------------------------------------ */
/* Shared result types (single source of truth; components import      */
/* these instead of re-declaring them).                                */
/* ------------------------------------------------------------------ */

export type TerminalResult = {
  job_id?: string;
  status?: string;
  stdout?: string;
  stderr?: string;
};

export type ProviderRow = {
  id: string;
  connect?: string;
  connect_detail?: string;
  supports_tools?: boolean;
  supports_vision?: boolean;
  supports_streaming?: boolean;
  default_model?: { 0?: string } | string;
  remote?: boolean;
};

export type ProviderKeyStatus = {
  provider: string;
  label: string;
  env_var: string;
  present: boolean;
  /** 'stored' | 'environment' | 'none' */
  source: string;
  hint?: string | null;
  base_url?: string | null;
  error?: string | null;
};

export type GatewayStatus = {
  inbox_pending?: number;
  inbox_claimed?: number;
  outbox_total?: number;
  ambiguous_sends?: number;
  note?: string;
};

export type InboxMessage = {
  id: string;
  channel: string;
  text: string;
  provider?: string;
  session_id?: string | null;
  received_unix?: number;
};

export type OutboxReceipt = {
  message_id: string;
  outbound: {
    id: string;
    in_reply_to: string;
    channel: string;
    text: string;
    status: string;
    sent_unix?: number;
  };
  terminal_status: string;
  terminal_reason?: string | null;
  delivered_unix?: number | null;
  ambiguous_send: boolean;
};

export type PaletteCommand = {
  id: string;
  name: string;
  description: string;
  surface?: string;
};

export type CronAttempt = {
  attempt_id: string;
  job_id: string;
  status: string;
  started_unix: number;
  completed_unix?: number | null;
  detail?: string | null;
};

export type SessionConsent = {
  command_class: string;
  capability: string;
  created_unix: number;
  expires_unix: number;
  revoked_unix?: number | null;
};

/* ------------------------------------------------------------------ */
/* Wire access: the single choke point for invoke, error typing, and   */
/* observability (law 11).                                             */
/* ------------------------------------------------------------------ */

async function call<T>(
  transport: OptimusTransport | null,
  observer: RuntimeObserver | undefined,
  method: DesktopMethod,
  params?: Record<string, unknown>
): Promise<T> {
  if (!transport) throw new NoTransportError();
  try {
    const result = await transport.invoke<T>(method, params);
    observer?.record({ type: 'invoke', method, ok: true });
    return result;
  } catch (error) {
    observer?.record({ type: 'invoke', method, ok: false });
    if (error instanceof IpcError || error instanceof NoTransportError) throw error;
    throw new IpcError(messageOf(error));
  }
}

function list<T>(result: unknown, key: string): T[] {
  if (Array.isArray(result)) return result as T[];
  if (result && typeof result === 'object' && key in result) {
    const items = (result as Record<string, unknown>)[key];
    if (Array.isArray(items)) return items as T[];
  }
  return [];
}

/* ------------------------------------------------------------------ */
/* Domain APIs                                                         */
/* ------------------------------------------------------------------ */

export interface SessionsApi {
  list(): Promise<SessionMeta[]>;
  newSession(projectId?: string): Promise<SessionMeta>;
  get(id: string): Promise<SessionDetail>;
  rename(id: string, title: string): Promise<unknown>;
  archive(id: string, archived: boolean): Promise<unknown>;
  pin(id: string, pinned: boolean): Promise<unknown>;
  delete(id: string): Promise<unknown>;
  search(params: Record<string, unknown>): Promise<SessionMeta[]>;
  startupContext(): Promise<{ session_id?: string | null }>;
}

export interface ApprovalsApi {
  list(): Promise<Approval[]>;
  grant(request: {
    job_id?: string;
    node_index?: number;
    call_id?: string;
    run_id?: string;
    effect_sha256?: string;
    session_id?: string;
    decision?: 'approve' | 'deny';
    grant_class?: string;
    project_id?: string;
  }): Promise<unknown>;
  releaseYolo(): Promise<unknown>;
}

export interface CronApi {
  list(): Promise<CronJob[]>;
  /** Fresh projection: resolves with the refreshed job list when the host
   *  returns one, else re-fetches (ADR-0090). */
  add(input: { name: string; every_secs: number; prompt: string; provider?: string }): Promise<CronJob[]>;
  history(id: string, limit?: number): Promise<CronAttempt[]>;
  setEnabled(id: string, enabled: boolean): Promise<CronJob[]>;
  remove(id: string): Promise<CronJob[]>;
  tick(id: string): Promise<unknown>;
}

export interface JobsApi {
  list(): Promise<Job[]>;
}

export interface ArtifactsApi {
  list(): Promise<ArtifactRecord[]>;
  get(sha256: string): Promise<ArtifactDetail>;
  putText(input: { path?: string; text: string }): Promise<unknown>;
  deleteMany(sha256s: string[]): Promise<unknown>;
  export(sha256: string): Promise<{ path?: string }>;
  exportZip(sha256s: string[]): Promise<{ path?: string }>;
}

export interface FsApi {
  roots(): Promise<{ roots?: string[] }>;
  list(path: string): Promise<FsEntry[]>;
  read(path: string): Promise<{ path: string; content: string; truncated: boolean }>;
}

export interface MemoryApi {
  list(input?: { limit?: number }): Promise<{ claims?: Array<Record<string, unknown>>; fence?: string }>;
  recall(input: {
    purpose?: string;
    subject?: string;
    predicate?: string;
    limit?: number;
  }): Promise<Record<string, unknown>>;
  search(input: Record<string, unknown>): Promise<Record<string, unknown>>;
  correct(id: unknown, object: string): Promise<unknown>;
  forget(id: unknown): Promise<unknown>;
}

export interface SkillsApi {
  list(input?: { include_deprecated?: boolean; limit?: number }): Promise<Array<Record<string, unknown>>>;
  pin(id: unknown): Promise<unknown>;
  deprecate(id: unknown): Promise<unknown>;
}

export interface PacksApi {
  state(): Promise<Record<string, unknown>>;
  activate(name: string): Promise<unknown>;
  deactivate(name: string): Promise<unknown>;
  verifySigned(): Promise<unknown>;
}

export interface GatewayApi {
  status(): Promise<GatewayStatus | null>;
  inbox(): Promise<InboxMessage[]>;
  outbox(limit?: number): Promise<OutboxReceipt[]>;
  ambiguous(): Promise<OutboxReceipt[]>;
  enqueue(text: string, channel: string): Promise<unknown>;
  ackDelivery(messageId: string, outboundId: string): Promise<unknown>;
  telegramStatus(): Promise<Record<string, unknown>>;
}

export interface ProvidersApi {
  catalog(): Promise<ProviderRow[]>;
  routePreview(input: {
    provider: string;
    model: string;
    allow_fallback: boolean;
    fallback_order: string[];
  }): Promise<{
    ok?: boolean;
    decision?: {
      provider?: string;
      model?: string | { 0?: string };
      fallback_from?: string;
    };
    error?: string;
  }>;
  keysStatus(): Promise<ProviderKeyStatus[]>;
  keySet(provider: string, apiKey: string): Promise<ProviderKeyStatus[]>;
  keyClear(provider: string): Promise<ProviderKeyStatus[]>;
}

export interface ConsentsApi {
  grant(sessionId: string, commandClass: string, projectId?: string): Promise<unknown>;
  list(sessionId: string, projectId?: string): Promise<SessionConsent[]>;
  revoke(sessionId: string, projectId?: string): Promise<{ revoked: number }>;
  revokeAll(sessionId: string, projectId?: string): Promise<{ revoked: number }>;
}

export interface ProjectsApi {
  scopesList(): Promise<ProjectRuntimeScope[]>;
  /** `project_id` and `primary_root` are `string | undefined` because the
   *  app-side `Project` type declares them optional (contract type); the
   *  runtime always sets both, and the host requires them. */
  authorize(input: {
    project_id?: string;
    root_paths: string[];
    primary_root?: string;
    grant_tokens?: string[];
  }): Promise<{ project?: ProjectRuntimeScope | null }>;
}

export interface SystemApi {
  ping(): Promise<unknown>;
  doctor(): Promise<Doctor>;
  developerAccess(): Promise<{ developer_access?: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>;
  enableDeveloperAccess(input: {
    confirmation: string;
    grant: Record<string, unknown>;
  }): Promise<{ developer_access: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>;
  revokeDeveloperAccess(): Promise<{ developer_access: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>;
  supervisorStatus(): Promise<DeveloperSupervisorStatus>;
  supervisorLaunch(params: Record<string, unknown>): Promise<DeveloperSupervisorStatus>;
  supervisorBuildLaunch(params: Record<string, unknown>): Promise<DeveloperSupervisorStatus>;
  supervisorStop(): Promise<DeveloperSupervisorStatus>;
  supervisorRestart(): Promise<DeveloperSupervisorStatus>;
  supervisorRollback(): Promise<DeveloperSupervisorStatus>;
  supervisorEmergencyStop(): Promise<DeveloperSupervisorStatus>;
  supervisorLog(): Promise<{ lines?: string; actions?: string; build?: string }>;
  commandsList(surface: string): Promise<PaletteCommand[]>;
  logsTail(input?: { limit?: number }): Promise<{ lines?: string[] }>;
  mcpTools(transportName: string): Promise<Array<Record<string, unknown>>>;
}

export interface SettingsApi {
  get(): Promise<ProductSettings | null>;
  set(next: Record<string, unknown>): Promise<unknown>;
  authStatus(): Promise<Record<string, unknown>>;
  authImportCli(): Promise<unknown>;
  authImportHermes(): Promise<unknown>;
}

export interface ShellApi {
  run(line: string): Promise<TerminalResult>;
  windowAction(action: 'minimize' | 'maximize' | 'close'): Promise<unknown>;
  openPath(path: string): Promise<unknown>;
  pickFolder(): Promise<ProjectRootSelection>;
}

export interface BrowserApi {
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
}

export interface CampaignsApi {
  list(): Promise<Campaign[]>;
  create(input: Record<string, unknown>): Promise<unknown>;
  run(id: string): Promise<unknown>;
  status(id: string): Promise<unknown>;
}

/* ------------------------------------------------------------------ */
/* Construction                                                       */
/* ------------------------------------------------------------------ */

export function createDomainApis(
  transport: OptimusTransport | null,
  observer: RuntimeObserver | undefined
) {
  const get = <T>(method: DesktopMethod, params?: Record<string, unknown>): Promise<T> =>
    call<T>(transport, observer, method, params);

  return {
    sessions: {
      list: () => get<{ sessions?: SessionMeta[] } | SessionMeta[]>('sessions').then((r) => list<SessionMeta>(r, 'sessions')),
      newSession: (projectId?: string) =>
        get<SessionMeta>('new_session', projectId ? { project_id: projectId } : undefined),
      get: (id: string) => get<SessionDetail>('get_session', { id }),
      rename: (id: string, title: string) => get('rename_session', { id, title }),
      archive: (id: string, archived: boolean) => get('archive_session', { id, archived }),
      pin: (id: string, pinned: boolean) => get('pin_session', { id, pinned }),
      delete: (id: string) => get('delete_session', { id }),
      search: (params: Record<string, unknown>) =>
        get<{ sessions?: SessionMeta[] }>('session_search', params).then((r) => r.sessions ?? []),
      startupContext: () => get<{ session_id?: string | null }>('startup_context'),
    } satisfies SessionsApi,
    approvals: {
      list: () => get<{ pending?: Approval[] }>('approvals_list').then((r) => r.pending ?? []),
      grant: (request) => get('approvals_grant', request),
      releaseYolo: () => get('approvals_release_yolo'),
    } satisfies ApprovalsApi,
    cron: {
      list: () => get<{ jobs?: CronJob[] }>('cron_list').then((r) => r.jobs ?? []),
      add: async (input) => {
        const result = await get<{ jobs?: CronJob[] } | null>('cron_add', input);
        return result?.jobs ?? (await get<{ jobs?: CronJob[] }>('cron_list')).jobs ?? [];
      },
      history: (id: string, limit = 20) =>
        get<{ attempts?: CronAttempt[] }>('cron_history', { id, limit }).then((r) => r.attempts ?? []),
      setEnabled: async (id: string, enabled: boolean) => {
        const result = await get<{ jobs?: CronJob[] } | null>('cron_set_enabled', { id, enabled });
        return result?.jobs ?? (await get<{ jobs?: CronJob[] }>('cron_list')).jobs ?? [];
      },
      remove: async (id: string) => {
        const result = await get<{ jobs?: CronJob[] } | null>('cron_remove', { id });
        return result?.jobs ?? (await get<{ jobs?: CronJob[] }>('cron_list')).jobs ?? [];
      },
      tick: (id: string) => get('cron_tick', { id }),
    } satisfies CronApi,
    jobs: {
      list: () => get<{ jobs?: Job[] } | Job[]>('jobs_list').then((r) => list<Job>(r, 'jobs')),
    } satisfies JobsApi,
    artifacts: {
      list: () => get<{ artifacts?: ArtifactRecord[] }>('artifacts_list').then((r) => r.artifacts ?? []),
      get: (sha256: string) => get<ArtifactDetail>('artifacts_get', { sha256 }),
      putText: (input) => get('artifacts_put_text', input),
      deleteMany: (sha256s: string[]) => get('artifacts_delete_many', { sha256s }),
      export: (sha256: string) => get<{ path?: string }>('artifacts_export', { sha256 }),
      exportZip: (sha256s: string[]) => get<{ path?: string }>('artifacts_export_zip', { sha256s }),
    } satisfies ArtifactsApi,
    fs: {
      roots: () => get<{ roots?: string[] }>('fs_roots'),
      list: (path: string) =>
        get<{ entries?: FsEntry[] }>('fs_list', { path }).then((r) => r.entries ?? []),
      read: (path: string) =>
        get<{ path: string; content: string; truncated: boolean }>('fs_read', { path }),
    } satisfies FsApi,
    memory: {
      list: (input) =>
        get<{ claims?: Array<Record<string, unknown>>; fence?: string }>('memory_list', input),
      recall: (input) => get<Record<string, unknown>>('memory_recall', input),
      search: (input) => get<Record<string, unknown>>('memory_search', input),
      correct: (id, object) => get('memory_correct', { id, object }),
      forget: (id) => get('memory_forget', { id }),
    } satisfies MemoryApi,
    skills: {
      list: (input) =>
        get<{ skills?: Array<Record<string, unknown>> }>('skills_list', input).then((r) => r.skills ?? []),
      pin: (id) => get('skills_pin', { id }),
      deprecate: (id) => get('skills_deprecate', { id }),
    } satisfies SkillsApi,
    packs: {
      state: () => get<Record<string, unknown>>('packs_state'),
      activate: (name: string) => get('packs_activate', { name }),
      deactivate: (name: string) => get('packs_deactivate', { name }),
      verifySigned: () => get('packs_verify_signed'),
    } satisfies PacksApi,
    gateway: {
      status: () => get<{ status?: GatewayStatus }>('gateway_status').then((r) => r.status ?? null),
      inbox: () => get<{ messages?: InboxMessage[] }>('gateway_inbox').then((r) => r.messages ?? []),
      outbox: (limit = 50) =>
        get<{ messages?: OutboxReceipt[] }>('gateway_outbox', { limit }).then((r) => r.messages ?? []),
      ambiguous: () => get<{ messages?: OutboxReceipt[] }>('gateway_ambiguous').then((r) => r.messages ?? []),
      enqueue: (text: string, channel: string) => get('gateway_enqueue', { text, channel }),
      ackDelivery: (messageId: string, outboundId: string) =>
        get('gateway_ack_delivery', { message_id: messageId, outbound_id: outboundId }),
      telegramStatus: () => get<Record<string, unknown>>('gateway_telegram_status'),
    } satisfies GatewayApi,
    providers: {
      catalog: () => get<{ providers?: ProviderRow[] }>('providers_catalog').then((r) => r.providers ?? []),
      routePreview: (input) => get('providers_route_preview', input),
      keysStatus: () => get<{ providers?: ProviderKeyStatus[] }>('provider_keys_status').then((r) => r.providers ?? []),
      keySet: (provider: string, apiKey: string) =>
        get<{ providers?: ProviderKeyStatus[] }>('provider_key_set', { provider, api_key: apiKey }).then((r) => r.providers ?? []),
      keyClear: (provider: string) =>
        get<{ providers?: ProviderKeyStatus[] }>('provider_key_clear', { provider }).then((r) => r.providers ?? []),
    } satisfies ProvidersApi,
    consents: {
      grant: (sessionId: string, commandClass: string, projectId?: string) =>
        get('session_consent_grant', {
          session_id: sessionId,
          command_class: commandClass,
          ...(projectId ? { project_id: projectId } : {}),
        }),
      list: (sessionId: string, projectId?: string) =>
        get<{ grants?: SessionConsent[] }>('session_consent_list', {
          session_id: sessionId,
          ...(projectId ? { project_id: projectId } : {}),
        }).then((r) => r.grants ?? []),
      revoke: (sessionId: string, projectId?: string) =>
        get<{ revoked: number }>('session_consent_revoke', {
          session_id: sessionId,
          ...(projectId ? { project_id: projectId } : {}),
        }),
      revokeAll: (sessionId: string, projectId?: string) =>
        get<{ revoked: number }>('session_consent_revoke_all', {
          session_id: sessionId,
          ...(projectId ? { project_id: projectId } : {}),
        }),
    } satisfies ConsentsApi,
    projects: {
      scopesList: () =>
        get<{ projects?: ProjectRuntimeScope[] }>('project_scopes_list').then((r) => r.projects ?? []),
      authorize: (input) => get<{ project?: ProjectRuntimeScope | null }>('project_scopes_authorize', input),
    } satisfies ProjectsApi,
    system: {
      ping: () => get('ping'),
      doctor: () => get<Doctor>('doctor'),
      developerAccess: () => get<{ developer_access?: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>('developer_access_get'),
      enableDeveloperAccess: (input) =>
        get<{ developer_access: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>('developer_access_enable', input),
      revokeDeveloperAccess: () => get<{ developer_access: DeveloperAccess; supervisor?: DeveloperSupervisorStatus }>('developer_access_revoke'),
      supervisorStatus: () => get<DeveloperSupervisorStatus>('developer_supervisor_status'),
      supervisorLaunch: (params) => get<DeveloperSupervisorStatus>('developer_supervisor_launch', params),
      supervisorBuildLaunch: (params) => get<DeveloperSupervisorStatus>('developer_supervisor_build_launch', params),
      supervisorStop: () => get<DeveloperSupervisorStatus>('developer_supervisor_stop'),
      supervisorRestart: () => get<DeveloperSupervisorStatus>('developer_supervisor_restart'),
      supervisorRollback: () => get<DeveloperSupervisorStatus>('developer_supervisor_rollback'),
      supervisorEmergencyStop: () => get<DeveloperSupervisorStatus>('developer_emergency_stop'),
      supervisorLog: () => get<{ lines?: string; actions?: string; build?: string }>('developer_supervisor_log', { lines: 120 }),
      commandsList: (surface: string) =>
        get<{ commands?: PaletteCommand[] }>('commands_list', { surface }).then((r) => r.commands ?? []),
      logsTail: (input) => get<{ lines?: string[] }>('logs_tail', input),
      mcpTools: (transportName: string) =>
        get<{ tools?: Array<Record<string, unknown>> }>('mcp_tools', { transport: transportName }).then((r) => r.tools ?? []),
    } satisfies SystemApi,
    settings: {
      get: () => get<{ settings?: ProductSettings }>('settings_get').then((r) => r.settings ?? null),
      set: (next) => get('settings_set', next),
      authStatus: () => get<Record<string, unknown>>('auth_status'),
      authImportCli: () => get('auth_import_cli'),
      authImportHermes: () => get('auth_import_hermes'),
    } satisfies SettingsApi,
    shell: {
      run: (line: string) => get<TerminalResult>('term_run', { line }),
      windowAction: (action) => {
        if (!transport) throw new NoTransportError();
        return transport.windowAction(action);
      },
      openPath: (path: string) => {
        if (!transport) throw new NoTransportError();
        return transport.openPath(path);
      },
      pickFolder: () => {
        if (!transport) throw new NoTransportError();
        return transport.pickFolder();
      },
    } satisfies ShellApi,
    campaigns: {
      list: () => get<{ campaigns?: Campaign[] }>('campaign_list').then((r) => r.campaigns ?? []),
      create: (input) => get('campaign_create', input),
      run: (id: string) => get('campaign_run', { id }),
      status: (id: string) => get('campaign_status', { id }),
    } satisfies CampaignsApi,
  };
}
