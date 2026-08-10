/**
 * Client-wide types for the renderer deep module (ADR-0090).
 *
 * The wire (spec-015) is frozen: this module wraps `OptimusTransport`
 * without changing it. Everything a caller must know about failure and
 * terminal outcomes lives here.
 */

/** Exactly one terminal classification per turn (law 10, ADR-0090).
 *  R4 (resume_error/still_pending) and R9 (connection loss) are folded
 *  into `failed` / `awaiting-approval` / `disconnected` in `turn.ts`. */
export type TurnOutcome =
  | { kind: 'completed' }
  | { kind: 'failed'; message: string }
  | { kind: 'cancelled'; error?: string }
  | { kind: 'awaiting-approval' }
  | { kind: 'disconnected' };

/** Known connection-loss codes (ADR-0090 follow-up, #147). `code` is an
 *  open string — a future wire or server frame may carry an unrelated
 *  code and must NOT classify as connection loss — and the classifier
 *  only matches these two. */
export type IpcErrorCode = 'connection_lost' | 'closed_unexpectedly';

/** The codes the R9 classifier treats as connection loss. */
export const CONNECTION_LOSS_CODES: readonly IpcErrorCode[] = [
  'connection_lost',
  'closed_unexpectedly',
];

/** A transport-level failure surfaced as a typed error (ADR-0090).
 *  The wire flattens JSON-RPC codes to text, so `code` is undefined for
 *  server-side failures; the ws transport attaches a structured
 *  connection-loss cause when the socket itself failed (additive, #147). */
export class IpcError extends Error {
  readonly code?: string;
  constructor(message: string, code?: string) {
    super(message);
    this.name = 'IpcError';
    this.code = code;
  }
}

/** The packaged renderer confirmed broker absence (spec-015 A6): the
 *  terminal affordance. Every client call rejects with this. */
export class NoTransportError extends Error {
  constructor() {
    super('The Optimus host is not reachable (no transport).');
    this.name = 'NoTransportError';
  }
}

/** A second send-turn started while one is still live (ADR-0090). */
export class TurnInFlightError extends Error {
  constructor() {
    super('A turn is already in flight for this session.');
    this.name = 'TurnInFlightError';
  }
}
