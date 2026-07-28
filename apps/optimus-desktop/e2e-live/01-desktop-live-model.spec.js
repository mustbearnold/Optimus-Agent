// @ts-check
// Live desktop leg (#82): with real Codex credentials in the home, the
// desktop face must BOOT on Codex + a real model — not offline echo — and a
// real turn must round-trip a nonce the model could not have cached. Missing
// env or credentials are FAILURES, never skips (live-tier law, see
// scripts/live_smoke.py). Run via `scripts/verify.sh live`.
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const { test, expect, waitForReady } = require('../e2e/support');

const HOME = process.env.OPTIMUS_E2E_HOME || '';

test('desktop boots on codex and a real model echoes the nonce', async ({ page }) => {
  if (!HOME) {
    throw new Error(
      'FAIL: OPTIMUS_E2E_HOME is unset — the live leg needs the credentialed home'
    );
  }
  if (!fs.existsSync(path.join(HOME, 'auth.json'))) {
    throw new Error(
      `FAIL: ${HOME} holds no auth.json — connect Codex first (optimus auth)`
    );
  }

  await page.goto('/');
  await page.waitForFunction(() => window.__optimusBridgeInstalled === true);
  await waitForReady(page);

  // The #82 contract: a credentialed home boots on the real model with no
  // human intervention, even if offline residue is persisted.
  await expect(page.locator('#provider')).toHaveValue('codex');
  await expect(page.locator('#model')).not.toHaveValue('offline-echo');

  // A real home carries real history (the host/TUI smoke legs land here
  // too), and boot opens the latest session. Start a fresh one so the
  // transcript this spec asserts against contains only its own turn.
  await page.click('#newThread');
  await expect(page.locator('.msg.user .bubble')).toHaveCount(0);

  // Minimal thinking bounds the spend, same as the host/TUI legs.
  await page.selectOption('#thinkingLevel', 'minimal');

  const token = `LIVE-${crypto.randomBytes(3).toString('hex').toUpperCase()}`;
  const input = page.locator('#input');
  await input.fill(`Reply with exactly this token and nothing else: ${token}`);
  await input.press('Enter');

  await expect(page.locator('.msg.user .bubble').last()).toContainText(token);
  const reply = page.locator('.msg.assistant .bubble').last();
  await expect(reply).toContainText(token, { timeout: 180_000 });
  await expect(reply).not.toContainText('offline echo');
});
