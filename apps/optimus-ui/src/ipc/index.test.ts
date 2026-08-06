import { beforeEach, describe, expect, it, vi } from 'vitest';

import { getTransport, initTransport, resetTransport } from './index';

describe('transport selection', () => {
  beforeEach(() => {
    resetTransport();
    delete window.__TAURI__;
    delete window.__TAURI_INTERNALS__;
    delete window.__OPTIMUS_BROKER_TICKET__;
  });

  it('selects the WS transport when a dev broker ticket global is present', async () => {
    window.__OPTIMUS_BROKER_TICKET__ = { port: 17865, ticket: 'dev-ticket' };
    await initTransport();
    expect(getTransport()?.kind).toBe('ws');
  });

  it('selects Tauri when the configured global bridge is present and no broker command exists', async () => {
    window.__TAURI__ = {};
    await initTransport();
    expect(getTransport()?.kind).toBe('tauri');
  });

  it('selects NO transport on confirmed broker absence in the packaged renderer', async () => {
    // The bridge is present and the broker command ANSWERS null: that is
    // a confirmed absence — the terminal affordance, never a fixture.
    window.__TAURI_INTERNALS__ = {};
    vi.stubGlobal(
      '__TAURI_INTERNALS__',
      {
        invoke: async () => null,
      }
    );
    await initTransport();
    expect(getTransport()).toBeNull();
    vi.unstubAllGlobals();
  });

  it('falls back to the fixture in dev with no bridge, no ticket, and no HTTP pairing', async () => {
    await initTransport();
    expect(getTransport()?.kind).toBe('fixture');
  });
});
