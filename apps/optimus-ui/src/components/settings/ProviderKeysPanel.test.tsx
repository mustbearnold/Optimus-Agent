import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { OptimusTransport } from '../../ipc/contracts';
import { createOptimusClient, type OptimusClient } from '../../ipc/client';
import { ProviderKeysPanel } from './ProviderKeysPanel';

function transportWith(
  handler: (method: string, params: Record<string, unknown>) => unknown
): OptimusClient {
  const transport = {
    kind: 'fixture',
    invoke: vi.fn(async (method: string, params?: Record<string, unknown>) =>
      handler(method, params || {})
    ),
  } as unknown as OptimusTransport;
  return createOptimusClient(transport);
}

const absent = {
  provider: 'deepseek',
  label: 'DeepSeek',
  env_var: 'DEEPSEEK_API_KEY',
  present: false,
  source: 'none',
  hint: null,
  base_url: null,
};

describe('ProviderKeysPanel', () => {
  it('offers a DeepSeek key field and tells the user no key is configured', async () => {
    const panel = render(
      <ProviderKeysPanel client={transportWith(() => ({ providers: [absent] }))} active />
    );
    await waitFor(() => expect(screen.getByLabelText('DeepSeek API key')).toBeTruthy());
    expect(screen.getByText(/No key saved/)).toBeTruthy();
    // The key must never be a readable plain-text field.
    expect(screen.getByLabelText('DeepSeek API key').getAttribute('type')).toBe('password');
    panel.unmount();
  });

  it('sends a pasted key to the host and clears it from the field', async () => {
    const calls: Array<{ method: string; params: Record<string, unknown> }> = [];
    const client = transportWith((method, params) => {
      calls.push({ method, params });
      if (method === 'provider_key_set') {
        return {
          providers: [
            { ...absent, present: true, source: 'stored', hint: '••••cdef' },
          ],
        };
      }
      return { providers: [absent] };
    });

    const panel = render(<ProviderKeysPanel client={client} active />);
    const field = await screen.findByLabelText('DeepSeek API key');
    await userEvent.type(field, 'sk-deepseek-abcdef');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(screen.getByText(/API key saved/)).toBeTruthy());
    const set = calls.find((call) => call.method === 'provider_key_set');
    expect(set?.params).toMatchObject({ provider: 'deepseek', api_key: 'sk-deepseek-abcdef' });
    // Plaintext must not linger in the field after the host has it.
    expect((screen.getByLabelText('DeepSeek API key') as HTMLInputElement).value).toBe('');
    expect(screen.getByText(/••••cdef/)).toBeTruthy();
    panel.unmount();
  });

  it('refuses to send an empty key', async () => {
    const calls: string[] = [];
    const client = transportWith((method) => {
      calls.push(method);
      return { providers: [absent] };
    });
    const panel = render(<ProviderKeysPanel client={client} active />);
    await screen.findByLabelText('DeepSeek API key');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(screen.getByText(/Enter a DeepSeek API key first/)).toBeTruthy());
    expect(calls).not.toContain('provider_key_set');
    panel.unmount();
  });

  it('reports an environment key as in use and offers to replace it', async () => {
    const fromEnv = { ...absent, present: true, source: 'environment', hint: '••••9999' };
    const panel = render(
      <ProviderKeysPanel client={transportWith(() => ({ providers: [fromEnv] }))} active />
    );
    await waitFor(() => expect(screen.getByText(/DEEPSEEK_API_KEY from the launch environment/)).toBeTruthy());
    // Nothing is stored yet, so there is nothing to remove.
    expect(screen.queryByRole('button', { name: 'Remove' })).toBeNull();
    panel.unmount();
  });

  it('removes a stored key', async () => {
    const stored = { ...absent, present: true, source: 'stored', hint: '••••cdef' };
    const calls: string[] = [];
    const client = transportWith((method) => {
      calls.push(method);
      return { providers: [method === 'provider_key_clear' ? absent : stored] };
    });
    const panel = render(<ProviderKeysPanel client={client} active />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Remove' })).toBeTruthy());
    await userEvent.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(screen.getByText(/API key removed/)).toBeTruthy());
    expect(calls).toContain('provider_key_clear');
    panel.unmount();
  });
});
