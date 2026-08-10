import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { OptimusTransport } from '../../ipc/contracts';
import { createOptimusClient, type OptimusClient } from '../../ipc/client';
import { DeveloperAccessPanel } from './DeveloperAccessPanel';

const access = {
  enabled: true,
  scope: { kind: 'selected_repository' as const, root: '/workspace/optimus-agent' },
  scope_label: 'Selected repository',
  roots: ['/workspace/optimus-agent'],
  capabilities: {
    workspace_files: true,
    terminal_execution: true,
    process_management: true,
    package_installation: true,
    network_access: true,
    external_services: false,
    production_systems: false,
    secrets: false,
  },
  pause_before_destructive: true,
  checkpoint_on_mutation: true,
};

function transportFor(invoke: OptimusTransport['invoke']): OptimusClient {
  return createOptimusClient({
    kind: 'fixture',
    invoke,
    chat: vi.fn(),
    chatApprovalResolve: vi.fn(),
    windowAction: vi.fn(),
    pickFolder: vi.fn(),
    openPath: vi.fn(),
  } as unknown as OptimusTransport);
}

describe('DeveloperAccessPanel self-development controls', () => {
  it('builds and launches the selected repository through the supervisor', async () => {
    const user = userEvent.setup();
    const invoke = vi.fn(async (method: string) => {
      if (method === 'developer_access_get') {
        return { developer_access: access, supervisor: { status: 'idle', healthy: false } };
      }
      if (method === 'developer_supervisor_build_launch') {
        return { status: 'healthy', healthy: true, pid: 42, port: 17866 };
      }
      return {};
    }) as unknown as OptimusTransport['invoke'];

    render(
      <DeveloperAccessPanel
        client={transportFor(invoke)}
        projects={[{ id: 'optimus', name: 'Optimus Agent', rootPaths: ['/workspace/optimus-agent'], primaryRoot: '/workspace/optimus-agent' }]}
        value={access}
        onValue={vi.fn()}
      />,
    );

    const launch = await screen.findByRole('button', { name: 'Build + launch development desktop' });
    await user.click(launch);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('developer_supervisor_build_launch', {
        workspace: '/workspace/optimus-agent',
        port: 17866,
        surface: 'desktop',
      });
    });
  });

  it('passes the selected session to the separate development desktop', async () => {
    const user = userEvent.setup();
    const invoke = vi.fn(async (method: string) => {
      if (method === 'developer_access_get') {
        return { developer_access: access, supervisor: { status: 'idle', healthy: false } };
      }
      if (method === 'developer_supervisor_build_launch') {
        return { status: 'healthy', healthy: true, pid: 43, port: 17866, handoff_session_id: 'session-42' };
      }
      return {};
    }) as unknown as OptimusTransport['invoke'];

    render(
      <DeveloperAccessPanel
        client={transportFor(invoke)}
        projects={[{ id: 'optimus', name: 'Optimus Agent', rootPaths: ['/workspace/optimus-agent'], primaryRoot: '/workspace/optimus-agent' }]}
        sessionId="session-42"
        value={access}
        onValue={vi.fn()}
      />,
    );

    await user.click(await screen.findByRole('button', { name: 'Build + launch development desktop' }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('developer_supervisor_build_launch', {
        workspace: '/workspace/optimus-agent',
        port: 17866,
        surface: 'desktop',
        session_id: 'session-42',
      });
    });
    expect(await screen.findByText(/selected session handed off/)).toBeInTheDocument();
  });
});
