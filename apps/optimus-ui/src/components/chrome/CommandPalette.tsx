import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';

import type { OptimusTransport } from '../../ipc/contracts';

export type PaletteCommand = {
  id: string;
  name: string;
  description: string;
  surface?: string;
};

/**
 * The command palette, on cmdk rather than a `<ul>` of buttons.
 *
 * The hand-written version could not be driven from the keyboard. Arrow keys
 * did nothing, so the only way through the list was Tab — one stop per command,
 * out of the dialog and into the page once you reached the end — or the mouse.
 * A command palette is a keyboard affordance; that one worked everywhere except
 * the keyboard.
 *
 * It also had no listbox semantics, so a screen reader read a stack of unrelated
 * buttons with no "3 of 12" position, no announcement when the filter changed
 * the list out from under it, and no way to tell which one was active. cmdk
 * supplies the roving `aria-activedescendant`, the `role="listbox"` /
 * `role="option"` pairing, and the live region. Radix supplies the focus trap,
 * the scroll lock, and the aria-hiding of everything behind (ADR-0050).
 *
 * Filtering stays exactly as it was — see `shouldFilter` below.
 */
export function CommandPalette({
  open,
  transport,
  onClose,
  onRun,
}: {
  open: boolean;
  transport: OptimusTransport;
  onClose: () => void;
  onRun: (commandId: string) => void;
}) {
  const [commands, setCommands] = useState<PaletteCommand[]>([]);
  const [query, setQuery] = useState('');
  const [error, setError] = useState('');
  const opener = useRef<HTMLElement | null>(null);
  const field = useRef<HTMLInputElement>(null);

  // Layout, not passive: React runs every layout effect before any passive one,
  // and Radix moves focus into the dialog from a passive effect. Reading
  // `activeElement` any later reads the input Radix just focused.
  useLayoutEffect(() => {
    if (open) opener.current = document.activeElement as HTMLElement | null;
  }, [open]);

  useEffect(() => {
    if (!open) return;
    setQuery('');
    setError('');
    void transport
      .invoke<{ commands?: PaletteCommand[] }>('commands_list', { surface: 'desktop' })
      .then((r) => setCommands(r.commands || []))
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }, [open, transport]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter(
      (c) =>
        c.id.toLowerCase().includes(q) ||
        c.name.toLowerCase().includes(q) ||
        c.description.toLowerCase().includes(q)
    );
  }, [commands, query]);

  // Works around cmdk 1.1.1 not announcing the row it opens on.
  //
  // cmdk marks the first command `aria-selected` as soon as the list renders,
  // but it only publishes `aria-activedescendant` from inside its `setState`
  // handler for `value` — which nothing calls until the user presses a key. So
  // the palette opens with a row highlighted on screen and nothing current to a
  // screen reader, and the first ArrowDown announces the *second* command. The
  // first one is silently skipped.
  //
  // Passing a controlled `value` makes it worse rather than better: that path
  // assigns cmdk's internal state directly and bypasses the `setState` that does
  // the publishing. So the id is mirrored across here instead, and only while
  // cmdk has not set one itself — every later render is cmdk's.
  //
  // Watched rather than derived from this component's render cycle: cmdk moves
  // the selection by re-rendering the item, not the palette, so a plain effect
  // here would run once on mount — before anything is selected — and never
  // again. The observer follows the attribute cmdk actually maintains.
  useEffect(() => {
    if (!open) return;
    const input = field.current;
    const listId = input?.getAttribute('aria-controls');
    const list = listId ? document.getElementById(listId) : null;
    if (!input || !list) return;

    const sync = () => {
      const selected = list.querySelector('[cmdk-item][aria-selected="true"]');
      if (selected?.id) input.setAttribute('aria-activedescendant', selected.id);
      else input.removeAttribute('aria-activedescendant');
    };
    sync();

    const observer = new MutationObserver(sync);
    observer.observe(list, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ['aria-selected'],
    });
    return () => observer.disconnect();
  }, [open, filtered]);

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <DialogContent
        className="p-0"
        showCloseButton={false}
        onCloseAutoFocus={(event) => {
          // Radix restores to its `DialogTrigger`; this palette is opened from a
          // keybinding and has none, so without this the caret lands on <body>.
          event.preventDefault();
          opener.current?.focus();
        }}
      >
        <DialogHeader className="sr-only">
          <DialogTitle>Command palette</DialogTitle>
        </DialogHeader>
        {/*
         * cmdk's own filter is a fuzzy scorer over each item's `value`. Turning
         * it off keeps the substring match this palette already had — over id,
         * name *and* description — so the conversion changes how the list is
         * navigated without changing what it contains. Ranking is a separate
         * decision from accessibility, and bundling the two would make this
         * diff impossible to review.
         */}
        <Command shouldFilter={false} loop>
          <CommandInput
            ref={field}
            placeholder="Type a command…"
            aria-label="Filter commands"
            value={query}
            onValueChange={setQuery}
          />
          <CommandList>
            <CommandEmpty>No matching commands.</CommandEmpty>
            {filtered.map((cmd) => (
              <CommandItem
                key={cmd.id}
                value={cmd.id}
                onSelect={() => {
                  onRun(cmd.id);
                  onClose();
                }}
                className="flex-col items-start gap-0.5"
              >
                <strong>/{cmd.name}</strong>
                <span className="text-muted-foreground">{cmd.description}</span>
              </CommandItem>
            ))}
          </CommandList>
        </Command>
        <p className="panel-muted px-3 pb-3">Surface catalog only — not a tool registry.</p>
        {error ? <div className="surface-error mx-3 mb-3">{error}</div> : null}
      </DialogContent>
    </Dialog>
  );
}
