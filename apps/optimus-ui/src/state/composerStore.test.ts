import { beforeEach, describe, expect, it } from 'vitest';
import {
  autoComposer,
  codexComposer,
  loadComposer,
  modelOverride,
  offlineComposer,
  PROVIDER_MODELS,
  REASONING_LEVELS,
  restoredAccess,
  saveComposer,
} from './composerStore';

describe('composer Auto persistence and resolution', () => {
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

  it('rejects an unknown provider or non-string model', () => {
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({ provider: 'skynet', model: 'x' })
    );
    expect(loadComposer()).toBeNull();
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({ provider: 'codex', model: null })
    );
    expect(loadComposer()).toBeNull();
  });

  it('round-trips Auto without inventing a model id', () => {
    saveComposer(autoComposer, false);
    expect(loadComposer()).toEqual({
      settings: autoComposer,
      providerChosen: false,
    });
    expect(modelOverride(loadComposer()!.settings.model)).toBeUndefined();
  });

  it('drops an invalid persisted model override from Auto', () => {
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({ ...autoComposer, model: 'gpt-5.6-sol', providerChosen: true })
    );
    expect(loadComposer()?.settings).toEqual(autoComposer);
  });

  it('migrates offline residue nobody chose to Auto', () => {
    // Any settings click persists the whole object; provider:'offline' saved
    // before sign-in represented no durable provider intent.
    saveComposer(offlineComposer, false);
    expect(loadComposer()).toEqual({
      settings: autoComposer,
      providerChosen: false,
    });
  });

  it('migrates the legacy OpenAI module spelling to the canonical wire id', () => {
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({
        provider: 'openai_compat',
        model: 'gpt-4.1',
        thinking: 'medium',
        access: 'standard',
        fast: false,
        providerChosen: true,
      })
    );
    expect(loadComposer()).toEqual({
      settings: {
        provider: 'open-ai-compat',
        model: 'gpt-4.1',
        thinking: 'medium',
        access: 'standard',
        fast: false,
      },
      providerChosen: true,
    });
  });

  it('drops a persisted model that is not owned by its provider', () => {
    localStorage.setItem(
      'optimus.react.composer',
      JSON.stringify({
        provider: 'openai_compat',
        model: 'gpt-5.6-sol',
        providerChosen: true,
      })
    );
    expect(loadComposer()?.settings).toEqual({
      provider: 'open-ai-compat',
      model: '',
      thinking: 'high',
      access: 'standard',
      fast: false,
    });
  });

  it('keeps explicit human provider choices sticky', () => {
    saveComposer(offlineComposer, true);
    expect(loadComposer()?.settings).toEqual(offlineComposer);
    saveComposer(codexComposer, true);
    expect(loadComposer()?.settings).toEqual(codexComposer);
  });

  it('keeps Auto as the durable selector for the canonical router', () => {
    expect(autoComposer).toEqual(expect.objectContaining({ provider: 'auto', model: '' }));
    expect(PROVIDER_MODELS).toEqual(expect.objectContaining({
      auto: [],
      offline: ['offline-scripted'],
      deepseek: ['deepseek-v4-flash', 'deepseek-v4-pro'],
      'open-ai-compat': ['gpt-4.1', 'gpt-4o'],
    }));
    expect(REASONING_LEVELS.map(({ value }) => value)).toEqual([
      'auto',
      'minimal',
      'low',
      'medium',
      'high',
      'xhigh',
      'max',
      'ultra',
    ]);
  });

  it('persists DeepSeek V4 model ownership and Auto reasoning', () => {
    saveComposer(
      {
        provider: 'deepseek',
        model: 'deepseek-v4-pro',
        thinking: 'auto',
        access: 'standard',
        fast: false,
      },
      true
    );
    expect(loadComposer()?.settings).toEqual({
      provider: 'deepseek',
      model: 'deepseek-v4-pro',
      thinking: 'auto',
      access: 'standard',
      fast: false,
    });
  });

  it('returns only real explicit model overrides', () => {
    expect(modelOverride('')).toBeUndefined();
    expect(modelOverride('   ')).toBeUndefined();
    expect(modelOverride(' gpt-5.6-sol ')).toBe('gpt-5.6-sol');
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
