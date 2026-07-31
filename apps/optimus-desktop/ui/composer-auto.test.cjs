const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const uiDir = __dirname;
const html = fs.readFileSync(path.join(uiDir, 'index.html'), 'utf8');
const app = fs.readFileSync(path.join(uiDir, 'app.js'), 'utf8');

test('Wry composer defaults durably to provider and model Auto', () => {
  assert.match(html, /<option value="auto" selected>Auto<\/option>/);
  assert.match(html, /<option value="" selected>Auto<\/option>/);
  assert.match(app, /provider:\s*\$\('provider'\)\.value/);
  assert.match(app, /model:\s*\$\('model'\)\.value/);
});

test('Wry send delegates provider Auto to core and omits model Auto', () => {
  assert.match(app, /const provider = \$\('provider'\)\.value;/);
  assert.match(app, /if \(model\) opts\.model = model;/);
  assert.match(app, /if \(provider === 'auto'\) return false;/);
  assert.match(app, /res\.model \|\| model \|\| 'auto'/);
  assert.doesNotMatch(
    app,
    /const opts = \{\s*provider,\s*model,/,
    'model must not be unconditionally serialized into a chat request'
  );
});

test('Wry migrates unchosen legacy Offline residue without rewriting explicit choices', () => {
  assert.match(
    app,
    /const legacyOfflineResidue = !composerProviderChosen && c\.provider === 'offline';/
  );
  assert.match(app, /if \(legacyOfflineResidue\) \{\s*\$\('provider'\)\.value = 'auto';/);
  assert.match(app, /if \(\$\('provider'\)\.value === 'auto'\) \{\s*\$\('model'\)\.value = '';/);
  assert.match(app, /composerProviderChosen = true;/);
});

test('Wry uses canonical provider ids and provider-owned model choices', () => {
  assert.match(html, /<option value="open-ai-compat">OpenAI compatible<\/option>/);
  assert.doesNotMatch(html, /<option value="openai_compat">/);
  assert.match(app, /c\.provider === 'openai_compat' \? 'open-ai-compat' : c\.provider/);
  assert.match(app, /provider === 'offline'\) return model === 'offline-scripted'/);
  assert.match(app, /provider === 'open-ai-compat'\) return model === 'gpt-4\.1' \|\| model === 'gpt-4o'/);
});
