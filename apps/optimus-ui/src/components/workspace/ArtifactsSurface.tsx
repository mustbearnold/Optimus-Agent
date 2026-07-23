import { useCallback, useEffect, useMemo, useState } from 'react';
import type { ArtifactDetail, ArtifactRecord, OptimusTransport } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

export function ArtifactsSurface({
  transport,
  active,
  standalone = false,
}: {
  transport: OptimusTransport;
  active: boolean;
  standalone?: boolean;
}) {
  const [artifacts, setArtifacts] = useState<ArtifactRecord[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [detail, setDetail] = useState<ArtifactDetail | null>(null);
  const [query, setQuery] = useState('');
  const [error, setError] = useState('');

  const load = useCallback(async () => {
    setError('');
    try {
      const result = await transport.invoke<{ artifacts?: ArtifactRecord[] }>('artifacts_list');
      setArtifacts(result.artifacts || []);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }, [transport]);

  useEffect(() => {
    if (active) void load();
  }, [active, load]);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return artifacts;
    return artifacts.filter((artifact) =>
      `${artifact.label} ${artifact.source} ${artifact.sha256}`.toLowerCase().includes(needle)
    );
  }, [artifacts, query]);

  const open = async (artifact: ArtifactRecord) => {
    try {
      setDetail(await transport.invoke<ArtifactDetail>('artifacts_get', { sha256: artifact.sha256 }));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const removeMany = async (sha256s: string[]) => {
    if (!sha256s.length) return;
    await transport.invoke('artifacts_delete_many', { sha256s });
    setSelected([]);
    if (detail && sha256s.includes(detail.artifact.sha256)) setDetail(null);
    await load();
  };

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
          className="danger-text"
          disabled={!selected.length}
          onClick={() => void removeMany(selected)}
        >
          <Icon name="trash" />
          Delete {selected.length || ''}
        </button>
        <button type="button" aria-label="Refresh artifacts" onClick={() => void load()}>
          <Icon name="refresh" />
        </button>
      </div>
      <div className="artifact-layout">
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
          {error ? <div className="surface-error"><Icon name="warning" />{error}</div> : null}
        </div>
        <div className="artifact-preview">
          {detail ? (
            <>
              <div className="artifact-preview-head">
                <div>
                  <strong>{detail.artifact.label || 'Artifact'}</strong>
                  <small>{detail.artifact.source || 'unknown source'}</small>
                </div>
                <button
                  type="button"
                  className="danger-text"
                  onClick={() => void removeMany([detail.artifact.sha256])}
                >
                  <Icon name="trash" />
                  Delete
                </button>
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
    </section>
  );
}
