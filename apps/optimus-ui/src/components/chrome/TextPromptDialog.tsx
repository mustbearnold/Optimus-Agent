import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from 'react';

/** In-app replacement for window.prompt — styled, focusable, Escape-safe. */
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
  const [value, setValue] = useState(initialValue);

  useEffect(() => {
    if (!open) return;
    setValue(initialValue);
    requestAnimationFrame(() => {
      input.current?.focus();
      input.current?.select();
    });
  }, [open, initialValue]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        onCancel();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [open, onCancel]);

  if (!open) return null;

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const next = value.trim();
    if (!next) return;
    onConfirm(next);
  };

  return (
    <div
      className="dialog-backdrop text-prompt-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
      onKeyDown={(event: KeyboardEvent) => {
        if (event.key === 'Escape') onCancel();
      }}
    >
      <form
        className="text-prompt-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="text-prompt-title"
        onSubmit={submit}
      >
        <h2 id="text-prompt-title">{title}</h2>
        <label className="field-stack">
          <span>{label}</span>
          <input
            ref={input}
            value={value}
            onChange={(event) => setValue(event.target.value)}
            aria-label={label}
          />
        </label>
        <footer>
          <button type="button" className="secondary-action" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="primary-action" disabled={!value.trim()}>
            {confirmLabel}
          </button>
        </footer>
      </form>
    </div>
  );
}
