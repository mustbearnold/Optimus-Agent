import { fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
}));

import { WindowResizeHandles } from './WindowResizeHandles';

const ALL_DIRECTIONS = [
  'north',
  'south',
  'east',
  'west',
  'northEast',
  'northWest',
  'southEast',
  'southWest',
];

describe('WindowResizeHandles (borderless-window resize hotspots)', () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    delete window.__TAURI__;
    delete window.__TAURI_INTERNALS__;
  });

  afterEach(() => {
    delete window.__TAURI__;
    delete window.__TAURI_INTERNALS__;
  });

  it('renders nothing outside the packaged webview', () => {
    const { container } = render(<WindowResizeHandles />);
    expect(container.querySelector('.window-resize-handles')).toBeNull();
  });

  it('renders eight edge/corner hotspots with honest resize cursors when packaged', () => {
    window.__TAURI_INTERNALS__ = {};
    const { container } = render(<WindowResizeHandles />);
    const overlay = container.querySelector('.window-resize-handles');
    expect(overlay).not.toBeNull();
    expect(overlay).toHaveAttribute('aria-hidden', 'true');
    const handles = container.querySelectorAll('[data-resize-handle]');
    expect(handles).toHaveLength(8);
    for (const direction of ALL_DIRECTIONS) {
      expect(container.querySelector(`[data-resize-handle="${direction}"]`)).not.toBeNull();
    }
    // Edge strips advertise the matching axis cursor; corners the diagonal.
    expect(container.querySelector('[data-resize-handle="north"]')).toHaveStyle({
      cursor: 'ns-resize',
    });
    expect(container.querySelector('[data-resize-handle="east"]')).toHaveStyle({
      cursor: 'ew-resize',
    });
    expect(container.querySelector('[data-resize-handle="northEast"]')).toHaveStyle({
      cursor: 'nesw-resize',
    });
    expect(container.querySelector('[data-resize-handle="southEast"]')).toHaveStyle({
      cursor: 'nwse-resize',
    });
  });

  it('hands the hotspot drag to the shell bridge on pointer down', async () => {
    window.__TAURI_INTERNALS__ = {};
    mocks.invoke.mockResolvedValue({ ok: true });
    const { container } = render(<WindowResizeHandles />);
    const corner = container.querySelector(
      '[data-resize-handle="northEast"]'
    ) as HTMLElement;
    fireEvent.pointerDown(corner, { buttons: 1 });
    await vi.waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('window_resize_start', {
        direction: 'northEast',
      })
    );
  });

  it('never starts a resize drag outside the packaged webview', () => {
    const { container } = render(<WindowResizeHandles />);
    const corner = container.querySelector('[data-resize-handle="northEast"]');
    expect(corner).toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
