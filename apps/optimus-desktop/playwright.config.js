// @ts-check
const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: './e2e',
  timeout: 90_000,
  expect: { timeout: 15_000 },
  // Each worker gets its own Rust host on an OS-assigned port and its own
  // OPTIMUS_HOME (see e2e/support.js `serverInfo`, scope: 'worker'), so workers
  // share no state. Capped rather than set to the core count because every
  // worker spawns a full desktop host process.
  fullyParallel: true,
  workers: process.env.CI ? 2 : 4,
  retries: 0,
  reporter: [['list'], ['html', { open: 'never', outputFolder: '../../local/tmp/playwright-report' }]],
  use: {
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'off',
  },
  outputDir: '../../local/tmp/playwright-output',
});
