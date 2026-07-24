const fs = require('fs');
const path = require('path');

function explicitAbsolutePath(value, label) {
  if (!value) return '';
  if (!path.isAbsolute(value)) {
    throw new Error(`${label} must be an absolute path`);
  }
  return path.resolve(value);
}

function resolveApplicationRoot(env, electronAppDir) {
  return (
    explicitAbsolutePath(env.OPTIMUS_APP_ROOT, 'OPTIMUS_APP_ROOT') ||
    path.resolve(electronAppDir, '../..')
  );
}

function resolveUiDist(env, applicationRoot, electronAppDir) {
  return (
    explicitAbsolutePath(env.OPTIMUS_UI_DIST, 'OPTIMUS_UI_DIST') ||
    (path.basename(electronAppDir) === 'app'
      ? path.join(electronAppDir, 'ui-dist')
      : path.join(applicationRoot, 'apps', 'optimus-ui', 'dist'))
  );
}

function resolveHostBinary(env, applicationRoot) {
  const explicit = explicitAbsolutePath(
    env.OPTIMUS_DESKTOP_BIN,
    'OPTIMUS_DESKTOP_BIN'
  );
  if (explicit) return explicit;

  const configuredTarget = env.CARGO_TARGET_DIR || path.join(applicationRoot, 'target');
  const targetDir = path.isAbsolute(configuredTarget)
    ? path.resolve(configuredTarget)
    : path.resolve(applicationRoot, configuredTarget);
  const release = path.join(targetDir, 'release', 'optimus-desktop');
  const debug = path.join(targetDir, 'debug', 'optimus-desktop');
  return fs.existsSync(release) ? release : debug;
}

module.exports = {
  resolveApplicationRoot,
  resolveHostBinary,
  resolveUiDist,
};
