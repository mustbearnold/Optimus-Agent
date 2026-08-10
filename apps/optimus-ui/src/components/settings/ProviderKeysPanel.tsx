import { useCallback, useEffect, useState } from 'react';
import type { OptimusClient, ProviderKeyStatus } from '../../ipc/client';

export type { ProviderKeyStatus } from '../../ipc/client';

type Props = {
  client: OptimusClient;
  active: boolean;
};

function sourceNote(status: ProviderKeyStatus) {
  if (status.error) return status.error;
  if (!status.present) {
    return `No key saved. Paste one here, or set ${status.env_var} before launching.`;
  }
  if (status.source === 'environment') {
    return `Using ${status.env_var} from the launch environment. Saving a key here replaces it.`;
  }
  return `Saved key ${status.hint || ''} is used for ${status.label} requests.`;
}

/**
 * Key-based provider credentials. Codex is deliberately absent: it signs in
 * through OAuth and is reported by the Credentials rows above.
 *
 * The key is write-only from the interface. The host returns presence, origin,
 * and a masked tail, so a saved key can never be read back out of the window.
 */
export function ProviderKeysPanel({ client, active }: Props) {
  const [providers, setProviders] = useState<ProviderKeyStatus[]>([]);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState('');
  const [message, setMessage] = useState('');
  const [failed, setFailed] = useState(false);

  const load = useCallback(async () => {
    try {
      setProviders(await client.providers.keysStatus());
    } catch (error) {
      setFailed(true);
      setMessage(error instanceof Error ? error.message : String(error));
    }
  }, [client]);

  useEffect(() => {
    if (!active) return;
    void load();
  }, [active, load]);

  const save = async (provider: ProviderKeyStatus) => {
    const apiKey = (drafts[provider.provider] || '').trim();
    if (!apiKey) {
      setFailed(true);
      setMessage(`Enter a ${provider.label} API key first.`);
      return;
    }
    setBusy(provider.provider);
    setFailed(false);
    try {
      const result = await client.providers.keySet(provider.provider, apiKey);
      setProviders(result);
      // Drop the plaintext from component state as soon as the host has it.
      setDrafts((current) => ({ ...current, [provider.provider]: '' }));
      setMessage(`${provider.label} API key saved.`);
    } catch (error) {
      setFailed(true);
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy('');
    }
  };

  const remove = async (provider: ProviderKeyStatus) => {
    setBusy(provider.provider);
    setFailed(false);
    try {
      const result = await client.providers.keyClear(provider.provider);
      setProviders(result);
      setMessage(`${provider.label} API key removed.`);
    } catch (error) {
      setFailed(true);
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy('');
    }
  };

  return (
    <div className="provider-keys-panel">
      {providers.map((provider) => {
        const inputId = `provider-key-${provider.provider}`;
        return (
          <div className="settings-row provider-key-row" key={provider.provider}>
            <div className="settings-row-text">
              <label htmlFor={inputId}>{provider.label} API key</label>
              <p>{sourceNote(provider)}</p>
            </div>
            <div className="provider-key-controls">
              <input
                id={inputId}
                type="password"
                autoComplete="off"
                spellCheck={false}
                placeholder={provider.present ? 'Replace saved key' : 'Paste API key'}
                value={drafts[provider.provider] || ''}
                onChange={(event) =>
                  setDrafts((current) => ({
                    ...current,
                    [provider.provider]: event.target.value,
                  }))
                }
                onKeyDown={(event) => {
                  if (event.key === 'Enter') {
                    event.preventDefault();
                    void save(provider);
                  }
                }}
              />
              <button
                type="button"
                disabled={busy === provider.provider}
                onClick={() => void save(provider)}
              >
                Save
              </button>
              {provider.present && provider.source === 'stored' ? (
                <button
                  type="button"
                  disabled={busy === provider.provider}
                  onClick={() => void remove(provider)}
                >
                  Remove
                </button>
              ) : null}
            </div>
          </div>
        );
      })}
      {message ? (
        <p className={failed ? 'provider-key-message is-error' : 'provider-key-message'} role="status">
          {message}
        </p>
      ) : null}
    </div>
  );
}
