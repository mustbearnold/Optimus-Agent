import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { TextPromptDialog } from './TextPromptDialog';

/**
 * The behaviours worth pinning are the ones the hand-written dialog lacked and
 * nothing noticed, because none of them change a pixel: focus that cannot leave
 * the dialog, focus that returns where it came from, and a page that assistive
 * technology can no longer read through the modal.
 *
 * They are asserted here rather than taken on trust from Radix, so that
 * replacing or re-hand-rolling this component later fails loudly.
 */

function open(overrides: Partial<Parameters<typeof TextPromptDialog>[0]> = {}) {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  render(
    <TextPromptDialog
      open
      title="Rename project"
      label="Project name"
      initialValue="old name"
      onConfirm={onConfirm}
      onCancel={onCancel}
      {...overrides}
    />
  );
  return { onConfirm, onCancel };
}

describe('TextPromptDialog', () => {
  it('opens with the existing value focused and selected, the way window.prompt does', async () => {
    open();
    const field = await screen.findByLabelText('Project name');
    await waitFor(() => expect(field).toHaveFocus());
    expect((field as HTMLInputElement).selectionStart).toBe(0);
    expect((field as HTMLInputElement).selectionEnd).toBe('old name'.length);
  });

  it('confirms the trimmed value and refuses to confirm an empty one', async () => {
    const user = userEvent.setup();
    const { onConfirm } = open({ initialValue: '' });
    const field = await screen.findByLabelText('Project name');

    // Nothing typed: the primary action is not merely ignored, it is disabled,
    // so the dialog never looks like it accepted something it discarded.
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    await user.type(field, '  spaced  ');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(onConfirm).toHaveBeenCalledWith('spaced');
  });

  it('cancels on Escape and on the Cancel button', async () => {
    const user = userEvent.setup();
    const { onCancel } = open();
    await user.keyboard('{Escape}');
    expect(onCancel).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it('traps Tab inside the dialog instead of letting it walk into the page behind', async () => {
    const user = userEvent.setup();
    render(
      <>
        <button type="button">behind the dialog</button>
        <TextPromptDialog
          open
          title="Rename project"
          label="Project name"
          initialValue="old name"
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </>
    );
    // Queried by text, not by role: while the dialog is open this button is
    // out of the accessibility tree entirely, so `getByRole` cannot see it.
    const outside = screen.getByText('behind the dialog');
    const dialog = await screen.findByRole('dialog');

    // Four tabs is more than the dialog has stops, so an untrapped dialog would
    // have handed focus to the page by now.
    for (let press = 0; press < 4; press += 1) await user.tab();

    expect(outside).not.toHaveFocus();
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
  });

  it('returns focus to whatever opened it when it closes', async () => {
    const user = userEvent.setup();

    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            rename
          </button>
          <TextPromptDialog
            open={open}
            title="Rename project"
            label="Project name"
            onConfirm={() => setOpen(false)}
            onCancel={() => setOpen(false)}
          />
        </>
      );
    }

    render(<Harness />);
    const trigger = screen.getByRole('button', { name: 'rename' });
    await user.click(trigger);
    await screen.findByLabelText('Project name');

    await user.keyboard('{Escape}');
    // Without focus restore the caret lands on <body>, and a keyboard user has
    // to tab from the top of the application to get back to where they were.
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it('hides the rest of the application from assistive technology while open', async () => {
    render(
      <>
        <button type="button">behind the dialog</button>
        <TextPromptDialog
          open
          title="Rename project"
          label="Project name"
          onConfirm={vi.fn()}
          onCancel={vi.fn()}
        />
      </>
    );
    await screen.findByRole('dialog');

    // Still in the document — but no longer in the accessibility tree, which is
    // the difference between a modal and something that merely looks like one.
    // A screen reader user could otherwise read and operate the whole
    // application through the dialog.
    expect(screen.getByText('behind the dialog')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'behind the dialog' })).toBeNull();
  });

  it('names the dialog by its title', async () => {
    open();
    const dialog = await screen.findByRole('dialog');
    expect(dialog).toHaveAccessibleName('Rename project');
  });
});
