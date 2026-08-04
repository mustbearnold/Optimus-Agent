#!/usr/bin/env node

const assert = require("node:assert/strict");
const { chromium } = require("../apps/optimus-electron/node_modules/playwright");

const URL = process.env.OPTIMUS_UI_URL || "http://127.0.0.1:4173";
const HEIGHT = 900;
const INTERACTIVE_SELECTOR = [
  "button",
  "a",
  "input",
  "textarea",
  "select",
  '[role="button"]',
  '[role="link"]',
  '[role="tab"]',
  '[role="menuitem"]',
  '[role="option"]',
  '[role="checkbox"]',
  '[role="radio"]',
  '[role="switch"]',
  '[contenteditable="true"]',
  "summary",
  "[data-audit-target]",
].join(", ");

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const scenarios = [
  ["desktop-open", 1440, async () => {}],
  ["desktop-collapsed-rail", 1440, async (page) => {
    await page.getByRole("button", { name: "Close project rail", exact: true }).click();
  }],
  ["product-menu", 1440, async (page) => {
    await page.getByRole("button", { name: "Optimus", exact: true }).click();
  }],
  ["project-scope-menu", 1440, async (page) => {
    await page.getByRole("button", { name: "All projects", exact: true }).click();
  }],
  ["access-menu", 1440, async (page) => {
    await page.getByRole("button", { name: "Access: Standard", exact: true }).click();
  }],
  ["run-settings-menu", 1440, async (page) => {
    await page.getByRole("button", { name: "Model and run settings", exact: true }).click();
  }],
  ["workspace-surface", 1440, async (page) => {
    await page.getByRole("button", { name: "Workspace", exact: true }).click();
  }],
  ["terminal-surface", 1440, async (page) => {
    await page.getByRole("button", { name: "Terminal", exact: true }).first().click();
  }],
  ["settings-dialog", 1440, async (page) => {
    await page.getByRole("button", { name: "Settings", exact: true }).click();
  }],
  ["project-hover", 1440, async (page) => {
    await page.locator(".project-heading").first().hover();
  }],
  ["project-sources-dialog", 1440, async (page) => {
    await page.locator(".project-heading").first().hover();
    await page.getByRole("button", { name: /Manage sources/ }).click();
  }],
  ["add-project-dialog", 1440, async (page) => {
    await page.getByRole("button", { name: "Add project", exact: true }).click();
  }],
  ["new-project-folder-dialog", 1440, async (page) => {
    await page.getByRole("button", { name: "Create project folder", exact: true }).click();
  }],
  ["session-context-menu", 1440, async (page) => {
    await page.locator(".session-row").first().click({ button: "right" });
  }],
  ["mobile-open", 600, async () => {}],
  ["mobile-browser", 600, async (page) => {
    await page.getByRole("tab", { name: "browser", exact: true }).click();
  }],
  ["mobile-files", 600, async (page) => {
    await page.getByRole("tab", { name: "files", exact: true }).click();
  }],
  ["mobile-artifacts", 600, async (page) => {
    await page.getByRole("tab", { name: "artifacts", exact: true }).click();
  }],
  ["mobile-execution", 600, async (page) => {
    await page.getByRole("tab", { name: "execution", exact: true }).click();
  }],
];

async function openPage(browser, width) {
  const context = await browser.newContext({ viewport: { width, height: HEIGHT } });
  const page = await context.newPage();
  const pageErrors = [];
  const consoleErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  await page.goto(URL, { waitUntil: "domcontentloaded" });
  await page.evaluate(() => localStorage.clear());
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.waitForTimeout(100);
  return { context, page, pageErrors, consoleErrors };
}

