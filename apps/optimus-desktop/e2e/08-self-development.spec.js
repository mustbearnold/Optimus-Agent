// @ts-check
const { test, expect, url, waitForReady } = require('./support');

// The Developer Full Access / self-development vertical (spec-013). These
// specs drive the real host's IPC surface over the desktop HTTP transport —
// the same registry the Tauri shell forwards through `host_invoke`
// (spec-002 R5) and the same methods the React DeveloperAccessPanel calls.
// The panel's React behaviour is pinned by vitest; the full supervisor
// lifecycle (build + launch, handoff, rollback, emergency stop) is pinned by
// `scripts/tests/test_self_development.py` on both surfaces.

function ipc(method, params = {}) {
  return fetch(`${url()}/api/ipc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id: Math.floor(Math.random() * 100000), method, params }),
  }).then((response) => response.json());
}

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
  const state = await ipc('developer_access_get');
  expect(state.ok).toBe(true);
  expect(state.result.developer_access.enabled).toBe(false);
  expect(state.result.supervisor.healthy).toBe(false);
  expect(state.result.confirmation).toBe(CONFIRMATION);
});

test('developer access enable requires the one-time confirmation', async ({ serverInfo }) => {
  const denied = await ipc('developer_access_enable', {
    confirmation: 'wrong confirmation',
    grant: grantFor(serverInfo.home),
  });
  expect(denied.ok).toBe(false);
  const state = await ipc('developer_access_get');
  expect(state.result.developer_access.enabled).toBe(false);
});

test('developer access enable + revoke round-trip on the real host', async ({ page, serverInfo }) => {
  await page.goto('/');
  await waitForReady(page);

  const enabled = await ipc('developer_access_enable', {
    confirmation: CONFIRMATION,
    grant: grantFor(serverInfo.home),
  });
  expect(enabled.ok).toBe(true);
  expect(enabled.result.developer_access.enabled).toBe(true);
  expect(enabled.result.developer_access.scope.root).toBe(serverInfo.home);
  // production_systems can never be enabled by a grant (ADR-0076/0078).
  expect(enabled.result.developer_access.capabilities.production_systems).toBe(false);

  const status = await ipc('developer_supervisor_status');
  expect(status.ok).toBe(true);
  expect(status.result.healthy).toBe(false);

  const revoked = await ipc('developer_access_revoke');
  expect(revoked.ok).toBe(true);
  expect(revoked.result.developer_access.enabled).toBe(false);
  expect(revoked.result.supervisor.healthy).toBe(false);
});
