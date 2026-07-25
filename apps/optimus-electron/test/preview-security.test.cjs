/**
 * P15 U3: Preview WebContentsView and main window must not enable Node in the
 * renderer. Static scan of main.cjs webPreferences (no Electron runtime required).
 */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const mainPath = path.join(__dirname, '..', 'main.cjs');
const preloadPath = path.join(__dirname, '..', 'preload.cjs');

test('main window webPreferences are sandboxed without nodeIntegration', () => {
  const text = fs.readFileSync(mainPath, 'utf8');
  // Main BrowserWindow block.
  assert.match(text, /nodeIntegration:\s*false/);
  assert.match(text, /contextIsolation:\s*true/);
  assert.match(text, /sandbox:\s*true/);
  assert.match(text, /webSecurity:\s*true/);
});

test('preview WebContentsView uses isolated partition and sandbox', () => {
  const text = fs.readFileSync(mainPath, 'utf8');
  assert.match(text, /partition:\s*'persist:optimus-preview'/);
  const previewBlock = text.slice(text.indexOf('createPreviewView'));
  assert.match(previewBlock, /nodeIntegration:\s*false/);
  assert.match(previewBlock, /contextIsolation:\s*true/);
  assert.match(previewBlock, /sandbox:\s*true/);
  assert.match(previewBlock, /setPermissionRequestHandler/);
  assert.match(previewBlock, /will-download/);
  assert.match(previewBlock, /setWindowOpenHandler/);
});

test('preload does not expose project_root_stage_native to renderer', () => {
  const text = fs.readFileSync(preloadPath, 'utf8');
  assert.doesNotMatch(text, /project_root_stage_native/);
  // Dedicated OS channels exist; main-only staging stays off invoke surface.
  assert.match(text, /optimus:pick-folder|pickFolder|pick-folder/);
});
