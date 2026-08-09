import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from 'react';
import type { Project, SessionMeta } from '../../ipc/contracts';
import type { SessionIndicatorState } from '../../state/conversationStore';
import { Icon } from '../chrome/Icon';

type Props = {
  collapsed: boolean;
  sessions: SessionMeta[];
  projects: Project[];
  assignments: Record<string, string>;
  expanded: Record<string, boolean>;
  selectedSessionId: string | null;
  sessionIndicators: Record<string, SessionIndicatorState>;
  onSearch: (query: string) => void;
  onSelectSession: (id: string) => void;
  onNewSession: (projectId?: string) => void;
  onAddProject: () => void;
  onManageProject: (project: Project) => void;
  onToggleProject: (id: string) => void;
  onTogglePin: (session: SessionMeta) => void;
  onToggleArchive: (session: SessionMeta) => void;
  onAssign: (sessionId: string, projectId: string | null) => void;
  onRename: (session: SessionMeta) => void;
  onDelete: (session: SessionMeta) => void;
  onSettings: () => void;
};

export function ProjectsRail(props: Props) {
  const [query, setQuery] = useState('');
  const [clock, setClock] = useState(() => Date.now());
  const [menuSession, setMenuSession] = useState<string | null>(null);
  const [menuPoint, setMenuPoint] = useState<{ x: number; y: number } | null>(null);
  const [projectScope, setProjectScope] = useState<string>('all');
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const [dropProjectId, setDropProjectId] = useState<string | null>(null);
  const menuTriggers = useRef<Record<string, HTMLButtonElement | null>>({});
  const menuRef = useRef<HTMLDivElement | null>(null);
  const projectMenuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const projectMenuRef = useRef<HTMLDivElement | null>(null);
  const closeProjectMenu = useCallback((restoreFocus: boolean) => {
    setProjectMenuOpen(false);
    if (restoreFocus) requestAnimationFrame(() => projectMenuTriggerRef.current?.focus());
  }, []);

  const projectForSession = useCallback((session: SessionMeta) => {
    return props.projects.find((candidate) => candidate.id === props.assignments[session.id]) || null;
  }, [props.assignments, props.projects]);

  const matchesScope = useCallback((session: SessionMeta) => {
    if (projectScope === 'archived') return Boolean(session.archived);
    if (session.archived) return false;
    return projectScope === 'all' || props.assignments[session.id] === projectScope;
  }, [projectScope, props.assignments]);

  const scopedSessions = useMemo(
    () => props.sessions.filter(matchesScope),
    [matchesScope, props.sessions]
  );
  // Pinned sits above Projects and is deliberately not filtered by project:
  // pinning is the user saying "keep this one reachable", which a project scope
  // should not hide. Without this section the pin action in the row menu
  // changed a flag and produced no visible effect anywhere.
  const pinnedSessions = useMemo(
    () => props.sessions.filter((session) => session.pinned && !session.archived),
    [props.sessions]
  );
  // Pinned rows are listed in their own band, so repeating them here would show
  // the same chat twice in one rail.
  const recentSessions = useMemo(
    () => scopedSessions.filter((session) => !projectForSession(session) && !session.pinned),
    [projectForSession, scopedSessions]
  );
  const archivedSessions = useMemo(
    () => scopedSessions.filter((session) => Boolean(session.archived)),
    [scopedSessions]
  );
  const scopedProject = props.projects.find((project) => project.id === projectScope) || null;
  const projectScopeLabel = projectScope === 'archived' ? 'Archived' : scopedProject?.name || 'All projects';

  useEffect(() => {
    const interval = window.setInterval(() => setClock(Date.now()), 60_000);
    return () => window.clearInterval(interval);
  }, []);

  useEffect(() => {
    if (!menuSession) return;
    const sessionId = menuSession;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      setMenuSession(null);
      setMenuPoint(null);
      requestAnimationFrame(() => menuTriggers.current[sessionId]?.focus());
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (menuRef.current?.contains(target)) return;
      if (menuTriggers.current[sessionId]?.contains(target)) return;
      setMenuSession(null);
      setMenuPoint(null);
    };
    document.addEventListener('keydown', onKeyDown, true);
    document.addEventListener('pointerdown', onPointerDown, true);
    return () => {
      document.removeEventListener('keydown', onKeyDown, true);
      document.removeEventListener('pointerdown', onPointerDown, true);
    };
  }, [menuSession]);

  useEffect(() => {
    if (!projectMenuOpen) return;
    const menu = projectMenuRef.current;
    const focusable = () => Array.from(
      menu?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"], [role="menuitem"]') || []
    );
    focusable()[0]?.focus();
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (projectMenuRef.current?.contains(target) || projectMenuTriggerRef.current?.contains(target)) return;
      closeProjectMenu(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        closeProjectMenu(true);
        return;
      }
      if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
      const items = focusable();
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      const offset = event.key === 'ArrowDown' ? 1 : -1;
      event.preventDefault();
      items[(index + offset + items.length) % items.length]?.focus();
    };
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('keydown', onKeyDown, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown, true);
      document.removeEventListener('keydown', onKeyDown, true);
    };
  }, [closeProjectMenu, projectMenuOpen]);

  const closeMenu = (sessionId: string, action: () => void) => {
    setMenuSession(null);
    setMenuPoint(null);
    action();
    requestAnimationFrame(() => menuTriggers.current[sessionId]?.focus());
  };

  const startDrag = (event: DragEvent<HTMLDivElement>, session: SessionMeta) => {
    event.dataTransfer.setData('text/optimus-session', session.id);
    event.dataTransfer.setData('text/plain', session.id);
    event.dataTransfer.effectAllowed = 'move';
  };

  const dropInto = (projectId: string) => (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const id = event.dataTransfer.getData('text/optimus-session');
    setDropProjectId(null);
    if (id) props.onAssign(id, projectId);
  };

  const renderSession = (session: SessionMeta, projectRow = false) => {
    const active = session.id === props.selectedSessionId;
    const indicator = props.sessionIndicators[session.id] || null;
    const project = projectForSession(session);
    const indicatorLabel =
      indicator === 'working'
        ? 'Optimus is working'
        : indicator === 'attention'
          ? 'Optimus needs your attention'
          : indicator === 'error'
            ? 'Optimus encountered an error'
            : '';
    const menuOpen = menuSession === session.id;
    // Rows inside a project folder show only the session name; the folder
    // heading already carries the project name and worktree.
    const hasStatus = !projectRow && (project || indicator);
    return (
      <div
        className={`session-row${active ? ' is-active' : ''}${project ? '' : ' is-unassigned'}${hasStatus ? ' has-status' : ''}`}
        key={session.id}
        draggable
        data-session-id={session.id}
        onContextMenu={(event) => {
          event.preventDefault();
          setMenuSession(session.id);
          setMenuPoint({ x: event.clientX, y: event.clientY });
          requestAnimationFrame(() => menuTriggers.current[session.id]?.focus());
        }}
        onDragStart={(event) => startDrag(event, session)}
      >
        <button
          ref={(node) => { menuTriggers.current[session.id] = node; }}
          type="button"
          className="session-select"
          aria-expanded={menuOpen}
          aria-haspopup="menu"
          onClick={() => props.onSelectSession(session.id)}
          onKeyDown={(event) => {
            if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
            event.preventDefault();
            setMenuPoint(null);
            setMenuSession(session.id);
          }}
          title={session.title || session.id}
        >
          {!projectRow && project ? (
            <span className="session-card-meta">
              <Icon name="folder" />
              <span>{project.name}</span>
            </span>
          ) : null}
          {hasStatus ? (
            <span className={`session-state is-${indicator || 'idle'}`}>
              <span className="session-status-dot" aria-hidden="true" />
              {indicator === 'working'
                ? 'Working'
                : indicator === 'attention'
                  ? 'Attention'
                  : indicator === 'error'
                    ? 'Error'
                    : formatSessionAge(session.updated_at || session.created_at, clock)}
            </span>
          ) : null}
          {!projectRow && indicatorLabel ? <span className="sr-only">{indicatorLabel}</span> : null}
          <strong className="session-title">{session.title || session.id.slice(0, 8)}</strong>
          {!projectRow && project ? <span className="session-worktree">{worktreeName(project)}</span> : null}
        </button>
        {menuOpen ? (
          <div
            ref={menuRef}
            className="row-menu"
            role="menu"
            aria-label={`Actions for ${session.title || session.id}`}
            style={menuPoint ? { position: 'fixed', left: menuPoint.x, top: menuPoint.y, right: 'auto', animation: 'none' } : undefined}
          >
            <button type="button" role="menuitem" onClick={() => closeMenu(session.id, () => props.onTogglePin(session))}>
              <Icon name="pin" />
              {session.pinned ? 'Unpin session' : 'Pin session'}
            </button>
            <button type="button" role="menuitem" onClick={() => closeMenu(session.id, () => props.onToggleArchive(session))}>
              {session.archived ? 'Unarchive session' : 'Archive session'}
            </button>
            {props.projects.map((candidate) => (
              <button
                type="button"
                role="menuitem"
                key={candidate.id}
                onClick={() => closeMenu(session.id, () => props.onAssign(session.id, candidate.id))}
              >
                <Icon name="folder" />
                {project?.id === candidate.id ? `Remove from ${candidate.name}` : `Move to ${candidate.name}`}
              </button>
            ))}
            {project ? (
              <button type="button" role="menuitem" onClick={() => closeMenu(session.id, () => props.onAssign(session.id, null))}>
                <Icon name="folder" />
                Move to Recent Chats
              </button>
            ) : null}
            <button type="button" role="menuitem" onClick={() => closeMenu(session.id, () => props.onRename(session))}>
              Rename
            </button>
            <button
              type="button"
              role="menuitem"
              className="danger-text"
              onClick={() => closeMenu(session.id, () => props.onDelete(session))}
            >
              <Icon name="trash" />
              Delete session
            </button>
          </div>
        ) : null}
      </div>
    );
  };

  const renderProject = (project: Project) => {
    const isOpen = props.expanded[project.id] !== false;
    const projectSessions = scopedSessions.filter((session) => props.assignments[session.id] === project.id);
    const sessionsId = `project-sessions-${project.id}`;
    const isDropTarget = dropProjectId === project.id;
    return (
      <section
        className={`project-group${isOpen ? ' is-open' : ''}${isDropTarget ? ' is-drop-target' : ''}`}
        key={project.id}
        data-project-id={project.id}
        data-drop-target="project"
        aria-label={project.name}
        onDragOver={(event) => {
          event.preventDefault();
          event.dataTransfer.dropEffect = 'move';
          setDropProjectId(project.id);
        }}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropProjectId(null);
        }}
        onDrop={dropInto(project.id)}
      >
        <div className="project-heading">
          <button
            type="button"
            className="project-toggle"
            aria-expanded={isOpen}
            aria-controls={sessionsId}
            onClick={() => props.onToggleProject(project.id)}
            title={`Toggle ${project.name}`}
          >
            <Icon name="chevron" />
            <Icon name="folder" />
            <span>{project.name}</span>
            <small>{projectSessions.length}</small>
          </button>
          <button
            type="button"
            className="project-new-session"
            aria-label={`New thread in ${project.name}`}
            title={`New thread in ${project.name}`}
            onClick={() => props.onNewSession(project.id)}
          >
            <Icon name="compose" />
          </button>
          <button
            type="button"
            className="project-manage"
            id={`project-manage-${project.id}`}
            aria-label={`Manage sources for ${project.name}`}
            title={`Manage sources for ${project.name}`}
            onClick={() => props.onManageProject(project)}
          >
            <Icon name="more" />
          </button>
        </div>
        <div id={sessionsId} className="project-sessions" role="group" aria-label={`Sessions in ${project.name}`}>
          {projectSessions.length ? projectSessions.map((session) => renderSession(session, true)) : <div className="project-drop-hint">Drop a session here</div>}
        </div>
      </section>
    );
  };

  return (
    <aside
      className={`project-rail${props.collapsed ? ' is-collapsed' : ''}`}
      aria-label="Projects and sessions"
    >
      <div className="rail-primary">
        <div className="rail-action-row">
          <label className="rail-search">
            <Icon name="search" />
            <span className="sr-only">Search threads</span>
            <input
              type="search"
              value={query}
              placeholder="Search"
              onChange={(event) => {
                const next = event.target.value;
                setQuery(next);
                props.onSearch(next);
              }}
            />
            <kbd>{typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform) ? '⌘K' : 'Ctrl+K'}</kbd>
          </label>
          <button
            type="button"
            className="new-session-icon"
            aria-label="New thread"
            title="New thread"
            onClick={() => props.onNewSession()}
          >
            <Icon name="compose" />
          </button>
        </div>
      </div>

      <div className="rail-project-scope">
        <button
          ref={projectMenuTriggerRef}
          type="button"
          className="project-scope-trigger"
          aria-label={projectScopeLabel}
          title={projectScopeLabel}
          aria-expanded={projectMenuOpen}
          aria-haspopup="menu"
          onClick={() => setProjectMenuOpen((open) => !open)}
        >
          <Icon name="folder" />
          <span>{projectScopeLabel}</span>
          <Icon name="chevron" />
        </button>
        <button type="button" aria-label="Add project" title="Add project" onClick={props.onAddProject}>
          <Icon name="project" />
        </button>
        {projectMenuOpen ? (
          <div ref={projectMenuRef} className="project-scope-menu" role="menu" aria-label="Filter sessions by project">
            <button
              type="button"
              role="menuitemradio"
              aria-checked={projectScope === 'all'}
              onClick={() => { setProjectScope('all'); closeProjectMenu(true); }}
            >
              <Icon name="folder" />
              <span>All projects</span>
              {projectScope === 'all' ? <Icon name="check" /> : null}
            </button>
            {props.projects.map((project) => (
              <div className="project-scope-option" role="none" key={project.id}>
                <button
                  type="button"
                  role="menuitemradio"
                  aria-checked={projectScope === project.id}
                  onClick={() => { setProjectScope(project.id); closeProjectMenu(true); }}
                >
                  <Icon name="folder" />
                  <span>{project.name}</span>
                  {projectScope === project.id ? <Icon name="check" /> : null}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  aria-label={`Manage sources for ${project.name}`}
                  title={`Manage sources for ${project.name}`}
                  onClick={() => {
                    closeProjectMenu(false);
                    props.onManageProject(project);
                  }}
                >
                  <Icon name="more" />
                </button>
              </div>
            ))}
            <button
              type="button"
              role="menuitemradio"
              aria-checked={projectScope === 'archived'}
              onClick={() => { setProjectScope('archived'); closeProjectMenu(true); }}
            >
              <Icon name="archive" />
              <span>Archived</span>
              {projectScope === 'archived' ? <Icon name="check" /> : null}
            </button>
          </div>
        ) : null}
      </div>

      <div className="rail-scroll">
        {projectScope === 'archived' ? (
          <section className="rail-section recent-section" data-testid="archived-chats-section" aria-labelledby="archived-chats-heading">
            <h2 className="rail-section-heading" id="archived-chats-heading">Archived Chats</h2>
            <div className="session-stack">
              {archivedSessions.length ? archivedSessions.map((session) => renderSession(session)) : <div className="rail-empty">No archived chats</div>}
            </div>
          </section>
        ) : (
          // Three bands, top to bottom: Pinned, Projects, Recent Chats. The
          // search box and project-scope control sit above them in the rail
          // header, so the top third is "how do I find something", the middle
          // third is the organised tree, and the bottom third is history.
          <>
            <section className="rail-section pinned-section" data-testid="pinned-section" aria-labelledby="pinned-heading">
              <h2 className="rail-section-heading" id="pinned-heading">
                Pinned
              </h2>
              <div className="session-stack pinned-session-stack">
                {pinnedSessions.length
                  ? pinnedSessions.map((session) => renderSession(session))
                  : <div className="rail-empty">Pin a chat to keep it here</div>}
              </div>
            </section>

            <section className="rail-section projects-section" data-testid="projects-section" aria-labelledby="projects-heading">
              <h2 className="rail-section-heading" id="projects-heading">
                <span>Projects</span>
                <button type="button" aria-label="Create project folder" title="Create project folder" onClick={props.onAddProject}>
                  <Icon name="project" />
                </button>
              </h2>
              <div className="project-stack">
                {props.projects.length
                  ? props.projects.map(renderProject)
                  : <div className="rail-empty">Add a project to organize chats</div>}
              </div>
            </section>

            <section className="rail-section recent-section" data-testid="recent-chats-section" aria-labelledby="recent-chats-heading">
              <h2 className="rail-section-heading" id="recent-chats-heading">
                <span>Recent Chats</span>
                <button type="button" aria-label="New recent chat" title="New recent chat" onClick={() => props.onNewSession()}>
                  <Icon name="compose" />
                </button>
              </h2>
              <div
                className="session-stack recent-session-stack"
                data-drop-target="recent-chats"
                onDragOver={(event) => {
                  event.preventDefault();
                  event.dataTransfer.dropEffect = 'move';
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  const id = event.dataTransfer.getData('text/optimus-session');
                  if (id) props.onAssign(id, null);
                }}
              >
                {recentSessions.length
                  ? recentSessions.map((session) => renderSession(session))
                  : (
                    <div className="rail-empty">
                      {scopedProject
                        ? `Chats in ${scopedProject.name} are listed under Projects`
                        : 'No recent chats'}
                    </div>
                  )}
              </div>
            </section>
          </>
        )}
      </div>

      <div className="rail-footer">
        <button
          type="button"
          className="rail-settings-button"
          aria-label="Settings"
          title="Settings"
          onClick={props.onSettings}
        >
          <Icon name="settings" />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  );
}

export function formatSessionAge(value: string | undefined, now = Date.now()) {
  if (!value) return '0m';
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return '0m';
  const minutes = Math.max(0, Math.floor((now - timestamp) / 60_000));
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h`;
}

function worktreeName(project: Project) {
  const root = project.primaryRoot || project.rootPaths[0] || project.name;
  const normalized = root.replace(/[\\/]+$/, '');
  return normalized.split(/[\\/]/).pop() || project.name;
}
