import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import type { DesktopMethod, OptimusTransport } from '../../ipc/contracts';
import { createOptimusClient } from '../../ipc/client';
import { CommandPalette } from './CommandPalette';

const CATALOG = [
  { id: 'skills', name: 'skills', description: 'Open skills console' },
  { id: 'doctor', name: 'doctor', description: 'Run doctor' },
  { id: 'cron', name: 'cron', description: 'Schedule a recurring run' },
];

function fixtureTransport(commands = CATALOG) {
  const invoke = vi.fn(async (method: DesktopMethod) => {
    if (method === 'commands_list') return { commands };
    throw new Error(method);
  });
  return createOptimusClient({
    kind: 'fixture',
    invoke,
    chat: vi.fn(),
    windowAction: vi.fn(),
    pickFolder: vi.fn(),
    openPath: vi.fn(),
  } as unknown as OptimusTransport);
}

/**
 * The palette was usable with a mouse and unusable with a keyboard, which for
 * this particular widget is the wrong way round. Arrow keys did nothing; the
 * only way down the list was Tab, one stop per command, and past the last one
 * focus left the dialog entirely. A screen reader got a stack of buttons with no
 * position, no active item, and no announcement when filtering changed the list.
 *
 * Those are the behaviours pinned below. The first test is the one that existed
 * before the conversion, unchanged, because a conversion that quietly alters
 * what the palette contains is not a conversion.
 */
describe('CommandPalette', () => {
  it('loads surface commands and runs selection', async () => {
    const user = userEvent.setup();
    const onRun = vi.fn();
    render(
      <CommandPalette open client={fixtureTransport()} onClose={vi.fn()} onRun={onRun} />
    );
    expect(await screen.findByText('/skills')).toBeInTheDocument();
    await user.click(screen.getByText('/skills'));
    await waitFor(() => expect(onRun).toHaveBeenCalledWith('skills'));
  });

  it('moves through the list with the arrow keys and runs the active command on Enter', async () => {
    const user = userEvent.setup();
    const onRun = vi.fn();
    render(
      <CommandPalette open client={fixtureTransport()} onClose={vi.fn()} onRun={onRun} />
    );
    await screen.findByText('/skills');

    // Two presses from the first item lands on the third. The old palette
    // ignored both and Enter would have submitted nothing.
    await user.keyboard('{ArrowDown}{ArrowDown}');
    await user.keyboard('{Enter}');
    await waitFor(() => expect(onRun).toHaveBeenCalledWith('cron'));
  });

  it('exposes the list as a listbox with exactly one active option', async () => {
    const user = userEvent.setup();
    render(
      <CommandPalette open client={fixtureTransport()} onClose={vi.fn()} onRun={vi.fn()} />
    );
    await screen.findByText('/skills');

    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(3);
    // `aria-activedescendant` on the input is what tells a screen reader which
    // row is current while focus stays in the search field. Without it the user
    // hears nothing as the selection moves.
    // cmdk resolves it after the items register themselves, one frame later, so
    // this is a `waitFor` rather than a straight read.
    const field = screen.getByLabelText('Filter commands');
    await waitFor(() => expect(field).toHaveAttribute('aria-activedescendant'));
    const activeId = field.getAttribute('aria-activedescendant');
    expect(document.getElementById(activeId as string)).toHaveAttribute('aria-selected', 'true');
    expect(options.filter((o) => o.getAttribute('aria-selected') === 'true')).toHaveLength(1);

    await user.keyboard('{ArrowDown}');
    await waitFor(() =>
      expect(field.getAttribute('aria-activedescendant')).not.toBe(activeId)
    );
  });

  it('filters on id, name and description, the way it always did', async () => {
    const user = userEvent.setup();
    render(
      <CommandPalette open client={fixtureTransport()} onClose={vi.fn()} onRun={vi.fn()} />
    );
    await screen.findByText('/skills');

    // "recurring" appears only in cron's description. cmdk's own fuzzy scorer
    // would not match it, which is why filtering is still done by this component.
    await user.type(screen.getByLabelText('Filter commands'), 'recurring');
    await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(1));
    expect(screen.getByText('/cron')).toBeInTheDocument();
  });

  it('says so when nothing matches instead of showing an empty box', async () => {
    const user = userEvent.setup();
    render(
      <CommandPalette open client={fixtureTransport()} onClose={vi.fn()} onRun={vi.fn()} />
    );
    await screen.findByText('/skills');
    await user.type(screen.getByLabelText('Filter commands'), 'zzzz');
    expect(await screen.findByText('No matching commands.')).toBeInTheDocument();
  });

  it('closes on Escape and returns focus to whatever opened it', async () => {
    const user = userEvent.setup();

    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            open palette
          </button>
          <CommandPalette
            open={open}
            client={fixtureTransport()}
            onClose={() => setOpen(false)}
            onRun={vi.fn()}
          />
        </>
      );
    }

    render(<Harness />);
    const trigger = screen.getByRole('button', { name: 'open palette' });
    await user.click(trigger);
    await screen.findByText('/skills');

    await user.keyboard('{Escape}');
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it('hides the rest of the application from assistive technology while open', async () => {
    render(
      <>
        <button type="button">behind the palette</button>
        <CommandPalette open client={fixtureTransport()} onClose={vi.fn()} onRun={vi.fn()} />
      </>
    );
    await screen.findByText('/skills');

    // Present, but out of the accessibility tree — the difference between a
    // modal and something that merely looks like one.
    expect(screen.getByText('behind the palette')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'behind the palette' })).toBeNull();
  });

  it('surfaces a transport failure rather than showing an empty palette', async () => {
    const client = createOptimusClient({
      kind: 'fixture',
      invoke: vi.fn(async () => {
        throw new Error('commands_list unavailable');
      }),
      chat: vi.fn(),
      windowAction: vi.fn(),
      pickFolder: vi.fn(),
      openPath: vi.fn(),
    } as unknown as OptimusTransport);

    render(<CommandPalette open client={client} onClose={vi.fn()} onRun={vi.fn()} />);
    expect(await screen.findByText('commands_list unavailable')).toBeInTheDocument();
  });
});
