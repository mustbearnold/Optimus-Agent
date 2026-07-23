import type { Doctor } from '../../ipc/contracts';

export function TruthStrip({
  doctor,
  transport,
  runLabel,
}: {
  doctor: Doctor | null;
  transport: string;
  runLabel: string;
}) {
  return (
    <footer className="truth-strip" aria-label="System status">
      <span>
        scope <strong>{doctor?.work_isolation || 'shared'}</strong>
      </span>
      <span>
        transport <strong>{transport}</strong>
      </span>
      <span>
        run <strong>{runLabel}</strong>
      </span>
      <span className="truth-grow">
        model <strong>{doctor?.streaming ? 'ready' : 'offline'}</strong>
      </span>
      <span className="hide-compact">
        browser <strong>{doctor?.browser || '—'}</strong>
      </span>
      <span className="hide-compact">
        v<strong>{doctor?.version || '—'}</strong>
      </span>
    </footer>
  );
}
