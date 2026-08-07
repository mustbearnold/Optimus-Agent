// @ts-check
// spec-015 A3: the Developer Full Access / self-development vertical
// (spec-013) over the WS wire. These specs drive the real host's IPC
// surface through the same JSON-RPC 2.0 frames the renderer's wsTransport
// speaks against a spawned `optimus serve`. The React DeveloperAccessPanel
// behaviour is pinned by vitest; the full supervisor lifecycle (build +
// launch, handoff, rollback, emergency stop) is pinned by
// `scripts/tests/test_self_development.py` on both surfaces.
const { test, expect, url, waitForReady, rpc } = require('./support');

const CONFIRMATION = 'I understand Developer Full Access risks';

function grantFor(root) {
  return {
    scope: { kind: 'selected_repository', root },
    capabilities: {
      workspace_files: true,
      terminal_execution: true,
      process_management: true,
      package_installation: true,
      network_access: true,
      external_services: false,
      production_systems: false,
      secrets: false,
    },
    pause_before_destructive: true,
    checkpoint_on_mutation: true,
  };
}

test('developer access starts disabled with an idle supervisor', async ({ serverInfo }) => {
  const state = await rpc(serverInfo, 'developer_access_get');
  expect(state.ok).toBe(true);
  expect(state.developer_access.enabled).toBe(false);
  expect(state.supervisor.healthy).toBe(false);
  expect(state.confirmation).toBe(CONFIRMATION);
});

test('developer access enable requires the one-time confirmation', async ({ serverInfo }) => {
  const denied = await rpc(serverInfo, 'developer_access_enable', {
    confirmation: 'wrong confirmation',
    grant: grantFor(serverInfo.home),
  });
  expect(denied.ok).toBe(false);
  const state = await rpc(serverInfo, 'developer_access_get');
  expect(state.developer_access.enabled).toBe(false);
});

test('developer access enable + revoke round-trip on the real host', async ({ serverInfo }) => {
  const enabled = await rpc(serverInfo, 'developer_access_enable', {
    confirmation: CONFIRMATION,
    grant: grantFor(serverInfo.home),
  });
  expect(enabled.ok).toBe(true);
  expect(enabled.developer_access.enabled).toBe(true);
  expect(enabled.developer_access.scope.root).toBe(serverInfo.home);
  // production_systems can never be enabled by a grant (ADR-0076/0078).
  expect(enabled.developer_access.capabilities.production_systems).toBe(false);

  const status = await rpc(serverInfo, 'developer_supervisor_status');
  expect(status.ok).toBe(true);
  expect(status.healthy).toBe(false);

  const revoked = await rpc(serverInfo, 'developer_access_revoke');
  expect(revoked.ok).toBe(true);
  expect(revoked.developer_access.enabled).toBe(false);
  expect(revoked.supervisor.healthy).toBe(false);
});
