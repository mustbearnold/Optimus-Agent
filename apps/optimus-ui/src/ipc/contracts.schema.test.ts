import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import type {
  ApprovalResolveRequest,
  ChatRequest,
  StreamEvent,
  TimingEvent,
  ToolLifecycleEvent,
} from './contracts';

/**
 * TS-side surface-contract conformance (spec-015 A5): the committed
 * wire schema must equal the renderer's method surface plus the
 * protocol-method set, and the schema's event set must equal the
 * runtime stream vocabulary plus the server-origin notifications. The
 * renderer union is read from the contracts.ts source (the same source
 * the Python gate parses) so the two gates can never drift apart.
 *
 * R10 (b)/(c) — the type-level half (landed in the round-3 revision,
 * spec-015 A5): (b) bidirectional assignability between each
 * schema-declared payload that has a TS counterpart (chat_request,
 * approval_resolve_request, and every StreamEvent member) and a mirror
 * declared in this test — the mirror ⊆ TS direction is proven by the
 * `satisfies` consts, the TS ⊆ mirror direction by the Assert<...>
 * type checks, both enforced by `tsc -b` (the `build react ui` gate,
 * verify.sh:361); (c) the schema's `required` arrays govern
 * optionality — the runtime walk below parses the TS type declarations
 * and fails when a schema-required field is optional in TS or a
 * schema-optional field is required in TS.
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
  chat_request?: unknown;
  approval_resolve_request?: unknown;
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

// ---------------------------------------------------------------------------
// R10 (b) — type-level bidirectional assignability (spec-015 round-3 revision)
// ---------------------------------------------------------------------------

type IsAssignable<A, B> = A extends B ? true : false;
type Assert<T extends true> = T;

// Schema-shaped mirror types: exact required/optional per the schema's
// `required` arrays. Indexed accesses reuse the TS unions for enum-ish
// fields so both directions hold.
type ChatRequestMirror = {
  session: string;
  message: string;
  provider: ChatRequest['provider'];
  model?: string;
  thinking_level?: string;
  fast?: boolean;
  access?: string;
  project_id?: string;
};
type ApprovalResolveMirror = {
  session_id: string;
  run_id: string;
  call_id: string;
  job_id: string;
  node_id: string;
  node_index: number;
  effect_sha256: string;
  decision: ApprovalResolveRequest['decision'];
  project_id?: string;
};
type ToolEventMirror = {
  type: 'tool';
  schema_version: 1;
  event_id: string;
  run_id: string;
  call_id: string;
  tool_id: string;
  phase: ToolLifecycleEvent['phase'];
  summary: string;
  duration_ms?: number;
  outcome?: ToolLifecycleEvent['outcome'];
  approval?: ToolLifecycleEvent['approval'];
};
type TimingMirror = { type: 'timing'; phase?: string; elapsed_ms?: number };
type DeltaMirror = { type: 'delta'; text: string };
type ThinkingMirror = { type: 'thinking'; text: string };
type StatusMirror = { type: 'status'; text: string };
type DoneMirror = { type: 'done'; result?: Record<string, unknown> };
type CancelledMirror = { type: 'cancelled'; error?: string };
type ErrorMirror = { type: 'error'; error: string };

// Const mirrors: `satisfies` proves mirror ⊆ TS at compile time, and the
// runtime key checks below tie each mirror's field set to the schema.
const chatRequestMirror = {
  session: 's',
  message: 'm',
  provider: 'offline',
  model: 'm',
  thinking_level: 'low',
  fast: false,
  access: 'a',
  project_id: 'p',
} as const satisfies ChatRequest;
const approvalResolveMirror = {
  session_id: 's',
  run_id: 'r',
  call_id: 'c',
  job_id: 'j',
  node_id: 'n',
  node_index: 0,
  effect_sha256: 'e',
  decision: 'approve',
  project_id: 'p',
} as const satisfies ApprovalResolveRequest;
const toolEventMirror = {
  type: 'tool',
  schema_version: 1,
  event_id: 'e',
  run_id: 'r',
  call_id: 'c',
  tool_id: 't',
  phase: 'started',
  summary: 's',
  duration_ms: 1,
  outcome: {
    version: 1,
    call_id: 'c',
    tool_id: 't',
    kind: 'succeeded',
    summary: 's',
    data: null,
    artifacts: [],
    replay: 'r',
  },
  approval: {
    run_id: 'r',
    call_id: 'c',
    tool_id: 't',
    job_id: 'j',
    node_id: 'n',
    node_index: 0,
    effect_sha256: 'e',
    summary: 's',
  },
} as const satisfies ToolLifecycleEvent;
const timingMirror = { type: 'timing', phase: 'p', elapsed_ms: 1 } as const satisfies TimingEvent;
const deltaMirror = { type: 'delta', text: 't' } as const satisfies Extract<StreamEvent, { type: 'delta' }>;
const thinkingMirror = { type: 'thinking', text: 't' } as const satisfies Extract<StreamEvent, { type: 'thinking' }>;
const statusMirror = { type: 'status', text: 't' } as const satisfies Extract<StreamEvent, { type: 'status' }>;
const doneMirror = { type: 'done', result: {} } as const satisfies Extract<StreamEvent, { type: 'done' }>;
const cancelledMirror = { type: 'cancelled', error: 'e' } as const satisfies Extract<StreamEvent, { type: 'cancelled' }>;
const errorMirror = { type: 'error', error: 'e' } as const satisfies Extract<StreamEvent, { type: 'error' }>;

// TS ⊆ mirror direction, per payload. Exported so the aggregate is "used"
// under noUnusedLocals; the checks themselves are compile-time.
type _chatFwd = Assert<IsAssignable<ChatRequestMirror, ChatRequest>>;
type _chatRev = Assert<IsAssignable<ChatRequest, ChatRequestMirror>>;
type _apprFwd = Assert<IsAssignable<ApprovalResolveMirror, ApprovalResolveRequest>>;
type _apprRev = Assert<IsAssignable<ApprovalResolveRequest, ApprovalResolveMirror>>;
type _toolFwd = Assert<IsAssignable<ToolEventMirror, ToolLifecycleEvent>>;
type _toolRev = Assert<IsAssignable<ToolLifecycleEvent, ToolEventMirror>>;
type _timingFwd = Assert<IsAssignable<TimingMirror, TimingEvent>>;
type _timingRev = Assert<IsAssignable<TimingEvent, TimingMirror>>;
type _deltaFwd = Assert<IsAssignable<DeltaMirror, Extract<StreamEvent, { type: 'delta' }>>>;
type _deltaRev = Assert<IsAssignable<Extract<StreamEvent, { type: 'delta' }>, DeltaMirror>>;
type _thinkFwd = Assert<IsAssignable<ThinkingMirror, Extract<StreamEvent, { type: 'thinking' }>>>;
type _thinkRev = Assert<IsAssignable<Extract<StreamEvent, { type: 'thinking' }>, ThinkingMirror>>;
type _statusFwd = Assert<IsAssignable<StatusMirror, Extract<StreamEvent, { type: 'status' }>>>;
type _statusRev = Assert<IsAssignable<Extract<StreamEvent, { type: 'status' }>, StatusMirror>>;
type _doneFwd = Assert<IsAssignable<DoneMirror, Extract<StreamEvent, { type: 'done' }>>>;
type _doneRev = Assert<IsAssignable<Extract<StreamEvent, { type: 'done' }>, DoneMirror>>;
type _cancelFwd = Assert<IsAssignable<CancelledMirror, Extract<StreamEvent, { type: 'cancelled' }>>>;
type _cancelRev = Assert<IsAssignable<Extract<StreamEvent, { type: 'cancelled' }>, CancelledMirror>>;
type _errorFwd = Assert<IsAssignable<ErrorMirror, Extract<StreamEvent, { type: 'error' }>>>;
type _errorRev = Assert<IsAssignable<Extract<StreamEvent, { type: 'error' }>, ErrorMirror>>;
export type SchemaConformanceTypeChecks = [
  _chatFwd, _chatRev, _apprFwd, _apprRev, _toolFwd, _toolRev,
  _timingFwd, _timingRev, _deltaFwd, _deltaRev, _thinkFwd, _thinkRev,
  _statusFwd, _statusRev, _doneFwd, _doneRev, _cancelFwd, _cancelRev,
  _errorFwd, _errorRev,
];

// ---------------------------------------------------------------------------
// R10 (c) — schema `required` arrays govern TS optionality (runtime walk)
// ---------------------------------------------------------------------------

interface SchemaPayload {
  required?: string[];
  properties?: Record<string, unknown>;
}

/** Parse `field?: type;` lines (named types) or `field: type;` segments. */
function parseFieldLines(text: string): Map<string, boolean> {
  const fields = new Map<string, boolean>();
  for (const part of text.split(/[;\n]/)) {
    const m = part.trim().match(/^([A-Za-z_$][\w$]*)(\?)?:/);
    if (m) fields.set(m[1], m[2] === '?');
  }
  return fields;
}