async function inventory(page) {
  return page.evaluate((selector) => {
    return Array.from(document.querySelectorAll(selector)).map((element, rawIndex) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      const pointX = Math.min(window.innerWidth - 1, Math.max(0, rect.left + rect.width / 2));
      const pointY = Math.min(window.innerHeight - 1, Math.max(0, rect.top + rect.height / 2));
      const hit = document.elementFromPoint(pointX, pointY);
      const label = (element.getAttribute("aria-label") ||
        element.getAttribute("title") ||
        element.textContent || "")
        .replace(/\s+/g, " ")
        .trim()
        .slice(0, 140);
      return {
        rawIndex,
        tag: element.tagName.toLowerCase(),
        label,
        id: element.id,
        className: typeof element.className === "string" ? element.className : "",
        disabled: element.hasAttribute("disabled") || element.getAttribute("aria-disabled") === "true",
        visible: rect.width > 0 && rect.height > 0 && style.display !== "none" && style.visibility !== "hidden",
        reachable: !hit || hit === element || element.contains(hit),
      };
    }).filter((item) => item.visible);
  }, INTERACTIVE_SELECTOR);
}

async function snapshot(page) {
  return page.evaluate((selector) => {
    const root = document.documentElement;
    const body = document.body;
    const rail = document.querySelector(".project-rail");
    const composer = document.querySelector(".composer-card");
    const workspace = document.querySelector(".workspace-surface");
    const terminal = document.querySelector(".terminal-surface");
    const controls = Array.from(document.querySelectorAll(selector)).filter((element) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return rect.width > 0 && rect.height > 0 && style.display !== "none" && style.visibility !== "hidden";
    });
    return {
      body: {
        clientWidth: body.clientWidth,
        scrollWidth: body.scrollWidth,
        clientHeight: body.clientHeight,
        scrollHeight: body.scrollHeight,
        rootScrollWidth: root.scrollWidth,
        rootClientWidth: root.clientWidth,
        rootScrollHeight: root.scrollHeight,
        rootClientHeight: root.clientHeight,
      },
      active: document.activeElement?.getAttribute("aria-label") || document.activeElement?.tagName || "",
      controls: controls.length,
      labels: controls.map((element) => (element.getAttribute("aria-label") || element.getAttribute("title") || element.textContent || "").replace(/\s+/g, " ").trim().slice(0, 100)),
      rail: rail ? { className: rail.className, width: Math.round(rail.getBoundingClientRect().width), overflow: getComputedStyle(rail).overflow } : null,
      composer: composer ? { background: getComputedStyle(composer).backgroundColor, border: getComputedStyle(composer).borderColor } : null,
      surfaces: {
        workspace: workspace ? getComputedStyle(workspace).display : "missing",
        terminal: terminal ? getComputedStyle(terminal).display : "missing",
      },
      dialogs: document.querySelectorAll('[role="dialog"], dialog, .dialog-surface, .menu-surface, .context-menu').length,
      bodyText: document.body.innerText.slice(0, 3000),
    };
  }, INTERACTIVE_SELECTOR);
}

function stableSnapshot(value) {
  return JSON.stringify({
    body: value.body,
    active: value.active,
    controls: value.controls,
    labels: value.labels,
    rail: value.rail,
    composer: value.composer,
    surfaces: value.surfaces,
    dialogs: value.dialogs,
    bodyText: value.bodyText,
  });
}

function overflow(snapshotValue) {
  const body = snapshotValue.body;
  return {
    horizontal: Math.max(body.scrollWidth - body.clientWidth, body.rootScrollWidth - body.rootClientWidth),
    vertical: Math.max(body.scrollHeight - body.clientHeight, body.rootScrollHeight - body.rootClientHeight),
  };
}

