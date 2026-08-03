import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import type { ToolActivity } from '../../ipc/contracts';
import { ActivityTimeline } from './ActivityTimeline';

const tools: ToolActivity[] = [
  {
    id: 'read-1',
    runId: 'run-1',
    callId: 'read-1',
    name: 'read_file',
    detail: 'Read apps/optimus-ui/src/app/OptimusApp.tsx',
    status: 'completed',
    durationMs: 42,
  },
  {
    id: 'search-1',
    runId: 'run-1',
    callId: 'search-1',
    name: 'web_search',
    detail: 'Search current workbench references',
    status: 'completed',
    durationMs: 231,
  },
];

describe('ActivityTimeline', () => {
  it('keeps the group summary simple and reveals technical call details on demand', async () => {
    const user = userEvent.setup();
    render(<ActivityTimeline tools={tools} />);

    const group = screen.getByLabelText('Tool activity');
    const groupToggle = screen.getByRole('button', { name: /Read files, searched/i });
    expect(group).toHaveAttribute('data-open', 'false');
    await user.click(groupToggle);
    expect(group).toHaveAttribute('data-open', 'true');

    const readRow = screen.getByRole('button', { name: 'Expand read tool details' });
    expect(readRow).toHaveAttribute('aria-expanded', 'false');
    await user.click(readRow);
    expect(readRow).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('region', { name: 'Technical details for read_file' })).toHaveTextContent('read-1');
  });
});
