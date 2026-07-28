import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useAlive } from '../../hooks/useAlive';
import type { Project, ProjectRootSelection } from '../../ipc/contracts';
import {
  addProjectRoot,
  removeProjectRoot,
  setPrimaryProjectRoot,
} from '../../state/projectStore';
import { Icon } from '../chrome/Icon';

export function ProjectSourcesDialog({
  project,
  authorizedRootPaths = [],
  allowContinueWithoutProject = false,
  onPickSource,
  onSave,
  onContinueWithoutProject,
  onClose,
}: {
  project: Project | null;
  /** Canonical roots already present in the Rust project allowlist for this project. */
  authorizedRootPaths?: string[];
  /** When true, offer an explicit path to unassign and chat without project roots. */
  allowContinueWithoutProject?: boolean;
  onPickSource: () => Promise<ProjectRootSelection>;
  onSave: (project: Project, grantTokens: string[]) => Promise<void>;
  onContinueWithoutProject?: () => void;
  onClose: () => void;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState<Project | null>(project);
  const [grantTokens, setGrantTokens] = useState<Record<string, string>>({});
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const alive = useAlive();

  const authorizedKeys = useMemo(
    () => new Set(authorizedRootPaths.map(normalizePathKey)),
    [authorizedRootPaths]
  );

  useEffect(() => {
    setDraft(project);
    setGrantTokens({});
    setError('');
    setSaving(false);
    if (!project) return;
    requestAnimationFrame(() => panel.current?.focus());
  }, [project]);

  // Capture Escape at the window level so dismiss works even if focus left the
  // dialog (e.g. after the native picker or when the composer still holds focus).
  useEffect(() => {
    if (!project) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      onClose();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [project, onClose]);

  if (!project || !draft) return null;

  const pendingRoots = draft.rootPaths.filter((path) => {
    const key = normalizePathKey(path);
    if (authorizedKeys.has(key)) return false;
    return !Object.keys(grantTokens).some((tokenPath) => normalizePathKey(tokenPath) === key);
  });
  const canSave = Boolean(draft.name.trim()) && pendingRoots.length === 0 && !saving;

  async function authorizeFolder(pathHint?: string) {
    const selection = await onPickSource();
    if (!alive()) return;
    if (!selection.ok || !selection.path || !selection.grantToken) {
      if (selection.cancelled) return;
      setError(
        pathHint
          ? `Native folder selection failed for ${basename(pathHint)}. Try again.`
          : 'Native folder selection failed. Try again.'
      );
      return;
    }
    setError('');
    setDraft((current) => (current ? addProjectRoot(current, selection.path!) : current));
    setGrantTokens((current) => ({
      ...current,
      [selection.path!]: selection.grantToken!,
    }));
  }

  return (
    <div
      className="dialog-backdrop project-sources-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.preventDefault();
          onClose();
          return;
        }
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
              onClick={() => void authorizeFolder()}
            >
              <Icon name="source" />
              Add source
            </button>
          </div>

          <div className="project-source-list">
            {draft.rootPaths.length ? draft.rootPaths.map((path) => {
              const primary = (draft.primaryRoot || draft.rootPaths[0]) === path;
              const key = normalizePathKey(path);
              const hasGrant = Object.keys(grantTokens).some(
                (tokenPath) => normalizePathKey(tokenPath) === key
              );
              const authorized = authorizedKeys.has(key) || hasGrant;
              return (
                <article key={path} className={primary ? 'is-primary' : ''}>
                  <span className="source-icon"><Icon name="folder" /></span>
                  <div>
                    <strong>{basename(path)}</strong>
                    <span title={path}>{path}</span>
                    <span
                      className={authorized ? 'source-auth is-ready' : 'source-auth is-pending'}
                      role="status"
                    >
                      {authorized
                        ? hasGrant
                          ? 'Ready to authorize'
                          : 'Authorized'
                        : 'Needs native folder re-selection'}
                    </span>
                  </div>
                  <div className="source-actions">
                    {!authorized ? (
                      <button
                        type="button"
                        className="primary-source"
                        onClick={() => void authorizeFolder(path)}
                      >
                        Re-select folder
                      </button>
                    ) : (
                      <button
                        type="button"
                        className={primary ? 'primary-source' : ''}
                        aria-pressed={primary}
                        onClick={() => setDraft(setPrimaryProjectRoot(draft, path))}
                      >
                        {primary ? 'Primary' : 'Make primary'}
                      </button>
                    )}
                    <button
                      type="button"
                      aria-label={`Remove ${basename(path)} from project`}
                      title="Remove source"
                      onClick={() => {
                        setDraft(removeProjectRoot(draft, path));
                        setGrantTokens((current) => {
                          const next = { ...current };
                          for (const tokenPath of Object.keys(next)) {
                            if (normalizePathKey(tokenPath) === key) delete next[tokenPath];
                          }
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
              {pendingRoots.length
                ? `Re-select ${pendingRoots.length === 1 ? 'this folder' : 'each folder'} with the system picker so Rust can stage a short-lived grant. Catalog paths alone are not enough.`
                : 'Saving consumes native folder selections into the Rust project allowlist. Every file mutation still requires an exact SmartDeny approval.'}
            </span>
          </div>
          {error ? <p className="project-source-error" role="alert">{error}</p> : null}
        </div>

        <footer>
          {allowContinueWithoutProject && onContinueWithoutProject ? (
            <button
              type="button"
              className="secondary-action"
              onClick={onContinueWithoutProject}
            >
              Continue without project
            </button>
          ) : null}
          <button type="button" className="secondary-action" onClick={onClose}>Cancel</button>
          <button
            type="button"
            className="primary-action"
            disabled={!canSave}
            title={
              pendingRoots.length
                ? 'Re-select each unauthorized folder with the system picker first'
                : undefined
            }
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
                if (!alive()) return;
                onClose();
              } catch (saveError) {
                if (!alive()) return;
                setError(formatAuthorizeError(saveError));
                setSaving(false);
              }
            }}
          >
            {saving ? 'Authorizing…' : pendingRoots.length ? 'Re-select folders to authorize' : 'Save & authorize'}
          </button>
        </footer>
      </div>
    </div>
  );
}

export function normalizePathKey(path: string) {
  return path.trim().replace(/\\/g, '/').replace(/\/+$/, '');
}

export function formatAuthorizeError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  if (!message || message === 'request failed') {
    return 'Authorization failed. Re-select each folder with the system picker, then try Save again.';
  }
  if (message.includes('native folder selection')) {
    return `${message} Use Re-select folder on each pending source, then Save again.`;
  }
  return message;
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
