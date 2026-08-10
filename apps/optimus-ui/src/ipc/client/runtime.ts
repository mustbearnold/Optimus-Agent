import type { StreamEvent } from '../contracts';

/** An ordered, observable record of every renderer→host interaction
 *  (law 11, ADR-0090). Ring-buffered for diagnostics; `subscribe` for
 *  live sinks. */
export type RuntimeEvent =
  | { type: 'invoke'; method: string; ok: boolean; at: number }
  | { type: 'stream'; method: string; terminal?: StreamEvent['type']; at: number };

/** `RuntimeEvent` without the timestamp — what `record` takes. */
export type RuntimeEventInput =
  | { type: 'invoke'; method: string; ok: boolean }
  | { type: 'stream'; method: string; terminal?: StreamEvent['type'] };

export class RuntimeObserver {
  private readonly events: RuntimeEvent[] = [];
  private readonly listeners = new Set<(event: RuntimeEvent) => void>();

  constructor(private readonly capacity = 200) {}

  record(event: RuntimeEventInput): void {
    const full: RuntimeEvent = { ...event, at: Date.now() };
    this.events.push(full);
    if (this.events.length > this.capacity) {
      this.events.splice(0, this.events.length - this.capacity);
    }
    for (const listener of this.listeners) listener(full);
  }

  /** The most recent events, in arrival order. */
  tail(limit = 50): RuntimeEvent[] {
    return this.events.slice(-limit);
  }

  subscribe(listener: (event: RuntimeEvent) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
}
