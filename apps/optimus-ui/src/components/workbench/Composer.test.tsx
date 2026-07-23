import { fireEvent, render, screen } from '@testing-library/react';
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
});
