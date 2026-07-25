import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { DesktopMethod, OptimusTransport } from '../../ipc/contracts';
import { CapabilitiesPage } from './CapabilitiesPage';

function transport(): OptimusTransport {
  const invoke = vi.fn(async (method: DesktopMethod) => {
    switch (method) {
      case 'providers_catalog':
        return {
          providers: [
            {
              id: 'offline',
              connect: 'connected',
              supports_tools: true,
              supports_vision: false,
              supports_streaming: false,
              default_model: 'offline-scripted',
              remote: false,
            },
          ],
        };
      case 'mcp_tools':
        return {
          tools: [{ id: 'mcp_echo', description: 'echo', available: false }],
          count: 1,
        };
      case 'providers_route_preview':
        return {
          ok: true,
          decision: { provider: 'offline', model: 'offline-scripted', fallback_from: 'codex' },
        };
      default:
        return {};
    }
  });
  return {
    kind: 'fixture',
    invoke,
    chat: vi.fn(),
    windowAction: vi.fn(),
    pickFolder: vi.fn(),
    openPath: vi.fn(),
  } as unknown as OptimusTransport;
}

describe('CapabilitiesPage', () => {
  it('shows provider catalog and mcp tools from IPC', async () => {
    const user = userEvent.setup();
    const t = transport();
    render(
      <CapabilitiesPage
        doctor={null}
        approvals={[]}
        campaigns={[]}
        transport={t}
        onOpenExecution={() => undefined}
      />
    );
    expect(await screen.findByText(/offline/i)).toBeInTheDocument();
    expect(screen.getByText(/mcp_echo/i)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Preview failover/i }));
    expect(await screen.findByText(/fallback from codex/i)).toBeInTheDocument();
  });
});
