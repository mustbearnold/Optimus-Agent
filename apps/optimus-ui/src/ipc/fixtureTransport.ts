import type {
  Approval,
  ApprovalResolveRequest,
  ArtifactDetail,
  ArtifactRecord,
  BrowserState,
  Campaign,
  ChatHandle,
  ChatRequest,
  CronJob,
  DesktopMethod,
  DeveloperAccess,
  Doctor,
  FsEntry,
  OptimusTransport,
  ProductSettings,
  SessionDetail,
  SessionMeta,
  StreamEvent,
  ToolLifecycleEvent,
} from './contracts';

// Every rail shape the product can render has to exist here, or the suites
// measure a rail the user never sees. A pinned row and a project-assigned row
// were both missing, and a project-assigned row is the one that stacks project
// name, run state, title, and worktree into a single card — the shape whose
// fixed height was clipping its own text in the shipped build.
const sessions: SessionMeta[] = [
  {
    id: 'fixture-assess',
    title: 'Assess Optimus Agent Project State',
    message_count: 8,
    updated_at: new Date().toISOString(),
  },
  {
    // Deliberately long. Short fixture titles fit any card, so a card that is
    // too short for its own content still measured clean; the shipped clipping
    // only appeared once a real title wrapped.
    id: 'fixture-pinned',
    title: 'Can you develop yourself optimus while I use the app and keep the session open',
    message_count: 12,
    pinned: true,
    updated_at: new Date().toISOString(),
  },
  { id: 'fixture-ui', title: 'Workspace shell redesign', message_count: 4 },
  { id: 'fixture-runtime', title: 'Runtime approval audit', message_count: 6 },
  { id: 'fixture-browser', title: 'Preview browser integration', message_count: 5 },
];

// Scale knob for layout audits: a rail with five chats hides every defect that
// only appears at volume (band scrolling, row rhythm at 60 entries, project
// trees with real depth). The fixture transport is already the deterministic
// stand-in, so the knob lives here, off by default, set only by test harnesses.
try {
  const bulk = Number(localStorage.getItem('optimus.fixture.bulkSessions') || 0);
  for (let index = 0; index < Math.min(bulk, 500); index += 1) {
    sessions.push({
      id: `fixture-bulk-${index}`,
      title: `Bulk conversation ${index} — a realistically long exploratory thread title`,
      message_count: 1 + (index % 9),
    });
  }
} catch {
  // No storage (SSR/test) means no bulk rows.
}

const details = new Map<string, SessionDetail>([
  [
    'fixture-assess',
    {
      ...sessions[0]!,
      messages: [
        {
          role: 'user',
          content:
            'Assess the current Optimus Agent project state and identify the most important next implementation slice.',
        },
        {
          role: 'assistant',
          content:
            'The durable Rust runtime is ahead of the new React surface. The highest-leverage slice is the workbench cutover: preserve the frozen IPC contract, port the daily-use surfaces, and prove the native Browser boundary separately.',
        },
        {
          role: 'user',
          content: 'Keep the interface dense, truthful, and fast on a high-refresh display.',
        },
        {
          role: 'assistant',
          content:
            'Understood. I’ll keep outcome and controls above raw activity, coalesce stream projection to the display clock, and avoid decorative work that competes with input.',
        },
        {
          role: 'assistant',
          content: 'The focused verification command is ready, but it needs your approval before Optimus can run it.',
          tool_events: [
            {
              type: 'tool',
              schema_version: 1,
              event_id: 'fixture-approval:required',
              run_id: '11111111-1111-4111-8111-111111111111',
              call_id: 'call_fixture_approval',
              tool_id: 'terminal',
              phase: 'approval_required',
              summary: 'Run the focused React verification command',
              approval: {
                run_id: '11111111-1111-4111-8111-111111111111',
                call_id: 'call_fixture_approval',
                tool_id: 'terminal',
                job_id: '22222222-2222-4222-8222-222222222222',
                node_id: '33333333-3333-4333-8333-333333333333',
                node_index: 0,
                effect_sha256: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                summary: 'Run the focused React verification command',
              },
            },
          ],
        },
      ],
    },
  ],
]);

// Key-based provider credentials. The fixture never holds a real key; it only
// mirrors the present/source/hint shape the host reports.
type FixtureProviderKey = {
  provider: string;
  label: string;
  env_var: string;
  present: boolean;
  source: string;
  hint: string | null;
  base_url: string | null;
};

