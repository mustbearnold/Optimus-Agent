import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import type { Project, ProjectRootSelection } from '../../ipc/contracts';
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
  onPickSource: () => Promise<ProjectRootSelection>;
  onSave: (project: Project, grantTokens: string[]) => Promise<void>;
  onClose: () => void;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState<Project | null>(project);
  const [grantTokens, setGrantTokens] = useState<Record<string, string>>({});
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setDraft(project);
    setGrantTokens({});
    setError('');
    setSaving(false);
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
                const selection = await onPickSource();
                if (selection.ok && selection.path && selection.grantToken) {
                  setDraft((current) => current ? addProjectRoot(current, selection.path!) : current);
                  setGrantTokens((current) => ({
                    ...current,
                    [selection.path!]: selection.grantToken!,
                  }));
                }
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
                      onClick={() => {
                        setDraft(removeProjectRoot(draft, path));
                        setGrantTokens((current) => {
                          const next = { ...current };
                          delete next[path];
                          return next;
                        });
                      }}
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
              Saving consumes native folder selections into the Rust project allowlist. Every file mutation still requires an exact SmartDeny approval.
            </span>
          </div>
          {error ? <p className="project-source-error" role="alert">{error}</p> : null}
        </div>

        <footer>
          <button type="button" className="secondary-action" onClick={onClose}>Cancel</button>
          <button
            type="button"
            className="primary-action"
            disabled={!draft.name.trim() || saving}
            onClick={async () => {
              setSaving(true);
              setError('');
              try {
                await onSave({
                  ...draft,
                  name: draft.name.trim(),
                  primaryRoot: draft.primaryRoot || draft.rootPaths[0],
                  updatedAt: new Date().toISOString(),
                }, Object.values(grantTokens));
                onClose();
              } catch (saveError) {
                setError(saveError instanceof Error ? saveError.message : String(saveError));
                setSaving(false);
              }
            }}
          >
            {saving ? 'Authorizing…' : 'Save & authorize'}
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
