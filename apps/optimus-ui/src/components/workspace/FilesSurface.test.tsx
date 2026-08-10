import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { DesktopMethod, FsEntry, OptimusTransport } from '../../ipc/contracts';
import { createOptimusClient, type OptimusClient } from '../../ipc/client';
import { FilesSurface } from './FilesSurface';

const homeEntries: FsEntry[] = [
  { path: '/home/test/docs', name: 'docs', kind: 'dir', is_dir: true, size: 0 },
  { path: '/home/test/notes.md', name: 'notes.md', kind: 'file', is_dir: false, size: 2048 },
];

const docsEntries: FsEntry[] = [
  { path: '/home/test/docs/spec.md', name: 'spec.md', kind: 'file', is_dir: false, size: 512 },
];

describe('FilesSurface parity (06-preview-browser.spec.js)', () => {
  it('lists the home directory over fs_list and opens a file preview', async () => {
    const list = vi.fn(async () => homeEntries);
    const read = vi.fn(async () => ({
      path: '/home/test/notes.md',
      content: '# Notes',
      truncated: false,
    }));
    render(<FilesSurface client={transportWithFs(list, read)} active />);

    const tree = await screen.findByRole('tree', { name: 'Directory contents' });
    await waitFor(() => expect(list).toHaveBeenCalledWith(''));
    expect(within(tree).getByRole('treeitem', { name: /docs/ })).toBeInTheDocument();

    fireEvent.click(within(tree).getByRole('treeitem', { name: /notes\.md/ }));
    await waitFor(() => expect(read).toHaveBeenCalled());
    expect(screen.getByText('# Notes')).toBeInTheDocument();
  });

  it('navigates into a folder and back via the breadcrumbs', async () => {
    const list = vi.fn(async (path: string) => (path === '' ? homeEntries : docsEntries));
    const read = vi.fn(async () => ({ path: 'x', content: '', truncated: false }));
    render(<FilesSurface client={transportWithFs(list, read)} active />);

    fireEvent.click(await screen.findByRole('treeitem', { name: /docs/ }));
    await waitFor(() => expect(list).toHaveBeenCalledWith('/home/test/docs'));
    expect(await screen.findByRole('treeitem', { name: /spec\.md/ })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Home' }));
    await waitFor(() => expect(list).toHaveBeenCalledWith(''));
  });

  it('renders with a null-transport client without crashing (bootstrap window)', () => {
    // Regression: the packaged renderer mounts with transport=null while the
    // spec-015 A3 broker ticket is awaited. The mount-time fs.list must not
    // throw; the client's fs API returns idle no-ops instead.
    render(<FilesSurface client={createOptimusClient(null)} active />);

    expect(screen.getByLabelText('Files')).toBeInTheDocument();
  });
});

function transportWithFs(
  list: (path: string) => Promise<FsEntry[]>,
  read: (path: string) => Promise<{ path: string; content: string; truncated: boolean }>
): OptimusClient {
  return createOptimusClient({
    kind: 'tauri',
    invoke: async (method: DesktopMethod, params?: Record<string, unknown>) => {
      if (method === 'fs_list') return { entries: list(String(params?.path ?? '')) } as never;
      if (method === 'fs_read') return read(String(params?.path ?? '')) as never;
      return {} as never;
    },
    chat: () => {
      throw new Error('not used');
    },
    chatApprovalResolve: () => {
      throw new Error('not used');
    },
    windowAction: async () => ({}),
    pickFolder: async () => ({ ok: false }),
    openPath: async () => ({}),
  } satisfies OptimusTransport);
}
