import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { Composer } from './Composer';

const settings = {
  provider: 'offline' as const,
  model: 'offline-echo',
  thinking: 'high',
  access: 'ask',
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

    const access = screen.getByRole('button', { name: 'Access: Ask before effects' });
    const controls = access.closest('.composer-selects');
    expect(controls?.firstElementChild).toContainElement(access);
    expect(screen.queryByLabelText('Provider')).not.toBeInTheDocument();

    await user.click(access);
    const accessMenu = screen.getByRole('listbox', { name: 'Access' });
    expect(within(accessMenu).getByRole('option', { name: 'Full access' })).toBeInTheDocument();
    expect(within(accessMenu).getByRole('option', { name: 'Read only' })).toBeInTheDocument();
    await user.click(within(accessMenu).getByRole('option', { name: 'Full access' }));
    expect(onSettings).toHaveBeenCalledWith({ ...settings, access: 'full' });

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

  it('marks only the selected Full access control for flame styling', () => {
    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={{ ...settings, access: 'full' }}
        onChange={() => undefined}
        onSettings={() => undefined}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    expect(screen.getByRole('button', { name: 'Access: Full access' })).toHaveClass('is-full-access');
    expect(screen.getByRole('button', { name: 'Model and run settings' })).not.toHaveClass('is-full-access');
  });
});
