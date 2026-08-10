import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { OptimusTransport } from '../../ipc/contracts';
import { createOptimusClient } from '../../ipc/client';
import { ExecutionDock } from './ExecutionDock';

/**
 * #112: a transport call that settles after the dock unmounts must be
 * dropped, not committed. The CI symptom was `setError` firing during vitest
 * teardown — after jsdom deleted `window` — and crashing whichever unrelated
 * test file was still finishing. `onState` is the observable half of the same
 * post-await continuation, so it is what these tests assert on.
 */
describe('ExecutionDock after unmount', () => {
  const controlledTransport = () => {
    const settlers: Array<{
      resolve: (value: unknown) => void;
      reject: (reason: Error) => void;
    }> = [];
    const transport = {
      invoke: () =>
        new Promise((resolve, reject) => {
          settlers.push({ resolve, reject });
        }),
    } as unknown as OptimusTransport;
    return { transport, settlers };
  };

  it('drops a result that lands after unmount instead of committing it', async () => {
    const { transport, settlers } = controlledTransport();
    const client = createOptimusClient(transport);
    const onState = vi.fn();
    const { unmount } = render(
      <ExecutionDock client={client} open onClose={() => {}} onState={onState} />
    );
    await waitFor(() => expect(settlers).toHaveLength(2));

    unmount();
    settlers[0].resolve({ pending: [] });
    settlers[1].resolve({ jobs: [] });
    // Let the awaited continuation run to completion.
    await Promise.resolve();
    await Promise.resolve();

    expect(onState).not.toHaveBeenCalled();
  });

  it('drops a rejection that lands after unmount instead of rendering it', async () => {
    const { transport, settlers } = controlledTransport();
    const client = createOptimusClient(transport);
    const { unmount } = render(
      <ExecutionDock client={client} open onClose={() => {}} onState={() => {}} />
    );
    await waitFor(() => expect(settlers).toHaveLength(2));

    unmount();
    settlers[0].reject(new Error('host went away'));
    settlers[1].reject(new Error('host went away'));
    await Promise.resolve();
    await Promise.resolve();
    // Reaching this point without vitest reporting an unhandled error is the
    // teardown half of the assertion; the guard skipping `setError` is why.
  });

  it('still commits results while mounted', async () => {
    const { transport, settlers } = controlledTransport();
    const client = createOptimusClient(transport);
    const onState = vi.fn();
    render(<ExecutionDock client={client} open onClose={() => {}} onState={onState} />);
    await waitFor(() => expect(settlers).toHaveLength(2));

    settlers[0].resolve({ pending: [] });
    settlers[1].resolve({ jobs: [] });

    await waitFor(() => expect(onState).toHaveBeenCalledWith([], []));
    expect(screen.getByRole('tablist', { name: 'Execution views' })).toBeInTheDocument();
  });
});
