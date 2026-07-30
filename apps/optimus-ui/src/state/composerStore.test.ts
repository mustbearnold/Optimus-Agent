import { beforeEach, describe, expect, it } from 'vitest';
import {
  codexComposer,
  loadComposer,
  offlineComposer,
  restoredAccess,
  saveComposer,
  shouldPreferCodex,
} from './composerStore';

describe('composer persistence and the codex-preference rule (#82)', () => {
  beforeEach(() => localStorage.clear());

  it('round-trips a saved choice', () => {
    saveComposer({ ...codexComposer, thinking: 'low', fast: true }, true);
    expect(loadComposer()).toEqual({
      settings: { ...codexComposer, thinking: 'low', fast: true },
      providerChosen: true,
    });
  });

  it('recovers from malformed persistence', () => {
    localStorage.setItem('optimus.react.composer', '{broken');
    expect(loadComposer()).toBeNull();
  });

  it('rejects an unknown provider or missing model', () => {
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({ provider: 'skynet', model: 'x' })
    );
    expect(loadComposer()).toBeNull();
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({ provider: 'codex', model: '' })
    );
    expect(loadComposer()).toBeNull();
  });

  it('prefers codex on a fresh profile', () => {
    expect(shouldPreferCodex()).toBe(true);
  });

  it('prefers codex over offline residue nobody chose', () => {
    // Any settings click persists the whole object; provider:'offline' saved
    // before sign-in must not pin the app to the echo model afterwards.
    saveComposer(offlineComposer, false);
    expect(shouldPreferCodex()).toBe(true);
  });

  it('never overrides an explicit human provider choice', () => {
    saveComposer(offlineComposer, true);
    expect(shouldPreferCodex()).toBe(false);
    saveComposer(codexComposer, true);
    expect(shouldPreferCodex()).toBe(false);
  });
});

describe('what a stored access value restores to (#118)', () => {
  beforeEach(() => localStorage.clear());

  it('restores the ADR-0044 profile behind each legacy word', () => {
    expect(restoredAccess('ask')).toBe('review_changes');
    expect(restoredAccess('smart_deny')).toBe('review_changes');
    expect(restoredAccess('read')).toBe('read_only');
    expect(restoredAccess('standard')).toBe('standard');
    expect(restoredAccess('full_project')).toBe('full_project');
  });

  it('does not restore break-glass, however it was spelled', () => {
    // 'full' was the label of a menu item that meant the whole host, and
    // unrestricted host is break-glass: ADR-0044 §5 keeps it out of anything
    // durable. Both come back as Standard, which the human can raise again in
    // one deliberate click.
    for (const stored of ['full', 'unrestricted_host', 'unrestricted', 'yolo', 'host']) {
      expect(restoredAccess(stored)).toBe('standard');
    }
  });

  it('treats an unknown or absent value as Standard', () => {
    expect(restoredAccess('nonsense')).toBe('standard');
    expect(restoredAccess(undefined)).toBe('standard');
    expect(restoredAccess(7)).toBe('standard');
    // Inherited property names are unknown values like any other: a lookup
    // table that answers for words nobody put in it is not a table.
    for (const inherited of ['constructor', '__proto__', 'toString', 'valueOf']) {
      expect(restoredAccess(inherited)).toBe('standard');
    }
  });

  it('reads a whole stored composer back at the migrated profile', () => {
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({ ...codexComposer, access: 'full', providerChosen: true })
    );
    expect(loadComposer()?.settings.access).toBe('standard');
  });
});
