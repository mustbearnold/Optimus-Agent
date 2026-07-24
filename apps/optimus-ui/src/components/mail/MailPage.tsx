import { useEffect, useMemo, useState } from 'react';
import type { Project, SessionMeta } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

type MailFact = {
  label: string;
  value: string;
};

type MailMessage = {
  id: string;
  sender: string;
  subject: string;
  preview: string;
  context: string;
  unread: boolean;
  paragraphs: string[];
  facts?: MailFact[];
};

type Props = {
  projects: Project[];
  sessions: SessionMeta[];
  assignments: Record<string, string>;
  activeRunSessionId: string | null;
};

export function MailPage({
  projects,
  sessions,
  assignments,
  activeRunSessionId,
}: Props) {
  const messages = useMemo(
    () => buildPreviewMessages(projects, sessions, assignments, activeRunSessionId),
    [activeRunSessionId, assignments, projects, sessions]
  );
  const [selectedId, setSelectedId] = useState(() => messages[0]?.id || '');
  const [readIds, setReadIds] = useState<Set<string>>(
    () => new Set(messages.filter((message) => !message.unread).map((message) => message.id))
  );
  const selected = messages.find((message) => message.id === selectedId) || messages[0] || null;

  useEffect(() => {
    if (!selected?.id) return;
    setReadIds((current) => {
      if (current.has(selected.id)) return current;
      const next = new Set(current);
      next.add(selected.id);
      return next;
    });
  }, [selected?.id]);

  const unreadCount = messages.filter((message) => message.unread && !readIds.has(message.id)).length;

  return (
    <main className="mail-page" aria-label="Mail">
      <header className="mail-toolbar">
        <div className="mail-title">
          <Icon name="mail" />
          <div>
            <h1>Mail</h1>
            <span>{unreadCount ? `${unreadCount} unread` : 'All caught up'}</span>
          </div>
        </div>
        <span className="mail-preview-label">Local preview</span>
      </header>

      <div className="mail-layout">
        <section className="mail-list" aria-label="Messages">
          {messages.map((message) => {
            const isSelected = selected?.id === message.id;
            const isUnread = message.unread && !readIds.has(message.id);
            return (
              <button
                type="button"
                className={`mail-list-item${isSelected ? ' is-selected' : ''}${isUnread ? ' is-unread' : ''}`}
                aria-current={isSelected ? 'true' : undefined}
                key={message.id}
                onClick={() => setSelectedId(message.id)}
              >
                <span className="mail-list-meta">
                  <strong>{message.sender}</strong>
                  <span>{message.context}</span>
                </span>
                <span className="mail-list-subject">
                  {isUnread ? <span className="mail-unread-dot"><span className="sr-only">Unread: </span></span> : null}
                  <strong>{message.subject}</strong>
                </span>
                <span className="mail-list-preview">{message.preview}</span>
              </button>
            );
          })}
        </section>

        {selected ? (
          <article className="mail-reader" aria-labelledby={`mail-subject-${selected.id}`}>
            <header className="mail-reader-header">
              <span className="mail-reader-context">{selected.context}</span>
              <h2 id={`mail-subject-${selected.id}`}>{selected.subject}</h2>
              <div className="mail-sender">
                <span className="mail-sender-mark" aria-hidden="true">O</span>
                <div>
                  <strong>{selected.sender}</strong>
                  <span>Generated from local Optimus preview state</span>
                </div>
              </div>
            </header>
            <div className="mail-body">
              {selected.paragraphs.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
              {selected.facts?.length ? (
                <dl className="mail-facts">
                  {selected.facts.map((fact) => (
                    <div key={fact.label}>
                      <dt>{fact.label}</dt>
                      <dd>{fact.value}</dd>
                    </div>
                  ))}
                </dl>
              ) : null}
            </div>
          </article>
        ) : (
          <div className="mail-empty">
            <Icon name="mail" />
            <p>No Optimus updates yet.</p>
          </div>
        )}
      </div>
    </main>
  );
}

function buildPreviewMessages(
  projects: Project[],
  sessions: SessionMeta[],
  assignments: Record<string, string>,
  activeRunSessionId: string | null
): MailMessage[] {
  const project = projects[0];
  const projectName = project?.name || 'Your workspace';
  const projectSessions = project
    ? sessions.filter((session) => assignments[session.id] === project.id)
    : [];
  const sourceCount = project?.rootPaths.length || 0;
  const activeRun = activeRunSessionId
    ? sessions.find((session) => session.id === activeRunSessionId)
    : null;

  return [
    {
      id: `project-summary-${project?.id || 'workspace'}`,
      sender: 'Optimus',
      subject: `${projectName} workspace summary`,
      preview: `${projectSessions.length} sessions assigned · ${sourceCount} sources connected`,
      context: projectName,
      unread: true,
      paragraphs: [
        'Optimus prepared this summary from the project state currently loaded in the app.',
        'This is preview mail inside Optimus. It has not been sent to an external inbox.',
      ],
      facts: [
        { label: 'Assigned sessions', value: String(projectSessions.length) },
        { label: 'Connected sources', value: String(sourceCount) },
        { label: 'Current activity', value: activeRun?.title || (activeRun ? activeRun.id : 'No active run') },
      ],
    },
    {
      id: 'welcome-to-optimus-mail',
      sender: 'Optimus',
      subject: 'Welcome to Optimus Mail',
      preview: 'A focused home for project updates and attention items',
      context: 'Product update',
      unread: true,
      paragraphs: [
        'Mail is where Optimus can present project updates without mixing them into your work sessions.',
        'This implementation supports a local message list, unread state, and message reading. External email delivery and account sync are not implemented.',
      ],
    },
    {
      id: 'mail-preferences-planned',
      sender: 'Optimus',
      subject: 'Notification controls are planned',
      preview: 'Project rules, schedules, and update types will become configurable',
      context: 'Planned capability',
      unread: false,
      paragraphs: [
        'Future versions can add per-project update rules, delivery schedules, and configurable message types.',
        'Those controls are intentionally not shown as active in this preview because the runtime does not support them yet.',
      ],
    },
  ];
}
