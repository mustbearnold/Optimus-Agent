import {
  Component,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type ErrorInfo,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from 'react';
import type {
  Approval,
  Campaign,
  ChatHandle,
  Doctor,
  Job,
  Project,
  ProjectRuntimeScope,
  SessionDetail,
  SessionMeta,
  ToolApprovalBinding,
} from '../ipc/contracts';
import { getTransport } from '../ipc';
import { useAlive } from '../hooks/useAlive';
import { appReducer } from '../state/appReducer';
import {
  conversationStore,
  useConversation,
  useConversationIndicators,
} from '../state/conversationStore';
import {
  codexComposer,
  loadComposer,
  offlineComposer,
  saveComposer,
  shouldPreferCodex,
} from '../state/composerStore';
import {
  defaultLayout,
  loadLayout,
  saveLayout,
  type AppRoute,
  type CompactSurface,
} from '../state/layoutStore';
import {
  createProject,
  loadAssignments,
  loadExpanded,
  loadProjects,
  saveAssignments,
  saveExpanded,
  saveProjects,
} from '../state/projectStore';
import { CapabilitiesPage } from '../components/capabilities/CapabilitiesPage';
import { ConsolesPage, type ConsoleTab } from '../components/consoles/ConsolesPage';
import { CommandPalette } from '../components/chrome/CommandPalette';
import { TopBar } from '../components/chrome/TopBar';
import { TextPromptDialog } from '../components/chrome/TextPromptDialog';
import { Icon } from '../components/chrome/Icon';
import { ExecutionDock } from '../components/execution/ExecutionDock';
import { TaskPanel } from '../components/execution/TaskPanel';
import { MailPage } from '../components/mail/MailPage';
import { ProjectsRail } from '../components/projects/ProjectsRail';
import { ProjectSourcesDialog } from '../components/projects/ProjectSourcesDialog';
import { SettingsDialog } from '../components/settings/SettingsDialog';
import { Composer } from '../components/workbench/Composer';
import { SessionBar } from '../components/workbench/SessionBar';
import { Transcript } from '../components/workbench/Transcript';
import { ArtifactsSurface } from '../components/workspace/ArtifactsSurface';
import { WorkspacePane } from '../components/workspace/WorkspacePane';
import { composeSendMessage } from './composeSendMessage';
import { approvalResolutionParams } from './approvalResolution';

const transport = getTransport();

function projectFromRuntimeScope(scope: ProjectRuntimeScope): Project {
  const normalizedRoot = scope.primary_root.replace(/[\\/]+$/, '');
  const name = normalizedRoot.split(/[\\/]/).pop() || scope.project_id;
  return {
    id: scope.project_id,
    name,
    rootPaths: scope.roots,
    primaryRoot: scope.primary_root,
  };
}

export function OptimusApp() {
  const [state, dispatch] = useReducer(appReducer, undefined, () => ({
    selectedSessionId: null,
    activeRunSessionId: null,
    layout: typeof localStorage === 'undefined' ? defaultLayout : loadLayout(),
    settingsOpen: false,
    taskPanelOpen: false,
    theme: (localStorage.getItem('optimus.react.theme') === 'dark' ? 'dark' : 'light') as 'dark' | 'light',
  }));
  const [doctor, setDoctor] = useState<Doctor | null>(null);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [campaigns, setCampaigns] = useState<Campaign[]>([]);
  const [projects, setProjects] = useState<Project[]>(loadProjects);
  const [authorizedProjects, setAuthorizedProjects] = useState<Set<string>>(new Set());
  const [projectScopes, setProjectScopes] = useState<ProjectRuntimeScope[]>([]);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [consoleTab, setConsoleTab] = useState<ConsoleTab>('skills');
  const [assignments, setAssignments] = useState<Record<string, string>>(loadAssignments);
  const [expanded, setExpanded] = useState<Record<string, boolean>>(loadExpanded);
  const [input, setInput] = useState('');
  const [annotation, setAnnotation] = useState('');
  const [sourceProjectId, setSourceProjectId] = useState<string | null>(null);
  const [renameSession, setRenameSession] = useState<SessionMeta | null>(null);
  const [workspaceMaximized, setWorkspaceMaximized] = useState(false);
  const [storedComposer] = useState(loadComposer);
  const [composer, setComposer] = useState(storedComposer?.settings ?? offlineComposer);
  const [bootError, setBootError] = useState('');
  const providerChosen = useRef(storedComposer?.providerChosen ?? false);
  // Bumped whenever a session is created locally. `refreshRuntime` reads it
  // before its await and again after: if it moved, the list it is holding was
  // taken before that session existed, and applying it would erase one.
  const sessionCreations = useRef(0);
  const activeHandle = useRef<ChatHandle | null>(null);
  const draggingLayout = useRef(false);
  const latestLayout = useRef(state.layout);
  latestLayout.current = state.layout;
  const selectedSession = sessions.find((session) => session.id === state.selectedSessionId) || null;
  const activeSession = sessions.find((session) => session.id === state.activeRunSessionId) || null;
  const selectedProject = projects.find(
    (project) => selectedSession && assignments[selectedSession.id] === project.id
  ) || null;
  const railProjects = useMemo(() => {
    const derived = projectScopes
      .filter((scope) => !projects.some((project) => (
        project.id === scope.project_id ||
        project.rootPaths.some((root) => scope.roots.includes(root))
      )))
      .map(projectFromRuntimeScope);
    return derived.length ? [...projects, ...derived] : projects;
  }, [projectScopes, projects]);
  const sourceProject = projects.find((project) => project.id === sourceProjectId) || null;
  const browserSuspended =
    state.settingsOpen || state.taskPanelOpen || Boolean(sourceProject);
  const conversation = useConversation(state.selectedSessionId);
  const activeConversation = useConversation(state.activeRunSessionId);
  const sessionIndicators = useConversationIndicators(
    sessions.map((session) => session.id)
  );

  const alive = useAlive();

  const refreshRuntime = useCallback(async () => {
    try {
      const creationsAtRequest = sessionCreations.current;
      const [doctorResult, sessionsResult, approvalResult, jobResult, campaignResult, scopeResult] = await Promise.all([
        transport.invoke<Doctor>('doctor'),
        transport.invoke<{ sessions?: SessionMeta[] } | SessionMeta[]>('sessions'),
        transport.invoke<{ pending?: Approval[] }>('approvals_list'),
        transport.invoke<{ jobs?: Job[] }>('jobs_list'),
        transport.invoke<{ campaigns?: Campaign[] }>('campaign_list'),
        transport.invoke<{ projects?: ProjectRuntimeScope[] }>('project_scopes_list'),
      ]);
      if (!alive()) return;
      const nextSessions = Array.isArray(sessionsResult) ? sessionsResult : sessionsResult.sessions || [];
      setDoctor(doctorResult);
      setApprovals(approvalResult.pending || []);
      setJobs(jobResult.jobs || []);
      setCampaigns(campaignResult.campaigns || []);
      setProjectScopes(scopeResult.projects || []);
      setAuthorizedProjects(new Set((scopeResult.projects || []).map((project) => project.project_id)));
      // A thread created while this refresh was in flight is newer than the
      // list it came back with. Overwriting would drop it and then re-select
      // from a list that never contained it, leaving the user staring at the
      // thread they just made having silently vanished. The rest of the
      // refresh is still current, so only the session half is skipped.
      if (sessionCreations.current === creationsAtRequest) {
        setSessions(nextSessions);
        dispatch({
          type: 'select-session',
          id:
            state.selectedSessionId && nextSessions.some((session) => session.id === state.selectedSessionId)
              ? state.selectedSessionId
              : nextSessions[0]?.id || null,
        });
      }
      setBootError('');
    } catch (error) {
      if (!alive()) return;
      setBootError(error instanceof Error ? error.message : String(error));
    }
  }, [alive, state.selectedSessionId]);

  const updateExecutionState = useCallback((nextApprovals: Approval[], nextJobs: Job[]) => {
    setApprovals(nextApprovals);
    setJobs(nextJobs);
  }, []);

  useEffect(() => {
    void refreshRuntime();
  }, [refreshRuntime]);

  useEffect(() => {
    // A stored human provider choice always wins. Absent one, prefer Codex
    // the moment auth is present — pre-sign-in offline residue must not
    // outlive the sign-in (#82). The fixture transport reports mode
    // 'fixture' and keeps its offline default so tests and the browser
    // demo stay deterministic.
    if (!shouldPreferCodex()) return;
    transport
      .invoke<{ present?: boolean; mode?: string }>('auth_status')
      .then((auth) => {
        if (auth.present !== true || auth.mode === 'fixture') return;
        if (shouldPreferCodex()) setComposer(codexComposer);
      })
      .catch(() => undefined);
  }, []);

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

  useEffect(() => saveAssignments(assignments), [assignments]);
  useEffect(() => saveExpanded(expanded), [expanded]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setPaletteOpen(true);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const applyRoute = (route: AppRoute) => {
    dispatch({
      type: 'patch-layout',
      patch: {
        route,
        compactSurface: route === 'work' ? 'work' : state.layout.compactSurface,
      },
    });
  };

  const setRoute = (route: AppRoute) => {
    applyRoute(route);
  };

  const openSession = (id: string) => {
    dispatch({ type: 'select-session', id });
    setRoute('work');
  };

  const newSession = async (projectId?: string) => {
    try {
      const created = await transport.invoke<SessionMeta>('new_session');
      if (!alive()) return;
      sessionCreations.current += 1;
      setSessions((current) => [created, ...current.filter((session) => session.id !== created.id)]);
      // Explicit project (rail "+ in project") always wins. Otherwise keep the
      // current authorized project only — never fall back to projects[0] (#42/#44).
      const targetProjectId =
        projectId ||
        (selectedProject && authorizedProjects.has(selectedProject.id)
          ? selectedProject.id
          : undefined);
      if (targetProjectId) {
        setAssignments((current) => ({ ...current, [created.id]: targetProjectId }));
      }
      openSession(created.id);
    } catch (error) {
      if (!alive()) return;
      setBootError(error instanceof Error ? error.message : String(error));
    }
  };

  const closeProjectSources = (options?: { clearAuthorizeBanner?: boolean }) => {
    const projectId = sourceProjectId;
    setSourceProjectId(null);
    if (options?.clearAuthorizeBanner !== false) {
      setBootError((current) =>
        current === 'Authorize this project folder before running its session.' ? '' : current
      );
    }
    window.setTimeout(() => {
      if (projectId && alive()) document.getElementById(`project-manage-${projectId}`)?.focus();
    }, 0);
  };

  const continueWithoutProject = () => {
    const sessionId = state.selectedSessionId;
    if (sessionId && assignments[sessionId]) {
      setAssignments((current) => {
        const next = { ...current };
        delete next[sessionId];
        return next;
      });
    }
    closeProjectSources();
  };

  const send = async () => {
    // Annotation is gallery-promoted context; must be sent with the user text
    // (display already merges them). Do not drop notes on Send (program P23).
    const text = composeSendMessage(input, annotation);
    if (!text || state.activeRunSessionId) return;
    let sessionId = state.selectedSessionId;
    if (!sessionId) {
      const created = await transport.invoke<SessionMeta>('new_session');
      if (!alive()) return;
      sessionCreations.current += 1;
      setSessions((current) => [created, ...current]);
      sessionId = created.id;
      dispatch({ type: 'select-session', id: created.id });
    }
    const projectId = assignments[sessionId];
    if (projectId && !authorizedProjects.has(projectId)) {
      setBootError('Authorize this project folder before running its session.');
      setSourceProjectId(projectId);
      return;
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
        ...(projectId ? { project_id: projectId } : {}),
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
        if (alive()) {
          dispatch({ type: 'set-active-run', id: null });
          void refreshRuntime();
        }
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

  const resolveTranscriptApproval = async (
    binding: ToolApprovalBinding,
    decision: 'approve' | 'deny'
  ) => {
    const sessionId = state.selectedSessionId;
    if (!sessionId) throw new Error('Select the session that owns this approval.');
    const projectId = assignments[sessionId];
    await transport.invoke(
      'chat_approval_resolve',
      approvalResolutionParams(sessionId, binding, decision, projectId)
    );
    const detail = await transport.invoke<SessionDetail>('get_session', { id: sessionId });
    conversationStore.load(detail);
    await refreshRuntime();
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
        dispatch({ type: 'patch-layout', patch: { leftWidth: clamp(original.leftWidth + nextEvent.clientX - startX, 200, 400) } });
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

  const resizeWithKeyboard = (
    event: ReactKeyboardEvent<HTMLDivElement>,
    lane: 'rail' | 'workspace' | 'execution'
  ) => {
    const step = event.shiftKey ? 40 : 10;
    const current = latestLayout.current;
    let value: number | null = null;
    if (lane === 'rail') {
      if (event.key === 'ArrowLeft') value = current.leftWidth - step;
      if (event.key === 'ArrowRight') value = current.leftWidth + step;
      if (event.key === 'Home') value = 200;
      if (event.key === 'End') value = 400;
      if (value !== null) dispatch({ type: 'patch-layout', patch: { leftWidth: clamp(value, 200, 400) } });
    } else if (lane === 'workspace') {
      if (event.key === 'ArrowLeft') value = current.workspaceWidth + step;
      if (event.key === 'ArrowRight') value = current.workspaceWidth - step;
      if (event.key === 'Home') value = 360;
      if (event.key === 'End') value = 1200;
      if (value !== null) dispatch({ type: 'patch-layout', patch: { workspaceWidth: clamp(value, 360, 1200) } });
    } else {
      if (event.key === 'ArrowUp') value = current.executionHeight + step;
      if (event.key === 'ArrowDown') value = current.executionHeight - step;
      if (event.key === 'Home') value = 120;
      if (event.key === 'End') value = 520;
      if (value !== null) dispatch({ type: 'patch-layout', patch: { executionHeight: clamp(value, 120, 520) } });
    }
    if (value !== null) event.preventDefault();
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
          workspaceOpen={workspaceVisible}
          workspaceMaximized={workspaceMaximized}
          executionOpen={state.layout.executionOpen}
          onHome={() => setRoute('work')}
          onToggleRail={() => dispatch({ type: 'patch-layout', patch: { leftCollapsed: !state.layout.leftCollapsed } })}
          onToggleWorkspace={() => {
            if (workspaceVisible) setWorkspaceMaximized(false);
            dispatch({ type: 'patch-layout', patch: { workspaceOpen: !workspaceVisible } });
          }}
          onToggleWorkspaceMaximized={() => {
            setWorkspaceMaximized((current) => !current);
            if (!workspaceVisible) dispatch({ type: 'patch-layout', patch: { workspaceOpen: true } });
          }}
          onToggleExecution={() => dispatch({ type: 'patch-layout', patch: { executionOpen: !state.layout.executionOpen, compactSurface: 'execution' } })}
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
            projects={railProjects}
            assignments={assignments}
            expanded={expanded}
            selectedSessionId={state.selectedSessionId}
            sessionIndicators={sessionIndicators}
            onSearch={(q) => {
              void (async () => {
                if (!q.trim()) {
                  await refreshRuntime();
                  return;
                }
                try {
                  const result = await transport.invoke<{ sessions?: SessionMeta[] }>(
                    'session_search',
                    { q, include_archived: true }
                  );
                  if (!alive()) return;
                  const list = Array.isArray(result.sessions) ? result.sessions : [];
                  setSessions(list);
                } catch {
                  // keep current list on search failure
                }
              })();
            }}
            onSelectSession={openSession}
            onNewSession={(projectId) => void newSession(projectId)}
            onAddProject={async () => {
              const result = await transport.pickFolder();
              if (!result.ok || !result.path || !result.grantToken) return;
              const parts = result.path.split(/[\\/]/).filter(Boolean);
              const project = createProject(parts.at(-1) || result.path, result.path);
              await transport.invoke('project_scopes_authorize', {
                project_id: project.id,
                root_paths: project.rootPaths,
                primary_root: project.primaryRoot,
                grant_tokens: [result.grantToken],
              });
              if (!alive()) return;
              setAuthorizedProjects((current) => new Set(current).add(project.id));
              setProjects((current) => [...current, project]);
              setExpanded((current) => ({ ...current, [project.id]: true }));
              setSourceProjectId(project.id);
            }}
            onManageProject={(project) => setSourceProjectId(project.id)}
            onToggleProject={(id) => setExpanded((current) => ({ ...current, [id]: current[id] === false }))}
            onTogglePin={async (session) => {
              const pinned = !session.pinned;
              await transport.invoke('pin_session', { id: session.id, pinned });
              if (!alive()) return;
              setSessions((current) =>
                sortSessions(
                  current.map((item) => (item.id === session.id ? { ...item, pinned } : item))
                )
              );
            }}
            onToggleArchive={async (session) => {
              const archived = !session.archived;
              await transport.invoke('archive_session', { id: session.id, archived });
              if (!alive()) return;
              setSessions((current) =>
                sortSessions(
                  current.map((item) => (item.id === session.id ? { ...item, archived } : item))
                )
              );
            }}
            onAssign={(id, projectId) => setAssignments((current) => {
              const next = { ...current };
              if (projectId) next[id] = projectId;
              else delete next[id];
              return next;
            })}
            onRename={(session) => setRenameSession(session)}
            onDelete={async (session) => {
              if (!window.confirm(`Delete “${session.title || session.id}”? This cannot be undone.`)) return;
              await transport.invoke('delete_session', { id: session.id });
              if (!alive()) return;
              const next = sessions.filter((item) => item.id !== session.id);
              setSessions(next);
              if (state.selectedSessionId === session.id) dispatch({ type: 'select-session', id: next[0]?.id || null });
            }}
            onSettings={() => dispatch({ type: 'settings', open: true })}
          />
          <div className="rail-resizer" role="separator" tabIndex={0} aria-label="Resize project rail" aria-orientation="vertical" aria-valuemin={200} aria-valuemax={400} aria-valuenow={state.layout.leftWidth} aria-valuetext={`${state.layout.leftWidth} pixels`} onKeyDown={(event) => resizeWithKeyboard(event, 'rail')} onPointerDown={(event) => beginResize(event, 'rail')} />

          <section className="app-stage">
            {bootError ? <div className="boot-error" role="alert"><Icon name="warning" /><span>{bootError}</span><button type="button" onClick={() => void refreshRuntime()}>Retry</button></div> : null}
            <div className={`surface-row${workspaceMaximized ? ' is-workspace-maximized' : ''}`}>
              <section className={`work-surface${workVisible ? ' is-compact-active' : ''}`} aria-label="Agent work surface">
                {state.layout.route === 'work' ? (
                  <>
                    <SessionBar title={title} project={selectedProject} showSeparator={workspaceVisible} />
                    <Transcript
                      messages={conversation.messages}
                      status={conversation.status}
                      statusText={conversation.statusText}
                      onStarter={(text) => setInput(text)}
                      onApprovalDecision={resolveTranscriptApproval}
                    />
                    <Composer
                      value={annotation ? `${input}${input ? '\n\n' : ''}${annotation}` : input}
                      runStatus={state.activeRunSessionId === state.selectedSessionId ? conversation.status : 'idle'}
                      disabled={Boolean(state.activeRunSessionId && state.activeRunSessionId !== state.selectedSessionId)}
                      isRunOwner={state.activeRunSessionId === state.selectedSessionId}
                      settings={composer}
                      onChange={(value) => { setAnnotation(''); setInput(value); }}
                      onSettings={(next) => {
                        if (next.provider !== composer.provider) providerChosen.current = true;
                        saveComposer(next, providerChosen.current);
                        setComposer(next);
                      }}
                      onSend={() => void send()}
                      onStop={() => void stop()}
                    />
                  </>
                ) : state.layout.route === 'capabilities' ? (
                  <CapabilitiesPage
                    doctor={doctor}
                    approvals={approvals}
                    campaigns={campaigns}
                    transport={transport}
                    onOpenExecution={() => dispatch({ type: 'patch-layout', patch: { executionOpen: true } })}
                  />
                ) : state.layout.route === 'consoles' ? (
                  <ConsolesPage
                    key={consoleTab}
                    transport={transport}
                    initialTab={consoleTab}
                  />
                ) : state.layout.route === 'mail' ? (
                  <MailPage transport={transport} />
                ) : state.layout.route === 'artifacts' ? (
                  <ArtifactsSurface transport={transport} active standalone />
                ) : null}
              </section>

              {workspaceVisible ? (
                <>
                  <div className="workspace-resizer" role="separator" tabIndex={0} aria-label="Resize evidence workspace" aria-orientation="vertical" aria-valuemin={360} aria-valuemax={1200} aria-valuenow={state.layout.workspaceWidth} aria-valuetext={`${state.layout.workspaceWidth} pixels`} onKeyDown={(event) => resizeWithKeyboard(event, 'workspace')} onPointerDown={(event) => beginResize(event, 'workspace')} />
                  <div className={`workspace-shell surface-${state.layout.compactSurface}`}>
                    <WorkspacePane
                      tab={state.layout.workspaceTab}
                      transport={transport}
                      suspended={browserSuspended}
                      onAddToPrompt={(text) => {
                        setAnnotation(text);
                        dispatch({ type: 'patch-layout', patch: { compactSurface: 'work' } });
                      }}
                      onSelectTab={(workspaceTab) =>
                        dispatch({ type: 'patch-layout', patch: { workspaceTab } })
                      }
                    />
                  </div>
                </>
              ) : null}
            </div>
            {state.layout.executionOpen ? <div className="execution-resizer" role="separator" tabIndex={0} aria-label="Resize execution dock" aria-orientation="horizontal" aria-valuemin={120} aria-valuemax={520} aria-valuenow={state.layout.executionHeight} aria-valuetext={`${state.layout.executionHeight} pixels`} onKeyDown={(event) => resizeWithKeyboard(event, 'execution')} onPointerDown={(event) => beginResize(event, 'execution')} /> : null}
            <ExecutionDock
              transport={transport}
              open={state.layout.executionOpen}
              onClose={() => dispatch({ type: 'patch-layout', patch: { executionOpen: false, compactSurface: 'work' } })}
              onState={updateExecutionState}
            />
          </section>
        </div>

        <TaskPanel open={state.taskPanelOpen} jobs={jobs} approvals={approvals} runSession={activeSession} runStatus={busyStatus} onClose={() => dispatch({ type: 'tasks', open: false })} onStop={() => void stop()} />
        <SettingsDialog
          open={state.settingsOpen}
          transport={transport}
          theme={state.theme}
          projects={projects}
          onTheme={(theme) => dispatch({ type: 'theme', theme })}
          onManageProject={(project) => setSourceProjectId(project.id)}
          onClose={() => dispatch({ type: 'settings', open: false })}
        />
        <ProjectSourcesDialog
          project={sourceProject}
          authorizedRootPaths={
            projectScopes.find((scope) => scope.project_id === sourceProject?.id)?.roots || []
          }
          allowContinueWithoutProject={Boolean(
            sourceProject &&
              state.selectedSessionId &&
              assignments[state.selectedSessionId] === sourceProject.id &&
              !authorizedProjects.has(sourceProject.id)
          )}
          onPickSource={async () => {
            const result = await transport.pickFolder();
            return result;
          }}
          onSave={async (project, grantTokens) => {
            const result = await transport.invoke<{ project?: ProjectRuntimeScope | null }>(
              'project_scopes_authorize',
              {
                project_id: project.id,
                root_paths: project.rootPaths,
                primary_root: project.primaryRoot,
                grant_tokens: grantTokens,
              }
            );
            if (!alive()) return;
            if (result.project) {
              setProjectScopes((current) => {
                const without = current.filter((scope) => scope.project_id !== project.id);
                return [...without, result.project as ProjectRuntimeScope];
              });
              setAuthorizedProjects((current) => new Set(current).add(project.id));
              setBootError((current) =>
                current === 'Authorize this project folder before running its session.'
                  ? ''
                  : current
              );
            } else {
              setProjectScopes((current) => current.filter((scope) => scope.project_id !== project.id));
              setAuthorizedProjects((current) => {
                const next = new Set(current);
                next.delete(project.id);
                return next;
              });
            }
            setProjects((current) => {
              if (current.some((item) => item.id === project.id)) {
                return current.map((item) => (item.id === project.id ? project : item));
              }
              return [...current, project];
            });
          }}
          onContinueWithoutProject={continueWithoutProject}
          onClose={() => closeProjectSources()}
        />
        <CommandPalette
          open={paletteOpen}
          transport={transport}
          onClose={() => setPaletteOpen(false)}
          onRun={(commandId) => {
            if (
              commandId === 'skills' ||
              commandId === 'memory' ||
              commandId === 'packs' ||
              commandId === 'logs'
            ) {
              setConsoleTab(commandId);
              setRoute('consoles');
            } else if (commandId === 'artifacts') {
              setRoute('artifacts');
            } else if (commandId === 'capabilities') {
              setRoute('capabilities');
            } else if (commandId === 'mail') {
              setRoute('mail');
            } else if (commandId === 'cron') {
              dispatch({ type: 'settings', open: true });
            } else if (commandId === 'new') {
              void newSession();
            } else if (commandId === 'doctor') {
              void refreshRuntime();
            }
          }}
        />
        <TextPromptDialog
          open={Boolean(renameSession)}
          title="Rename session"
          label="Session title"
          initialValue={renameSession?.title || ''}
          confirmLabel="Rename"
          onCancel={() => setRenameSession(null)}
          onConfirm={async (title) => {
            if (!renameSession) return;
            await transport.invoke('rename_session', { id: renameSession.id, title });
            if (!alive()) return;
            setSessions((current) =>
              current.map((item) =>
                item.id === renameSession.id ? { ...item, title } : item
              )
            );
            setRenameSession(null);
          }}
        />
      </div>
    </ErrorBoundary>
  );
}

function sortSessions(list: SessionMeta[]): SessionMeta[] {
  return [...list].sort((a, b) => {
    const pin = Number(Boolean(b.pinned)) - Number(Boolean(a.pinned));
    if (pin !== 0) return pin;
    const arch = Number(Boolean(a.archived)) - Number(Boolean(b.archived));
    if (arch !== 0) return arch;
    return String(b.updated_at || '').localeCompare(String(a.updated_at || ''));
  });
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
