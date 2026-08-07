import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mocks.invoke,
}));

import { isPackaged, startResize, windowAction } from './windowBridge';

describe('windowBridge (spec-001 R5 chrome seam)', () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    delete window.__TAURI__;
    delete window.__TAURI_INTERNALS__;
  });

  afterEach(() => {
    delete window.__TAURI__;
    delete window.__TAURI_INTERNALS__;
  });

  it('is inert outside the packaged webview', async () => {
    expect(isPackaged()).toBe(false);
    await expect(windowAction('minimize')).resolves.toEqual({ ok: false });
    await expect(startResize('southEast')).resolves.toEqual({ ok: false });
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it('routes window actions to the shell bridge when packaged', async () => {
    window.__TAURI_INTERNALS__ = {};
    mocks.invoke.mockResolvedValue({ ok: true });
    await expect(windowAction('close')).resolves.toEqual({ ok: true });
    expect(mocks.invoke).toHaveBeenCalledWith('window_action', { action: 'close' });
  });

  it('routes resize drags to the shell bridge with the hotspot direction', async () => {
    window.__TAURI_INTERNALS__ = {};
    mocks.invoke.mockResolvedValue({ ok: true });
    await expect(startResize('northWest')).resolves.toEqual({ ok: true });
    expect(mocks.invoke).toHaveBeenCalledWith('window_resize_start', {
      direction: 'northWest',
    });
  });

  it('degrades to ok:false when the bridge call fails', async () => {
    window.__TAURI_INTERNALS__ = {};
    mocks.invoke.mockRejectedValue(new Error('bridge down'));
    await expect(windowAction('maximize')).resolves.toEqual({ ok: false });
  });
});
