import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type {
  BrowserState,
  OptimusTransport,
} from '../../ipc/contracts';
import { BrowserSurface } from './BrowserSurface';

const nativeState: BrowserState = {
  url: 'https://example.test/dashboard',
  title: 'Dashboard',
  loading: false,
  canGoBack: false,
  canGoForward: false,
  visible: true,
  native: true,
};

describe('BrowserSurface annotations', () => {
  it('keeps browser navigation compact without a tab strip or status strip', async () => {
    render(
      <BrowserSurface
        transport={transportWithBrowser(vi.fn(async () => ({ cancelled: true })))}
        active
        onAnnotation={vi.fn()}
      />
    );

    expect(
      await screen.findByRole('toolbar', { name: 'Preview browser navigation' })
    ).toBeInTheDocument();
    expect(screen.getByText(/Preview browser — sandboxed user navigation/i)).toBeInTheDocument();
    expect(screen.queryByLabelText('Preview tabs')).not.toBeInTheDocument();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('projects bounded native element context into a human-readable composer note', async () => {
    const onAnnotation = vi.fn();
    const annotate = vi.fn(async () => ({
      cancelled: false,
      url: nativeState.url,
      pageTitle: nativeState.title,
      tag: 'button',
      role: 'button',
      label: 'Deploy preview',
      text: 'Deploy',
      rect: { x: 30, y: 40, width: 128, height: 32 },
    }));

    render(
      <BrowserSurface
        transport={transportWithBrowser(annotate)}
        active
        onAnnotation={onAnnotation}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Annotate preview' }));
    await waitFor(() => expect(annotate).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(onAnnotation).toHaveBeenCalledWith(
      'Preview context: button “Deploy preview” on example.test, 128 × 32px.'
    ));
  });
});

function transportWithBrowser(
  annotate: NonNullable<OptimusTransport['browser']>['annotate']
): OptimusTransport {
  return {
    kind: 'electron',
    invoke: async () => ({}) as never,
    chat: () => {
      throw new Error('not used');
    },
    windowAction: async () => ({}),
    pickFolder: async () => ({ ok: false }),
    openPath: async () => ({}),
    browser: {
      setBounds: () => undefined,
      setVisible: () => undefined,
      navigate: async () => nativeState,
      back: async () => nativeState,
      forward: async () => nativeState,
      reload: async () => nativeState,
      state: async () => nativeState,
      annotate,
      cancelAnnotation: async () => ({ cancelled: true }),
      subscribe: () => () => undefined,
    },
  };
}