/** Parse a named object type's fields (field -> optional). */
function parseNamedTypeFields(name: string): Map<string, boolean> {
  const block = contractsSource.match(new RegExp(`export type ${name} = \\{([\\s\\S]*?)\\n\\};`));
  expect(block, `contracts.ts must declare type ${name}`).toBeTruthy();
  return parseFieldLines(block![1]);
}

/** Parse the inline `{ type: '...'; ... }` members of the StreamEvent union. */
function parseStreamEventMembers(): Map<string, Map<string, boolean>> {
  const members = new Map<string, Map<string, boolean>>();
  const union = contractsSource.match(/export type StreamEvent =([\s\S]*?);\n/);
  expect(union, 'contracts.ts must declare the StreamEvent union').toBeTruthy();
  const memberRe = /\{\s*(type: '[^']+')([\s\S]*?)\}/g;
  let m: RegExpExecArray | null;
  while ((m = memberRe.exec(union![1]))) {
    const name = m[1].match(/'([^']+)'/)?.[1] ?? '';
    members.set(name, parseFieldLines(`${m[1]};${m[2]}`));
  }
  return members;
}

function expectOptionalityAgreement(
  tsFields: Map<string, boolean>,
  payload: SchemaPayload,
  label: string,
): void {
  const required = payload.required ?? [];
  const declared = Object.keys(payload.properties ?? {});
  for (const field of required) {
    expect(tsFields.has(field), `${label}: schema-required field "${field}" missing from the TS type`).toBe(true);
    expect(tsFields.get(field) === false, `${label}: schema-required field "${field}" must be REQUIRED in the TS type (no ?)`).toBe(true);
  }
  for (const field of declared) {
    if (required.includes(field)) continue;
    expect(tsFields.has(field), `${label}: schema-optional field "${field}" missing from the TS type`).toBe(true);
    expect(tsFields.get(field) === true, `${label}: schema-optional field "${field}" must be OPTIONAL in the TS type (with ?)`).toBe(true);
  }
}

