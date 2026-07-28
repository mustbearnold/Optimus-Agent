import { beforeEach, describe, expect, it } from 'vitest';
import {
  codexComposer,
  loadComposer,
  offlineComposer,
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
