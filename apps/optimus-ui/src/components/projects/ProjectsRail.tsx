import { useMemo, useState, type DragEvent } from 'react';
import type { Project, SessionMeta } from '../../ipc/contracts';
import type { AppRoute } from '../../state/layoutStore';
import { Icon } from '../chrome/Icon';

type Props = {
  collapsed: boolean;
  sessions: SessionMeta[];
  projects: Project[];
  assignments: Record<string, string>;
  pins: string[];
  expanded: Record<string, boolean>;
  selectedSessionId: string | null;
  activeRunSessionId: string | null;
  route: AppRoute;
  onSelectSession: (id: string) => void;
  onNewSession: (projectId?: string) => void;
  onRoute: (route: AppRoute) => void;
  onAddProject: () => void;
  onManageProject: (project: Project) => void;
  onToggleProject: (id: string) => void;
  onTogglePin: (sessionId: string) => void;
  onAssign: (sessionId: string, projectId: string | null) => void;
  onRename: (session: SessionMeta) => void;
  onDelete: (session: SessionMeta) => void;
  onSettings: () => void;
};

export function ProjectsRail(props: Props) {
  const [query, setQuery] = useState('');
  const [menuSession, setMenuSession] = useState<string | null>(null);
  const needle = query.trim().toLowerCase();
  const visibleSessions = useMemo(
    () =>
      props.sessions.filter((session) =>
        needle ? (session.title || session.id).toLowerCase().includes(needle) : true
      ),
    [needle, props.sessions]
  );
  const pinnedSessions = visibleSessions.filter((session) => props.pins.includes(session.id));
  const assignedIds = new Set(Object.keys(props.assignments));
  const inbox = visibleSessions.filter(
    (session) => !assignedIds.has(session.id) && !props.pins.includes(session.id)
  );

  const renderSession = (session: SessionMeta, projectId?: string) => {
    const active = session.id === props.selectedSessionId;
    const working = session.id === props.activeRunSessionId;
    const menuOpen = menuSession === session.id;
    return (
      <div
        className={`session-row${active ? ' is-active' : ''}${working ? ' is-working' : ''}`}
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
          <span className="session-state" aria-hidden="true" />
          <span className="session-copy">
            <strong>{session.title || session.id.slice(0, 8)}</strong>
            <small>
              {working ? 'Working' : `${session.message_count || 0} messages`}
            </small>
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
            <button type="button" role="menuitem" onClick={() => props.onTogglePin(session.id)}>
              <Icon name="pin" />
              {props.pins.includes(session.id) ? 'Unpin session' : 'Pin session'}
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

  const dropInto = (projectId: string | null) => (event: DragEvent) => {
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
        <div className="rail-action-row">
          <label className="rail-search">
            <Icon name="search" />
            <span className="sr-only">Search sessions</span>
            <input
              type="search"
              value={query}
              placeholder="Search"
              onChange={(event) => setQuery(event.target.value)}
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
          className={props.route === 'work' ? 'is-active' : ''}
          onClick={() => props.onRoute('work')}
        >
          <Icon name="chat" />
          <span>Work</span>
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
          <div className="rail-section-heading">
            <span>Projects</span>
            <button type="button" aria-label="Add project" title="Add project" onClick={props.onAddProject}>
              <Icon name="plus" />
            </button>
          </div>
          {props.projects.map((project) => {
            const projectSessions = visibleSessions.filter(
              (session) =>
                props.assignments[session.id] === project.id && !props.pins.includes(session.id)
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
                  <button type="button" onClick={() => props.onToggleProject(project.id)}>
                    <Icon name="chevron" />
                    <Icon name="folder" />
                    <span className="project-copy">
                      <strong>{project.name}</strong>
                      <small>
                        {project.rootPaths.length
                          ? `${project.rootPaths.length} source${project.rootPaths.length === 1 ? '' : 's'}`
                          : 'No sources'}
                      </small>
                    </span>
                    {props.activeRunSessionId &&
                    props.assignments[props.activeRunSessionId] === project.id ? (
                      <span className="working-dot" title="Working" />
                    ) : null}
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
          <div
            className={`project-group inbox-group${
              props.expanded.inbox !== false ? ' is-open' : ''
            }`}
            onDragOver={(event) => event.preventDefault()}
            onDrop={dropInto(null)}
          >
            <div className="project-heading">
              <button type="button" onClick={() => props.onToggleProject('inbox')}>
                <Icon name="chevron" />
                <Icon name="folder" />
                <span>Inbox</span>
              </button>
            </div>
            <div className="project-sessions">
              {inbox.length ? inbox.map((session) => renderSession(session)) : <div className="rail-empty">No unassigned sessions</div>}
            </div>
          </div>
        </section>
      </div>

      <div className="rail-footer">
        <button type="button" onClick={props.onSettings}>
          <Icon name="settings" />
          <span>Settings</span>
        </button>
        <span className="rail-brand-dot" aria-hidden="true" />
      </div>
    </aside>
  );
}
