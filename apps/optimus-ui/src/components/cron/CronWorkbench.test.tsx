import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { CronJob, DesktopMethod, OptimusTransport } from '../../ipc/contracts';
import { CronWorkbench } from './CronWorkbench';

const jobs: CronJob[] = [
  {
    id: 'job-1',
    name: 'Nightly',
    every_secs: 86400,
    enabled: true,
    provider: 'offline',
    prompt: 'report',
    last_status: 'ok',
  },
];

function createTransport() {
  const localJobs = [...jobs];
  const invoke = vi.fn(async (method: DesktopMethod, params?: Record<string, unknown>) => {
    if (method === 'cron_list') return { jobs: localJobs };
    if (method === 'cron_add') {
      const job = {
        id: 'job-new',
        name: String(params?.name || 'x'),
        every_secs: Number(params?.every_secs || 60),
        enabled: true,
        provider: String(params?.provider || 'offline'),
        prompt: String(params?.prompt || 'tick'),
      };
      localJobs.unshift(job);
      return job;
    }
    if (method === 'cron_set_enabled') {
      const job = localJobs.find((item) => item.id === params?.id);
      if (job) job.enabled = Boolean(params?.enabled);
      return { id: params?.id, enabled: Boolean(params?.enabled) };
    }
    if (method === 'cron_remove') {
      const index = localJobs.findIndex((item) => item.id === params?.id);
      if (index >= 0) localJobs.splice(index, 1);
      return { removed: true, id: params?.id };
    }
    if (method === 'cron_history') {
      return {
        attempts: [
          {
            attempt_id: 'a1',
            job_id: params?.id,
            status: 'succeeded',
            started_unix: 1,
            completed_unix: 2,
            detail: 'ok',
          },
        ],
      };
    }
    throw new Error(`unexpected ${method}`);
  });
  return {
    invoke,
    transport: {
      kind: 'fixture',
      invoke,
      chat: vi.fn(),
      windowAction: vi.fn(),
      pickFolder: vi.fn(),
      openPath: vi.fn(),
    } as unknown as OptimusTransport,
  };
}

describe('CronWorkbench', () => {
  it('lists schedules, pauses, and shows history', async () => {
    const user = userEvent.setup();
    const { invoke, transport } = createTransport();
    render(<CronWorkbench transport={transport} active />);

    expect(await screen.findByText('Nightly')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Pause' }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('cron_set_enabled', { id: 'job-1', enabled: false })
    );

    await user.click(screen.getByText('Nightly'));
    expect(await screen.findByText('succeeded')).toBeInTheDocument();
  });

  it('creates a schedule via the form', async () => {
    const user = userEvent.setup();
    const { invoke, transport } = createTransport();
    render(<CronWorkbench transport={transport} active />);

    await screen.findByText('Nightly');
    const nameInput = screen.getByPlaceholderText('Nightly status');
    const everyInput = screen.getByDisplayValue('3600');
    const promptInput = screen.getByPlaceholderText(
      'What should Optimus do on this schedule?'
    );
    await user.clear(nameInput);
    await user.type(nameInput, 'Hourly');
    await user.clear(everyInput);
    await user.type(everyInput, '120');
    await user.clear(promptInput);
    await user.type(promptInput, 'check health');
    await user.selectOptions(screen.getByLabelText('Provider'), 'open-ai-compat');
    await user.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'cron_add',
        expect.objectContaining({
          name: 'Hourly',
          every_secs: 120,
          prompt: 'check health',
          provider: 'open-ai-compat',
        })
      )
    );
  });
});
