import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { ArtifactRecord, DesktopMethod, OptimusTransport } from '../../ipc/contracts';
import { createOptimusClient } from '../../ipc/client';
import { ArtifactsSurface } from './ArtifactsSurface';

const artifacts: ArtifactRecord[] = [
  {
    sha256: 'a'.repeat(64),
    label: 'First artifact',
    source: 'test',
    media_type: 'text/plain',
  },
  {
    sha256: 'b'.repeat(64),
    label: 'Second artifact',
    source: 'test',
    media_type: 'text/plain',
  },
];

function createTransport() {
  const invoke = vi.fn(async (method: DesktopMethod, params?: Record<string, unknown>) => {
    if (method === 'artifacts_list') return { artifacts };
    if (method === 'artifacts_delete_many') {
      return { ok: true, deleted: artifacts.map((artifact) => artifact.sha256), failed: [] };
    }
    if (method === 'artifacts_export') {
      return { ok: true, path: `/tmp/${params?.sha256}.txt` };
    }
    if (method === 'artifacts_export_zip') {
      return { ok: true, path: '/tmp/batch.zip', count: 2 };
    }
    if (method === 'artifacts_get') {
      return {
        artifact: artifacts[0],
        kind: 'text',
        text: 'hello',
      };
    }
    throw new Error(`unexpected method: ${method}`);
  });

  return {
    invoke,
    client: createOptimusClient({
      kind: 'fixture',
      invoke,
      chat: vi.fn(),
      windowAction: vi.fn(),
      pickFolder: vi.fn(),
      openPath: vi.fn(),
    } as unknown as OptimusTransport),
  };
}

describe('ArtifactsSurface deletion', () => {
  it('requires confirmation, cancels safely, and deletes the exact selected hashes', async () => {
    const user = userEvent.setup();
    const { invoke, client } = createTransport();
    render(<ArtifactsSurface client={client} active />);

    await user.click(await screen.findByLabelText('Select First artifact'));
    await user.click(screen.getByLabelText('Select Second artifact'));
    await user.click(screen.getByRole('button', { name: 'Delete 2' }));

    expect(screen.getByRole('alertdialog', { name: 'Delete 2 artifacts?' })).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith('artifacts_delete_many', expect.anything());

    await user.click(screen.getByRole('button', { name: 'Cancel deletion' }));
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith('artifacts_delete_many', expect.anything());

    await user.click(screen.getByRole('button', { name: 'Delete 2' }));
    await user.click(screen.getByRole('button', { name: 'Confirm delete' }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('artifacts_delete_many', {
        sha256s: artifacts.map((artifact) => artifact.sha256),
      })
    );
  });

  it('filters by type chip and exports a zip of the selection', async () => {
    const user = userEvent.setup();
    const { invoke, client } = createTransport();
    render(<ArtifactsSurface client={client} active />);

    await screen.findByLabelText('Select First artifact');
    await user.click(screen.getByRole('button', { name: 'text', pressed: false }));
    expect(screen.getByLabelText('Select First artifact')).toBeInTheDocument();

    await user.click(screen.getByLabelText('Select First artifact'));
    await user.click(screen.getByLabelText('Select Second artifact'));
    await user.click(screen.getByRole('button', { name: /Zip 2/ }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('artifacts_export_zip', {
        sha256s: artifacts.map((artifact) => artifact.sha256),
      })
    );
  });

  it('renders with a null-transport client without crashing (bootstrap window)', () => {
    // Regression: the packaged renderer mounts with transport=null while the
    // spec-015 A3 broker ticket is awaited; the mount-time artifacts_list
    // load must surface NoTransportError into the error slot, not throw.
    render(<ArtifactsSurface client={createOptimusClient(null)} active />);

    expect(screen.getByRole('region', { name: 'Artifacts' })).toBeInTheDocument();
  });
});
