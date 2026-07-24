import type {
  Approval,
  ArtifactDetail,
  ArtifactRecord,
  BrowserState,
  Campaign,
  ChatHandle,
  ChatRequest,
  CronJob,
  DesktopMethod,
  Doctor,
  FsEntry,
  OptimusTransport,
  ProductSettings,
  SessionDetail,
  SessionMeta,
  StreamEvent,
  ToolLifecycleEvent,
} from './contracts';

const sessions: SessionMeta[] = [
  {
    id: 'fixture-assess',
    title: 'Assess Optimus Agent Project State',
    message_count: 8,
    updated_at: new Date().toISOString(),
  },
  { id: 'fixture-ui', title: 'T3-style React workbench', message_count: 4 },
  { id: 'fixture-runtime', title: 'Runtime approval audit', message_count: 6 },
  { id: 'fixture-browser', title: 'Preview browser integration', message_count: 5 },
];

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
      ],
    },
  ],
]);

const settings: ProductSettings = {
  work_isolation: 'shared',
  work_isolation_label: 'Shared workbench',
  allow_concurrent_projects: false,
  enforcement_active: true,
};

const doctor: Doctor = {
  version: '0.1.0',
  home: '/home/dev/.local/share/optimus',
  phase: 'electron-react-workbench',
  browser: 'electron-webcontents-view',
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
  isolation_enforcement_active: true,
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
    node_label: 'npm test',
    node_index: 0,
    has_grant: false,
    effect_json: '{"RunCommand":{"program":"npm","args":["test"]}}',
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

export function createFixtureTransport(): OptimusTransport {
  let streamId = 1;
  return {
    kind: 'fixture',
    async invoke<T>(method: DesktopMethod, params: Record<string, unknown> = {}) {
      await sleep(35);
      const value = await fixtureInvoke(method, params);
      return value as T;
    },
    chat(request: ChatRequest, onEvent: (event: StreamEvent) => void): ChatHandle {
      const id = streamId++;
      let cancelled = false;
      const response =
        `I’m working from the ${request.provider} fixture transport. ` +
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
        onEvent({ type: 'done', result: { provider: request.provider } });
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
      path: '/home/dev/Projects/New Project',
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

async function fixtureInvoke(method: DesktopMethod, params: Record<string, unknown>) {
  switch (method) {
    case 'doctor':
      return doctor;
    case 'auth_status':
      return { present: true, mode: 'fixture' };
    case 'settings_get':
      return { settings };
    case 'settings_set':
      Object.assign(settings, params);
      doctor.work_isolation = settings.work_isolation;
      doctor.work_isolation_label = settings.work_isolation_label;
      doctor.allow_concurrent_projects = settings.allow_concurrent_projects;
      return { settings };
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
    case 'cron_list':
      return { jobs: cronJobs };
    case 'cron_add': {
      const cron = {
        id: `cron-${Date.now()}`,
        name: String(params.name || 'New schedule'),
        every_secs: Number(params.every_secs || 3600),
        enabled: true,
        provider: String(params.provider || 'offline'),
      };
      cronJobs.unshift(cron);
      return cron;
    }
    case 'cron_tick':
      return { ran: [] };
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
          roots: ['/home/mustbearnold/Projects/Optimus Agent'],
          primary_root: '/home/mustbearnold/Projects/Optimus Agent',
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
