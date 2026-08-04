import { beforeEach, describe, expect, it } from 'vitest';
import { defaultLayout, loadLayout, railResizePatch, saveLayout } from './layoutStore';

describe('layout persistence', () => {
  beforeEach(() => localStorage.clear());

  it('recovers from malformed persistence', () => {
    localStorage.setItem('optimus.react.layout.v1', '{broken');
    expect(loadLayout()).toEqual(defaultLayout);
  });

  it('clamps widths and restores valid surface state', () => {
    localStorage.setItem(
      'optimus.react.layout.v1',
      JSON.stringify({
        leftWidth: 9,
        workspaceWidth: 9000,
        executionHeight: 1,
        workspaceTab: 'files',
        compactSurface: 'files',
      })
    );
    expect(loadLayout()).toMatchObject({
      leftWidth: 200,
      workspaceWidth: 1200,
      executionHeight: 120,
      workspaceTab: 'files',
      compactSurface: 'files',
    });
  });

  it('round-trips the versioned presentation schema', () => {
    const value = { ...defaultLayout, leftWidth: 280, workspaceTab: 'artifacts' as const };
    saveLayout(value);
    expect(loadLayout()).toEqual(value);
  });

  it('collapses only after a left drag crosses the rail threshold', () => {
    expect(railResizePatch(240, false, 240, 130)).toEqual({
      leftWidth: 200,
      leftCollapsed: false,
    });
    expect(railResizePatch(240, false, 240, 90)).toEqual({
      leftWidth: 240,
      leftCollapsed: true,
    });
  });

  it('reopens a collapsed rail from its 52px hit area and rejects invalid pointer data', () => {
    expect(railResizePatch(240, true, 52, 252)).toEqual({
      leftWidth: 252,
      leftCollapsed: false,
    });
    expect(railResizePatch(240, false, Number.NaN, 90)).toEqual({
      leftWidth: 240,
      leftCollapsed: false,
    });
  });
});
