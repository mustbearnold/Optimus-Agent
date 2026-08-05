import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { DesktopMethod, OptimusTransport } from '../../ipc/contracts';
import { ConsolesPage } from './ConsolesPage';

function transport(): OptimusTransport {
  const invoke = vi.fn(async (method: DesktopMethod) => {
    if (method === 'skills_list') {
      return {
        skills: [
          {
            id: 's1',
            name: 'demo',
            version: 1,
            status: 'candidate',
            uses: 2,
            success_rate: 1,
            body_preview: 'body',
          },
        ],
      };
    }
    if (method === 'skills_pin') return { id: 's1', status: 'pinned' };
    if (method === 'memory_list') {
      return {
        fence: 'EVIDENCE_DATA_NOT_INSTRUCTION_NOT_CAPABILITY',
        claims: [{ id: 'c1', subject: 'user', predicate: 'likes', object: 'tea' }],
      };
    }
    if (method === 'memory_recall') {
      return { fence: 'EVIDENCE_DATA', purpose: 'inform', current: [], abstained: true };
    }
    if (method === 'packs_state') {
      return {
        loaded: ['core'],
        schema_tokens: 100,
        max_schema_tokens: 8000,
        catalog: [{ id: 'core', summary: 'core pack', schema_tokens: 100, tools: [] }],
      };
    }
    if (method === 'logs_tail') return { lines: ['doctor home=~', 'skills.registry name=demo'], redacted: true };
    throw new Error(method);
  });
  return {
    kind: 'fixture',
    invoke,
    chat: vi.fn(),
    chatApprovalResolve: vi.fn(),
    windowAction: vi.fn(),
    pickFolder: vi.fn(),
    openPath: vi.fn(),
  } as unknown as OptimusTransport;
}

describe('ConsolesPage', () => {
  it('lists skills and can pin', async () => {
    const user = userEvent.setup();
    const t = transport();
    render(<ConsolesPage transport={t} />);
    expect(await screen.findByText(/demo v1/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Pin' }));
    await waitFor(() => expect(t.invoke).toHaveBeenCalledWith('skills_pin', { id: 's1' }));
  });

  it('shows memory fence and claims', async () => {
    const user = userEvent.setup();
    const t = transport();
    render(<ConsolesPage transport={t} />);
    await user.click(screen.getByRole('tab', { name: 'Memory' }));
    expect(await screen.findByText(/EVIDENCE_DATA/)).toBeInTheDocument();
    expect(screen.getByText(/user · likes/)).toBeInTheDocument();
  });
});
