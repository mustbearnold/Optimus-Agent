const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = path.join(__dirname, '../../..');
const mainSource = fs.readFileSync(path.join(__dirname, '..', 'main.cjs'), 'utf8');
const contractsSource = fs.readFileSync(
  path.join(__dirname, '../../optimus-ui/src/ipc/contracts.ts'),
  'utf8'
);
const routerSource = fs.readFileSync(
  path.join(__dirname, '../../optimus-desktop/src/ipc/router.rs'),
  'utf8'
);

function parseSetBlock(source, marker) {
  const start = source.indexOf(marker);
  assert.notEqual(start, -1, `missing ${marker}`);
  const open = source.indexOf('[', start);
  const close = source.indexOf(']);', open);
  assert.ok(open > start && close > open, `unterminated set for ${marker}`);
  const slice = source.slice(open, close);
  return new Set([...slice.matchAll(/'([a-z0-9_]+)'/g)].map((match) => match[1]));
}

function parseRustMethods(source) {
  const start = source.indexOf('const METHOD_DOMAINS');
  assert.notEqual(start, -1);
  const end = source.indexOf('];', start);
  const block = source.slice(start, end);
  return new Set([...block.matchAll(/\("([a-z0-9_]+)",\s*Domain::/g)].map((m) => m[1]));
}

function parseDesktopMethod(source) {
  const start = source.indexOf('export type DesktopMethod');
  assert.notEqual(start, -1);
  const end = source.indexOf(';', start);
  const block = source.slice(start, end);
  return new Set([...block.matchAll(/'([a-z0-9_]+)'/g)].map((m) => m[1]));
}

const CRITICAL = new Set([
  'ping',
  'doctor',
  'sessions',
  'new_session',
  'get_session',
  'chat_approval_resolve',
  'project_scopes_list',
  'project_scopes_authorize',
  'approvals_list',
  'approvals_grant',
  'fs_roots',
  'fs_list',
  'fs_read',
  'settings_get',
  'settings_set',
]);

test('approval resolution remains an explicit renderer-to-host allowlist entry', () => {
  assert.match(mainSource, /'chat_approval_resolve',/);
});

test('electron allowlist is a subset of the rust host registry', () => {
  const electron = parseSetBlock(mainSource, 'const DESKTOP_METHODS = new Set([');
  const rust = parseRustMethods(routerSource);
  for (const method of electron) {
    assert.ok(rust.has(method), `electron method missing from rust: ${method}`);
  }
});

test('react DesktopMethod matches electron DESKTOP_METHODS exactly', () => {
  const electron = parseSetBlock(mainSource, 'const DESKTOP_METHODS = new Set([');
  const react = parseDesktopMethod(contractsSource);
  assert.deepEqual([...electron].sort(), [...react].sort());
});

test('critical invoke paths remain allowlisted and never expose main-only staging', () => {
  const electron = parseSetBlock(mainSource, 'const DESKTOP_METHODS = new Set([');
  for (const method of CRITICAL) {
    assert.ok(electron.has(method), `critical method missing: ${method}`);
  }
  assert.equal(electron.has('project_root_stage_native'), false);
});

test('python desktop IPC matrix checker passes for the live tree', () => {
  const result = spawnSync(
    process.env.PYTHON || 'python3',
    [path.join(root, 'scripts/check-desktop-ipc-matrix.py')],
    { encoding: 'utf8', cwd: root }
  );
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /DESKTOP_IPC_MATRIX_OK/);
});
