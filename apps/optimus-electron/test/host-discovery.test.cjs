const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const http = require('http');
const os = require('os');
const path = require('path');
const {
  RUNTIME_RECORD_FILE,
  discoverHealthyHost,
  readRuntimeRecord,
  resolveHome,
} = require('../host-discovery.cjs');

function tempHome() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'optimus-discovery-'));
}

function writeRecord(home, record) {
  fs.writeFileSync(path.join(home, RUNTIME_RECORD_FILE), JSON.stringify(record));
}

// A scripted health endpoint: 200 {"ok":true} for the expected bearer token,
// 401 for anything else — the two live shapes the probe must distinguish.
function healthServer(expectedToken) {
  return new Promise((resolve) => {
    const server = http.createServer((request, response) => {
      const authorized =
        request.url === '/api/health' &&
        request.headers.authorization === `Bearer ${expectedToken}`;
      response.writeHead(authorized ? 200 : 401, {
        'Content-Type': 'application/json',
      });
      response.end(authorized ? '{"ok":true,"streaming":true}' : '{"ok":false}');
    });
    server.listen(0, '127.0.0.1', () =>
      resolve({ server, port: server.address().port })
    );
  });
}

test('explicit OPTIMUS_HOME wins and is trimmed and absolutised', () => {
  assert.equal(
    resolveHome({ OPTIMUS_HOME: '  /var/lib/optimus  ' }, 'linux', '/home/user'),
    path.resolve('/var/lib/optimus')
  );
});

test('platform defaults mirror the Rust host resolve_home fallbacks', () => {
  assert.equal(
    resolveHome({}, 'linux', '/home/user'),
    path.join('/home/user', '.local', 'share', 'optimus')
  );
  assert.equal(
    resolveHome({ XDG_DATA_HOME: '/xdg-data' }, 'linux', '/home/user'),
    path.join('/xdg-data', 'optimus')
  );
  assert.equal(
    resolveHome({}, 'darwin', '/Users/user'),
    path.join('/Users/user', 'Library', 'Application Support', 'optimus')
  );
  assert.equal(
    resolveHome({ LOCALAPPDATA: 'C:\\Users\\user\\AppData\\Local' }, 'win32', 'C:\\Users\\user'),
    path.join('C:\\Users\\user\\AppData\\Local', 'optimus')
  );
});

test('missing, malformed, and out-of-contract records read as unserved', () => {
  const home = tempHome();
  assert.equal(readRuntimeRecord(home), null, 'missing file');
  fs.writeFileSync(path.join(home, RUNTIME_RECORD_FILE), 'not json');
  assert.equal(readRuntimeRecord(home), null, 'malformed json');
  writeRecord(home, { version: 99, port: 1, pid: 1, token: 'x' });
  assert.equal(readRuntimeRecord(home), null, 'unknown version');
  writeRecord(home, { version: 1, port: 0, pid: 1, token: 'x' });
  assert.equal(readRuntimeRecord(home), null, 'invalid port');
  writeRecord(home, { version: 1, port: 4321, pid: 1, token: '' });
  assert.equal(readRuntimeRecord(home), null, 'empty token');
});

test('a healthy advertised host is discovered for attach', async () => {
  const home = tempHome();
  const { server, port } = await healthServer('token-live');
  try {
    writeRecord(home, { version: 1, port, pid: 4242, token: 'token-live' });
    assert.deepEqual(await discoverHealthyHost(home), {
      port,
      token: 'token-live',
    });
  } finally {
    server.close();
  }
});

test('a record whose token no longer authenticates falls through to spawn', async () => {
  const home = tempHome();
  const { server, port } = await healthServer('token-current');
  try {
    writeRecord(home, { version: 1, port, pid: 4242, token: 'token-stale' });
    assert.equal(await discoverHealthyHost(home), null);
  } finally {
    server.close();
  }
});

test('a crash-stale record pointing at a dead port falls through to spawn', async () => {
  const home = tempHome();
  const placeholder = await healthServer('unused');
  const deadPort = placeholder.port;
  placeholder.server.close();
  await new Promise((resolve) => placeholder.server.once('close', resolve));
  writeRecord(home, { version: 1, port: deadPort, pid: 4242, token: 'token-dead' });
  assert.equal(await discoverHealthyHost(home, 500), null);
});
