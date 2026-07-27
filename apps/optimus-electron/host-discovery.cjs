// One core per home (criterion C3, docs/architecture/north-star-2026-07.md):
// before spawning a host, a surface probes the home's runtime record and
// attaches to a healthy host that already serves it. The record is written by
// optimus-desktop after a successful bind (apps/optimus-desktop/src/
// host_runtime.rs); a record that is missing, malformed, dead, or no longer
// authenticating simply falls through to the spawn path.

const fs = require('fs');
const http = require('http');
const os = require('os');
const path = require('path');

const RUNTIME_RECORD_FILE = 'host-runtime.json';
const RUNTIME_RECORD_VERSION = 1;
const PROBE_TIMEOUT_MS = 2000;

// Mirrors optimus_host::resolve_home: explicit OPTIMUS_HOME first, then the
// platform's local-data directory, exactly where the spawned host would land
// when the parent passes no --home.
function resolveHome(env = process.env, platform = process.platform, homedir = os.homedir()) {
  const explicit = (env.OPTIMUS_HOME || '').trim();
  if (explicit) return path.resolve(explicit);
  if (platform === 'darwin') {
    return path.join(homedir, 'Library', 'Application Support', 'optimus');
  }
  if (platform === 'win32') {
    const base = (env.LOCALAPPDATA || '').trim() || path.join(homedir, 'AppData', 'Local');
    return path.join(base, 'optimus');
  }
  const base = (env.XDG_DATA_HOME || '').trim() || path.join(homedir, '.local', 'share');
  return path.join(base, 'optimus');
}

function readRuntimeRecord(home) {
  let raw;
  try {
    raw = fs.readFileSync(path.join(home, RUNTIME_RECORD_FILE), 'utf8');
  } catch {
    return null;
  }
  let record;
  try {
    record = JSON.parse(raw);
  } catch {
    return null;
  }
  if (
    !record ||
    record.version !== RUNTIME_RECORD_VERSION ||
    !Number.isInteger(record.port) ||
    record.port < 1 ||
    record.port > 65535 ||
    typeof record.token !== 'string' ||
    record.token.length === 0
  ) {
    return null;
  }
  return { port: record.port, token: record.token };
}

function probeHealth(port, token, timeoutMs = PROBE_TIMEOUT_MS) {
  return new Promise((resolve) => {
    const request = http.get(
      `http://127.0.0.1:${port}/api/health`,
      {
        timeout: timeoutMs,
        headers: {
          Authorization: `Bearer ${token}`,
          Origin: `http://127.0.0.1:${port}`,
        },
      },
      (response) => {
        let body = '';
        response.on('data', (chunk) => (body += chunk));
        response.on('end', () => {
          let ok = false;
          try {
            ok = response.statusCode === 200 && JSON.parse(body).ok === true;
          } catch {
            ok = false;
          }
          resolve(ok);
        });
      }
    );
    request.on('timeout', () => request.destroy());
    request.on('error', () => resolve(false));
  });
}

// The C3 probe: a healthy record means attach ({port, token}), anything else
// means the caller owns the spawn.
async function discoverHealthyHost(home, timeoutMs = PROBE_TIMEOUT_MS) {
  const record = readRuntimeRecord(home);
  if (!record) return null;
  return (await probeHealth(record.port, record.token, timeoutMs)) ? record : null;
}

module.exports = {
  RUNTIME_RECORD_FILE,
  discoverHealthyHost,
  probeHealth,
  readRuntimeRecord,
  resolveHome,
};
