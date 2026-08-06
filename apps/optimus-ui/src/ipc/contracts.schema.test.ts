import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * TS-side surface-contract conformance (spec-015 A5): the committed
 * wire schema must equal the renderer's method surface plus the
 * protocol-method set, and the schema's event set must equal the
 * runtime stream vocabulary plus the server-origin notifications. The
 * renderer union is read from the contracts.ts source (the same source
 * the Python gate parses) so the two gates can never drift apart.
 */

const UI = join(__dirname, '..', '..');
const ROOT = join(UI, '..', '..');
const contractsSource = readFileSync(join(UI, 'src', 'ipc', 'contracts.ts'), 'utf8');
const schema = JSON.parse(
  readFileSync(join(ROOT, 'docs', 'architecture', 'surface-protocol.schema.json'), 'utf8')
) as {
  protocol_version: number;
  methods: Record<string, unknown>;
  events: Record<string, unknown>;
};

/** The server-origin protocol methods (R6): notifications only serve→client,
 *  plus the hello handshake (wire methods with no renderer union member). */
const PROTOCOL_METHODS = new Set(['hello', 'event', 'host.ready', 'host.error']);
/** The shell-gated bucket (R12): on the wire, kind-restricted, never in
 *  the renderer union. */
const SHELL_GATED = new Set(['project_root_stage_native']);
/** The runtime stream vocabulary (StreamEvent types). */
const STREAM_VOCABULARY = new Set([
  'delta',
  'thinking',
  'tool',
  'status',
  'timing',
  'done',
  'cancelled',
  'error',
]);

function parseDesktopMethodUnion(source: string): string[] {
  const match = source.match(/export type DesktopMethod =\n((?:\s*\| '[^']+'\n?)+)/);
  expect(match, 'DesktopMethod union literal must exist in contracts.ts').toBeTruthy();
  return [...(match?.[1].matchAll(/'([^']+)'/g) ?? [])].map((m) => m[1]);
}

describe('surface contract (TS conformance)', () => {
  it('schema wire set == renderer union ∪ protocol set ∪ shell-gated bucket', () => {
    const union = new Set(parseDesktopMethodUnion(contractsSource));
    const schemaMethods = new Set(Object.keys(schema.methods));
    expect(schema.protocol_version).toBe(1);
    // The union may not invent methods the wire does not declare, and
    // the shell-gated method must never enter the renderer union.
    for (const method of union) {
      expect(schemaMethods.has(method), `renderer union member ${method} not on the wire`).toBe(true);
    }
    const shellGatedInUnion = [...union].filter((method) => SHELL_GATED.has(method));
    expect(shellGatedInUnion).toEqual([]);
    // The schema's only extras over the union are the protocol methods
    // plus the shell-gated bucket — nothing phantom, nothing missing.
    const extras = [...schemaMethods].filter((method) => !union.has(method));
    expect(new Set(extras)).toEqual(new Set([...PROTOCOL_METHODS, ...SHELL_GATED]));
  });

  it('schema event set == the runtime stream vocabulary', () => {
    const events = new Set(Object.keys(schema.events));
    expect(events).toEqual(STREAM_VOCABULARY);
  });

  it('every union member the renderer invokes resolves in the schema', () => {
    const schemaMethods = new Set(Object.keys(schema.methods));
    const union = parseDesktopMethodUnion(contractsSource);
    // The renderer's runtime transports (tauri/http/fixture/ws) all
    // dispatch through DesktopMethod; the schema is the wire truth.
    for (const method of union) {
      expect(schemaMethods.has(method), `schema missing ${method}`).toBe(true);
    }
  });
});
