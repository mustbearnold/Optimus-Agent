import { useEffect, useMemo, useState } from 'react';
import type { OptimusTransport } from '../../ipc/contracts';

export type PaletteCommand = {
  id: string;
  name: string;
  description: string;
  surface?: string;
};

export function CommandPalette({
  open,
  transport,
  onClose,
  onRun,
}: {
  open: boolean;
  transport: OptimusTransport;
  onClose: () => void;
  onRun: (commandId: string) => void;
}) {
  const [commands, setCommands] = useState<PaletteCommand[]>([]);
  const [query, setQuery] = useState('');
  const [error, setError] = useState('');

  useEffect(() => {
    if (!open) return;
    setQuery('');
    setError('');
    void transport
      .invoke<{ commands?: PaletteCommand[] }>('commands_list', { surface: 'desktop' })
      .then((r) => setCommands(r.commands || []))
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }, [open, transport]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (c) =>
        c.id.toLowerCase().includes(q) ||
        c.name.toLowerCase().includes(q) ||
        c.description.toLowerCase().includes(q)
    );
  }, [commands, query]);

  if (!open) return null;

  return (
    <div
      className="dialog-backdrop"
      onClick={onClose}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
    >
      <div
        className="command-palette"
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          type="search"
          placeholder="Type a command…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label="Filter commands"
        />
        <ul>
          {filtered.map((cmd) => (
            <li key={cmd.id}>
              <button
                type="button"
                onClick={() => {
                  onRun(cmd.id);
                  onClose();
                }}
              >
                <strong>/{cmd.name}</strong>
                <span>{cmd.description}</span>
              </button>
            </li>
          ))}
          {!filtered.length ? <li className="surface-empty">No matching commands.</li> : null}
        </ul>
        <p className="panel-muted">Surface catalog only — not a tool registry.</p>
        {error ? <div className="surface-error">{error}</div> : null}
      </div>
    </div>
  );
}
