import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { Composer } from './Composer';

const settings = {
  provider: 'offline' as const,
  model: 'offline-echo',
  thinking: 'high',
  access: 'standard',
  fast: false,
};

describe('Composer', () => {
  it('does not submit Enter while IME composition is active', () => {
    const onSend = vi.fn();
    render(
      <Composer
        value="こんにちは"
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={settings}
        onChange={() => undefined}
        onSettings={() => undefined}
        onSend={onSend}
        onStop={() => undefined}
      />
    );
    const input = screen.getByLabelText('Message Optimus');
    fireEvent.compositionStart(input);
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.compositionEnd(input);
    fireEvent.keyDown(input, { key: 'Enter', isComposing: false });
    expect(onSend).toHaveBeenCalledTimes(1);
  });

  it('preserves Shift+Enter and exposes Stop to the owning session', () => {
    const onSend = vi.fn();
    const onStop = vi.fn();
    render(
      <Composer
        value="working"
        runStatus="working"
        disabled={false}
        isRunOwner
        settings={settings}
        onChange={() => undefined}
        onSettings={() => undefined}
        onSend={onSend}
        onStop={onStop}
      />
    );
    fireEvent.keyDown(screen.getByLabelText('Message Optimus'), { key: 'Enter', shiftKey: true });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText('Stop current run'));
    expect(onStop).toHaveBeenCalledTimes(1);
  });

  it('puts Access first and moves the remaining run settings into one popover', async () => {
    const user = userEvent.setup();
    const onSettings = vi.fn();
    render(
      <Composer
        value="ship it"
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={settings}
        onChange={() => undefined}
        onSettings={onSettings}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    const access = screen.getByRole('button', { name: 'Access: Standard' });
    const controls = access.closest('.composer-selects');
    expect(controls?.firstElementChild).toContainElement(access);
    expect(screen.queryByLabelText('Provider')).not.toBeInTheDocument();

    await user.click(access);
    const accessMenu = screen.getByRole('listbox', { name: 'Access' });
    expect(within(accessMenu).getByRole('option', { name: /Read only/ })).toBeInTheDocument();
    await user.click(within(accessMenu).getByRole('option', { name: /Read only/ }));
    expect(onSettings).toHaveBeenCalledWith({ ...settings, access: 'read_only' });

    const trigger = screen.getByRole('button', { name: 'Model and run settings' });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await user.click(trigger);

    const popover = screen.getByRole('dialog', { name: 'Model and run settings' });
    expect(within(popover).getByLabelText('Provider')).toHaveValue('offline');
    expect(within(popover).getByLabelText('Model')).toHaveValue('offline-echo');
    expect(within(popover).getByLabelText('Thinking level')).toHaveValue('high');
    await user.click(within(popover).getByRole('switch', { name: 'Fast mode' }));
    expect(onSettings).toHaveBeenCalledWith({ ...settings, fast: true });

    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog', { name: 'Model and run settings' })).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(screen.queryByText('Local checkout')).not.toBeInTheDocument();
    expect(screen.queryByText('Ready')).not.toBeInTheDocument();
  });

  it('uses a concise number, model-name, and thinking-level summary for Codex models', () => {
    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={{ ...settings, provider: 'codex', model: 'gpt-5.6-terra' }}
        onChange={() => undefined}
        onSettings={() => undefined}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    const trigger = screen.getByRole('button', { name: 'Model and run settings' });
    expect(trigger).toHaveTextContent('5.6 Terra');
    expect(trigger).toHaveTextContent('High');
    expect(trigger).not.toHaveTextContent('effort');
    expect(trigger).not.toHaveTextContent('gpt-5.6-terra');
  });

  it('marks only the selected Unrestricted host control for flame styling', () => {
    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={{ ...settings, access: 'unrestricted_host' }}
        onChange={() => undefined}
        onSettings={() => undefined}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    expect(screen.getByRole('button', { name: 'Access: Unrestricted host' })).toHaveClass(
      'is-unrestricted-host'
    );
    expect(screen.getByRole('button', { name: 'Model and run settings' })).not.toHaveClass(
      'is-unrestricted-host'
    );
  });

  // Issue #118: the menu offered `Full access` first, and that value turned
  // SmartDeny off. What the first item is, and where break-glass sits, is the
  // security property — so it is asserted, not left to the eye.
  it('offers Standard first and keeps break-glass last, under Expert', async () => {
    const user = userEvent.setup();
    const onSettings = vi.fn();
    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={settings}
        onChange={() => undefined}
        onSettings={onSettings}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Access: Standard' }));
    const accessMenu = screen.getByRole('listbox', { name: 'Access' });
    const options = within(accessMenu).getAllByRole('option');
    expect(options).toHaveLength(5);
    expect(options[0]).toHaveTextContent('Standard');
    expect(options[4]).toHaveTextContent('Unrestricted host');
    expect(options[4].closest('.composer-access-tier')).toHaveClass('is-expert');
    expect(within(accessMenu).getByRole('group', { name: 'Expert' })).toContainElement(options[4]);

    await user.click(options[4]);
    expect(onSettings).toHaveBeenCalledWith({ ...settings, access: 'unrestricted_host' });
  });

  // A wrapping arrow key would put break-glass one keystroke above the
  // default, which is the distance the tiers exist to create.
  it('does not wrap ArrowUp from the first option round to break-glass', async () => {
    const user = userEvent.setup();
    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={settings}
        onChange={() => undefined}
        onSettings={() => undefined}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Access: Standard' }));
    const options = within(screen.getByRole('listbox', { name: 'Access' })).getAllByRole('option');
    options[0].focus();

    await user.keyboard('{ArrowUp}');
    expect(options[0]).toHaveFocus();

    options[4].focus();
    await user.keyboard('{ArrowDown}');
    expect(options[4]).toHaveFocus();
  });
});