describe('surface contract (TS schema conformance, R10 b/c)', () => {
  const events = schema.events as Record<string, SchemaPayload>;
  const chatRequest = schema.chat_request as SchemaPayload;
  const approvalResolve = schema.approval_resolve_request as SchemaPayload;

  it('(b) const mirror field sets equal the schema-declared payloads', () => {
    const schemaFields = (payload: SchemaPayload): string[] => [
      ...new Set([...(payload.required ?? []), ...Object.keys(payload.properties ?? {})]),
    ].sort();
    expect(Object.keys(chatRequestMirror).sort()).toEqual(schemaFields(chatRequest));
    expect(Object.keys(approvalResolveMirror).sort()).toEqual(schemaFields(approvalResolve));
    expect(Object.keys(toolEventMirror).sort()).toEqual(schemaFields(events.tool));
    expect(Object.keys(timingMirror).sort()).toEqual(schemaFields(events.timing));
    expect(Object.keys(deltaMirror).sort()).toEqual(schemaFields(events.delta));
    expect(Object.keys(thinkingMirror).sort()).toEqual(schemaFields(events.thinking));
    expect(Object.keys(statusMirror).sort()).toEqual(schemaFields(events.status));
    expect(Object.keys(doneMirror).sort()).toEqual(schemaFields(events.done));
    expect(Object.keys(cancelledMirror).sort()).toEqual(schemaFields(events.cancelled));
    expect(Object.keys(errorMirror).sort()).toEqual(schemaFields(events.error));
  });

  it('(c) schema required arrays govern TS optionality (named types)', () => {
    const chatRequestFields = parseNamedTypeFields('ChatRequest');
    const approvalResolveFields = parseNamedTypeFields('ApprovalResolveRequest');
    const toolFields = parseNamedTypeFields('ToolLifecycleEvent');
    const timingFields = parseNamedTypeFields('TimingEvent');
    expectOptionalityAgreement(chatRequestFields, chatRequest, 'chat_request');
    expectOptionalityAgreement(approvalResolveFields, approvalResolve, 'approval_resolve_request');
    expectOptionalityAgreement(toolFields, events.tool, 'event tool');
    expectOptionalityAgreement(timingFields, events.timing, 'event timing');
  });

  it('(c) schema required arrays govern TS optionality (StreamEvent members)', () => {
    const members = parseStreamEventMembers();
    for (const [name, payload] of Object.entries(events)) {
      const fields = members.get(name);
      if (!fields) continue; // tool/timing are named types, covered above
      expectOptionalityAgreement(fields, payload, `event ${name}`);
    }
  });
});
