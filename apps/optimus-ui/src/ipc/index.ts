import type { OptimusTransport } from './contracts';
import { createFixtureTransport } from './fixtureTransport';
import { createHttpTransport, hasHttpConfig } from './httpTransport';
import { createTauriTransport } from './tauriTransport';
import { createWsTransport, type BrokerTicket } from './wsTransport';
import { isPackaged } from './windowBridge';

let transport: OptimusTransport | null = null;
let initPromise: Promise<OptimusTransport | null> | null = null;

/**
 * The packaged-vs-dev discriminator (spec-001 R8's existing predicate,
 * pinned by spec-015 A3): the Tauri bridge exists ONLY in the packaged
 * webview. Dev-mode tests that fake it must set `__TAURI_INTERNALS__`.
 * Single implementation lives in `windowBridge.ts` (the shell-owned
 * chrome seam) and is shared with the transport selection here.
 */

/**
 * Await the broker ticket (or its confirmed absence) BEFORE the first
 * transport construction — the transport is created once and cached, so
 * a wrong ordering silently picks HTTP/fixture in the packaged app
 * (spec-015 A3, `index.ts:6` ordering pin).
 *
 * Broker states:
 *  - ws ticket global present (dev injection) → WS transport.
 *  - Packaged + broker answers a ws record → WS transport.
 *  - Packaged + broker answers NO ticket → CONFIRMED absence → no
 *    transport (the terminal affordance; never a silent fixture).
 *  - Packaged + no broker command yet (pre-lifecycle shell) → the
 *    in-process Tauri transport (the pre-wire world; A4 adds the broker).
 *  - Dev + no ticket + HTTP pairing query → HTTP transport (dev-only).
 *  - Dev + neither → fixture transport (dev-only).
 */
async function resolveBroker(): Promise<BrokerTicket | 'none' | 'unavailable'> {
  const globalTicket = window.__OPTIMUS_BROKER_TICKET__;
  if (globalTicket) return globalTicket;
  if (isPackaged()) {
    try {
      // The shell broker command (added with the desktop lifecycle, A4):
      // answers the healthy ws record, or null when no serve is available.
      const { invoke } = await import('@tauri-apps/api/core');
      const record = await invoke<{ port: number; ticket: string } | null>('broker_ticket');
      return record ?? 'none';
    } catch {
      // No broker command in this shell yet: the in-process bridge still
      // owns the surface (the pre-wire window).
      return 'unavailable';
    }
  }
  return 'none';
}

/**
 * Initialize the surface transport exactly once. Resolves to the chosen
 * transport, or null when the packaged renderer confirmed broker absence
 * (the terminal affordance). Safe to call any number of times.
 */
export function initTransport(): Promise<OptimusTransport | null> {
  if (initPromise) return initPromise;
  initPromise = (async () => {
    const broker = await resolveBroker();
    if (broker !== 'none' && broker !== 'unavailable') {
      transport = createWsTransport(broker);
    } else if (broker === 'unavailable') {
      // Pre-broker shell: the in-process Tauri bridge owns the surface.
      transport = createTauriTransport();
    } else if (isPackaged()) {
      // Confirmed absence: the bridge is present and the broker answered
      // no ticket — NO transport, the terminal affordance (never a
      // silent fixture in the packaged renderer).
      transport = null;
    } else if (hasHttpConfig()) {
      transport = createHttpTransport(); // dev-only
    } else {
      transport = createFixtureTransport(); // dev-only
    }
    return transport;
  })();
  return initPromise;
}

/** The cached transport, or null before `initTransport()` settles. */
export function getTransport(): OptimusTransport | null {
  return transport;
}

/** Drop the cached transport and re-run the broker next init (Retry). */
export function resetTransport(): void {
  transport = null;
  initPromise = null;
}

export type * from './contracts';
