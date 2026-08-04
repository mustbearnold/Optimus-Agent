import { useCallback, useEffect, useState } from 'react';
import type { OptimusTransport } from '../../ipc/contracts';

export type ProviderKeyStatus = {
  provider: string;
  label: string;
  env_var: string;
  present: boolean;
  /** 'stored' | 'environment' | 'none' */
  source: string;
  hint?: string | null;
  base_url?: string | null;
  error?: string | null;
};

type Props = {
  transport: OptimusTransport;
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
export function ProviderKeysPanel({ transport, active }: Props) {
  const [providers, setProviders] = useState<ProviderKeyStatus[]>([]);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState('');
  const [message, setMessage] = useState('');
  const [failed, setFailed] = useState(false);

  const load = useCallback(async () => {
    try {
      const result = await transport.invoke<{ providers?: ProviderKeyStatus[] }>(
        'provider_keys_status'
      );
      setProviders(result.providers || []);
    } catch (error) {
      setFailed(true);
      setMessage(error instanceof Error ? error.message : String(error));
    }
  }, [transport]);

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
      const result = await transport.invoke<{ providers?: ProviderKeyStatus[] }>(
        'provider_key_set',
        { provider: provider.provider, api_key: apiKey }
      );
      setProviders(result.providers || []);
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
      const result = await transport.invoke<{ providers?: ProviderKeyStatus[] }>(
        'provider_key_clear',
        { provider: provider.provider }
      );
      setProviders(result.providers || []);
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
