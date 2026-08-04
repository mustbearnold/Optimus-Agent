import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { Composer } from './Composer';

const settings = {
  provider: 'offline' as const,
  model: 'offline-scripted',
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
    expect(within(popover).getByLabelText('Model')).toHaveValue('offline-scripted');
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

  it('keeps Auto selected for canonical routing', async () => {
    const user = userEvent.setup();
    const onSettings = vi.fn();
    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={{ ...settings, provider: 'auto', model: '' }}
        onChange={() => undefined}
        onSettings={onSettings}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    expect(screen.getByRole('button', { name: 'Model and run settings' })).toHaveTextContent('Auto');
    await user.click(screen.getByRole('button', { name: 'Model and run settings' }));
    const popover = screen.getByRole('dialog', { name: 'Model and run settings' });
    const providerSelect = within(popover).getByLabelText('Provider');
    expect(providerSelect).toHaveValue('auto');
    expect(within(providerSelect).getByRole('option', { name: 'Auto' })).toBeInTheDocument();
    const modelSelect = within(popover).getByLabelText('Model');
    expect(modelSelect).toHaveValue('');
    expect(within(modelSelect).getAllByRole('option')).toHaveLength(1);

    await user.selectOptions(within(popover).getByLabelText('Provider'), 'offline');
    expect(onSettings).toHaveBeenCalledWith({
      ...settings,
      provider: 'offline',
      model: '',
    });
  });

  it('sends the canonical OpenAI wire id and exposes only OpenAI-owned models', async () => {
    const user = userEvent.setup();
    const onSettings = vi.fn();
    const first = render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={{ ...settings, provider: 'codex', model: 'gpt-5.6-terra' }}
        onChange={() => undefined}
        onSettings={onSettings}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Model and run settings' }));
    const popover = screen.getByRole('dialog', { name: 'Model and run settings' });
    await user.selectOptions(within(popover).getByLabelText('Provider'), 'open-ai-compat');
    expect(onSettings).toHaveBeenCalledWith({
      ...settings,
      provider: 'open-ai-compat',
      model: '',
    });
    first.unmount();

    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={{ ...settings, provider: 'open-ai-compat', model: 'gpt-4.1' }}
        onChange={() => undefined}
        onSettings={onSettings}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );
    await user.click(screen.getByRole('button', { name: 'Model and run settings' }));
    const model = within(screen.getByRole('dialog', { name: 'Model and run settings' }))
      .getByLabelText('Model');
    expect(
      within(model).getAllByRole('option').map((option) => option.getAttribute('value'))
    ).toEqual(['', 'gpt-4.1', 'gpt-4o']);
  });

  it('exposes DeepSeek V4 models and Auto above every reasoning budget', async () => {
    const user = userEvent.setup();
    render(
      <Composer
        value=""
        runStatus="idle"
        disabled={false}
        isRunOwner={false}
        settings={{ ...settings, provider: 'deepseek', model: 'deepseek-v4-flash', thinking: 'auto' }}
        onChange={() => undefined}
        onSettings={() => undefined}
        onSend={() => undefined}
        onStop={() => undefined}
      />
    );
    await user.click(screen.getByRole('button', { name: 'Model and run settings' }));
    const popover = screen.getByRole('dialog', { name: 'Model and run settings' });
    expect(within(popover).getByLabelText('Provider')).toHaveValue('deepseek');
    expect(within(popover).getByLabelText('Model').querySelectorAll('option')).toHaveLength(3);
    expect(within(popover).getByLabelText('Model')).toHaveValue('deepseek-v4-flash');
    const thinking = within(popover).getByLabelText('Thinking level');
    expect(Array.from(thinking.querySelectorAll('option')).map((option) => option.value)).toEqual([
      'auto',
      'minimal',
      'low',
      'medium',
      'high',
      'xhigh',
      'max',
      'ultra',
    ]);
    expect(thinking).toHaveValue('auto');
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
    expect(options).toHaveLength(6);
    expect(options[0]).toHaveTextContent('Standard');
    expect(options[4]).toHaveTextContent('Developer Full Access');
    expect(options[5]).toHaveTextContent('Unrestricted host');
    expect(options[5].closest('.composer-access-tier')).toHaveClass('is-expert');
    expect(within(accessMenu).getByRole('group', { name: 'Expert' })).toContainElement(options[5]);

    await user.click(options[5]);
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
    // Re-query at every step: a slow run can re-render the listbox between
    // interactions, and a held node from before the render is a different
    // element than the one focus management is moving.
    const option = (index: number) =>
      within(screen.getByRole('listbox', { name: 'Access' })).getAllByRole('option')[index];
    // Opening the menu focuses the selected option on the next animation
    // frame. Let that land first — moving focus before it fires hands the
    // frame a stale target to steal focus back to, mid-test.
    await waitFor(() => expect(option(0)).toHaveFocus());

    await user.keyboard('{ArrowUp}');
    await waitFor(() => expect(option(0)).toHaveFocus());

    option(5).focus();
    await user.keyboard('{ArrowDown}');
    await waitFor(() => expect(option(5)).toHaveFocus());
  });
});
