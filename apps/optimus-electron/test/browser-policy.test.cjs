const test = require('node:test');
const assert = require('node:assert/strict');
const { assertPreviewUrl } = require('../browser-policy.cjs');

test('allows HTTPS and loopback HTTP preview URLs', () => {
  assert.equal(assertPreviewUrl('https://example.com/docs'), 'https://example.com/docs');
  assert.equal(assertPreviewUrl('http://127.0.0.1:8787/fixture'), 'http://127.0.0.1:8787/fixture');
  assert.equal(assertPreviewUrl('http://localhost:3000/'), 'http://localhost:3000/');
});

test('rejects unsafe, privileged, malformed, and remote HTTP URLs', () => {
  for (const value of [
    'http://example.com/',
    'file:///etc/passwd',
    'javascript:alert(1)',
    'data:text/html,bad',
    'optimus-app://ui/index.html',
    'not a url',
  ]) {
    assert.throws(() => assertPreviewUrl(value));
  }
});