const providerKeys: FixtureProviderKey[] = [
  {
    provider: 'deepseek',
    label: 'DeepSeek',
    env_var: 'DEEPSEEK_API_KEY',
    present: false,
    source: 'none',
    hint: null,
    base_url: null,
  },
];

function providerKeyEntry(provider: string): FixtureProviderKey {
  const entry = providerKeys.find((item) => item.provider === provider);
  if (!entry) throw new Error(`unsupported key provider: ${provider}`);
  return entry;
}

const settings: ProductSettings = {
  work_isolation: 'shared',
  work_isolation_label: 'Shared workbench',
  allow_concurrent_projects: false,
  enforcement_active: false,
  product_fs_enforced: false,
  configured_mode: 'shared',
  enforced_mode: 'shared',
  command_envelope_enforced: true,
  developer_access: disabledDeveloperAccess(),
};

function disabledDeveloperAccess(): DeveloperAccess {
  return {
    enabled: false,
    scope: { kind: 'selected_repository', root: '' },
    scope_label: 'Selected repository',
    roots: [],
    capabilities: {
      workspace_files: true,
      terminal_execution: true,
      process_management: true,
      package_installation: true,
      network_access: true,
      external_services: false,
      production_systems: false,
      secrets: false,
    },
    pause_before_destructive: true,
    checkpoint_on_mutation: true,
  };
}

const doctor: Doctor = {
  version: '0.1.0',
  home: '/home/dev/.local/share/optimus',
  phase: 'product-complete',
  browser: 'cdp',
  preview_browser: true,
  streaming: true,
  files: true,
  approvals: true,
  campaigns: true,
  cron: true,
  gateway: true,
  cron_jobs: 2,
  campaigns_active: 1,
  approvals_pending: 1,
  core_schema_tokens: 1834,
  max_budget: 2500,
  work_isolation: 'shared',
  work_isolation_label: 'Shared workbench',
  allow_concurrent_projects: false,
  isolation_enforcement_active: false,
  product_fs_enforced: false,
  configured_mode: 'shared',
  enforced_mode: 'shared',
  command_envelope_enforced: true,
  settings,
  pack_catalog: [
    {
      id: 'core',
      description: 'Workspace-safe read, write, memory, and skill tools.',
      tools: [
        { id: 'read_file', policy: 'workspace_read' },
        { id: 'write_file', policy: 'durable_effect' },
        { id: 'terminal', policy: 'smart_deny' },
      ],
    },
    {
      id: 'browser',
      description: 'Bounded navigation and inspection.',
      tools: [
        { id: 'browser_navigate', policy: 'desktop' },
        { id: 'browser_snapshot', policy: 'desktop' },
      ],
    },
  ],
};

const approvals: Approval[] = [
  {
    job_id: 'job-fixture-1',
    job_label: 'Run focused React tests',
    job_status: 'AwaitingApproval',
    node_label: 'bun test',
    node_index: 0,
    has_grant: false,
    effect_json: '{"RunCommand":{"program":"bun","args":["test"]}}',
  },
];

const campaigns: Campaign[] = [
  { id: 'campaign-1', name: 'React cutover verification', status: 'Running', current_step: 2 },
];

const cronJobs: CronJob[] = [
  {
    id: 'cron-1',
    name: 'Daily project check',
    every_secs: 86400,
    enabled: true,
    last_status: 'Succeeded',
    provider: 'offline',
  },
  {
    id: 'cron-2',
    name: 'Weekly capability audit',
    every_secs: 604800,
    enabled: true,
    provider: 'offline',
  },
];

const fileEntries: FsEntry[] = [
  { name: 'apps', path: 'apps', kind: 'directory', is_dir: true },
  { name: 'crates', path: 'crates', kind: 'directory', is_dir: true },
  { name: 'docs', path: 'docs', kind: 'directory', is_dir: true },
  { name: 'AGENTS.md', path: 'AGENTS.md', kind: 'file', size: 6841 },
  { name: 'Cargo.toml', path: 'Cargo.toml', kind: 'file', size: 1714 },
  { name: 'DESIGN.md', path: 'DESIGN.md', kind: 'file', size: 18112 },
];

