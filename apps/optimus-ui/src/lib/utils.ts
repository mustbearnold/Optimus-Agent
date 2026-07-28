import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

/**
 * Join class names, letting a later Tailwind utility win over an earlier one of
 * the same kind. `clsx` alone would emit `px-2 px-4` and leave the winner to
 * source order in the generated stylesheet rather than to the caller.
 *
 * Every shadcn component takes a `className` and merges it through here, which
 * is what makes them restyleable in place instead of wrapped.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
