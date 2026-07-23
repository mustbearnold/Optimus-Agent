import { useCallback, useEffect, useState } from 'react';
import type { FsEntry, OptimusTransport } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

export function FilesSurface({
  transport,
  active,
}: {
  transport: OptimusTransport;
  active: boolean;
}) {
  const [path, setPath] = useState('');
  const [entries, setEntries] = useState<FsEntry[]>([]);
  const [preview, setPreview] = useState<{ path: string; content: string; truncated: boolean } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const load = useCallback(async (nextPath: string) => {
    setLoading(true);
    setError('');
    try {
      const result = await transport.invoke<{ entries?: FsEntry[] }>('fs_list', { path: nextPath });
      setEntries(result.entries || []);
      setPath(nextPath);
      setPreview(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [transport]);

  useEffect(() => {
    if (active && !entries.length && !loading && !error) void load('');
  }, [active, entries.length, error, load, loading]);

  const openEntry = async (entry: FsEntry) => {
    const isDirectory = entry.is_dir || /dir/i.test(entry.kind || '');
    if (isDirectory) {
      await load(entry.path);
      return;
    }
    setLoading(true);
    setError('');
    try {
      const result = await transport.invoke<{ path: string; content: string; truncated: boolean }>(
        'fs_read',
        { path: entry.path, max_bytes: 512_000 }
      );
      setPreview(result);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  };

  const parent = path.split(/[\\/]/).slice(0, -1).join('/');
  return (
    <section className="files-surface" aria-label="Files">
      <div className="surface-toolbar">
        <div>
          <Icon name="files" />
          <strong>Files</strong>
        </div>
        <button type="button" disabled={!path} onClick={() => void load(parent)}>
          <Icon name="back" />
          Up
        </button>
        <button type="button" aria-label="Refresh files" onClick={() => void load(path)}>
          <Icon name="refresh" />
        </button>
      </div>
      <nav className="file-crumbs" aria-label="Current file path">
        <button type="button" onClick={() => void load('')}>Home</button>
        {path
          .split(/[\\/]/)
          .filter(Boolean)
          .map((part, index, parts) => (
            <button
              type="button"
              key={`${part}:${index}`}
              onClick={() => void load(parts.slice(0, index + 1).join('/'))}
            >
              <span aria-hidden="true">/</span>{part}
            </button>
          ))}
      </nav>
      <div className="file-layout">
        <div className="file-list" role="tree" aria-label="Directory contents">
          {loading && !entries.length ? <div className="surface-empty">Loading files…</div> : null}
          {!loading && !entries.length && !error ? <div className="surface-empty">This folder is empty.</div> : null}
          {entries.map((entry) => {
            const directory = entry.is_dir || /dir/i.test(entry.kind || '');
            return (
              <button
                type="button"
                role="treeitem"
                className="file-row"
                key={entry.path}
                onClick={() => void openEntry(entry)}
              >
                <Icon name={directory ? 'folder' : 'files'} />
                <span>{entry.name}</span>
                <small>{directory ? 'Folder' : formatBytes(entry.size)}</small>
              </button>
            );
          })}
          {error ? <div className="surface-error"><Icon name="warning" />{error}</div> : null}
        </div>
        <div className="file-preview">
          <div className="file-preview-head">
            <span>{preview?.path || 'Select a file to preview'}</span>
            {preview?.truncated ? <span className="status-badge is-warning">Truncated</span> : null}
          </div>
          <pre>{preview?.content || 'Sandboxed text previews appear here.'}</pre>
        </div>
      </div>
    </section>
  );
}

function formatBytes(size?: number) {
  if (typeof size !== 'number') return 'File';
  if (size < 1024) return `${size} B`;
  return `${Math.round(size / 1024)} KB`;
}