const artifacts: ArtifactRecord[] = [
  {
    sha256: '5b87f4f731a22c1d',
    label: 'Workbench acceptance notes',
    source: 'react-cutover',
    media_type: 'text/markdown',
    size_bytes: 2840,
  },
  {
    sha256: 'd73f9aa24180e439',
    label: 'Wide workbench capture',
    source: 'browser-contract',
    media_type: 'image/png',
    size_bytes: 184220,
  },
];

let browserState: BrowserState = {
  url: 'https://www.google.com/',
  title: 'Google',
  loading: false,
  canGoBack: false,
  canGoForward: false,
  visible: true,
  native: false,
};

const browserListeners = new Set<(state: BrowserState) => void>();
const sleep = (milliseconds: number) =>
  new Promise<void>((resolve) => window.setTimeout(resolve, milliseconds));

export function createFixtureTransport(): OptimusTransport & {
  legacyInvoke<T>(method: string, params?: Record<string, unknown>): Promise<T>;
} {
  let streamId = 1;
  return {
    kind: 'fixture',
    async invoke<T>(method: DesktopMethod, params: Record<string, unknown> = {}) {
      await sleep(35);
      const value = await fixtureInvoke(method, params);
      return value as T;
    },
    // Named typed legacy shim (spec-015 A3): a STRING-typed invoke path for
    // the superseded `chat_approval_resolve` member only (removed from the
    // DesktopMethod union; exempted in the surface-contract gate as the
    // HTTP-legacy bucket). Everything else delegates to the typed switch.
    async legacyInvoke<T>(
      method: string,
      params: Record<string, unknown> = {}
    ): Promise<T> {
      await sleep(35);
      const value = await fixtureLegacyInvoke(method, params);
      return value as T;
    },
    chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
      const id = streamId++;
      let cancelled = false;
      // Scenario: chat-start configuration errors reject the handle so the
      // workbench must surface the real message (regression: a rejected chat
      // start used to be flattened into "Connection lost").
      if (request.message.includes('fixture chat error')) {
        const done = (async () => {
          await sleep(30);
          throw new Error(
            'No DeepSeek API key. Add one in Settings > Authentication, or set DEEPSEEK_API_KEY before launching.'
          );
        })();
        return {
          streamId: id,
          done,
          cancel: async () => ({ requested: true }),
        };
      }
      const provider = request.provider === 'auto' ? 'offline' : request.provider;
      const model =
        request.model ||
        (provider === 'offline'
          ? 'offline-scripted'
          : provider === 'codex'
            ? 'gpt-5.6-terra'
            : provider === 'deepseek'
              ? 'deepseek-v4-flash'
            : 'gpt-4.1');
      const response =
        `I’m working from the ${provider} fixture transport. ` +
        `The React workbench keeps this stream attached to “${sessionTitle(request.session)}”, ` +
        'projects activity at most once per display frame, and preserves partial text if you stop it.';
      const chunks = response.match(/.{1,7}/g) || [response];
      const done = (async () => {
        const runId = `fixture-run-${id}`;
        const callId = `fixture-call-${id}`;
        const startedEvent: ToolLifecycleEvent = {
          type: 'tool',
          schema_version: 1,
          event_id: `${runId}:${callId}:started`,
          run_id: runId,
          call_id: callId,
          tool_id: 'read_file',
          phase: 'started',
          summary: 'Inspecting React workbench contracts',
        };
        const succeededEvent: ToolLifecycleEvent = {
          type: 'tool',
          schema_version: 1,
          event_id: `${runId}:${callId}:succeeded`,
          run_id: runId,
          call_id: callId,
          tool_id: 'read_file',
          phase: 'succeeded',
          summary: 'Read React workbench contracts',
          duration_ms: 120,
        };
        onEvent({ type: 'status', text: 'Planning the implementation boundary…' });
        await sleep(120);
        onEvent(startedEvent);
        for (const chunk of chunks) {
          if (cancelled) {
            onEvent({ type: 'cancelled', error: 'cancelled by user' });
            return;
          }
          onEvent({ type: 'delta', text: chunk });
          await sleep(24);
        }
        onEvent(succeededEvent);
        onEvent({ type: 'timing', elapsed_ms: chunks.length * 24 + 120 });
        const detail = details.get(request.session) || {
          ...(sessions.find((session) => session.id === request.session) || {
            id: request.session,
            title: 'Session',
          }),
          messages: [],
        };
        detail.messages.push(
          { role: 'user', content: request.message },
          {
            role: 'assistant',
            content: response,
            tool_events: [startedEvent, succeededEvent],
          }
        );
        detail.run_status = 'succeeded';
        detail.message_count = detail.messages.length;
        details.set(request.session, detail);
        const session = sessions.find((candidate) => candidate.id === request.session);
        if (session) session.message_count = detail.messages.length;
        onEvent({
          type: 'done',
          result: { provider, model },
        });
      })();
      return {
        streamId: id,
        done,
        cancel: async () => {
          cancelled = true;
          return { requested: true };
        },
      };
    },
    // Mirrors the host's streaming resolve (ADR-0046): settle the exact call,
    // then stream the resumed continuation until it parks or finishes. The
    // workbench contract under test is that the decision is not a blocking
    // request/response — events arrive as they happen and the handle cancels.
    chatApprovalResolve(
      request: ApprovalResolveRequest,
      onEvent: (event: StreamEvent) => void
    ): ChatHandle {
      const id = streamId++;
      let cancelled = false;
      const decision = request.decision === 'deny' ? 'denied' : 'approved';
      const settlement: ToolLifecycleEvent = {
        type: 'tool',
        schema_version: 1,
        event_id: `${request.run_id}:${request.call_id}:${decision}`,
        run_id: request.run_id,
        call_id: request.call_id,
        tool_id: 'terminal',
        phase: decision === 'approved' ? 'succeeded' : 'cancelled',
        summary:
          decision === 'approved'
            ? 'Approved exact action completed'
            : 'Exact action denied',
        duration_ms: 76,
      };
      const answer =
        decision === 'approved'
          ? 'The approved command ran. Looking at the result, the next milestone is the streaming approval continuation.'
          : 'The exact action was denied. Nothing was executed.';
      const chunks = answer.match(/.{1,9}/g) || [answer];
      const done = (async () => {
        const sessionId = request.session_id;
        const detail = details.get(sessionId) || {
          ...(sessions.find((session) => session.id === sessionId) || {
            id: sessionId,
            title: 'Session',
          }),
          messages: [],
        };
        onEvent({ type: 'status', text: 'Resolving approval…' });
        await sleep(40);
        onEvent(settlement);
        detail.messages = detail.messages.map((message) => ({
          ...message,
          ...(message.tool_events
            ? {
                tool_events: message.tool_events.map((event) =>
                  event.call_id === request.call_id ? settlement : event
                ),
              }
            : {}),
        }));
        for (const chunk of chunks) {
          if (cancelled) {
            onEvent({ type: 'cancelled', error: 'cancelled by user' });
            return;
          }
          onEvent({ type: 'delta', text: chunk });
          await sleep(20);
        }
        detail.messages.push({ role: 'assistant', content: answer });
        detail.run_status = 'succeeded';
        detail.message_count = detail.messages.length;
        details.set(sessionId, detail);
        const session = sessions.find((candidate) => candidate.id === sessionId);
        if (session) session.message_count = detail.messages.length;
        onEvent({ type: 'timing', elapsed_ms: 120 });
        onEvent({
          type: 'done',
          result: { status: decision, resume_error: null, assistant_text: answer },
        });
      })();
      return {
        streamId: id,
        done,
        cancel: async () => {
          cancelled = true;
          return { requested: true };
        },
      };
    },
    windowAction: async () => ({ ok: true }),
    pickFolder: async () => ({
      ok: true,
      path: '/projects/new-project',
      grantToken: 'fixture-native-grant',
    }),
    openPath: async () => ({ ok: true }),
    browser: {
      setBounds: () => undefined,
      setVisible: (visible) => {
        browserState = { ...browserState, visible };
        emitBrowser();
      },
      navigate: async (url) => {
        browserState = { ...browserState, url, title: hostLabel(url), loading: true };
        emitBrowser();
        await sleep(180);
        browserState = {
          ...browserState,
          loading: false,
          canGoBack: true,
          title: hostLabel(url),
        };
        emitBrowser();
        return browserState;
      },
      back: async () => browserState,
      forward: async () => browserState,
      reload: async () => {
        browserState = { ...browserState, loading: true };
        emitBrowser();
        await sleep(120);
        browserState = { ...browserState, loading: false };
        emitBrowser();
        return browserState;
      },
      state: async () => browserState,
      annotate: async () => ({ cancelled: true }),
      cancelAnnotation: async () => ({ cancelled: true }),
      subscribe: (listener) => {
        browserListeners.add(listener);
        return () => browserListeners.delete(listener);
      },
    },
  };
}

