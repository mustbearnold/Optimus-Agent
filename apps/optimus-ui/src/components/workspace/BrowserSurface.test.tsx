import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { BrowserState, OptimusTransport } from '../../ipc/contracts';
import { createOptimusClient, type OptimusClient } from '../../ipc/client';
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
        client={transportWithBrowser(vi.fn(async () => ({ cancelled: true })))}
        active
        onAddToPrompt={vi.fn()}
      />
    );

    expect(
      await screen.findByRole('toolbar', { name: 'Preview browser navigation' })
    ).toBeInTheDocument();
    expect(screen.queryByText(/Preview only|sandboxed|ADR-/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Preview annotation gallery')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Preview tabs')).not.toBeInTheDocument();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();
  });

  it('captures native annotation into the gallery without auto-injecting the prompt', async () => {
    const onAddToPrompt = vi.fn();
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
        client={transportWithBrowser(annotate)}
        active
        onAddToPrompt={onAddToPrompt}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Annotate preview' }));
    await waitFor(() => expect(annotate).toHaveBeenCalledTimes(1));
    // Gallery only — no composer inject until Add to prompt.
    expect(onAddToPrompt).not.toHaveBeenCalled();
    expect(
      await screen.findByText(/Preview context \(untrusted\): button “Deploy preview”/i)
    ).toBeInTheDocument();
    expect(screen.getByLabelText('Preview annotation gallery')).toBeInTheDocument();
  });

  it('requires explicit Add to prompt to inject composer context', async () => {
    const onAddToPrompt = vi.fn();
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
        client={transportWithBrowser(annotate)}
        active
        onAddToPrompt={onAddToPrompt}
      />
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Annotate preview' }));
    await waitFor(() => expect(annotate).toHaveBeenCalledTimes(1));
    fireEvent.click(await screen.findByRole('button', { name: 'Add to prompt' }));
    await waitFor(() =>
      expect(onAddToPrompt).toHaveBeenCalledWith(
        'Preview context (untrusted): button “Deploy preview” on example.test, 128 × 32px.'
      )
    );
  });

  it('renders with a null-transport client without crashing (bootstrap window)', () => {
    // Regression: the packaged renderer mounts with transport=null while the
    // spec-015 A3 broker ticket is awaited (and permanently in the confirmed
    // broker-absence terminal affordance). The mount-time state()/subscribe()
    // must not throw; the client's browser API returns idle no-ops instead.
    render(<BrowserSurface client={createOptimusClient(null)} active onAddToPrompt={vi.fn()} />);

    expect(screen.getByLabelText('Preview browser')).toBeInTheDocument();
  });
});

function transportWithBrowser(
  annotate: NonNullable<OptimusTransport['browser']>['annotate']
): OptimusClient {
  return createOptimusClient({
    kind: 'tauri',
    invoke: async () => ({}) as never,
    chat: () => {
      throw new Error('not used');
    },
    chatApprovalResolve: () => {
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
  });
}
