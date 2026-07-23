import {
  Component,
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
  type ErrorInfo,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from 'react';
import type {
  Approval,
  Campaign,
  ChatHandle,
  Doctor,
  Job,
  Project,
  SessionDetail,
  SessionMeta,
} from '../ipc/contracts';
import { getTransport } from '../ipc';
import { appReducer } from '../state/appReducer';
import { conversationStore, useConversation } from '../state/conversationStore';
import {
  defaultLayout,
  loadLayout,
  saveLayout,
  type AppRoute,
  type CompactSurface,
  type WorkspaceTab,
} from '../state/layoutStore';
import {
  loadAssignments,
  loadExpanded,
  loadProjects,
  loadSessionPins,
  saveAssignments,
  saveExpanded,
  saveProjects,
  saveSessionPins,
} from '../state/projectStore';
import { CapabilitiesPage } from '../components/capabilities/CapabilitiesPage';
import { TopBar } from '../components/chrome/TopBar';
import { TruthStrip } from '../components/chrome/TruthStrip';
import { Icon } from '../components/chrome/Icon';
import { ExecutionDock } from '../components/execution/ExecutionDock';
import { TaskPanel } from '../components/execution/TaskPanel';
import { ProjectsRail } from '../components/projects/ProjectsRail';
import { SettingsDialog } from '../components/settings/SettingsDialog';
import { Composer } from '../components/workbench/Composer';
import { Transcript } from '../components/workbench/Transcript';
import { ArtifactsSurface } from '../components/workspace/ArtifactsSurface';
import { WorkspacePane } from '../components/workspace/WorkspacePane';

const transport = getTransport();

type ComposerSettings = {
  provider: 'offline' | 'codex' | 'openai_compat';
  model: string;
  thinking: string;
  access: string;
  fast: boolean;
};

const initialComposer: ComposerSettings = {
  provider: 'offline',
  model: 'offline-echo',
  thinking: 'high',
  access: 'ask',
  fast: false,
};

export function OptimusApp() {
  const [state, dispatch] = useReducer(appReducer, undefined, () => ({
    selectedSessionId: null,
    activeRunSessionId: null,
    layout: typeof localStorage === 'undefined' ? defaultLayout : loadLayout(),
    settingsOpen: false,
    taskPanelOpen: false,
    theme: (localStorage.getItem('optimus.react.theme') === 'light' ? 'light' : 'dark') as 'dark' | 'light',
  }));
  const [doctor, setDoctor] = useState<Doctor | null>(null);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [campaigns, setCampaigns] = useState<Campaign[]>([]);
  const [projects, setProjects] = useState<Project[]>(loadProjects);
  const [pins, setPins] = useState<string[]>(loadSessionPins);
  const [assignments, setAssignments] = useState<Record<string, string>>(loadAssignments);
  const [expanded, setExpanded] = useState<Record<string, boolean>>(loadExpanded);
  const [input, setInput] = useState('');
  const [annotation, setAnnotation] = useState('');
  const [composer, setComposer] = useState(initialComposer);
  const [bootError, setBootError] = useState('');
  const activeHandle = useRef<ChatHandle | null>(null);
  const draggingLayout = useRef(false);
  const latestLayout = useRef(state.layout);
  latestLayout.current = state.layout;
  const selectedSession = sessions.find((session) => session.id === state.selectedSessionId) || null;
  const activeSession = sessions.find((session) => session.id === state.activeRunSessionId) || null;
  const conversation = useConversation(state.selectedSessionId);
  const activeConversation = useConversation(state.activeRunSessionId);

  const refreshRuntime = useCallback(async () => {
    try {
      const [doctorResult, sessionsResult, approvalResult, jobResult, campaignResult] = await Promise.all([
        transport.invoke<Doctor>('doctor'),
        transport.invoke<{ sessions?: SessionMeta[] } | SessionMeta[]>('sessions'),
        transport.invoke<{ pending?: Approval[] }>('approvals_list'),
        transport.invoke<{ jobs?: Job[] }>('jobs_list'),
        transport.invoke<{ campaigns?: Campaign[] }>('campaign_list'),
      ]);
      const nextSessions = Array.isArray(sessionsResult) ? sessionsResult : sessionsResult.sessions || [];
      setDoctor(doctorResult);
      setSessions(nextSessions);
      setApprovals(approvalResult.pending || []);
      setJobs(jobResult.jobs || []);
      setCampaigns(campaignResult.campaigns || []);
      dispatch({
        type: 'select-session',
        id:
          state.selectedSessionId && nextSessions.some((session) => session.id === state.selectedSessionId)
            ? state.selectedSessionId
            : nextSessions[0]?.id || null,
      });
      setBootError('');
    } catch (error) {
      setBootError(error instanceof Error ? error.message : String(error));
    }
  }, [state.selectedSessionId]);

  const updateExecutionState = useCallback((nextApprovals: Approval[], nextJobs: Job[]) => {
    setApprovals(nextApprovals);
    setJobs(nextJobs);
  }, []);

  useEffect(() => {
    void refreshRuntime();
  }, [refreshRuntime]);

  useEffect(() => {
    const id = state.selectedSessionId;
    if (!id || conversation.loaded) return;
    transport.invoke<SessionDetail>('get_session', { id }).then((detail) => {
      conversationStore.load(detail);
    }).catch((error) => setBootError(error instanceof Error ? error.message : String(error)));
  }, [conversation.loaded, state.selectedSessionId]);

  useEffect(() => {
    document.documentElement.dataset.theme = state.theme;
    localStorage.setItem('optimus.react.theme', state.theme);
  }, [state.theme]);

  useEffect(() => {
    if (!draggingLayout.current) saveLayout(state.layout);
  }, [state.layout]);
  useEffect(() => saveProjects(projects), [projects]);
  useEffect(() => saveSessionPins(pins), [pins]);
  useEffect(() => saveAssignments(assignments), [assignments]);
  useEffect(() => saveExpanded(expanded), [expanded]);

  const openSession = (id: string) => {
    dispatch({ type: 'select-session', id });
    dispatch({ type: 'patch-layout', patch: { route: 'work', compactSurface: 'work' } });
  };

  const newSession = async (projectId?: string) => {
    try {
      const created = await transport.invoke<SessionMeta>('new_session');
      setSessions((current) => [created, ...current.filter((session) => session.id !== created.id)]);
      if (projectId) setAssignments((current) => ({ ...current, [created.id]: projectId }));
      openSession(created.id);
    } catch (error) {
      setBootError(error instanceof Error ? error.message : String(error));
    }
  };

  const send = async () => {
    const text = input.trim();
    if (!text || state.activeRunSessionId) return;
    let sessionId = state.selectedSessionId;
    if (!sessionId) {
      const created = await transport.invoke<SessionMeta>('new_session');
      setSessions((current) => [created, ...current]);
      sessionId = created.id;
      dispatch({ type: 'select-session', id: created.id });
    }
    conversationStore.begin(sessionId, text);
    setInput('');
    setAnnotation('');
    dispatch({ type: 'set-active-run', id: sessionId });
    const handle = transport.chat(
      {
        session: sessionId,
        message: text,
        provider: composer.provider,
        model: composer.model,
        thinking_level: composer.thinking,
        fast: composer.fast,
        access: composer.access,
      },
      (event) => conversationStore.apply(sessionId, event)
    );
    activeHandle.current = handle;
    try {
      await handle.done;
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') {
        conversationStore.apply(sessionId, { type: 'cancelled', error: 'cancelled by user' });
      } else {
        conversationStore.markDisconnected(sessionId);
      }
    } finally {
      if (activeHandle.current === handle) {
        activeHandle.current = null;
        dispatch({ type: 'set-active-run', id: null });
        void refreshRuntime();
      }
    }
  };

  const stop = async () => {
    const sessionId = state.activeRunSessionId;
    const handle = activeHandle.current;
    if (!sessionId || !handle) return;
    conversationStore.markCancelling(sessionId);
    try {
      await handle.cancel();
    } catch {
      conversationStore.markDisconnected(sessionId);
    }
  };

  const setRoute = (route: AppRoute) => {
    dispatch({ type: 'patch-layout', patch: { route, compactSurface: route === 'work' ? 'work' : state.layout.compactSurface } });
  };

  const setWorkspaceTab = (tab: WorkspaceTab) => {
    dispatch({ type: 'patch-layout', patch: { workspaceTab: tab, workspaceOpen: true, compactSurface: tab } });
  };

  const beginResize = (
    event: ReactPointerEvent<HTMLDivElement>,
    lane: 'rail' | 'workspace' | 'execution'
  ) => {
    event.preventDefault();
    const startX = event.clientX;
    const startY = event.clientY;
    const original = state.layout;
    draggingLayout.current = true;
    event.currentTarget.setPointerCapture(event.pointerId);
    const move = (nextEvent: PointerEvent) => {
      if (lane === 'rail') {
        dispatch({ type: 'patch-layout', patch: { leftWidth: clamp(original.leftWidth + nextEvent.clientX - startX, 196, 360) } });
      } else if (lane === 'workspace') {
        dispatch({ type: 'patch-layout', patch: { workspaceWidth: clamp(original.workspaceWidth + startX - nextEvent.clientX, 360, 1200) } });
      } else {
        dispatch({ type: 'patch-layout', patch: { executionHeight: clamp(original.executionHeight + startY - nextEvent.clientY, 120, 520) } });
      }
    };
    const up = () => {
      draggingLayout.current = false;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      requestAnimationFrame(() => saveLayout(latestLayout.current));
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up, { once: true });
  };

  const title = selectedSession?.title || 'New session';
  const busyStatus = activeConversation.status;
  const workVisible = state.layout.compactSurface === 'work';
  const workspaceVisible = state.layout.workspaceOpen;
  const style = {
    '--rail-width': `${state.layout.leftCollapsed ? 52 : state.layout.leftWidth}px`,
    '--workspace-width': `${state.layout.workspaceWidth}px`,
    '--execution-height': `${state.layout.executionHeight}px`,
  } as CSSProperties;

  return (
    <ErrorBoundary>
      <div className="optimus-app" style={style} data-compact-surface={state.layout.compactSurface}>
        <TopBar
          title={title}
          route={state.layout.route}
          activeTasks={(state.activeRunSessionId ? 1 : 0) + approvals.length}
          workspaceOpen={workspaceVisible}
          executionOpen={state.layout.executionOpen}
          theme={state.theme}
          onToggleRail={() => dispatch({ type: 'patch-layout', patch: { leftCollapsed: !state.layout.leftCollapsed } })}
          onToggleWorkspace={() => dispatch({ type: 'patch-layout', patch: { workspaceOpen: !workspaceVisible } })}
          onToggleExecution={() => dispatch({ type: 'patch-layout', patch: { executionOpen: !state.layout.executionOpen, compactSurface: 'execution' } })}
          onToggleTasks={() => dispatch({ type: 'tasks', open: !state.taskPanelOpen })}
          onRoute={setRoute}
          onTheme={() => dispatch({ type: 'theme', theme: state.theme === 'dark' ? 'light' : 'dark' })}
          onWindow={(action) => void transport.windowAction(action)}
        />

        <div className="compact-switcher" role="tablist" aria-label="Primary surface">
          <SurfaceButton surface="work" current={state.layout.compactSurface} onSelect={(surface) => dispatch({ type: 'patch-layout', patch: { compactSurface: surface } })} />
          <SurfaceButton surface="browser" current={state.layout.compactSurface} onSelect={(surface) => { dispatch({ type: 'patch-layout', patch: { compactSurface: surface, workspaceTab: 'browser', workspaceOpen: true } }); }} />
          <SurfaceButton surface="files" current={state.layout.compactSurface} onSelect={(surface) => { dispatch({ type: 'patch-layout', patch: { compactSurface: surface, workspaceTab: 'files', workspaceOpen: true } }); }} />
          <SurfaceButton surface="artifacts" current={state.layout.compactSurface} onSelect={(surface) => { dispatch({ type: 'patch-layout', patch: { compactSurface: surface, workspaceTab: 'artifacts', workspaceOpen: true } }); }} />
          <SurfaceButton surface="execution" current={state.layout.compactSurface} onSelect={(surface) => { dispatch({ type: 'patch-layout', patch: { compactSurface: surface, executionOpen: true } }); }} />
        </div>

        <div className="app-body">
          <ProjectsRail
            collapsed={state.layout.leftCollapsed}
            sessions={sessions}
            projects={projects}
            assignments={assignments}
            pins={pins}
            expanded={expanded}
            selectedSessionId={state.selectedSessionId}
            activeRunSessionId={state.activeRunSessionId}
            route={state.layout.route}
            onSelectSession={openSession}
            onNewSession={(projectId) => void newSession(projectId)}
            onRoute={setRoute}
            onAddProject={async () => {
              const result = await transport.pickFolder();
              if (!result.ok || !result.path) return;
              const parts = result.path.split(/[\\/]/).filter(Boolean);
              const project: Project = { id: `project-${Date.now()}`, name: parts.at(-1) || result.path, path: result.path, pinned: true };
              setProjects((current) => [...current, project]);
              setExpanded((current) => ({ ...current, [project.id]: true }));
            }}
            onToggleProject={(id) => setExpanded((current) => ({ ...current, [id]: current[id] === false }))}
            onTogglePin={(id) => setPins((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id])}
            onAssign={(id, projectId) => setAssignments((current) => {
              const next = { ...current };
              if (projectId) next[id] = projectId;
              else delete next[id];
              return next;
            })}
            onRename={async (session) => {
              const titleValue = window.prompt('Rename session', session.title || '');
              if (!titleValue?.trim()) return;
              await transport.invoke('rename_session', { id: session.id, title: titleValue.trim() });
              setSessions((current) => current.map((item) => item.id === session.id ? { ...item, title: titleValue.trim() } : item));
            }}
            onDelete={async (session) => {
              if (!window.confirm(`Delete “${session.title || session.id}”? This cannot be undone.`)) return;
              await transport.invoke('delete_session', { id: session.id });
              const next = sessions.filter((item) => item.id !== session.id);
              setSessions(next);
              if (state.selectedSessionId === session.id) dispatch({ type: 'select-session', id: next[0]?.id || null });
            }}
            onSettings={() => dispatch({ type: 'settings', open: true })}
          />
          <div className="rail-resizer" role="separator" aria-label="Resize project rail" aria-orientation="vertical" onPointerDown={(event) => beginResize(event, 'rail')} />

          <section className="app-stage">
            {bootError ? <div className="boot-error" role="alert"><Icon name="warning" /><span>{bootError}</span><button type="button" onClick={() => void refreshRuntime()}>Retry</button></div> : null}
            <div className="surface-row">
              <section className={`work-surface${workVisible ? ' is-compact-active' : ''}`} aria-label="Agent work surface">
                {state.layout.route === 'work' ? (
                  <>
                    <Transcript
                      messages={conversation.messages}
                      tools={conversation.tools}
                      status={conversation.status}
                      statusText={conversation.statusText}
                      onStarter={(text) => setInput(text)}
                    />
                    <Composer
                      value={annotation ? `${input}${input ? '\n\n' : ''}${annotation}` : input}
                      runStatus={state.activeRunSessionId === state.selectedSessionId ? conversation.status : 'idle'}
                      disabled={Boolean(state.activeRunSessionId && state.activeRunSessionId !== state.selectedSessionId)}
                      isRunOwner={state.activeRunSessionId === state.selectedSessionId}
                      settings={composer}
                      onChange={(value) => { setAnnotation(''); setInput(value); }}
                      onSettings={setComposer}
                      onSend={() => void send()}
                      onStop={() => void stop()}
                    />
                  </>
                ) : state.layout.route === 'capabilities' ? (
                  <CapabilitiesPage doctor={doctor} approvals={approvals} campaigns={campaigns} onOpenExecution={() => dispatch({ type: 'patch-layout', patch: { executionOpen: true } })} />
                ) : state.layout.route === 'artifacts' ? (
                  <ArtifactsSurface transport={transport} active standalone />
                ) : (
                  <UnavailableRoute />
                )}
              </section>

              {workspaceVisible ? (
                <>
                  <div className="workspace-resizer" role="separator" aria-label="Resize evidence workspace" aria-orientation="vertical" onPointerDown={(event) => beginResize(event, 'workspace')} />
                  <div className={`workspace-shell surface-${state.layout.compactSurface}`}>
                    <WorkspacePane
                      tab={state.layout.workspaceTab}
                      transport={transport}
                      onTab={setWorkspaceTab}
                      onClose={() => dispatch({ type: 'patch-layout', patch: { workspaceOpen: false, compactSurface: 'work' } })}
                      onAnnotation={(text) => { setAnnotation(text); dispatch({ type: 'patch-layout', patch: { compactSurface: 'work' } }); }}
                    />
                  </div>
                </>
              ) : null}
            </div>
            {state.layout.executionOpen ? <div className="execution-resizer" role="separator" aria-label="Resize execution dock" onPointerDown={(event) => beginResize(event, 'execution')} /> : null}
            <ExecutionDock
              transport={transport}
              open={state.layout.executionOpen}
              onClose={() => dispatch({ type: 'patch-layout', patch: { executionOpen: false, compactSurface: 'work' } })}
              onState={updateExecutionState}
            />
          </section>
        </div>

        <TruthStrip doctor={doctor} transport={transport.kind} runLabel={state.activeRunSessionId ? statusLabel(activeConversation.status) : 'idle'} />
        <TaskPanel open={state.taskPanelOpen} jobs={jobs} approvals={approvals} runSession={activeSession} runStatus={busyStatus} onClose={() => dispatch({ type: 'tasks', open: false })} onStop={() => void stop()} />
        <SettingsDialog open={state.settingsOpen} transport={transport} theme={state.theme} onTheme={(theme) => dispatch({ type: 'theme', theme })} onClose={() => dispatch({ type: 'settings', open: false })} />
      </div>
    </ErrorBoundary>
  );
}

