import type { ChatHandle, StreamEvent } from '../contracts';
import type { TurnOutcome } from './types';

/** The single documented place where a stream terminal is classified
 *  (ADR-0090). R4: a failed continuation is `failed`, a re-parked approval
 *  is `awaiting-approval`. R9: connection loss is not a model failure. */
export function classifyTerminal(
  event: StreamEvent | undefined,
  startError?: unknown
): TurnOutcome {
  if (startError !== undefined) {
    // A rejected chat start is a configuration error, not a transport loss:
    // surface the real cause (e.g. "No DeepSeek API key…") as `failed`.
    return { kind: 'failed', message: messageOf(startError) };
  }
  if (!event) {
    return { kind: 'failed', message: 'Turn ended without a terminal event.' };
  }
  switch (event.type) {
    case 'done':
      return { kind: 'completed' };
    case 'cancelled':
      return { kind: 'cancelled', error: event.error };
    case 'error': {
      const text = event.error;
      // spec-014 R4: the resolve/chat terminal payload decides the state.
      if (/resume_error/i.test(text)) return { kind: 'failed', message: text };
      if (/still_pending/i.test(text)) return { kind: 'awaiting-approval' };
      // spec-014 R9: connection loss synthesizes a terminal error.
      if (/connection lost|closed unexpectedly/i.test(text)) {
        return { kind: 'disconnected' };
      }
      return { kind: 'failed', message: text };
    }
    default:
      // Non-terminal event types (delta/thinking/tool/timing) can only
      // arrive via `onEvent`, never as a terminal — defensive.
      return { kind: 'failed', message: `Unexpected terminal event: ${event.type}` };
  }
}

export function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** One user request carried to exactly one terminal outcome (law 10). */
export interface Turn {
  /** Settles exactly once with the classified terminal. */
  readonly outcome: Promise<TurnOutcome>;
  /** The underlying stream handle, or null when no stream started. */
  readonly handle: ChatHandle | null;
  /** Cancel the underlying stream (idempotent; no-op with no handle). */
  cancel(): Promise<void>;
}

/** Wrap a `ChatHandle` (or a refused start) into a classified Turn.
 *
 *  `onStartFailure` mirrors a rejected start into the caller's event
 *  stream as `{ type: 'error', error }` — parity with the previous
 *  caller-side catch in OptimusApp, kept inside the module. */
export function createTurn(
  handle: ChatHandle | null,
  onStartFailure?: (message: string) => void
): Turn {
  const outcome = (handle ? handle.done : Promise.resolve(undefined)).then(
    (event) => classifyTerminal(event),
    (error) => {
      const message = messageOf(error);
      onStartFailure?.(message);
      return classifyTerminal(undefined, error);
    }
  );
  return {
    outcome,
    handle,
    cancel: async () => {
      if (handle) await handle.cancel();
    },
  };
}
