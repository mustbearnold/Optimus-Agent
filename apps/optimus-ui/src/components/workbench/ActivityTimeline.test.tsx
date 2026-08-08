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

  const failedCall = (id: string): ToolActivity => ({
    id,
    runId: 'run-1',
    callId: id,
    name: 'terminal',
    detail: 'policy denied',
    status: 'failed',
    durationMs: 17,
  });

  it('does not report a whole step as failed when only one call failed', () => {
    // Observed live: a step opened with one denied `terminal` call and then read
    // and searched twenty times without a single failure. The collapsed header
    // said "Ran a command, Read files, and searched — failed", which is the only
    // line most steps are ever read by.
    render(<ActivityTimeline tools={[failedCall('term-1'), ...tools]} />);

    const group = screen.getByLabelText('Tool activity');
    expect(group).not.toHaveClass('is-failed');
    expect(group).toHaveClass('has-failure');
    expect(screen.getByRole('button', { name: /1 of 3 failed/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /— failed/ })).toBeNull();
  });

  it('still reports a step as failed when every call in it failed', () => {
    render(<ActivityTimeline tools={[failedCall('term-1'), failedCall('term-2')]} />);

    expect(screen.getByLabelText('Tool activity')).toHaveClass('is-failed');
    expect(screen.getByRole('button', { name: /Ran a command — failed/ })).toBeInTheDocument();
  });

  it('counts the failures so far while other calls are still running', () => {
    render(
      <ActivityTimeline
        tools={[failedCall('term-1'), { ...tools[0]!, status: 'running' }]}
      />
    );

    expect(screen.getByRole('button', { name: /1 failed so far/ })).toBeInTheDocument();
  });

  it('computes the R11 tool-to-tool gap breakdown from timing offsets', async () => {
    const user = userEvent.setup();
    render(
      <ActivityTimeline
        tools={[
          { ...tools[0]!, finishedAtMs: 120 },
          { ...tools[1]!, startedAtMs: 1500 },
        ]}
      />
    );

    // The heading shows the aggregate idle time between the two tools.
    expect(
      screen.getByRole('button', { name: /idle between tools/i })
    ).toHaveTextContent('1.4s idle between tools');

    const groupToggle = screen.getByRole('button', { name: /Read files, searched/i });
    await user.click(groupToggle);

    // Each tool detail carries its own idle-after value…
    const readRow = screen.getByRole('button', { name: 'Expand read tool details' });
    await user.click(readRow);
    expect(
      screen.getByRole('region', { name: 'Technical details for read_file' })
    ).toHaveTextContent('Idle after');

    // …and the list shows the gap chip between items.
    expect(screen.getAllByText('idle 1.4s').length).toBeGreaterThan(0);
  });
});
