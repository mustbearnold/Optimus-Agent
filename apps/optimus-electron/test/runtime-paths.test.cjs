const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('path');
const {
  resolveApplicationRoot,
  resolveHostBinary,
  resolveUiDist,
} = require('../runtime-paths.cjs');

test('installed paths use explicit absolute package and Rust host boundaries', () => {
  const env = {
    OPTIMUS_APP_ROOT: '/opt/optimus-agent',
    OPTIMUS_DESKTOP_BIN: '/opt/optimus-agent/bin/optimus-desktop-host',
    OPTIMUS_UI_DIST:
      '/opt/optimus-agent/app-bundle/electron/resources/app/ui-dist',
  };
  const appDir = '/opt/optimus-agent/app-bundle/electron/resources/app';
  const root = resolveApplicationRoot(env, appDir);

  assert.equal(root, '/opt/optimus-agent');
  assert.equal(resolveHostBinary(env, root), env.OPTIMUS_DESKTOP_BIN);
  assert.equal(resolveUiDist(env, root, appDir), env.OPTIMUS_UI_DIST);
});

test('repository paths retain release-first Cargo and Vite defaults', () => {
  const root = '/workspace/Optimus Agent';
  const appDir = path.join(root, 'apps', 'optimus-electron');

  assert.equal(resolveApplicationRoot({}, appDir), root);
  assert.equal(
    resolveUiDist({}, root, appDir),
    path.join(root, 'apps', 'optimus-ui', 'dist')
  );
  assert.equal(
    resolveHostBinary({ CARGO_TARGET_DIR: '/tmp/optimus-target' }, root),
    '/tmp/optimus-target/debug/optimus-desktop'
  );
});

test('installed path overrides reject relative authority-bearing paths', () => {
  assert.throws(
    () => resolveHostBinary({ OPTIMUS_DESKTOP_BIN: 'bin/optimus-desktop' }, '/opt/app'),
    /must be an absolute path/
  );
  assert.throws(
    () => resolveApplicationRoot({ OPTIMUS_APP_ROOT: '../app' }, '/opt/app'),
    /must be an absolute path/
  );
});
