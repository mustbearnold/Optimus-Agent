// Durable provider/model intent. `auto` is a real user-facing selection, not
// a temporary alias for whichever provider happened to be available at boot.
// Resolution happens at send/display time, so authentication may change the
// concrete route without overwriting the durable Auto choice.

export type ComposerProvider = 'auto' | 'offline' | 'codex' | 'open-ai-compat';

export type ComposerSettings = {
  provider: ComposerProvider;
  // Empty means Auto: omit the model field and let the selected provider's
  // routing contract choose. Never persist or send a made-up model id.
  model: string;
  thinking: string;
  access: string;
  fast: boolean;
};

export type StoredComposer = {
  settings: ComposerSettings;
  // True only after the human picked a provider by hand. Every settings
  // change persists the whole object, so a stored provider:'offline' does
  // not by itself mean offline was chosen.
  providerChosen: boolean;
};

const STORAGE_KEY = 'optimus.react.composer';
const PROVIDERS: ReadonlyArray<ComposerProvider> = [
  'auto',
  'offline',
  'codex',
  'open-ai-compat',
];

// Mirrors the canonical provider catalog in optimus-kernel/src/routing.rs.
// The empty option rendered by Composer is Model Auto and is deliberately not
// a model identity in this table.
export const PROVIDER_MODELS: Readonly<Record<ComposerProvider, readonly string[]>> = {
  auto: [],
  offline: ['offline-scripted'],
  codex: [
    'gpt-5.6-sol',
    'gpt-5.6-terra',
    'gpt-5.6-luna',
    'gpt-5.5',
    'gpt-5.4',
    'gpt-5.4-mini',
    'gpt-5.3-codex-spark',
  ],
  'open-ai-compat': ['gpt-4.1', 'gpt-4o'],
};

// The ADR-0044 profile a stored access value means today. Builds before #118
// wrote the composer's own three-word vocabulary, whose first item — 'full' —
// meant unrestricted host; those words are read here and nowhere else.
// Prototype-free: a plain literal answers to 'constructor' and '__proto__'
// with something that is not a profile, and a lookup table that returns
// truthy for words nobody put in it is not a table.
const ACCESS_ALIASES: Readonly<Record<string, string>> = Object.assign(Object.create(null), {
  standard: 'standard',
  review_changes: 'review_changes',
  smart_deny: 'review_changes',
  ask: 'review_changes',
  read_only: 'read_only',
  read: 'read_only',
  full_project: 'full_project',
  developer_full_access: 'developer_full_access',
});

/**
 * The profile a stored value restores to.
 *
 * Two values do not restore to themselves. Legacy `'full'` said "Full access"
 * on the label and meant unrestricted host underneath, so restoring it would
 * carry authority nobody knowingly picked; and `'unrestricted_host'` is
 * break-glass, which ADR-0044 §5 keeps out of anything durable — break-glass
 * that survives a restart is not break-glass. Both land on Standard, and the
 * expert choice is one deliberate click away.
 */
export function restoredAccess(raw: unknown): string {
  if (typeof raw !== 'string') return 'standard';
  return ACCESS_ALIASES[raw.trim().toLowerCase()] ?? 'standard';
}

export const offlineComposer: ComposerSettings = {
  provider: 'offline',
  model: 'offline-scripted',
  thinking: 'high',
  access: 'standard',
  fast: false,
};

// Model mirrors DEFAULT_CODEX_MODEL in optimus-kernel/src/routing.rs.
export const codexComposer: ComposerSettings = {
  provider: 'codex',
  model: 'gpt-5.6-terra',
  thinking: 'high',
  access: 'standard',
  fast: false,
};

export const autoComposer: ComposerSettings = {
  provider: 'auto',
  model: '',
  thinking: 'high',
  access: 'standard',
  fast: false,
};

export function loadComposer(): StoredComposer | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const storedProvider = parsed.provider;
    let provider: ComposerProvider;
    if (storedProvider === 'openai_compat') {
      // Pre-canonical React builds persisted the Rust module spelling rather
      // than the provider catalog's wire identity.
      provider = 'open-ai-compat';
    } else if (
      typeof storedProvider === 'string' &&
      PROVIDERS.includes(storedProvider as ComposerProvider)
    ) {
      provider = storedProvider as ComposerProvider;
    } else {
      return null;
    }
    if (typeof parsed.model !== 'string') return null;
    let model = parsed.model;
    const providerChosen = parsed.providerChosen === true;
    // Builds before Auto used providerChosen:false + Offline as their
    // "works before sign-in" residue. Preserve the lack of intent by
    // migrating that exact legacy shape to Auto.
    if (!providerChosen && provider === 'offline') {
      provider = 'auto';
      model = '';
    }
    if (provider === 'auto') model = '';
    if (model && !PROVIDER_MODELS[provider].includes(model)) model = '';
    return {
      settings: {
        provider,
        model,
        thinking: typeof parsed.thinking === 'string' ? parsed.thinking : 'high',
        access: restoredAccess(parsed.access),
        fast: parsed.fast === true,
      },
      providerChosen,
    };
  } catch {
    return null;
  }
}

export function saveComposer(settings: ComposerSettings, providerChosen: boolean): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...settings, providerChosen }));
  } catch {
    // Storage full or denied: the session still works, the choice just
    // does not survive a reload.
  }
}

export function modelOverride(model: string): string | undefined {
  const value = model.trim();
  return value || undefined;
}
