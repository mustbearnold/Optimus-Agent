import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { frameCoordinator } from '../../performance/frameCoordinator';
import { Transcript } from './Transcript';

describe('Transcript', () => {
  it('keeps completed assistant replies free of a repeated Optimus label', () => {
    render(
      <Transcript
        messages={[{ id: 'assistant-1', role: 'assistant', content: 'The answer is ready.', status: 'completed', durationMs: 65_400 }]}
        status="completed"
        statusText="Completed"
        onStarter={vi.fn()}
      />
    );

    expect(screen.getByText('The answer is ready.')).toBeInTheDocument();
    expect(screen.getByLabelText('Worked for 1m 05s')).toBeInTheDocument();
    expect(screen.queryByText('Optimus')).not.toBeInTheDocument();
  });

  it('keeps an active assistant status visible without a sender label', () => {
    render(
      <Transcript
        messages={[{ id: 'assistant-1', role: 'assistant', content: 'Still working', status: 'working' }]}
        status="working"
        statusText="Working"
        onStarter={vi.fn()}
      />
    );

    const reply = screen.getByText('Still working').closest('article');
    expect(reply?.querySelector('.message-status')).toHaveTextContent('Working');
    expect(screen.queryByText('Optimus')).not.toBeInTheDocument();
  });

  it('reveals a live assistant response one character per display frame', () => {
    const onStarter = vi.fn();
    const { rerender } = render(
      <Transcript
        messages={[{ id: 'assistant-stream', role: 'assistant', content: '', status: 'working' }]}
        status="working"
        statusText="Working"
        onStarter={onStarter}
      />
    );

    rerender(
      <Transcript
        messages={[{ id: 'assistant-stream', role: 'assistant', content: 'ABC', status: 'working' }]}
        status="working"
        statusText="Working"
        onStarter={onStarter}
      />
    );

    act(() => frameCoordinator.flushNow());
    expect(screen.getByText('A')).toBeInTheDocument();
    act(() => frameCoordinator.flushNow());
    expect(screen.getByText('AB')).toBeInTheDocument();

    rerender(
      <Transcript
        messages={[{ id: 'assistant-stream', role: 'assistant', content: 'ABC', status: 'completed', durationMs: 1_000 }]}
        status="completed"
        statusText="Completed"
        onStarter={onStarter}
      />
    );
    expect(screen.queryByLabelText('Worked for 0m 01s')).not.toBeInTheDocument();

    act(() => frameCoordinator.flushNow());
    expect(screen.getByText('ABC')).toBeInTheDocument();
    expect(screen.getByLabelText('Worked for 0m 01s')).toBeInTheDocument();
  });

  it('keeps tool calls collapsed on their owning assistant turn until clicked', () => {
    const { container } = render(
      <Transcript
        messages={[
          {
            id: 'assistant-tools',
            role: 'assistant',
            content: 'I inspected the project.',
            status: 'completed',
            tools: [
              {
                id: 'read-1',
                runId: 'run-1',
                callId: 'read-1',
                name: 'read_file',
                detail: 'AGENTS.md',
                status: 'completed',
              },
              {
                id: 'command-1',
                runId: 'run-1',
                callId: 'command-1',
                name: 'run_command',
                detail: 'npm test',
                status: 'completed',
              },
            ],
          },
        ]}
        status="completed"
        statusText="Completed"
        onStarter={vi.fn()}
      />
    );

    const details = container.querySelector('details.activity-timeline');
    expect(details).not.toHaveAttribute('open');
    expect(screen.getByText('Read files, ran a command')).toBeInTheDocument();

    fireEvent.click(screen.getByText('Read files, ran a command'));

    expect(details).toHaveAttribute('open');
    expect(screen.getByText('AGENTS.md')).toBeVisible();
    expect(screen.getByText('npm test')).toBeVisible();
  });

  it('labels approval-required tool activity without implying it ran', () => {
    const { container } = render(
      <Transcript
        messages={[
          {
            id: 'assistant-approval',
            role: 'assistant',
            content: '',
            status: 'awaiting_approval',
            tools: [
              {
                id: 'write-1',
                runId: 'run-1',
                callId: 'write-1',
                name: 'write_file',
                detail: 'Write src/app.ts (12 bytes)',
                status: 'awaiting_approval',
              },
            ],
          },
        ]}
        status="awaiting_approval"
        statusText="Permission required"
        onStarter={vi.fn()}
      />
    );

    expect(screen.getByText('Approval required')).toBeInTheDocument();
    expect(container.querySelector('.activity-timeline')).toHaveClass('is-attention');
    expect(screen.queryByText('Editing files')).not.toBeInTheDocument();
  });
});
