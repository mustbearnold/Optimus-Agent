import type {
  ApprovalResolveRequest,
  ChatRequest,
  OptimusTransport,
  StreamEvent,
  ToolApprovalBinding,
} from '../contracts';
import type { RuntimeObserver } from './runtime';
import { createTurn, type Turn } from './turn';
import { NoTransportError, TurnInFlightError } from './types';
import type { TurnOutcome } from './types';

/** Everything a caller must know to start a chat turn (ADR-0090). Wire
 *  names (spec-015 `ChatRequest`) are mapped here, in one place. */
export interface SendOptions {
  message: string;
  provider: ChatRequest['provider'];
  model?: string;
  thinkingLevel?: string;
  fast?: boolean;
  access?: string;
  projectId?: string;
}

export interface SendResult {
  readonly turn: Turn;
  /** Settles exactly once: completed / failed / cancelled /
   *  awaiting-approval / disconnected. */
  readonly outcome: Promise<TurnOutcome>;
}

/** A pre-bound conversation: the dominant flow, one line each
 *  (ADR-0090). One live send-turn per session. */
export class ChatSession {
  private active: Turn | null = null;

  constructor(
    private readonly transport: OptimusTransport | null,
    private readonly sessionId: string,
    private readonly observer?: RuntimeObserver
  ) {}

  get session(): string {
    return this.sessionId;
  }

  /** True while a send/approve turn is live (no second turn starts). */
  get busy(): boolean {
    return this.active !== null;
  }

  send(options: SendOptions, onEvent: (event: StreamEvent) => void): SendResult {
    if (this.active) throw new TurnInFlightError();
    if (!this.transport) throw new NoTransportError();
    const request: ChatRequest = {
      session: this.sessionId,
      message: options.message,
      provider: options.provider,
      ...(options.model ? { model: options.model } : {}),
      ...(options.thinkingLevel ? { thinking_level: options.thinkingLevel } : {}),
      ...(options.fast !== undefined ? { fast: options.fast } : {}),
      ...(options.access ? { access: options.access } : {}),
      ...(options.projectId ? { project_id: options.projectId } : {}),
    };
    const handle = this.transport.chat(request, onEvent);
    this.observer?.record({ type: 'stream', method: 'chat_start' });
    return this.arm(createTurn(handle, (message) => {
      // Parity: a rejected start still reaches the transcript as an error
      // event (previously the caller synthesized it in a catch).
      onEvent({ type: 'error', error: message });
    }));
  }

  /** Resolve a parked approval as a streaming turn (ADR-0046): the
   *  continuation's events arrive as they happen and stay cancellable. */
  approve(
    binding: ToolApprovalBinding,
    decision: 'approve' | 'deny',
    projectId?: string,
    onEvent?: (event: StreamEvent) => void
  ): SendResult {
    if (this.active) throw new TurnInFlightError();
    if (!this.transport) throw new NoTransportError();
    const request: ApprovalResolveRequest = {
      session_id: this.sessionId,
      run_id: binding.run_id,
      call_id: binding.call_id,
      job_id: binding.job_id,
      node_id: binding.node_id,
      node_index: binding.node_index,
      effect_sha256: binding.effect_sha256,
      decision,
      ...(projectId ? { project_id: projectId } : {}),
    };
    const listener = onEvent ?? (() => undefined);
    const handle = this.transport.chatApprovalResolve(request, listener);
    this.observer?.record({ type: 'stream', method: 'chat_approval_resolve_start' });
    return this.arm(createTurn(handle));
  }

  /** Cancel the live turn (idempotent). Rejection propagates when the
   *  stream is already gone (the caller maps that to disconnected). */
  async cancel(): Promise<void> {
    await this.active?.cancel();
  }

  private arm(turn: Turn): SendResult {
    this.active = turn;
    void turn.outcome.then(
      () => {
        if (this.active === turn) this.active = null;
      },
      () => {
        if (this.active === turn) this.active = null;
      }
    );
    return { turn, outcome: turn.outcome };
  }
}
