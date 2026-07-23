import { useCallback, useEffect, useState } from 'react';
import { chatStream, getHost, invoke, windowAction } from './ipc';

type Doctor = {
  version?: string;
  home?: string;
  work_isolation?: string;
  work_isolation_label?: string;
  phase?: string;
  browser?: string;
  preview_browser?: boolean;
};

type SessionMeta = { id: string; title?: string };

export function App() {
  const [doctor, setDoctor] = useState<Doctor | null>(null);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [input, setInput] = useState('');
  const [log, setLog] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hostLabel, setHostLabel] = useState('…');

  const push = useCallback((line: string) => {
    setLog((prev) => [...prev.slice(-200), line]);
  }, []);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const host = await getHost();
      setHostLabel(host.baseUrl);
      const d = await invoke<Doctor>('doctor');
      setDoctor(d);
      const s = await invoke<{ sessions?: SessionMeta[] } | SessionMeta[]>('sessions');
      const list = Array.isArray(s) ? s : s.sessions || [];
      setSessions(list);
      if (!sessionId && list[0]?.id) setSessionId(list[0].id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [sessionId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function onNewSession() {
    try {
      const s = await invoke<SessionMeta>('new_session', {});
      setSessionId(s.id);
      await refresh();
      push(`session ${s.id.slice(0, 8)}…`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function onSend() {
    const text = input.trim();
    if (!text || busy) return;
    setBusy(true);
    setError(null);
    push(`you: ${text}`);
    setInput('');
    try {
      let sid = sessionId;
      if (!sid) {
        const s = await invoke<SessionMeta>('new_session', {});
        sid = s.id;
        setSessionId(sid);
      }
      await chatStream(
        {
          session_id: sid,
          message: text,
          provider: 'offline',
        },
        (ev) => {
          if (ev.type === 'delta') {
            const t = (ev as { text?: string; delta?: string }).text
              ?? (ev as { delta?: string }).delta
              ?? '';
            if (t) push(String(t));
          } else if (ev.type === 'done') {
            push('— done —');
          } else if (ev.type === 'error') {
            push(`error: ${String(ev.error || 'stream')}`);
          } else if (ev.type === 'status') {
            push(`[${String(ev.status || ev.type)}]`);
          }
        }
      );
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="shell">
      <header className="titlebar">
        <div className="brand">Optimus</div>
        <div className="title">
          {doctor?.work_isolation_label || doctor?.work_isolation || 'Workbench'} · React shell
        </div>
        <div className="win">
          <button type="button" onClick={() => void windowAction('minimize')}>
            —
          </button>
          <button type="button" onClick={() => void windowAction('maximize')}>
            □
          </button>
          <button type="button" className="close" onClick={() => void windowAction('close')}>
            ×
          </button>
        </div>
      </header>

      <div className="body">
        <aside className="rail">
          <button type="button" className="primary" onClick={() => void onNewSession()}>
            New session
          </button>
          <button type="button" onClick={() => void refresh()}>
            Refresh
          </button>
          <div className="section">Sessions</div>
          <ul className="session-list">
            {sessions.map((s) => (
              <li key={s.id}>
                <button
                  type="button"
                  className={s.id === sessionId ? 'active' : ''}
                  onClick={() => setSessionId(s.id)}
                >
                  {s.title || s.id.slice(0, 8)}
                </button>
              </li>
            ))}
          </ul>
          <div className="meta">
            <div>host {hostLabel}</div>
            <div>{doctor?.home || '…'}</div>
            <div>
              v{doctor?.version || '—'} · {doctor?.browser || '—'}
            </div>
          </div>
        </aside>

        <main className="main">
          {error ? <div className="error">{error}</div> : null}
          <div className="log">
            {log.length === 0 ? (
              <div className="empty">
                <h2>React shell over Rust host</h2>
                <p>Offline chat stream via frozen IPC. Full Vantage UI ports next.</p>
              </div>
            ) : (
              log.map((line, i) => (
                <div key={i} className="line">
                  {line}
                </div>
              ))
            )}
          </div>
          <div className="composer">
            <textarea
              value={input}
              rows={2}
              placeholder="Message Optimus (offline echo)…"
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  void onSend();
                }
              }}
            />
            <button type="button" className="send" disabled={busy} onClick={() => void onSend()}>
              {busy ? '…' : 'Send'}
            </button>
          </div>
        </main>
      </div>
    </div>
  );
}
