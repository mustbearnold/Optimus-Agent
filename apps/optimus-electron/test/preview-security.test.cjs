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
  const winBlock = text.slice(
    text.indexOf('new BrowserWindow'),
    text.indexOf('createPreviewView')
  );
  assert.match(winBlock, /nodeIntegration:\s*false/);
  assert.match(winBlock, /contextIsolation:\s*true/);
  assert.match(winBlock, /sandbox:\s*true/);
  assert.match(winBlock, /webSecurity:\s*true/);
});

test('preview WebContentsView uses isolated partition and sandbox', () => {
  const text = fs.readFileSync(mainPath, 'utf8');
  assert.match(text, /partition:\s*'persist:optimus-preview'/);
  const previewBlock = text.slice(text.indexOf('function createPreviewView'));
  assert.match(previewBlock, /nodeIntegration:\s*false/);
  assert.match(previewBlock, /contextIsolation:\s*true/);
  assert.match(previewBlock, /sandbox:\s*true/);
  assert.match(previewBlock, /webSecurity:\s*true/);
  assert.match(previewBlock, /setPermissionRequestHandler/);
  assert.match(previewBlock, /setPermissionCheckHandler/);
  assert.match(previewBlock, /will-download/);
  assert.match(previewBlock, /setWindowOpenHandler/);
  assert.match(previewBlock, /will-navigate/);
  // Preview must not load the app preload.
  assert.doesNotMatch(previewBlock.slice(0, 800), /preload:\s*path\.join/);
});

test('preload does not expose project_root_stage_native to renderer', () => {
  const text = fs.readFileSync(preloadPath, 'utf8');
  assert.doesNotMatch(text, /project_root_stage_native/);
  // Dedicated OS channels exist; main-only staging stays off invoke surface.
  assert.match(text, /optimus:pick-folder|pickFolder|pick-folder/);
});
