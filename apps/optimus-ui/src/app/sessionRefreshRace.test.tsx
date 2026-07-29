import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, expect, it, vi } from 'vitest';

// A late `sessions` response carries a list from before the click. The gate
// holds only the FIRST such response, so the second one — issued after the
// session exists, and correct — lands first and the stale one lands last.
// That ordering is the whole bug: a correct answer arriving before a stale one
// is not a problem, and a test where the stale one lands first proves nothing.
const gate = vi.hoisted(() => {
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { held, release: () => release(), calls: 0, armed: false };
});

vi.mock('../ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../ipc')>();
  return {
    ...actual,
    getTransport: () => {
      const real = actual.getTransport();
      return {
        ...real,
        async invoke(method: string, params?: Record<string, unknown>) {
          const value = await (real.invoke as (m: string, p?: Record<string, unknown>) => Promise<unknown>)(
            method,
            params
          );
          if (method !== 'sessions' || !gate.armed) return value;
          gate.calls += 1;
          if (gate.calls > 1) return value;
          // The fixture hands back a live reference to its own array; copy it
          // so this response keeps the pre-click snapshot it was built from.
          const snapshot = value as { sessions?: unknown[] };
          const frozen = { ...snapshot, sessions: [...(snapshot.sessions ?? [])] };
          await gate.held;
          return frozen;
        },
      };
    },
  };
});

const { OptimusApp } = await import('./OptimusApp');

beforeEach(() => localStorage.clear());

it('a thread created while the session list is still loading is not erased by it', async () => {
  gate.armed = true;
  const user = userEvent.setup();
  const { container } = render(<OptimusApp />);

  // The rail renders before the runtime refresh resolves — that window is
  // exactly when a user clicks "New thread", and on a loaded machine it is
  // seconds wide.
  await user.click(await screen.findByRole('button', { name: 'New thread' }));
  await waitFor(() =>
    expect(container.querySelectorAll('.session-row.is-active')).toHaveLength(1)
  );
  const created = container.querySelector('.session-row.is-active')?.textContent ?? '';
  expect(created).toContain('New Optimus session');

  // Now let the pre-click snapshot land. It does not contain the new thread.
  gate.release();

  // Nothing should change: the thread is still there and still selected.
  await waitFor(() =>
    expect(container.querySelectorAll('.session-row.is-active')).toHaveLength(1)
  );
  expect(container.querySelector('.session-row.is-active')?.textContent).toContain(
    'New Optimus session'
  );
  // And the older threads the snapshot did know about are still listed, so the
  // fix is "ignore a stale list", not "stop refreshing".
  await waitFor(() =>
    expect(container.querySelectorAll('.session-row').length).toBeGreaterThan(1)
  );
});
