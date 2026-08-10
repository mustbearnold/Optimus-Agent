import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useAlive } from '../../hooks/useAlive';
import type { ArtifactDetail, ArtifactRecord } from '../../ipc/contracts';
import type { OptimusClient } from '../../ipc/client';
import { Icon } from '../chrome/Icon';

type TypeFilter = 'all' | 'image' | 'text' | 'binary';

function artifactKind(mediaType?: string): TypeFilter {
  const m = (mediaType || '').toLowerCase();
  if (m.startsWith('image/')) return 'image';
  if (m.startsWith('text/') || m.includes('json') || m.includes('xml')) return 'text';
  return 'binary';
}

export function ArtifactsSurface({
  client,
  active,
  standalone = false,
}: {
  client: OptimusClient;
  active: boolean;
  standalone?: boolean;
}) {
  const [artifacts, setArtifacts] = useState<ArtifactRecord[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [detail, setDetail] = useState<ArtifactDetail | null>(null);
  const [query, setQuery] = useState('');
  const [typeFilter, setTypeFilter] = useState<TypeFilter>('all');
  const [labelFilter, setLabelFilter] = useState<string | null>(null);
  const [gallery, setGallery] = useState(false);
  const [thumbs, setThumbs] = useState<Record<string, string>>({});
  const [error, setError] = useState('');
  const [status, setStatus] = useState('');
  const [pendingDelete, setPendingDelete] = useState<string[]>([]);
  const [deleting, setDeleting] = useState(false);
  const alive = useAlive();
  const deleteTrigger = useRef<HTMLElement | null>(null);
  const cancelDeleteButton = useRef<HTMLButtonElement>(null);
  const confirmDeleteButton = useRef<HTMLButtonElement>(null);

  const load = useCallback(async () => {
    setError('');
    try {
      const artifacts = await client.artifacts.list();
      if (!alive()) return;
      setArtifacts(artifacts);
    } catch (reason) {
      if (!alive()) return;
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [alive, client]);

  useEffect(() => {
    if (active) void load();
  }, [active, load]);

  useEffect(() => {
    if (pendingDelete.length) cancelDeleteButton.current?.focus();
  }, [pendingDelete]);

  const labels = useMemo(() => {
    const set = new Set<string>();
    for (const a of artifacts) {
      if (a.label?.trim()) set.add(a.label.trim());
    }
    return [...set].sort((a, b) => a.localeCompare(b)).slice(0, 24);
  }, [artifacts]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return artifacts.filter((artifact) => {
      if (typeFilter !== 'all' && artifactKind(artifact.media_type) !== typeFilter) return false;
      if (labelFilter && artifact.label !== labelFilter) return false;
      if (!needle) return true;
      return `${artifact.label} ${artifact.source} ${artifact.sha256} ${artifact.media_type}`
        .toLowerCase()
        .includes(needle);
    });
  }, [artifacts, query, typeFilter, labelFilter]);

  // Lazy-load image thumbnails for gallery mode.
  useEffect(() => {
    if (!active || !gallery) return;
    let cancelled = false;
    const images = filtered.filter((a) => artifactKind(a.media_type) === 'image').slice(0, 24);
    void (async () => {
      for (const artifact of images) {
        if (cancelled || thumbs[artifact.sha256]) continue;
        try {
          const detail = await client.artifacts.get(artifact.sha256);
          if (cancelled) return;
          if (detail.kind === 'image' && detail.data_url) {
            setThumbs((current) => ({ ...current, [artifact.sha256]: detail.data_url! }));
          }
        } catch {
          // skip failed thumbs
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [active, gallery, filtered, thumbs, client]);

  const open = async (artifact: ArtifactRecord) => {
    try {
      const next = await client.artifacts.get(artifact.sha256);
      if (!alive()) return;
      setDetail(next);
    } catch (reason) {
      if (!alive()) return;
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const removeMany = async (sha256s: string[]) => {
    if (!sha256s.length) return;
    await client.artifacts.deleteMany(sha256s);
    if (!alive()) return;
    setSelected([]);
    if (detail && sha256s.includes(detail.artifact.sha256)) setDetail(null);
    await load();
  };

  const requestDelete = (sha256s: string[]) => {
    if (!sha256s.length) return;
    deleteTrigger.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setPendingDelete([...sha256s]);
  };

  const closeDeleteConfirmation = () => {
    if (deleting) return;
    setPendingDelete([]);
    requestAnimationFrame(() => deleteTrigger.current?.focus());
  };

  const confirmDelete = async () => {
    const sha256s = [...pendingDelete];
    if (!sha256s.length || deleting) return;
    setDeleting(true);
    setError('');
    try {
      await removeMany(sha256s);
      if (!alive()) return;
      setPendingDelete([]);
      requestAnimationFrame(() => deleteTrigger.current?.focus());
    } catch (reason) {
      if (!alive()) return;
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (alive()) setDeleting(false);
    }
  };

  const exportOne = async (sha256: string) => {
    setError('');
    setStatus('');
    try {
      const result = await client.artifacts.export(sha256);
      if (!alive()) return;
      setStatus(`Exported to ${result.path || 'host path'}`);
      if (result.path) {
        await client.shell.openPath(result.path);
      }
    } catch (reason) {
      if (!alive()) return;
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const exportZip = async (sha256s: string[]) => {
    if (!sha256s.length) return;
    setError('');
    setStatus('');
    try {
      const result = await client.artifacts.exportZip(sha256s);
      if (!alive()) return;
      setStatus(`Zip exported to ${result.path || 'host path'}`);
      if (result.path) {
        await client.shell.openPath(result.path);
      }
    } catch (reason) {
      if (!alive()) return;
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const deleteLabel =
    pendingDelete.length === 1 ? 'Delete 1 artifact?' : `Delete ${pendingDelete.length} artifacts?`;

  return (
    <section className={`artifacts-surface${standalone ? ' is-standalone' : ''}`} aria-label="Artifacts">
      <div className="surface-toolbar">
        <div><Icon name="artifact" /><strong>Artifacts</strong></div>
        <label className="surface-search">
          <Icon name="search" />
          <input
            type="search"
            aria-label="Filter artifacts"
            placeholder="Filter label, source, or hash"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <button
          type="button"
          className={gallery ? 'is-active' : ''}
          aria-pressed={gallery}
          onClick={() => setGallery((v) => !v)}
        >
          Gallery
        </button>
        <button
          type="button"
          disabled={!selected.length}
          onClick={() => void exportZip(selected)}
        >
          Zip {selected.length || ''}
        </button>
        <button
          type="button"
          className="danger-text"
          disabled={!selected.length}
          onClick={() => requestDelete(selected)}
        >
          <Icon name="trash" />
          Delete {selected.length || ''}
        </button>
        <button type="button" aria-label="Refresh artifacts" onClick={() => void load()}>
          <Icon name="refresh" />
        </button>
      </div>
      <div className="artifact-filter-chips" role="group" aria-label="Artifact type filters">
        {(['all', 'image', 'text', 'binary'] as TypeFilter[]).map((kind) => (
          <button
            key={kind}
            type="button"
            className={typeFilter === kind ? 'is-active' : ''}
            aria-pressed={typeFilter === kind}
            onClick={() => setTypeFilter(kind)}
          >
            {kind}
          </button>
        ))}
        {labels.map((label) => (
          <button
            key={label}
            type="button"
            className={labelFilter === label ? 'is-active' : ''}
            aria-pressed={labelFilter === label}
            onClick={() => setLabelFilter((current) => (current === label ? null : label))}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="artifact-layout">
        {gallery ? (
          <div className="artifact-gallery" aria-label="Artifact gallery">
            {filtered.map((artifact) => {
              const kind = artifactKind(artifact.media_type);
              const thumb = thumbs[artifact.sha256];
              return (
                <button
                  key={artifact.sha256}
                  type="button"
                  className={`artifact-tile${detail?.artifact.sha256 === artifact.sha256 ? ' is-active' : ''}`}
                  onClick={() => void open(artifact)}
                >
                  {kind === 'image' && thumb ? (
                    <img src={thumb} alt={artifact.label || 'Artifact'} />
                  ) : (
                    <span className="artifact-tile-fallback">
                      <Icon name="artifact" />
                      <small>{kind}</small>
                    </span>
                  )}
                  <strong>{artifact.label || 'Untitled'}</strong>
                </button>
              );
            })}
            {!filtered.length ? <div className="surface-empty">No matching artifacts.</div> : null}
          </div>
        ) : (
          <div className="artifact-list">
            {filtered.map((artifact) => (
              <div
                className={`artifact-row${detail?.artifact.sha256 === artifact.sha256 ? ' is-active' : ''}`}
                key={artifact.sha256}
              >
                <input
                  type="checkbox"
                  aria-label={`Select ${artifact.label || artifact.sha256}`}
                  checked={selected.includes(artifact.sha256)}
                  onChange={(event) =>
                    setSelected((current) =>
                      event.target.checked
                        ? [...current, artifact.sha256]
                        : current.filter((sha) => sha !== artifact.sha256)
                    )
                  }
                />
                <button type="button" onClick={() => void open(artifact)}>
                  <span className="artifact-icon"><Icon name="artifact" /></span>
                  <span>
                    <strong>{artifact.label || 'Untitled artifact'}</strong>
                    <small>{artifact.source || 'unknown source'} · {artifact.media_type || 'binary'}</small>
                    <code>{artifact.sha256.slice(0, 16)}</code>
                  </span>
                </button>
              </div>
            ))}
            {!filtered.length ? <div className="surface-empty">No matching artifacts.</div> : null}
          </div>
        )}
        <div className="artifact-preview">
          {detail ? (
            <>
              <div className="artifact-preview-head">
                <div>
                  <strong>{detail.artifact.label || 'Artifact'}</strong>
                  <small>{detail.artifact.source || 'unknown source'}</small>
                </div>
                <div className="artifact-preview-actions">
                  <button type="button" onClick={() => void exportOne(detail.artifact.sha256)}>
                    Export
                  </button>
                  <button
                    type="button"
                    className="danger-text"
                    onClick={() => requestDelete([detail.artifact.sha256])}
                  >
                    <Icon name="trash" />
                    Delete
                  </button>
                </div>
              </div>
              {detail.kind === 'image' && detail.data_url ? (
                <img src={detail.data_url} alt={detail.artifact.label || 'Artifact preview'} />
              ) : detail.kind === 'text' ? (
                <pre>{detail.text}</pre>
              ) : (
                <pre>{detail.hex_preview || 'Binary preview unavailable'}</pre>
              )}
            </>
          ) : (
            <div className="surface-empty">Select an artifact to inspect its bounded preview.</div>
          )}
        </div>
      </div>
      {error ? <div className="surface-error"><Icon name="warning" />{error}</div> : null}
      {status ? <div className="surface-status" role="status">{status}</div> : null}
      {pendingDelete.length ? (
        <div
          className="dialog-backdrop"
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              closeDeleteConfirmation();
            } else if (event.key === 'Tab') {
              if (event.shiftKey && document.activeElement === cancelDeleteButton.current) {
                event.preventDefault();
                confirmDeleteButton.current?.focus();
              } else if (!event.shiftKey && document.activeElement === confirmDeleteButton.current) {
                event.preventDefault();
                cancelDeleteButton.current?.focus();
              }
            }
          }}
        >
          <div
            className="artifact-delete-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="artifact-delete-title"
            aria-describedby="artifact-delete-description"
          >
            <header>
              <span className="artifact-delete-mark"><Icon name="trash" /></span>
              <div>
                <h2 id="artifact-delete-title">{deleteLabel}</h2>
                <span>Permanent local deletion</span>
              </div>
            </header>
            <p id="artifact-delete-description">
              This removes the selected content from the local artifact store. It cannot be undone.
            </p>
            <footer>
              <button
                type="button"
                aria-label="Cancel deletion"
                disabled={deleting}
                ref={cancelDeleteButton}
                onClick={closeDeleteConfirmation}
              >
                Cancel
              </button>
              <button
                type="button"
                className="confirm-danger"
                aria-label="Confirm delete"
                disabled={deleting}
                ref={confirmDeleteButton}
                onClick={() => void confirmDelete()}
              >
                {deleting ? 'Deleting…' : 'Delete'}
              </button>
            </footer>
          </div>
        </div>
      ) : null}
    </section>
  );
}
