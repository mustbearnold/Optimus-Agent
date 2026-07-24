const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const mainSource = fs.readFileSync(path.join(__dirname, '..', 'main.cjs'), 'utf8');

test('approval resolution remains an explicit renderer-to-host allowlist entry', () => {
  assert.match(mainSource, /'chat_approval_resolve',/);
});
