import { beforeEach, describe, expect, it } from 'vitest';
import { defaultLayout, loadLayout, saveLayout } from './layoutStore';

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
});
