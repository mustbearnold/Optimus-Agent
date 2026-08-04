import { beforeEach, describe, expect, it } from 'vitest';

import { getTransport } from './index';

describe('transport selection', () => {
  beforeEach(() => {
    window.__TAURI__ = {};
    delete window.__TAURI_INTERNALS__;
    delete window.optimusElectron;
  });

  it('selects Tauri when the configured global bridge is present', () => {
    expect(getTransport().kind).toBe('tauri');
  });
});
