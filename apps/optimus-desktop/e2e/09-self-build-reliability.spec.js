// @ts-check
// spec-014 R4–R6: the approval-resolution vertical over the wire and in the
// workbench. A real command effect parks an approval; the dock lists the exact
// job; granting settles it; the pending list empties. The kernel-level
// multi-node re-park and the resolve done-payload wire shape (`still_pending`,
// `resume_error`) are pinned by `crates/optimus-kernel/tests/approval_vertical.rs`
// and `crates/optimus-host/tests/serve_protocol.rs`; this spec pins the
// human-visible surface.
const { test, expect, url, waitForReady, rpc } = require('./support');

test('execution dock: a parked command shows a grantable card and settles on grant', async ({
  page,
  serverInfo,
}) => {
  // Park a real command effect (job-stream mode; no PTY).
  const run = await rpc(serverInfo, 'term_run', { line: 'echo self-build-ok' });
  expect(run.ok).toBe(true);
  expect(run.status).toBe('AwaitingApproval');
  expect(String(run.stdout || '')).toBe('');
  expect(run.mode).toBe('job-stream');

  const pending = await rpc(serverInfo, 'approvals_list');
  // The shared fixture home accumulates parked jobs from earlier specs; find
  // THIS run's job by label instead of assuming a fresh list.
  const mine = (pending.pending || []).find((approval) =>
    String(approval.job_label || '').includes('echo self-build-ok')
  );
  expect(mine).toBeTruthy();
  const jobId = mine.job_id;
  expect(jobId).toBeTruthy();
  expect(String(mine.job_label || '')).toContain('echo self-build-ok');

  // The workbench dock surfaces the card.
  await page.goto('/');
  await waitForReady(page);
  await page.getByRole('button', { name: 'Terminal' }).click();
  const dock = page.getByRole('complementary', { name: 'Execution dock' });
  await expect(dock).toBeVisible();
  await dock.getByRole('tab', { name: /Approvals/ }).click();
  const list = dock.getByLabel('Pending approvals');
  await expect(list).toContainText('echo self-build-ok');

  // Granting from the dock settles the exact job and removes its card.
  // Other specs park jobs and never grant them, so scope the click to THIS
  // run's card instead of the first approval in the list.
  const card = dock.getByText('echo self-build-ok').locator('xpath=ancestor::article').first();
  await card.getByRole('button', { name: /Approve|Grant/i }).click();
  await expect(card).toHaveCount(0);

  const after = await rpc(serverInfo, 'approvals_list');
  expect(
    (after.pending || []).some((approval) => String(approval.job_id) === String(jobId))
  ).toBe(false);
  const jobs = await rpc(serverInfo, 'jobs_list');
  const settled = (jobs.jobs || []).find((job) => String(job.job_id) === String(jobId));
  expect(settled).toBeTruthy();
  expect(String(settled.status || settled.state || '')).toMatch(/Succeeded|Done|Completed/);
});

test('stale approval grants fail truthfully instead of double-settling', async ({
  serverInfo,
}) => {
  const run = await rpc(serverInfo, 'term_run', { line: 'echo once-only' });
  expect(run.ok).toBe(true);
  const pending = await rpc(serverInfo, 'approvals_list');
  const mine = (pending.pending || []).find((approval) =>
    String(approval.job_label || '').includes('echo once-only')
  );
  expect(mine).toBeTruthy();
  const jobId = mine.job_id;

  const first = await rpc(serverInfo, 'approvals_grant', { job_id: jobId });
  expect(first.ok).toBe(true);

  // The same job id is no longer pending: a second grant must not silently
  // succeed — the card is gone, so a stale click fails truthfully (R4 rule:
  // "stale-card clicks fail truthfully at missing or already resolved").
  const second = await rpc(serverInfo, 'approvals_grant', { job_id: jobId });
  expect(second.ok).toBe(false);
});

test('session consent surface: grant, list, revoke, revoke-all over the wire', async ({
  serverInfo,
}) => {
  // spec-014 R7 / ADR-0081: the session-consent surface. The class
  // discriminator is what the UI sends (from the approval card's binding);
  // the host re-derives the capability server-side, so a bogus class must be
  // rejected instead of minting a grant under a made-up capability.
  // Consents are project-scoped (ScopePolicy::Project), so authorize the
  // project first and pass project_id on every call. The full desktop flow
  // is stage-native-selection → authorize (shell-gated staging is not on
  // the WS wire surface), so seed the authority document the same way
  // `project_scopes_authorize` would — the e2e home is disposable.
  const fs = require('node:fs');
  const path = require('node:path');
  const root = path.join(serverInfo.home, '..', 'e2e-project-root');
  fs.mkdirSync(root, { recursive: true });
  const authority = path.join(serverInfo.home, 'project-authority.json');
  fs.writeFileSync(authority, JSON.stringify({
    version: 1,
    projects: {
      'e2e-project': {
        project_id: 'e2e-project',
        roots: [root],
        primary_root: root,
        updated_unix: Math.floor(Date.now() / 1000),
      },
    },
    staged_roots: [],
  }), { mode: 0o600 });
  const consent = (params) => ({ project_id: 'e2e-project', ...params });

  const bad = await rpc(serverInfo, 'session_consent_grant', consent({
    session_id: 'e2e-session-1',
    command_class: 'not_a_real_class',
  }));
  expect(bad.ok).toBe(false);

  const granted = await rpc(serverInfo, 'session_consent_grant', consent({
    session_id: 'e2e-session-1',
    command_class: 'opaque_shell',
  }));
  expect(granted.ok).toBe(true);
  expect(String(granted.command_class || '')).toBe('opaque_shell');

  // The consent is listed for its session only.
  const listed = await rpc(serverInfo, 'session_consent_list', consent({
    session_id: 'e2e-session-1',
  }));
  const rows = listed.grants || [];
  expect(
    rows.some((row) => String(row.command_class) === 'opaque_shell' && !row.revoked_unix)
  ).toBe(true);

  // Revoke-all flips the session back to asking; a second revoke is a no-op.
  const revoked = await rpc(serverInfo, 'session_consent_revoke_all', consent({
    session_id: 'e2e-session-1',
  }));
  expect(revoked.ok).toBe(true);
  expect(Number(revoked.revoked)).toBeGreaterThan(0);
  const after = await rpc(serverInfo, 'session_consent_list', consent({
    session_id: 'e2e-session-1',
  }));
  expect(
    (after.grants || []).some(
      (row) => String(row.command_class) === 'opaque_shell' && !row.revoked_unix
    )
  ).toBe(false);

  // Single-class revoke for a fresh grant.
  const granted2 = await rpc(serverInfo, 'session_consent_grant', consent({
    session_id: 'e2e-session-1',
    command_class: 'project_execute',
  }));
  expect(granted2.ok).toBe(true);
  const one = await rpc(serverInfo, 'session_consent_revoke', consent({
    session_id: 'e2e-session-1',
    command_class: 'project_execute',
  }));
  expect(one.ok).toBe(true);
  expect(one.revoked).toBe(true);
});