function SurfaceButton({
  surface,
  current,
  onSelect,
}: {
  surface: CompactSurface;
  current: CompactSurface;
  onSelect: (surface: CompactSurface) => void;
}) {
  return <button type="button" role="tab" aria-selected={current === surface} onClick={() => onSelect(surface)}>{surface}</button>;
}

function UnavailableRoute() {
  return (
    <main className="unavailable-route" aria-label="Messaging unavailable">
      <span className="unavailable-icon"><Icon name="chat" /></span>
      <h1>Messaging is unavailable</h1>
      <p>This build does not implement cross-user messaging. Optimus will not pretend that a configured route is a working capability.</p>
    </main>
  );
}

function statusLabel(status: string) {
  return status.replaceAll('_', ' ');
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.round(Math.max(minimum, Math.min(maximum, value)));
}

class ErrorBoundary extends Component<{ children: ReactNode }, { error: string }> {
  state = { error: '' };

  static getDerivedStateFromError(error: unknown) {
    return { error: error instanceof Error ? error.message : String(error) };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[optimus-ui] render failure', error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="fatal-shell">
          <span className="unavailable-icon"><Icon name="warning" /></span>
          <h1>Workbench rendering failed</h1>
          <p>{this.state.error}</p>
          <button type="button" onClick={() => location.reload()}>Reload workbench</button>
        </div>
      );
    }
    return this.props.children;
  }
}
