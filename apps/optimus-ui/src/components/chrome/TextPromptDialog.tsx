import { useLayoutEffect, useRef, useState, type FormEvent } from 'react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

/**
 * In-app replacement for `window.prompt`.
 *
 * The hand-written version had the parts you can see — `role="dialog"`,
 * `aria-modal`, a labelled title, Escape, backdrop dismissal, focus on open —
 * and none of the parts you cannot. Tab walked straight out of it into the page
 * behind; closing it left focus on `<body>` rather than back where it started;
 * the page kept scrolling underneath; and a screen reader could still read the
 * whole application through the "modal". Radix supplies those four, which is
 * the reason this component now delegates rather than re-implements
 * (ADR-0050).
 *
 * The public props are unchanged, so no caller moves.
 */
export function TextPromptDialog({
  open,
  title,
  label,
  initialValue = '',
  confirmLabel = 'Save',
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  label: string;
  initialValue?: string;
  confirmLabel?: string;
  onConfirm: (value: string) => void;
  onCancel: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const [value, setValue] = useState(initialValue);

  // A layout effect, not a passive one, and the distinction is the whole point:
  // Radix moves focus into the dialog from a passive effect, and React runs
  // every layout effect in a commit before any passive one. Reading
  // `activeElement` from `useEffect` would read the input Radix just focused.
  useLayoutEffect(() => {
    if (!open) return;
    opener.current = document.activeElement as HTMLElement | null;
    setValue(initialValue);
  }, [open, initialValue]);

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const next = value.trim();
    if (!next) return;
    onConfirm(next);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
    >
      <DialogContent
        // The close affordance here is Cancel, which is already in the footer
        // and carries a word rather than a glyph.
        showCloseButton={false}
        // Radix focuses the first tabbable child on open. This dialog exists to
        // collect one value, so the caret belongs in the field with the
        // existing text selected — the behaviour `window.prompt` has.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          input.current?.focus();
          input.current?.select();
        }}
        // Radix restores focus to its `DialogTrigger`, and this dialog has
        // none — it is opened from application state, like every dialog in this
        // surface. Left alone, closing drops the caret on `<body>` and a
        // keyboard user tabs from the top of the app to get back. So the opener
        // is captured above and returned to here.
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          opener.current?.focus();
        }}
      >
        <form onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
          </DialogHeader>
          <Label className="mt-4 flex flex-col items-start gap-2">
            <span>{label}</span>
            <Input
              ref={input}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              aria-label={label}
            />
          </Label>
          <DialogFooter className="mt-6">
            <Button type="button" variant="secondary" onClick={onCancel}>
              Cancel
            </Button>
            <Button type="submit" disabled={!value.trim()}>
              {confirmLabel}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
