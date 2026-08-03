import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
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

  it('renders thinking blocks separate from assistant answer text', () => {
    render(
      <Transcript
        messages={[
          {
            id: 'assistant-think',
            role: 'assistant',
            content: 'The answer is 42.',
            thinking: 'Consider the hitchhiker path.',
            status: 'completed',
          },
        ]}
        status="completed"
        statusText="Completed"
        onStarter={vi.fn()}
      />
    );
    expect(screen.getByText('Thinking')).toBeInTheDocument();
    expect(screen.getByText('Consider the hitchhiker path.')).toBeInTheDocument();
    expect(screen.getByText('The answer is 42.')).toBeInTheDocument();
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

  it('renders the latest live assistant response without a second typewriter queue', () => {
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

    expect(screen.getByText('ABC')).toBeInTheDocument();

    rerender(
      <Transcript
        messages={[{ id: 'assistant-stream', role: 'assistant', content: 'ABC', status: 'completed', durationMs: 1_000 }]}
        status="completed"
        statusText="Completed"
        onStarter={onStarter}
      />
    );
    // Completed turns snap to full text immediately (no fake post-stream typewriter).
    expect(screen.getByText('ABC')).toBeInTheDocument();
    expect(screen.getByLabelText('Worked for 0m 01s')).toBeInTheDocument();
  });

  it('renders common assistant Markdown as rich text without exposing raw markers', () => {
    render(
      <Transcript
        messages={[{
          id: 'assistant-markdown',
          role: 'assistant',
          content: 'The **compute race** matters.\n\n- **Open models:** more choice\n- `npm test` stays visible\n\n[Read the source](https://example.com/source)',
          status: 'completed',
        }]}
        status="completed"
        statusText="Completed"
        onStarter={vi.fn()}
      />
    );

    expect(screen.getByText('compute race').tagName).toBe('STRONG');
    expect(screen.getAllByRole('listitem')[0]).toHaveTextContent('Open models: more choice');
    expect(screen.getByText('npm test').tagName).toBe('CODE');
    expect(screen.getByRole('link', { name: 'Read the source' })).toHaveAttribute(
      'href',
      'https://example.com/source'
    );
    expect(screen.queryByText('**compute race**')).not.toBeInTheDocument();
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

    const timeline = container.querySelector('.activity-timeline');
    expect(timeline).toHaveAttribute('data-open', 'false');
    expect(screen.getByText('Read files, ran a command')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Read files, ran a command' }));

    expect(timeline).toHaveAttribute('data-open', 'true');
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
    expect(screen.queryByRole('button', { name: 'Approve and continue' })).not.toBeInTheDocument();
  });

  it('renders exact-action approval controls and sends only their durable binding', async () => {
    const onApprovalDecision = vi.fn().mockResolvedValue(undefined);
    const { container } = render(
      <Transcript
        messages={[
          {
            id: 'assistant-bound-approval',
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
                approval: {
                  run_id: 'run-1',
                  call_id: 'write-1',
                  tool_id: 'write_file',
                  job_id: 'job-1',
                  node_id: 'node-3',
                  node_index: 3,
                  effect_sha256: 'a'.repeat(64),
                  summary: 'Write src/app.ts (12 bytes)',
                },
              },
            ],
          },
        ]}
        status="awaiting_approval"
        statusText="Permission required"
        onStarter={vi.fn()}
        onApprovalDecision={onApprovalDecision}
      />
    );

    expect(container.querySelector('.activity-timeline')).toHaveAttribute('data-open', 'true');
    expect(screen.getAllByText('Write src/app.ts (12 bytes)')).toHaveLength(2);

    fireEvent.click(screen.getByRole('button', { name: 'Approve and continue' }));

    await waitFor(() =>
      expect(onApprovalDecision).toHaveBeenCalledWith(
        {
          run_id: 'run-1',
          call_id: 'write-1',
          tool_id: 'write_file',
          job_id: 'job-1',
          node_id: 'node-3',
          node_index: 3,
          effect_sha256: 'a'.repeat(64),
          summary: 'Write src/app.ts (12 bytes)',
        },
        'approve'
      )
    );
    expect(screen.getByText('Approval submitted.')).toBeInTheDocument();
  });

  it('disables both approval decisions while a choice is pending and exposes callback errors', async () => {
    let rejectDecision!: (error: Error) => void;
    const onApprovalDecision = vi.fn(
      () => new Promise<void>((_resolve, reject) => {
        rejectDecision = reject;
      })
    );
    const { container } = render(
      <Transcript
        messages={[
          {
            id: 'assistant-pending-approval',
            role: 'assistant',
            content: '',
            status: 'awaiting_approval',
            tools: [
              {
                id: 'command-1',
                runId: 'run-1',
                callId: 'command-1',
                name: 'run_command',
                detail: 'Run npm test',
                status: 'awaiting_approval',
                approval: {
                  run_id: 'run-1',
                  call_id: 'command-1',
                  tool_id: 'run_command',
                  job_id: 'job-2',
                  node_id: 'node-4',
                  node_index: 4,
                  effect_sha256: 'b'.repeat(64),
                  summary: 'Run npm test',
                },
              },
            ],
          },
        ]}
        status="awaiting_approval"
        statusText="Permission required"
        onStarter={vi.fn()}
        onApprovalDecision={onApprovalDecision}
      />
    );

    expect(container.querySelector('.activity-timeline')).toHaveAttribute('data-open', 'true');
    fireEvent.click(screen.getByRole('button', { name: 'Deny' }));

    expect(screen.getByRole('button', { name: 'Denying…' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Approve and continue' })).toBeDisabled();

    await act(async () => rejectDecision(new Error('The approval is no longer pending')));
    expect(screen.getByRole('alert')).toHaveTextContent('The approval is no longer pending');
  });
});
