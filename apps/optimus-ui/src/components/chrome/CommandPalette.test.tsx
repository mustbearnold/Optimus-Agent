import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { DesktopMethod, OptimusTransport } from '../../ipc/contracts';
import { CommandPalette } from './CommandPalette';

describe('CommandPalette', () => {
  it('loads surface commands and runs selection', async () => {
    const user = userEvent.setup();
    const onRun = vi.fn();
    const invoke = vi.fn(async (method: DesktopMethod) => {
      if (method === 'commands_list') {
        return {
          commands: [
            { id: 'skills', name: 'skills', description: 'Open skills console' },
            { id: 'doctor', name: 'doctor', description: 'Run doctor' },
          ],
        };
      }
      throw new Error(method);
    });
    const transport = {
      kind: 'fixture',
      invoke,
      chat: vi.fn(),
      windowAction: vi.fn(),
      pickFolder: vi.fn(),
      openPath: vi.fn(),
    } as unknown as OptimusTransport;

    render(
      <CommandPalette open transport={transport} onClose={vi.fn()} onRun={onRun} />
    );
    expect(await screen.findByText('/skills')).toBeInTheDocument();
    await user.click(screen.getByText('/skills'));
    await waitFor(() => expect(onRun).toHaveBeenCalledWith('skills'));
  });
});