/// The superseded `chat_approval_resolve` fixture behavior (spec-015 A3):
/// resolves the parked approval card exactly as the old typed switch did.
function fixtureResolveApproval(params: Record<string, unknown>): Record<string, unknown> {
  const sessionId = String(params.session_id || '');
  const detail = details.get(sessionId);
  if (!detail) throw new Error('Approval session was not found.');
  const decision = params.decision === 'deny' ? 'denied' : 'approved';
  detail.run_status = 'succeeded';
  detail.messages = detail.messages.map((message) => ({
    ...message,
    ...(message.tool_events
      ? {
          tool_events: message.tool_events.map((event) =>
            event.call_id === params.call_id
              ? {
                  ...event,
                  event_id: `${event.run_id}:${event.call_id}:${decision}`,
                  phase: decision === 'approved' ? 'succeeded' : 'cancelled',
                  summary:
                    decision === 'approved'
                      ? 'Approved exact action completed'
                      : 'Exact action denied',
                  approval: undefined,
                }
              : event
          ),
        }
      : {}),
  }));
  detail.messages.push({
    role: 'assistant',
    content:
      decision === 'approved'
        ? 'Approved and completed the exact requested action.'
        : 'Denied the exact requested action. Nothing was executed.',
  });
  return {
    session_id: sessionId,
    run_id: params.run_id,
    call_id: params.call_id,
    job_id: params.job_id,
    node_id: params.node_id,
    node_index: params.node_index,
    effect_sha256: params.effect_sha256,
    tool_id: 'terminal',
    summary: 'Run the focused React verification command',
    status: decision,
  };
}

