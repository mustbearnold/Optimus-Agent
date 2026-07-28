// @ts-check
// Live-tier config: same worker fixture as ./e2e (support.js), but the specs
// under ./e2e-live drive a real model through a real credentialed home
// (OPTIMUS_E2E_HOME), so timeouts are model-scale and nothing is retried —
// a flaky pass against real tokens is not evidence.
const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: './e2e-live',
  timeout: 240_000,
  expect: { timeout: 30_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  use: {
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'off',
  },
  outputDir: '../../local/tmp/playwright-live-output',
});
