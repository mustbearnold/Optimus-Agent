import {
  Component,
  memo,
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
  OptimusTransport,
  DeveloperAccess,
  Doctor,
  Project,
  ProjectRuntimeScope,
  SessionMeta,
  ToolApprovalBinding,
} from '../ipc/contracts';
import { getTransport, initTransport, resetTransport } from '../ipc';
import { createOptimusClient, type ChatSession } from '../ipc/client';
import { useAlive } from '../hooks/useAlive';
import { appReducer } from '../state/appReducer';
import {
  conversationStore,
  useConversation,
  useConversationIndicators,
} from '../state/conversationStore';
import {
  autoComposer,
  loadComposer,
  modelOverride,
  saveComposer,
  type ComposerSettings,
} from '../state/composerStore';
import {
  defaultLayout,
  loadLayout,
  railResizePatch,
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
import { WindowResizeHandles } from '../components/chrome/WindowResizeHandles';
import { TextPromptDialog } from '../components/chrome/TextPromptDialog';
import { Icon } from '../components/chrome/Icon';
import { ExecutionDock } from '../components/execution/ExecutionDock';
import { MailPage } from '../components/mail/MailPage';
import { ProjectsRail } from '../components/projects/ProjectsRail';
import { ProjectSourcesDialog } from '../components/projects/ProjectSourcesDialog';
import { SettingsDialog } from '../components/settings/SettingsDialog';
import { Composer } from '../components/workbench/Composer';
import { SessionBar } from '../components/workbench/SessionBar';
import { Transcript } from '../components/workbench/Transcript';
import { WorkbenchStatusBar } from '../components/workbench/WorkbenchStatusBar';
import { ArtifactsSurface } from '../components/workspace/ArtifactsSurface';
import { WorkspacePane } from '../components/workspace/WorkspacePane';
import { composeSendMessage } from './composeSendMessage';

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
  // The broker ticket is awaited BEFORE the first transport construction
  // (spec-015 A3): the transport is created once and cached, so the
  // bootstrap must not race it. `null` = confirmed broker absence in the
  // packaged renderer — the terminal affordance.
  const [transport, setTransport] = useState<OptimusTransport | null>(getTransport());
  useEffect(() => {
    let live = true;
    void initTransport().then((chosen) => {
      if (live) setTransport(chosen);
    });
    return () => { live = false; };
  }, []);
  const [state, dispatch] = useReducer(appReducer, undefined, () => ({
    selectedSessionId: null,
    activeRunSessionId: null,
    layout: typeof localStorage === 'undefined' ? defaultLayout : loadLayout(),
    settingsOpen: false,
    theme: (localStorage.getItem('optimus.react.theme') === 'dark' ? 'dark' : 'light') as 'dark' | 'light',
  }));
  const [doctor, setDoctor] = useState<Doctor | null>(null);
  const [developerAccess, setDeveloperAccess] = useState<DeveloperAccess | null>(null);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [approvals, setApprovals] = useState<Approval[]>([]);
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
  const [composer, setComposer] = useState(storedComposer?.settings ?? autoComposer);
  const [bootError, setBootError] = useState('');
  const providerChosen = useRef(storedComposer?.providerChosen ?? false);
  // Bumped whenever a session is created locally. `refreshRuntime` reads it
  // before its await and again after: if it moved, the list it is holding was
  // taken before that session existed, and applying it would erase one.
  const sessionCreations = useRef(0);
  const client = useMemo(() => createOptimusClient(transport), [transport]);

  const activeChat = useRef<ChatSession | null>(null);
  const draggingLayout = useRef(false);
  const latestLayout = useRef(state.layout);
  latestLayout.current = state.layout;
  const selectedSession = sessions.find((session) => session.id === state.selectedSessionId) || null;
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
  const browserSuspended = state.settingsOpen || Boolean(sourceProject);
  const sessionIndicators = useConversationIndicators(
    sessions.map((session) => session.id)
  );
  // The app-wide footer reads the selected session's run status regardless of
  // which route is active (Hermes-style status bar sibling of the layout).
  const statusConversation = useConversation(state.selectedSessionId);

  const alive = useAlive();

  useEffect(() => {
    if (!transport) return;
    let live = true;
    void client.system.developerAccess()
      .then((result) => {
        if (live && result.developer_access) setDeveloperAccess(result.developer_access);
      })
      .catch(() => undefined);
    return () => { live = false; };
  }, [transport]);

  const refreshCapabilityState = useCallback(async () => {
    if (!transport) return;
    try {
      const [doctorResult, campaignResult] = await Promise.all([
        client.system.doctor(),
        client.campaigns.list(),
      ]);
      if (!alive()) return;
      setDoctor(doctorResult);
      setDeveloperAccess(doctorResult.settings?.developer_access || doctorResult.developer_access || null);
      setCampaigns(campaignResult);
    } catch (error) {
      if (!alive()) return;
      setBootError(error instanceof Error ? error.message : String(error));
    }
  }, [alive, transport]);

  const refreshRuntime = useCallback(async () => {
    if (!transport) return;
    try {
      const creationsAtRequest = sessionCreations.current;
      // The host serves IPC requests in order. Keep the initial workbench
      // refresh to the small, interactive set so New thread is not queued
      // behind diagnostics and campaign inventory on a busy machine.
      const startupContext = transport.kind === 'tauri'
        ? client.sessions.startupContext()
        : Promise.resolve({ session_id: null });
      const [nextSessions, approvalResult, scopeResult, startupResult] = await Promise.all([
        client.sessions.list(),
        client.approvals.list(),
        client.projects.scopesList(),
        startupContext,
      ]);
      if (!alive()) return;
      setApprovals(approvalResult);
      setProjectScopes(scopeResult);
      setAuthorizedProjects(new Set(scopeResult.map((project) => project.project_id)));
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
              : startupResult.session_id && nextSessions.some((session) => session.id === startupResult.session_id)
                ? startupResult.session_id
              : nextSessions[0]?.id || null,
        });
      }
      setBootError('');
    } catch (error) {
      if (!alive()) return;
      setBootError(error instanceof Error ? error.message : String(error));
    }
  }, [alive, state.selectedSessionId, transport]);

  const updateExecutionState = useCallback((nextApprovals: Approval[]) => {
    setApprovals(nextApprovals);
  }, []);

  useEffect(() => {
    void refreshRuntime();
  }, [refreshRuntime]);

  useEffect(() => {
    if (state.layout.route === 'capabilities') void refreshCapabilityState();
  }, [refreshCapabilityState, state.layout.route]);

  useEffect(() => {
    const id = state.selectedSessionId;
    if (!id || conversationStore.get(id).loaded) return;
    if (!transport) return;
    client.sessions.get(id).then((detail) => {
      conversationStore.load(detail);
    }).catch((error) => setBootError(error instanceof Error ? error.message : String(error)));
  }, [state.selectedSessionId, transport]);

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
    if (!transport) return;
    try {
      const created = await client.sessions.newSession();
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
    if (!transport) return;
    let sessionId = state.selectedSessionId;
    if (!sessionId) {
      const created = await client.sessions.newSession();
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
    const model = modelOverride(composer.model);
    if (!transport) return;
    const chat = client.chat(sessionId);
    activeChat.current = chat;
    try {
      // The client classifies the terminal (R4/R9, ADR-0090) and mirrors a
      // refused start into the transcript as an error event — a rejected chat
      // start is a configuration error, not a transport loss.
      await chat.send(
        {
          message: text,
          provider: composer.provider,
          ...(model ? { model } : {}),
          thinkingLevel: composer.thinking,
          fast: composer.fast,
          access: composer.access,
          ...(projectId ? { projectId } : {}),
        },
        (event) => conversationStore.apply(sessionId, event)
      ).outcome;
    } finally {
      if (activeChat.current === chat) {
        activeChat.current = null;
        if (alive()) {
          dispatch({ type: 'set-active-run', id: null });
          void refreshRuntime();
        }
      }
    }
  };

  const stop = async () => {
    const sessionId = state.activeRunSessionId;
    const chat = activeChat.current;
    if (!sessionId || !chat) return;
    conversationStore.markCancelling(sessionId);
    try {
      await chat.cancel();
    } catch {
      conversationStore.markDisconnected(sessionId);
    }
  };

  // Stable identity, because this reaches every message in the transcript.
  // `MessageRow` is memoised, so a new function here changed a prop on all of
  // them and re-rendered the whole conversation — re-parsing every markdown
  // body — on each keystroke in the composer, which shares this component's
  // state. Typing into a long chat was doing the work of rendering that chat
  // from scratch, per character.
  const resolveTranscriptApproval = useCallback(
    async (
      binding: ToolApprovalBinding,
      decision: 'approve' | 'deny',
      grantClass?: string
    ) => {
      const sessionId = state.selectedSessionId;
      if (!sessionId) throw new Error('Select the session that owns this approval.');
      const projectId = assignments[sessionId];
      // "Always allow <class> in this project (this session)" (spec-014 R7):
      // the consent must exist BEFORE the resolve settles, so the resumed
      // turn's next same-class effect auto-grants instead of re-parking.
      if (decision === 'approve' && grantClass && transport) {
        await client.consents.grant(sessionId, grantClass, projectId);
      }
      // Settling resumes the paused turn (ADR-0046), so this is a streaming
      // turn, not a request/response: the continuation's events must reach the
      // transcript as they happen, and the turn must be cancellable. A
      // blocking resolve left the button on "Approving…" with no feedback for
      // the whole continuation and no way to stop it.
      if (!transport) return;
      const chat = client.chat(sessionId);
      activeChat.current = chat;
      dispatch({ type: 'set-active-run', id: sessionId });
      try {
        await chat.approve(binding, decision, projectId, (event) =>
          conversationStore.apply(sessionId, event)
        ).outcome;
      } finally {
        if (activeChat.current === chat) activeChat.current = null;
        if (alive()) dispatch({ type: 'set-active-run', id: null });
      }
      const detail = await client.sessions.get(sessionId);
      conversationStore.load(detail);
      await refreshRuntime();
    },
    [state.selectedSessionId, assignments, refreshRuntime]
  );

  const beginResize = (
    event: ReactPointerEvent<HTMLDivElement>,
    lane: 'rail' | 'workspace' | 'execution'
  ) => {
    event.preventDefault();
    const startX = event.clientX;
    const startY = event.clientY;
    const original = state.layout;
    const resizeTarget = event.currentTarget;
    draggingLayout.current = true;
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Pointer capture is unavailable in a few embedded WebView test hosts;
      // the window-level listeners below still provide the drag path.
    }
    const move = (nextEvent: PointerEvent) => {
      if (lane === 'rail') {
        dispatch({
          type: 'patch-layout',
          patch: railResizePatch(original.leftWidth, original.leftCollapsed, startX, nextEvent.clientX),
        });
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
      window.removeEventListener('pointercancel', up);
      resizeTarget.removeEventListener('pointermove', move as EventListener);
      resizeTarget.removeEventListener('pointerup', up as EventListener);
      resizeTarget.removeEventListener('pointercancel', up as EventListener);
      requestAnimationFrame(() => saveLayout(latestLayout.current));
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up, { once: true });
    window.addEventListener('pointercancel', up, { once: true });
    resizeTarget.addEventListener('pointermove', move as EventListener);
    resizeTarget.addEventListener('pointerup', up as EventListener, { once: true });
    resizeTarget.addEventListener('pointercancel', up as EventListener, { once: true });
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
      if (value !== null) {
        dispatch({
          type: 'patch-layout',
          patch: {
            leftWidth: clamp(value, 200, 400),
            leftCollapsed: current.leftCollapsed ? event.key !== 'ArrowLeft' : false,
          },
        });
      }
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
          railCollapsed={state.layout.leftCollapsed}
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
          onToggleExecution={() => dispatch({
            type: 'patch-layout',
            patch: {
              executionOpen: !state.layout.executionOpen,
              // On compact widths the execution dock owns the only visible
              // surface. Closing it must hand that surface back to chat;
              // otherwise the dock disappears while the work surface stays
              // hidden behind `data-compact-surface="execution"`.
              compactSurface: state.layout.executionOpen ? 'work' : 'execution',
            },
          })}
          onWindow={(action) => void transport?.windowAction(action)}
        />

        {/* Borderless-window resize hotspots (spec-001 R5 chrome): the
            shell owns window geometry, not the surface protocol. */}
        <WindowResizeHandles />

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
                  if (!transport) return;
                  const result = await client.sessions.search({ q, include_archived: true });
                  if (!alive()) return;
                  setSessions(result);
                } catch {
                  // keep current list on search failure
                }
              })();
            }}
            onSelectSession={openSession}
            onNewSession={(projectId) => void newSession(projectId)}
            onAddProject={async () => {
              if (!transport) return;
              const result = await client.shell.pickFolder();
              if (!result.ok || !result.path || !result.grantToken) return;
              const parts = result.path.split(/[\\/]/).filter(Boolean);
              const project = createProject(parts.at(-1) || result.path, result.path);
              await client.projects.authorize({
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
              if (!transport) return;
              const pinned = !session.pinned;
              await client.sessions.pin(session.id, pinned);
              if (!alive()) return;
              setSessions((current) =>
                sortSessions(
                  current.map((item) => (item.id === session.id ? { ...item, pinned } : item))
                )
              );
            }}
            onToggleArchive={async (session) => {
              if (!transport) return;
              const archived = !session.archived;
              await client.sessions.archive(session.id, archived);
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
              if (!transport) return;
              if (!window.confirm(`Delete “${session.title || session.id}”? This cannot be undone.`)) return;
              await client.sessions.delete(session.id);
              if (!alive()) return;
              const next = sessions.filter((item) => item.id !== session.id);
              setSessions(next);
              if (state.selectedSessionId === session.id) dispatch({ type: 'select-session', id: next[0]?.id || null });
            }}
            onSettings={() => dispatch({ type: 'settings', open: true })}
          />
          <div className="rail-resizer" role="separator" tabIndex={0} aria-label="Resize project rail" aria-orientation="vertical" aria-valuemin={200} aria-valuemax={400} aria-valuenow={state.layout.leftWidth} aria-valuetext={`${state.layout.leftWidth} pixels`} onKeyDown={(event) => resizeWithKeyboard(event, 'rail')} onPointerDown={(event) => beginResize(event, 'rail')} />

          <section className="app-stage">
            {bootError ? <div className="boot-error" role="alert"><Icon name="warning" /><span>{bootError}</span><button type="button" onClick={() => { resetTransport(); void initTransport().then(setTransport); }}>Retry</button></div> : null}
            {statusConversation.suggestProfileBanner && state.selectedSessionId ? (
              <div className="profile-suggestion-banner" role="status">
                <Icon name="info" />
                <span>
                  <strong>Many approvals this session</strong> — Developer Full
                  Access can auto-grant these command classes. Consider enabling
                  it in Settings, or use “Always allow” on the approval card.
                </span>
                <button
                  type="button"
                  className="banner-dismiss"
                  aria-label="Dismiss suggestion"
                  onClick={() => {
                    if (state.selectedSessionId) {
                      conversationStore.dismissProfileBanner(state.selectedSessionId);
                    }
                  }}
                >
                  Dismiss
                </button>
              </div>
            ) : null}
            <div className={`surface-row${workspaceMaximized ? ' is-workspace-maximized' : ''}`}>
              <div className="work-column">
                <section className={`work-surface${workVisible ? ' is-compact-active' : ''}`} aria-label="Agent work surface">
                  {transport ? state.layout.route === 'work' ? (
                    <WorkbenchChat
                      title={title}
                      project={selectedProject}
                      showSeparator={workspaceVisible}
                      sessionId={state.selectedSessionId}
                      activeRunSessionId={state.activeRunSessionId}
                      input={input}
                      annotation={annotation}
                      settings={composer}
                      onStarter={setInput}
                      onApprovalDecision={resolveTranscriptApproval}
                      onChange={(value) => { setAnnotation(''); setInput(value); }}
                      onSettings={(next) => {
                        if (next.provider !== composer.provider) providerChosen.current = true;
                        saveComposer(next, providerChosen.current);
                        setComposer(next);
                      }}
                      onSend={() => void send()}
                      onStop={() => void stop()}
                    />
                  ) : state.layout.route === 'capabilities' ? (
                    <CapabilitiesPage
                      doctor={doctor}
                      approvals={approvals}
                      campaigns={campaigns}
                      client={client}
                      onOpenExecution={() => dispatch({ type: 'patch-layout', patch: { executionOpen: true } })}
                    />
                  ) : state.layout.route === 'consoles' ? (
                    <ConsolesPage
                      key={consoleTab}
                      client={client}
                      initialTab={consoleTab}
                    />
                  ) : state.layout.route === 'mail' ? (
                    <MailPage transport={transport} />
                  ) : state.layout.route === 'artifacts' ? (
                    <ArtifactsSurface transport={transport} active standalone />
                  ) : null : null}
                </section>
                {state.layout.executionOpen ? <div className="execution-resizer" role="separator" tabIndex={0} aria-label="Resize execution dock" aria-orientation="horizontal" aria-valuemin={120} aria-valuemax={520} aria-valuenow={state.layout.executionHeight} aria-valuetext={`${state.layout.executionHeight} pixels`} onKeyDown={(event) => resizeWithKeyboard(event, 'execution')} onPointerDown={(event) => beginResize(event, 'execution')} /> : null}
                <ExecutionDock
                  // Dialogs and the dock render only via post-boot user
                  // interaction; a null transport never reaches them (the
                  // boot-error banner is the terminal affordance instead).
                  client={client}
                  open={state.layout.executionOpen}
                  onClose={() => dispatch({ type: 'patch-layout', patch: { executionOpen: false, compactSurface: 'work' } })}
                  onState={updateExecutionState}
                />
              </div>

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
          </section>
        </div>

        <WorkbenchStatusBar
          status={statusConversation.status}
          statusText={statusConversation.statusText}
          settings={composer}
          developerAccess={developerAccess || undefined}
          project={selectedProject}
        />

        <SettingsDialog
          open={state.settingsOpen}
          transport={transport!}
          client={client}
          theme={state.theme}
          projects={projects}
          sessionId={state.selectedSessionId}
          projectId={state.selectedSessionId ? (assignments[state.selectedSessionId] || undefined) : undefined}
          onTheme={(theme) => dispatch({ type: 'theme', theme })}
          onManageProject={(project) => setSourceProjectId(project.id)}
          onDeveloperAccess={setDeveloperAccess}
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
            if (!transport) return { ok: false, cancelled: true };
            const result = await client.shell.pickFolder();
            return result;
          }}
          onSave={async (project, grantTokens) => {
            if (!transport) return;
            const result = await client.projects.authorize({
              project_id: project.id,
              root_paths: project.rootPaths,
              primary_root: project.primaryRoot,
              grant_tokens: grantTokens,
            });
            if (!alive()) return;
            if (result.project) {
              const scope = result.project;
              setProjectScopes((current) => {
                const without = current.filter((item) => item.project_id !== project.id);
                return [...without, scope];
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
          transport={transport!}
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
              void refreshCapabilityState();
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
            if (!transport) return;
            if (!renameSession) return;
            await transport?.invoke('rename_session', { id: renameSession.id, title });
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

const WorkbenchChat = memo(function WorkbenchChat({
  title,
  project,
  showSeparator,
  sessionId,
  activeRunSessionId,
  input,
  annotation,
  settings,
  onStarter,
  onApprovalDecision,
  onChange,
  onSettings,
  onSend,
  onStop,
}: {
  title: string;
  project: Project | null;
  showSeparator: boolean;
  sessionId: string | null;
  activeRunSessionId: string | null;
  input: string;
  annotation: string;
  settings: ComposerSettings;
  onStarter: (text: string) => void;
  onApprovalDecision: (
    binding: ToolApprovalBinding,
    decision: 'approve' | 'deny'
  ) => void | Promise<void>;
  onChange: (value: string) => void;
  onSettings: (settings: ComposerSettings) => void;
  onSend: () => void;
  onStop: () => void;
}) {
  // Keep the high-frequency stream subscription below the shell. Tool deltas
  // should repaint the conversation, not the rail, browser, and window chrome.
  const conversation = useConversation(sessionId);
  const isRunOwner = activeRunSessionId === sessionId;

  return (
    <>
      <SessionBar title={title} project={project} showSeparator={showSeparator} />
      <div className="workbench-conversation-row">
        <Transcript
          messages={conversation.messages}
          status={conversation.status}
          statusText={conversation.statusText}
          onStarter={onStarter}
          onApprovalDecision={onApprovalDecision}
        />
      </div>
      <Composer
        value={annotation ? `${input}${input ? '\n\n' : ''}${annotation}` : input}
        runStatus={isRunOwner ? conversation.status : 'idle'}
        disabled={Boolean(activeRunSessionId && !isRunOwner)}
        isRunOwner={isRunOwner}
        settings={settings}
        onChange={onChange}
        onSettings={onSettings}
        onSend={onSend}
        onStop={onStop}
      />
    </>
  );
});

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
