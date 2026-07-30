// Which provider/model the composer boots with, and when the app may change
// it. The rule (#82): an explicit human provider choice always wins; absent
// one, prefer Codex the moment auth is present and offline otherwise, so
// first-run chat always works but pre-sign-in offline residue never outlives
// the sign-in.

export type ComposerSettings = {
  provider: 'offline' | 'codex' | 'openai_compat';
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
const PROVIDERS: ReadonlyArray<ComposerSettings['provider']> = [
  'offline',
  'codex',
  'openai_compat',
];

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
  model: 'offline-echo',
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

export function loadComposer(): StoredComposer | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const provider = parsed.provider as ComposerSettings['provider'];
    if (!PROVIDERS.includes(provider)) return null;
    if (typeof parsed.model !== 'string' || !parsed.model) return null;
    return {
      settings: {
        provider,
        model: parsed.model,
        thinking: typeof parsed.thinking === 'string' ? parsed.thinking : 'high',
        access: restoredAccess(parsed.access),
        fast: parsed.fast === true,
      },
      providerChosen: parsed.providerChosen === true,
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

// Whether boot may promote the composer to Codex: nothing stored, or the
// stored provider is offline residue nobody explicitly chose.
export function shouldPreferCodex(): boolean {
  const stored = loadComposer();
  return !stored || (!stored.providerChosen && stored.settings.provider === 'offline');
}
