import type { ChatHandle, StreamEvent } from '../contracts';
import { CONNECTION_LOSS_CODES, IpcError } from './types';
import type { TurnOutcome } from './types';

/** The single documented place where a stream terminal is classified
 *  (ADR-0090). R4: a failed continuation is `failed`, a re-parked approval
 *  is `awaiting-approval`. R9: connection loss is not a model failure —
 *  a structured `IpcError.code` (`connection_lost` / `closed_unexpectedly`,
 *  attached by the ws transport, #147) classifies FIRST; the text sniff
 *  remains the documented fallback for message-only rejections and the
 *  frozen wire terminal payload (fixture/tauri paths). */
export function classifyTerminal(
  event: StreamEvent | undefined,
  startError?: unknown
): TurnOutcome {
  if (startError !== undefined) {
    // A rejected chat start is usually a configuration error, not a
    // transport loss: surface the real cause (e.g. "No DeepSeek API
    // key…") as `failed`. The R9 exception is a structured
    // connection-loss code — or its sniffable text when the code is
    // absent (#147).
    if (isConnectionLoss(startError)) {
      return { kind: 'disconnected' };
    }
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

/** R9 connection-loss detection (ADR-0090, #147). A structured
 *  `IpcError.code` — when present — is authoritative: `connection_lost`
 *  (socket errored) and `closed_unexpectedly` (socket closed with
 *  requests/streams pending) classify as connection loss, and an
 *  unrelated code (`auth_failed`, …) NEVER does, whatever its message
 *  says. Message-only errors fall back to the documented text sniff. */
export function isConnectionLoss(error: unknown): boolean {
  if (error instanceof IpcError && error.code !== undefined) {
    return (CONNECTION_LOSS_CODES as readonly string[]).includes(error.code);
  }
  return /connection lost|closed unexpectedly/i.test(messageOf(error));
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
 *  A rejected start is a configuration error, not a transport loss: the
 *  REAL cause is surfaced as `failed` and mirrored into the caller's
 *  event stream as `{ type: 'error' }` — parity with the previous
 *  caller-side catch in OptimusApp. An `AbortError` rejection is the
 *  user's cancel: classified `cancelled` and mirrored as such. */
export function createTurn(
  handle: ChatHandle | null,
  onStartFailure?: (event: StreamEvent) => void
): Turn {
  const outcome = (handle ? handle.done : Promise.resolve(undefined)).then(
    (event) => classifyTerminal(event),
    (error) => {
      if (isAbortError(error)) {
        const event: StreamEvent = { type: 'cancelled', error: 'cancelled by user' };
        onStartFailure?.(event);
        return { kind: 'cancelled', error: 'cancelled by user' } as const;
      }
      const message = messageOf(error);
      onStartFailure?.({ type: 'error', error: message });
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

export function isAbortError(error: unknown): boolean {
  return (
    error instanceof DOMException && error.name === 'AbortError'
  );
}