/// Named typed legacy shim dispatcher (spec-015 A3): the superseded
/// `chat_approval_resolve` member is handled here (string-typed), everything
/// else delegates to the typed `fixtureInvoke` switch.
async function fixtureLegacyInvoke(method: string, params: Record<string, unknown>) {
  switch (method) {
    case 'chat_approval_resolve':
      return fixtureResolveApproval(params);
    default:
      return fixtureInvoke(method as DesktopMethod, params);
  }
}

async function fixtureInvoke(method: DesktopMethod, params: Record<string, unknown>) {
  switch (method) {
    case 'doctor':
      return doctor;
    case 'auth_status':
      return { present: true, mode: 'fixture' };
    case 'provider_keys_status':
      return { providers: providerKeys };
    case 'provider_key_set': {
      const apiKey = String(params.api_key ?? '').trim();
      if (!apiKey) throw new Error('provider API key must not be empty');
      const entry = providerKeyEntry(String(params.provider ?? 'deepseek'));
      entry.present = true;
      entry.source = 'stored';
      entry.hint = apiKey.length > 8 ? `••••${apiKey.slice(-4)}` : '••••••••';
      entry.base_url = (params.base_url as string) || null;
      return { providers: providerKeys };
    }
    case 'provider_key_clear': {
      const entry = providerKeyEntry(String(params.provider ?? 'deepseek'));
      entry.present = false;
      entry.source = 'none';
      entry.hint = null;
      entry.base_url = null;
      return { providers: providerKeys };
    }
    case 'settings_get':
      return { settings };
    case 'settings_set':
      Object.assign(settings, params);
      doctor.work_isolation = settings.work_isolation;
      doctor.work_isolation_label = settings.work_isolation_label;
      doctor.allow_concurrent_projects = settings.allow_concurrent_projects;
      return { settings };
    case 'developer_access_get':
      return {
        developer_access: settings.developer_access,
        supervisor: { status: 'idle', healthy: false, previous_available: false },
        confirmation: 'I understand Developer Full Access risks',
        confirmation_version: 1,
      };
    case 'developer_access_enable': {
      const grant = (params.grant || {}) as Partial<DeveloperAccess>;
      settings.developer_access = {
        ...disabledDeveloperAccess(),
        ...grant,
        enabled: true,
        scope: (grant.scope || disabledDeveloperAccess().scope) as DeveloperAccess['scope'],
      };
      return { developer_access: settings.developer_access, supervisor: { status: 'idle', healthy: false } };
    }
    case 'developer_access_revoke':
      settings.developer_access = disabledDeveloperAccess();
      return { developer_access: settings.developer_access, supervisor: { status: 'emergency_stopped', healthy: false } };
    case 'startup_context':
      return { session_id: null, handoff: false };
    case 'developer_supervisor_status':
      return { status: 'idle', healthy: false, previous_available: false };
    case 'developer_supervisor_build_launch':
      return {
        status: 'healthy',
        healthy: true,
        pid: 4242,
        port: Number(params.port || 17866),
        binary: String(params.binary || '/fixture/target/debug/optimus-desktop'),
        surface: String(params.surface || 'desktop'),
        workspace: String(params.workspace || '/fixture/optimus-agent'),
        previous_available: false,
        build: {
          binary: String(params.binary || '/fixture/target/debug/optimus-desktop'),
          surface: String(params.surface || 'desktop'),
          workspace: String(params.workspace || '/fixture/optimus-agent'),
          profile: 'debug',
          log_path: '/fixture/developer-supervisor/build.log',
        },
      };
    case 'developer_supervisor_log':
      return { path: '/fixture/developer-supervisor/instance.log', lines: '', line_count: 0, action_path: '/fixture/developer-supervisor/actions.log', actions: '', action_line_count: 0 };
    case 'developer_supervisor_stop':
    case 'developer_supervisor_restart':
    case 'developer_supervisor_rollback':
    case 'developer_emergency_stop':
      return { status: method === 'developer_emergency_stop' ? 'emergency_stopped' : 'stopped', healthy: false };
    case 'sessions':
      return { sessions };
    case 'new_session': {
      const session: SessionMeta = {
        id: `fixture-${Date.now()}`,
        title: 'New Optimus session',
        message_count: 0,
      };
      sessions.unshift(session);
      details.set(session.id, { ...session, messages: [] });
      return session;
    }
    case 'get_session': {
      const id = String(params.id || '');
      return (
        details.get(id) || {
          ...(sessions.find((session) => session.id === id) || { id, title: 'Session' }),
          messages: [],
        }
      );
    }
    case 'rename_session': {
      const session = sessions.find((item) => item.id === params.id);
      if (session) session.title = String(params.title || session.title);
      return { id: params.id, title: params.title };
    }
    case 'delete_session': {
      const index = sessions.findIndex((item) => item.id === params.id);
      if (index >= 0) sessions.splice(index, 1);
      return { deleted: index >= 0, id: params.id };
    }
    case 'session_search': {
      const q = String(params.q || params.query || '').toLowerCase();
      const includeArchived = params.include_archived !== false;
      const filtered = sessions.filter((session) => {
        if (!includeArchived && session.archived) return false;
        if (!q) return true;
        return (session.title || session.id).toLowerCase().includes(q);
      });
      return { sessions: filtered, q };
    }
    case 'archive_session': {
      const session = sessions.find((item) => item.id === params.id);
      if (session) session.archived = Boolean(params.archived);
      return { id: params.id, archived: Boolean(params.archived) };
    }
    case 'pin_session': {
      const session = sessions.find((item) => item.id === params.id);
      if (session) session.pinned = Boolean(params.pinned);
      return { id: params.id, pinned: Boolean(params.pinned) };
    }
    case 'approvals_list':
      return { pending: approvals };
    case 'approvals_grant': {
      const index = approvals.findIndex((approval) => approval.job_id === params.job_id);
      if (index >= 0) approvals.splice(index, 1);
      doctor.approvals_pending = approvals.length;
      return { job_id: params.job_id, status: 'Succeeded', stdout: 'fixture verification passed' };
    }
    case 'jobs_list':
      return { jobs: [{ job_id: 'job-active', label: 'React workbench', status: 'Running' }] };
    case 'campaign_list':
      return { campaigns };
    case 'campaign_create': {
      const campaign = {
        id: `campaign-${Date.now()}`,
        name: String(params.name || 'UI campaign'),
        status: 'Pending',
      };
      campaigns.unshift(campaign);
      return campaign;
    }
    case 'campaign_run': {
      const campaign = campaigns.find((item) => item.id === params.id);
      if (campaign) campaign.status = 'Succeeded';
      return { id: params.id, status: 'Succeeded' };
    }

    case 'skills_list':
      return { skills: [] };
    case 'skills_pin':
    case 'skills_deprecate':
      return { id: params.id, ok: true };
    case 'memory_list':
      return { fence: 'EVIDENCE_DATA_NOT_INSTRUCTION_NOT_CAPABILITY', claims: [] };
    case 'memory_recall':
      return { fence: 'EVIDENCE_DATA', purpose: 'inform', current: [], historical: [], conflicts: [], citations: [], abstained: true };
    case 'memory_correct':
    case 'memory_forget':
      return { id: params.id, ok: true };
    case 'packs_state':
      return { loaded: ['core'], schema_tokens: 100, max_schema_tokens: 8000, catalog: [], on_demand_loaded: 0, max_on_demand_packs: 2 };
    case 'packs_activate':
    case 'packs_deactivate':
      return { loaded: ['core'], schema_tokens: 100, max_schema_tokens: 8000, catalog: [] };
    case 'logs_tail':
      return { lines: ['doctor home=~'], count: 1, redacted: true };
    case 'commands_list':
      return {
        commands: [
          { id: 'help', name: 'help', description: 'Show commands' },
          { id: 'capabilities', name: 'capabilities', description: 'Open runtime capabilities' },
        ],
        surface: 'desktop',
      };
    case 'gateway_status':
      return {
        status: {
          inbox_pending: 1,
          inbox_claimed: 0,
          outbox_total: 1,
          ambiguous_sends: 1,
          note: 'Local SQLite is delivery authority. External exactly-once is not claimed.',
        },
      };
    case 'gateway_inbox':
      return {
        messages: [
          {
            id: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
            channel: 'local',
            text: 'Fixture inbound for messaging UI',
            provider: 'offline',
            session_id: null,
            received_unix: 1,
          },
        ],
        count: 1,
      };
    case 'gateway_outbox':
      return {
        messages: [
          {
            message_id: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
            outbound: {
              id: 'cccccccc-cccc-cccc-cccc-cccccccccccc',
              in_reply_to: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
              channel: 'local',
              text: 'Fixture outbound reply',
              status: 'ok',
              sent_unix: 2,
            },
            terminal_status: 'succeeded',
            delivered_unix: null,
            ambiguous_send: true,
          },
        ],
        count: 1,
        note: 'delivered_unix is a local adapter receipt, not external EO.',
      };
    case 'gateway_ambiguous':
      return {
        messages: [
          {
            message_id: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
            outbound: {
              id: 'cccccccc-cccc-cccc-cccc-cccccccccccc',
              in_reply_to: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
              channel: 'local',
              text: 'Fixture outbound reply',
              status: 'ok',
              sent_unix: 2,
            },
            terminal_status: 'succeeded',
            delivered_unix: null,
            ambiguous_send: true,
          },
        ],
        count: 1,
      };
    case 'gateway_enqueue':
      return {
        message: {
          id: 'dddddddd-dddd-dddd-dddd-dddddddddddd',
          channel: params.channel || 'local',
          text: params.text || '',
          provider: 'offline',
          received_unix: 3,
        },
      };
    case 'gateway_ack_delivery':
      return {
        acked: true,
        message_id: params.message_id,
        outbound_id: params.outbound_id,
        delivered_unix: 4,
      };
    case 'gateway_telegram_status':
      return {
        enabled: false,
        bot_token_env: 'OPTIMUS_TELEGRAM_BOT_TOKEN',
        token_present: false,
        allowed_chat_ids: [],
        mode: 'mock-or-disabled',
        note: 'Default path is mock/long-poll client. No public listen port. External EO not claimed.',
      };
    case 'providers_catalog':
      return {
        providers: [
          {
            id: 'offline',
            connect: 'connected',
            connect_detail: 'local scripted',
            supports_tools: true,
            supports_vision: false,
            supports_streaming: false,
            default_model: 'offline-scripted',
            remote: false,
          },
          {
            id: 'codex',
            connect: 'disconnected',
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            default_model: 'gpt-5.6-terra',
            remote: true,
          },
        ],
      };
    case 'providers_route_preview':
      return {
        ok: true,
        decision: {
          provider: 'offline',
          model: 'offline-scripted',
          fallback_from: 'codex',
        },
      };
    case 'mcp_status':
      return { session: { pack_id: 'mcp.mock', transport: 'stdio' } };
    case 'mcp_tools':
      return {
        tools: [
          {
            id: 'mcp_echo',
            description: 'Echo via mock MCP',
            available: false,
            policy: 'NetworkRead',
          },
        ],
        count: 1,
        pack_id: 'mcp.mock',
      };
    case 'packs_verify_signed':
      return { ok: true, manifest: { pack_id: 'example.signed', version: '1.0.0' } };
    case 'cron_list':
      return { jobs: cronJobs };
    case 'cron_add': {
      const cron = {
        id: `cron-${Date.now()}`,
        name: String(params.name || 'New schedule'),
        every_secs: Number(params.every_secs || 3600),
        enabled: true,
        provider: String(params.provider || 'offline'),
        prompt: String(params.prompt || 'tick'),
      };
      cronJobs.unshift(cron);
      return cron;
    }
    case 'cron_set_enabled': {
      const job = cronJobs.find((item) => item.id === params.id);
      if (job) job.enabled = Boolean(params.enabled);
      return { id: params.id, enabled: Boolean(params.enabled) };
    }
    case 'cron_remove': {
      const index = cronJobs.findIndex((item) => item.id === params.id);
      if (index >= 0) cronJobs.splice(index, 1);
      return { id: params.id, removed: index >= 0 };
    }
    case 'cron_history':
      return {
        job_id: params.id,
        attempts: [
          {
            attempt_id: 'attempt-1',
            job_id: params.id,
            status: 'succeeded',
            started_unix: 1,
            completed_unix: 2,
            detail: 'ok',
          },
        ],
      };
    case 'cron_tick':
      return { ran: [] };
    case 'artifacts_export':
      return { ok: true, path: `/tmp/export-${params.sha256}.txt`, sha256: params.sha256 };
    case 'artifacts_export_zip':
      return { ok: true, path: '/tmp/artifacts-export.zip', count: Array.isArray(params.sha256s) ? params.sha256s.length : 0 };
    case 'fs_roots':
      return { roots: [{ id: 'home', path: '/home/dev/.local/share/optimus' }] };
    case 'fs_list':
      return { entries: params.path ? [] : fileEntries };
    case 'fs_read':
      return {
        path: params.path,
        content:
          '# Optimus Agent\n\nThis deterministic preview stands in for a sandboxed file read.',
        truncated: false,
      };
    case 'project_scopes_list':
      return {
        projects: [{
          project_id: 'optimus-agent',
          roots: ['/projects/optimus-agent'],
          primary_root: '/projects/optimus-agent',
          updated_unix: 1,
        }],
      };
    case 'project_scopes_authorize':
      return {
        project: Array.isArray(params.root_paths) && params.root_paths.length
          ? {
              project_id: params.project_id,
              roots: params.root_paths,
              primary_root: params.primary_root,
              updated_unix: 1,
            }
          : null,
      };
    case 'artifacts_list':
      return { artifacts };
    case 'artifacts_get': {
      const artifact = artifacts.find((item) => item.sha256 === params.sha256) || artifacts[0]!;
      const detail: ArtifactDetail = {
        artifact,
        kind: 'text',
        text:
          '# Workbench acceptance\n\n- React source harness rendered\n- Frozen IPC contract retained\n- Native proof remains a separate evidence layer',
      };
      return detail;
    }
    case 'artifacts_delete': {
      const index = artifacts.findIndex((item) => item.sha256 === params.sha256);
      if (index >= 0) artifacts.splice(index, 1);
      return { ok: true, sha256: params.sha256 };
    }
    case 'artifacts_delete_many':
      return { ok: true, deleted: params.sha256s || [], failed: [] };
    case 'term_run':
      return {
        job_id: 'job-terminal-fixture',
        status: 'AwaitingApproval',
        stdout: '',
        stderr: '',
        mode: 'job-stream',
        pty: false,
      };
    case 'browser_navigate':
      return { ok: true, url: params.url, title: hostLabel(String(params.url || '')) };
    default:
      return {};
  }
}

function sessionTitle(id: string) {
  return sessions.find((session) => session.id === id)?.title || 'current session';
}

function hostLabel(url: string) {
  try {
    return new URL(url).hostname || url;
  } catch {
    return url;
  }
}

function emitBrowser() {
  browserListeners.forEach((listener) => listener(browserState));
}
