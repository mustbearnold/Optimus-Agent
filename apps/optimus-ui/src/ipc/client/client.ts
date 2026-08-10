/**
 * The renderer client (ADR-0090): one typed door to the host, wrapping the
 * frozen wire (spec-015) without changing it. `createOptimusClient` is the
 * only entry point.
 */
import type { OptimusTransport } from '../contracts';
import { ChatSession } from './chatSession';
import { createDomainApis, type BrowserApi } from './domains';
import { RuntimeObserver } from './runtime';

export interface OptimusClient {
  /** A pre-bound conversation: the dominant flow. */
  chat(sessionId: string): ChatSession;
  sessions: ReturnType<typeof createDomainApis>['sessions'];
  approvals: ReturnType<typeof createDomainApis>['approvals'];
  cron: ReturnType<typeof createDomainApis>['cron'];
  jobs: ReturnType<typeof createDomainApis>['jobs'];
  artifacts: ReturnType<typeof createDomainApis>['artifacts'];
  fs: ReturnType<typeof createDomainApis>['fs'];
  memory: ReturnType<typeof createDomainApis>['memory'];
  skills: ReturnType<typeof createDomainApis>['skills'];
  packs: ReturnType<typeof createDomainApis>['packs'];
  gateway: ReturnType<typeof createDomainApis>['gateway'];
  providers: ReturnType<typeof createDomainApis>['providers'];
  consents: ReturnType<typeof createDomainApis>['consents'];
  projects: ReturnType<typeof createDomainApis>['projects'];
  system: ReturnType<typeof createDomainApis>['system'];
  settings: ReturnType<typeof createDomainApis>['settings'];
  shell: ReturnType<typeof createDomainApis>['shell'];
  campaigns: ReturnType<typeof createDomainApis>['campaigns'];
  /** Preview browser — native surface when the transport has one, else the
   *  fixture-mode RPC fallback (browser_navigate) with idle no-ops. */
  browser: BrowserApi;
  /** Transport kind ('tauri' | 'ws' | 'http' | 'fixture') for status chips. */
  kind: OptimusTransport['kind'];
  /** Ordered observability log of every renderer→host interaction. */
  observer: RuntimeObserver;
}

/**
 * Build the client over the chosen transport. `null` is the packaged
 * renderer's confirmed broker absence (spec-015 A6): the terminal
 * affordance — every call rejects with `NoTransportError`.
 */
export function createOptimusClient(transport: OptimusTransport | null): OptimusClient {
  const observer = new RuntimeObserver();
  const apis = createDomainApis(transport, observer);
  return {
    chat: (sessionId) => new ChatSession(transport, sessionId, observer),
    ...apis,
    kind: transport?.kind ?? 'fixture',
    observer,
  };
}
