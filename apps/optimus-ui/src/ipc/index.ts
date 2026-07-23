import type { OptimusTransport } from './contracts';
import { createElectronTransport } from './electronTransport';
import { createFixtureTransport } from './fixtureTransport';
import { createHttpTransport, hasHttpConfig } from './httpTransport';

let transport: OptimusTransport | null = null;

export function getTransport(): OptimusTransport {
  if (transport) return transport;
  if (window.optimusElectron?.isElectron) {
    transport = createElectronTransport();
  } else if (hasHttpConfig()) {
    transport = createHttpTransport();
  } else {
    transport = createFixtureTransport();
  }
  return transport;
}

export type * from './contracts';
