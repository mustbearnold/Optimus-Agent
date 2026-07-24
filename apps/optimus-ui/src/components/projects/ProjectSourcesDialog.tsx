import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import type { Project } from '../../ipc/contracts';
import {
  addProjectRoot,
  removeProjectRoot,
  setPrimaryProjectRoot,
} from '../../state/projectStore';
import { Icon } from '../chrome/Icon';

export function ProjectSourcesDialog({
  project,
  onPickSource,
  onSave,
  onClose,
}: {
  project: Project | null;
  onPickSource: () => Promise<string | null>;
  onSave: (project: Project) => void;
  onClose: () => void;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState<Project | null>(project);

  useEffect(() => {
    setDraft(project);
    if (!project) return;
    requestAnimationFrame(() => panel.current?.focus());
  }, [project]);

  if (!project || !draft) return null;

  return (
    <div
      className="dialog-backdrop project-sources-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') onClose();
        trapFocus(event, panel.current);
      }}
    >
      <div
        className="project-sources-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="project-sources-title"
        tabIndex={-1}
        ref={panel}
      >
        <header>
          <div>
            <span className="dialog-icon"><Icon name="project" /></span>
            <div>
              <h2 id="project-sources-title">Project sources</h2>
              <p>One project can group several folders without merging their permissions.</p>
            </div>
          </div>
          <button type="button" aria-label="Close project sources" onClick={onClose}>
            <Icon name="close" />
          </button>
        </header>

        <div className="project-sources-content">
          <label className="field-stack">
            <span>Project name</span>
            <input
              aria-label="Project name"
              value={draft.name}
              onChange={(event) => setDraft({ ...draft, name: event.target.value })}
            />
          </label>

          <div className="project-source-heading">
            <div>
              <h3>Folders</h3>
              <p>{draft.rootPaths.length} source{draft.rootPaths.length === 1 ? '' : 's'} in this local project</p>
            </div>
            <button
              type="button"
              className="secondary-action"
              onClick={async () => {
                const path = await onPickSource();
                if (path) setDraft((current) => current ? addProjectRoot(current, path) : current);
              }}
            >
              <Icon name="source" />
              Add source
            </button>
          </div>

          <div className="project-source-list">
            {draft.rootPaths.length ? draft.rootPaths.map((path) => {
              const primary = (draft.primaryRoot || draft.rootPaths[0]) === path;
              return (
                <article key={path} className={primary ? 'is-primary' : ''}>
                  <span className="source-icon"><Icon name="folder" /></span>
                  <div>
                    <strong>{basename(path)}</strong>
                    <span title={path}>{path}</span>
                  </div>
                  <div className="source-actions">
                    <button
                      type="button"
                      className={primary ? 'primary-source' : ''}
                      aria-pressed={primary}
                      onClick={() => setDraft(setPrimaryProjectRoot(draft, path))}
                    >
                      {primary ? 'Primary' : 'Make primary'}
                    </button>
                    <button
                      type="button"
                      aria-label={`Remove ${basename(path)} from project`}
                      title="Remove source"
                      onClick={() => setDraft(removeProjectRoot(draft, path))}
                    >
                      <Icon name="close" />
                    </button>
                  </div>
                </article>
              );
            }) : (
              <div className="project-source-empty">
                <Icon name="source" />
                <strong>No source folders</strong>
                <span>Add at least one folder before starting project-bound work.</span>
              </div>
            )}
          </div>

          <div className="settings-callout project-source-note">
            <Icon name="info" />
            <span>
              This catalog is confirmed local presentation state. Runtime folder access remains governed by Optimus approvals and filesystem allowlists.
            </span>
          </div>
        </div>

        <footer>
          <button type="button" className="secondary-action" onClick={onClose}>Cancel</button>
          <button
            type="button"
            className="primary-action"
            disabled={!draft.name.trim()}
            onClick={() => {
              onSave({
                ...draft,
                name: draft.name.trim(),
                primaryRoot: draft.primaryRoot || draft.rootPaths[0],
                updatedAt: new Date().toISOString(),
              });
              onClose();
            }}
          >
            Save project
          </button>
        </footer>
      </div>
    </div>
  );
}

function basename(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) || path;
}

function trapFocus(event: KeyboardEvent, container: HTMLElement | null) {
  if (event.key !== 'Tab' || !container) return;
  const focusable = Array.from(
    container.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])'
    )
  );
  if (!focusable.length) {
    event.preventDefault();
    container.focus();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