async function clickTarget(browser, scenario, target) {
  const [name, width, setup] = scenario;
  const started = Date.now();
  const opened = await openPage(browser, width);
  const { context, page, pageErrors, consoleErrors } = opened;
  try {
    await setup(page);
    await sleep(80);
    const before = await snapshot(page);
    const locator = page.locator(INTERACTIVE_SELECTOR).nth(target.rawIndex);
    if (target.disabled) {
      const box = await locator.boundingBox();
      assert(box, `${name}/${target.label}: disabled control has no bounding box`);
      await page.mouse.click(box.x + Math.max(1, box.width / 2), box.y + Math.max(1, box.height / 2));
    } else {
      await locator.click({ timeout: 1500 });
    }
    await sleep(120);
    const after = await snapshot(page);
    const overflowValue = overflow(after);
    assert(overflowValue.horizontal <= 1, `${name}/${target.label}: horizontal overflow ${overflowValue.horizontal}px`);
    assert(overflowValue.vertical <= 1, `${name}/${target.label}: vertical overflow ${overflowValue.vertical}px`);
    assert.equal(pageErrors.length, 0, `${name}/${target.label}: page errors ${pageErrors.join(" | ")}`);
    assert.equal(consoleErrors.length, 0, `${name}/${target.label}: console errors ${consoleErrors.join(" | ")}`);
    return {
      scenario: name,
      label: target.label || `${target.tag}[${target.rawIndex}]`,
      tag: target.tag,
      disabled: target.disabled,
      changed: stableSnapshot(before) !== stableSnapshot(after),
      before,
      after,
      elapsedMs: Date.now() - started,
    };
  } finally {
    await context.close();
  }
}

async function run() {
  const browser = await chromium.launch({ headless: true });
  const results = [];
  const failures = [];
  try {
    for (const scenario of scenarios) {
      const [name, width, setup] = scenario;
      const opened = await openPage(browser, width);
      let targets;
      try {
        await setup(opened.page);
        await sleep(100);
        const allTargets = await inventory(opened.page);
        targets = allTargets.filter((target) => target.reachable);
        const afterSetup = await snapshot(opened.page);
        const overflowValue = overflow(afterSetup);
        assert(overflowValue.horizontal <= 1, `${name}: setup horizontal overflow ${overflowValue.horizontal}px`);
        assert(overflowValue.vertical <= 1, `${name}: setup vertical overflow ${overflowValue.vertical}px`);
        console.log(JSON.stringify({
          event: "scenario",
          name,
          width,
          targets: targets.length,
          covered: allTargets.filter((target) => !target.reachable).map((target) => target.label || `${target.tag}[${target.rawIndex}]`),
          labels: targets.map((target) => target.label).filter(Boolean),
        }));
      } catch (error) {
        failures.push({ scenario: name, phase: "setup", error: error.message });
        console.log(JSON.stringify({ event: "scenario-failure", name, phase: "setup", error: error.message }));
        continue;
      } finally {
        await opened.context.close();
      }

      for (const target of targets) {
        try {
          const result = await clickTarget(browser, scenario, target);
          results.push(result);
          console.log(JSON.stringify({ event: "click", scenario: result.scenario, label: result.label, tag: result.tag, disabled: result.disabled, changed: result.changed, elapsedMs: result.elapsedMs }));
        } catch (error) {
          failures.push({ scenario: name, target: target.label || `${target.tag}[${target.rawIndex}]`, error: error.message });
          console.log(JSON.stringify({ event: "click-failure", scenario: name, label: target.label, error: error.message }));
        }
      }
    }
  } finally {
    await browser.close();
  }
  const noops = results.filter((result) => !result.changed);
  const summary = {
    event: "summary",
    scenarios: scenarios.length,
    clicked: results.length,
    disabledClicked: results.filter((result) => result.disabled).length,
    noops: noops.length,
    failures: failures.length,
    elapsedMs: results.reduce((total, result) => total + result.elapsedMs, 0),
    failureDetails: failures,
    noopDetails: noops.map((result) => ({ scenario: result.scenario, label: result.label, tag: result.tag })),
  };
  console.log(JSON.stringify(summary));
  if (failures.length > 0) process.exitCode = 1;
}

run().catch((error) => {
  console.error(`fatal=${error.stack}`);
  process.exitCode = 1;
});
