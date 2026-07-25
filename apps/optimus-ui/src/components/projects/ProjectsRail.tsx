import { useMemo, useState, type DragEvent } from 'react';
import type { Project, SessionMeta } from '../../ipc/contracts';
import type { AppRoute } from '../../state/layoutStore';
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
  route: AppRoute;
  showArchived: boolean;
  onShowArchived: (show: boolean) => void;
  onSearch: (query: string) => void;
  onSelectSession: (id: string) => void;
  onNewSession: (projectId?: string) => void;
  onRoute: (route: AppRoute) => void;
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
  const [menuSession, setMenuSession] = useState<string | null>(null);
  const visibleSessions = useMemo(() => {
    return props.sessions.filter((session) => {
      if (!props.showArchived && session.archived) return false;
      return true;
    });
  }, [props.sessions, props.showArchived]);
  const pinnedSessions = visibleSessions.filter((session) => Boolean(session.pinned));

  const renderSession = (session: SessionMeta, projectId?: string) => {
    const active = session.id === props.selectedSessionId;
    const indicator = props.sessionIndicators[session.id] || null;
    const indicatorLabel =
      indicator === 'working'
        ? 'Optimus is working'
        : indicator === 'attention'
          ? 'Optimus needs your attention'
          : indicator === 'error'
            ? 'Optimus encountered an error'
            : '';
    const menuOpen = menuSession === session.id;
    return (
      <div
        className={`session-row${active ? ' is-active' : ''}`}
        key={`${projectId || 'root'}:${session.id}`}
        draggable
        onDragStart={(event) => {
          event.dataTransfer.setData('text/optimus-session', session.id);
          event.dataTransfer.effectAllowed = 'move';
        }}
      >
        <button
          type="button"
          className="session-select"
          onClick={() => props.onSelectSession(session.id)}
          title={session.title || session.id}
        >
          <span
            className={`session-status-dot is-${indicator || 'idle'}`}
            aria-hidden="true"
          />
          {indicatorLabel ? <span className="sr-only">{indicatorLabel}</span> : null}
          <span className="session-copy">
            <strong>{session.title || session.id.slice(0, 8)}</strong>
            <small>{`${session.message_count || 0} messages`}</small>
          </span>
        </button>
        <button
          type="button"
          className="row-more"
          aria-label={`Actions for ${session.title || session.id}`}
          aria-expanded={menuOpen}
          onClick={() => setMenuSession(menuOpen ? null : session.id)}
        >
          <Icon name="more" />
        </button>
        {menuOpen ? (
          <div className="row-menu" role="menu">
            <button type="button" role="menuitem" onClick={() => props.onTogglePin(session)}>
              <Icon name="pin" />
              {session.pinned ? 'Unpin session' : 'Pin session'}
            </button>
            <button type="button" role="menuitem" onClick={() => props.onToggleArchive(session)}>
              {session.archived ? 'Unarchive session' : 'Archive session'}
            </button>
            <button type="button" role="menuitem" onClick={() => props.onRename(session)}>
              Rename
            </button>
            <button
              type="button"
              role="menuitem"
              className="danger-text"
              onClick={() => props.onDelete(session)}
            >
              <Icon name="trash" />
              Delete session
            </button>
          </div>
        ) : null}
      </div>
    );
  };

  const dropInto = (projectId: string) => (event: DragEvent) => {
    event.preventDefault();
    const id = event.dataTransfer.getData('text/optimus-session');
    if (id) props.onAssign(id, projectId);
  };

  return (
    <aside
      className={`project-rail${props.collapsed ? ' is-collapsed' : ''}`}
      aria-label="Projects and sessions"
    >
      <div className="rail-primary">
        <button
          type="button"
          className="rail-product-mark"
          onClick={() => props.onRoute('work')}
          aria-label="Optimus"
          title="Optimus"
        >
          <span>Optimus</span>
          <Icon name="chevron" />
        </button>
        <div className="rail-action-row">
          <label className="rail-search">
            <Icon name="search" />
            <span className="sr-only">Search sessions</span>
            <input
              type="search"
              value={query}
              placeholder="Search sessions"
              onChange={(event) => {
                const next = event.target.value;
                setQuery(next);
                props.onSearch(next);
              }}
            />
            <kbd>⌘K</kbd>
          </label>
          <button
            type="button"
            className="new-session-icon"
            aria-label="New session"
            title="New session"
            onClick={() => props.onNewSession()}
          >
            <Icon name="plus" />
          </button>
        </div>
      </div>

      <nav className="rail-nav" aria-label="Primary">
        <button
          type="button"
          className={props.route === 'mail' ? 'is-active' : ''}
          onClick={() => props.onRoute('mail')}
        >
          <Icon name="mail" />
          <span>Mail</span>
        </button>
        <button
          type="button"
          className={props.route === 'capabilities' ? 'is-active' : ''}
          onClick={() => props.onRoute('capabilities')}
        >
          <Icon name="capabilities" />
          <span>Capabilities</span>
        </button>
        <button
          type="button"
          className={props.route === 'consoles' ? 'is-active' : ''}
          onClick={() => props.onRoute('consoles')}
        >
          <Icon name="settings" />
          <span>Consoles</span>
        </button>
        <button
          type="button"
          className={props.route === 'artifacts' ? 'is-active' : ''}
          onClick={() => props.onRoute('artifacts')}
        >
          <Icon name="artifact" />
          <span>Artifacts</span>
        </button>
      </nav>

      <div className="rail-scroll">
        {pinnedSessions.length ? (
          <section className="rail-section">
            <div className="rail-section-heading">
              <span>Pinned</span>
              <span>{pinnedSessions.length}</span>
            </div>
            <div className="session-stack">
              {pinnedSessions.map((session) => renderSession(session))}
            </div>
          </section>
        ) : null}

        <section className="rail-section">
          <div className="rail-section-heading projects-heading">
            <span>Projects</span>
            <button
              type="button"
              className="add-project-button"
              aria-label="Add project"
              title="Add project"
              onClick={props.onAddProject}
            >
              <Icon name="project" />
            </button>
          </div>
          {props.projects.map((project) => {
            const projectSessions = visibleSessions.filter(
              (session) =>
                props.assignments[session.id] === project.id && !session.pinned
            );
            const open = props.expanded[project.id] !== false;
            return (
              <div
                className={`project-group${open ? ' is-open' : ''}`}
                key={project.id}
                onDragOver={(event) => event.preventDefault()}
                onDrop={dropInto(project.id)}
              >
                <div className="project-heading">
                  <button
                    className="project-toggle"
                    type="button"
                    aria-expanded={open}
                    onClick={() => props.onToggleProject(project.id)}
                  >
                    <Icon name="folder" />
                    <span className="project-copy">
                      <strong>{project.name}</strong>
                      <small>
                        {project.rootPaths.length
                          ? `${project.rootPaths.length} source${project.rootPaths.length === 1 ? '' : 's'}`
                          : 'No sources'}
                      </small>
                    </span>
                  </button>
                  <button
                    type="button"
                    id={`project-manage-${project.id}`}
                    className="project-manage-button"
                    aria-label={`Manage sources for ${project.name}`}
                    title={`Manage sources for ${project.name}`}
                    onClick={() => props.onManageProject(project)}
                  >
                    <Icon name="source" />
                  </button>
                  <button
                    type="button"
                    aria-label={`New session in ${project.name}`}
                    title={`New session in ${project.name}`}
                    onClick={() => props.onNewSession(project.id)}
                  >
                    <Icon name="plus" />
                  </button>
                </div>
                <div className="project-sessions">
                  {projectSessions.length ? (
                    projectSessions.map((session) => renderSession(session, project.id))
                  ) : (
                    <div className="rail-empty">Drop a session here</div>
                  )}
                </div>
              </div>
            );
          })}
        </section>
      </div>

      <div className="rail-footer">
        <button
          type="button"
          aria-pressed={props.showArchived}
          onClick={() => props.onShowArchived(!props.showArchived)}
        >
          <span>{props.showArchived ? 'Hide archived' : 'Show archived'}</span>
        </button>
        <button type="button" onClick={props.onSettings}>
          <Icon name="settings" />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  );
}
