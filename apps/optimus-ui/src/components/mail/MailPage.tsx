import { useCallback, useEffect, useState } from 'react';
import { useAlive } from '../../hooks/useAlive';
import type { OptimusTransport } from '../../ipc/contracts';
import { Icon } from '../chrome/Icon';

type InboxMessage = {
  id: string;
  channel: string;
  text: string;
  provider?: string;
  session_id?: string | null;
  received_unix?: number;
};

type OutboxReceipt = {
  message_id: string;
  outbound: {
    id: string;
    in_reply_to: string;
    channel: string;
    text: string;
    status: string;
    sent_unix?: number;
  };
  terminal_status: string;
  terminal_reason?: string | null;
  delivered_unix?: number | null;
  ambiguous_send: boolean;
};

type GatewayStatus = {
  inbox_pending?: number;
  inbox_claimed?: number;
  outbox_total?: number;
  ambiguous_sends?: number;
  note?: string;
};

type Tab = 'inbox' | 'outbox' | 'ambiguous';

export function MailPage({ transport }: { transport: OptimusTransport }) {
  const [tab, setTab] = useState<Tab>('inbox');
  const [status, setStatus] = useState<GatewayStatus | null>(null);
  const [inbox, setInbox] = useState<InboxMessage[]>([]);
  const [outbox, setOutbox] = useState<OutboxReceipt[]>([]);
  const [ambiguous, setAmbiguous] = useState<OutboxReceipt[]>([]);
  const [telegram, setTelegram] = useState<Record<string, unknown> | null>(null);
  const [selectedId, setSelectedId] = useState('');
  const [draft, setDraft] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const alive = useAlive();

  const load = useCallback(async () => {
    setError('');
    try {
      const [st, inb, out, amb, tg] = await Promise.all([
        transport.invoke<{ status?: GatewayStatus }>('gateway_status'),
        transport.invoke<{ messages?: InboxMessage[] }>('gateway_inbox'),
        transport.invoke<{ messages?: OutboxReceipt[] }>('gateway_outbox', { limit: 50 }),
        transport.invoke<{ messages?: OutboxReceipt[] }>('gateway_ambiguous'),
        transport.invoke<Record<string, unknown>>('gateway_telegram_status'),
      ]);
      if (!alive()) return;
      setStatus(st.status || null);
      setInbox(inb.messages || []);
      setOutbox(out.messages || []);
      setAmbiguous(amb.messages || []);
      setTelegram(tg);
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [alive, transport]);

  useEffect(() => {
    void load();
  }, [load]);

  const list =
    tab === 'inbox' ? inbox : tab === 'outbox' ? outbox : ambiguous;
  const selected =
    tab === 'inbox'
      ? inbox.find((m) => m.id === selectedId) || inbox[0] || null
      : (tab === 'outbox' ? outbox : ambiguous).find((m) => m.message_id === selectedId)
        || (tab === 'outbox' ? outbox : ambiguous)[0]
        || null;

  const enqueueLocal = async () => {
    const text = draft.trim();
    if (!text) return;
    setBusy(true);
    setError('');
    try {
      await transport.invoke('gateway_enqueue', { text, channel: 'local' });
      if (!alive()) return;
      setDraft('');
      await load();
      if (!alive()) return;
      setTab('inbox');
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (alive()) setBusy(false);
    }
  };

  const ack = async (row: OutboxReceipt) => {
    setBusy(true);
    setError('');
    try {
      await transport.invoke('gateway_ack_delivery', {
        message_id: row.message_id,
        outbound_id: row.outbound.id,
      });
      await load();
    } catch (e) {
      if (!alive()) return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (alive()) setBusy(false);
    }
  };

  return (
    <main className="mail-page" aria-label="Messaging">
      <header className="mail-toolbar">
        <div className="mail-title">
          <Icon name="mail" />
          <div>
            <h1>Messaging</h1>
            <span>
              inbox {status?.inbox_pending ?? '—'} · outbox {status?.outbox_total ?? '—'} · ambiguous{' '}
              {status?.ambiguous_sends ?? '—'}
            </span>
          </div>
        </div>
        <div className="mail-toolbar-actions">
          <button type="button" onClick={() => void load()} disabled={busy} aria-label="Refresh messaging">
            <Icon name="refresh" />
          </button>
        </div>
      </header>

      <div className="console-tabs" role="tablist" aria-label="Messaging sections">
        {(['inbox', 'outbox', 'ambiguous'] as Tab[]).map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? 'is-active' : ''}
            onClick={() => {
              setTab(id);
              setSelectedId('');
            }}
          >
            {id === 'ambiguous' ? 'Needs review' : id[0]!.toUpperCase() + id.slice(1)}
            {id === 'ambiguous' && (status?.ambiguous_sends || 0) > 0
              ? ` (${status?.ambiguous_sends})`
              : ''}
          </button>
        ))}
      </div>

      <div className="mail-enqueue">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Add a local test message…"
          aria-label="Inbound message text"
          disabled={busy}
        />
        <button type="button" onClick={() => void enqueueLocal()} disabled={busy || !draft.trim()}>
          Add to inbox
        </button>
      </div>

      <div className="mail-layout">
        <section className="mail-list" aria-label={`${tab} messages`}>
          {tab === 'inbox'
            ? inbox.map((message) => (
                <button
                  type="button"
                  key={message.id}
                  className={`mail-list-item${selected && 'id' in selected && selected.id === message.id ? ' is-selected' : ''}`}
                  onClick={() => setSelectedId(message.id)}
                >
                  <span className="mail-list-meta">
                    <strong>{message.channel}</strong>
                    <span>{message.provider || '—'}</span>
                  </span>
                  <span className="mail-list-subject">
                    <strong>{message.id.slice(0, 8)}…</strong>
                  </span>
                  <span className="mail-list-preview">{message.text}</span>
                </button>
              ))
            : (list as OutboxReceipt[]).map((row) => (
                <button
                  type="button"
                  key={row.message_id}
                  className={`mail-list-item${
                    selected && 'message_id' in selected && selected.message_id === row.message_id
                      ? ' is-selected'
                      : ''
                  }${row.ambiguous_send ? ' is-unread' : ''}`}
                  onClick={() => setSelectedId(row.message_id)}
                >
                  <span className="mail-list-meta">
                    <strong>{row.outbound.channel}</strong>
                    <span>
                      {row.ambiguous_send
                        ? 'Needs review'
                        : row.delivered_unix
                          ? 'Delivered (local)'
                          : row.terminal_status || 'Pending'}
                    </span>
                  </span>
                  <span className="mail-list-subject">
                    <strong>{row.message_id.slice(0, 8)}…</strong>
                  </span>
                  <span className="mail-list-preview">{row.outbound.text}</span>
                </button>
              ))}
          {!list.length ? (
            <div className="mail-empty surface-empty">
              <Icon name="mail" />
              <p>No {tab} messages.</p>
            </div>
          ) : null}
        </section>

        {selected && tab === 'inbox' && 'text' in selected ? (
          <article className="mail-reader" aria-label="Inbound detail">
            <header className="mail-reader-header">
              <span className="mail-reader-context">{selected.channel}</span>
              <h2>{selected.id}</h2>
              <div className="mail-sender">
                <span className="mail-sender-mark" aria-hidden="true">
                  in
                </span>
                <div>
                  <strong>Inbound</strong>
                  <span>{selected.provider || 'provider unknown'}</span>
                </div>
              </div>
            </header>
            <div className="mail-body">
              <p>{selected.text}</p>
              <dl className="mail-facts">
                <div>
                  <dt>Session</dt>
                  <dd>{selected.session_id || '—'}</dd>
                </div>
                <div>
                  <dt>Received</dt>
                  <dd>{selected.received_unix ?? '—'}</dd>
                </div>
              </dl>
            </div>
          </article>
        ) : selected && 'outbound' in selected ? (
          <article className="mail-reader" aria-label="Outbox detail">
            <header className="mail-reader-header">
              <span className="mail-reader-context">{selected.outbound.channel}</span>
              <h2>{selected.message_id}</h2>
              <div className="mail-sender">
                <span className="mail-sender-mark" aria-hidden="true">
                  out
                </span>
                <div>
                  <strong>{selected.ambiguous_send ? 'Needs review' : 'Outbound'}</strong>
                  <span>
                    {selected.delivered_unix
                      ? 'Local delivery recorded'
                      : 'No local delivery recorded yet'}
                  </span>
                </div>
              </div>
            </header>
            <div className="mail-body">
              <p>{selected.outbound.text}</p>
              <dl className="mail-facts">
                <div>
                  <dt>Outbound id</dt>
                  <dd>{selected.outbound.id}</dd>
                </div>
                <div>
                  <dt>Terminal</dt>
                  <dd>
                    {selected.terminal_status}
                    {selected.terminal_reason ? ` / ${selected.terminal_reason}` : ''}
                  </dd>
                </div>
                <div>
                  <dt>Reply to</dt>
                  <dd>{selected.outbound.in_reply_to}</dd>
                </div>
              </dl>
              {selected.ambiguous_send ? (
                <button type="button" disabled={busy} onClick={() => void ack(selected)}>
                  Mark as delivered locally
                </button>
              ) : null}
            </div>
          </article>
        ) : (
          <div className="mail-empty">
            <Icon name="mail" />
            <p>Select a message to read it.</p>
          </div>
        )}
      </div>

      {telegram?.enabled ? (
        <footer className="mail-telegram-status" aria-label="Telegram status">
          <strong>Telegram</strong>
          <span>{telegram.token_present ? 'Connected' : 'Token missing'}</span>
        </footer>
      ) : null}
      {error ? <div className="surface-error">{error}</div> : null}
    </main>
  );
}
