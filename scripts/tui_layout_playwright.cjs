#!/usr/bin/env node
/*
 * Layout oracle for the native terminal face.
 *
 * The TUI is not a web page, so Playwright cannot inspect it directly. This
 * test drives the real binary in a tmux pty, captures the terminal's cells,
 * and exposes those cells as a deliberately boring DOM. Playwright then
 * checks the same layout contracts a browser workbench would: every row is
 * inside the viewport, the composer stays anchored, and narrow rails degrade
 * as complete labels instead of clipped fragments.
 */

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { chromium } = require(path.join(
  __dirname,
  "..",
  "apps",
  "optimus-electron",
  "node_modules",
  "playwright",
));

const ROOT = path.resolve(__dirname, "..");
const DEFAULT_BINARY = path.join(ROOT, "target", "debug", "optimus");
const VIEWPORTS = [
  [110, 32],
  [80, 24],
  [60, 20],
  [40, 20],
];

function tmux(...args) {
  return spawnSync("tmux", args, { encoding: "utf8" });
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function capture(session, rows) {
  const result = tmux("capture-pane", "-t", session, "-p");
  if (result.status !== 0) {
    throw new Error(result.stderr || `tmux capture failed for ${session}`);
  }
  const lines = result.stdout.replaceAll("\r", "").split("\n");
  while (lines.length < rows) lines.push("");
  return lines.slice(0, rows).map((line) => Array.from(line).slice(0, 400).join(""));
}

async function waitFor(session, rows, predicate, description) {
  const deadline = Date.now() + 15_000;
  let frame = capture(session, rows);
  while (Date.now() < deadline) {
    if (predicate(frame)) return frame;
    await sleep(80);
    frame = capture(session, rows);
  }
  throw new Error(`${description}\n${renderFrame(frame)}`);
}

function renderFrame(lines) {
  return lines.map((line, index) => `${String(index).padStart(2, "0")} ${line}`).join("\n");
}

function composerGeometry(lines, cols) {
  const top = lines.findIndex((line) => line.includes("┌"));
  const bottom = lines.findIndex((line, index) => index > top && line.includes("└"));
  assert(top >= 0, `composer top border is missing at ${cols} columns\n${renderFrame(lines)}`);
  assert(bottom > top, `composer bottom border is missing at ${cols} columns\n${renderFrame(lines)}`);
  const left = lines[top].indexOf("┌");
  const right = lines[top].lastIndexOf("┐");
  assert(right > left, `composer right border is missing at ${cols} columns\n${renderFrame(lines)}`);
  return { top, bottom, left, right, status: bottom + 1, help: bottom + 2 };
}

function normalizeCells(lines, cols) {
  return lines.map((line) => {
    const cells = Array.from(line);
    while (cells.length < cols) cells.push(" ");
    return cells.slice(0, cols);
  });
}

function regionFor(row, geometry) {
  if (row === 0) return "context";
  if (row >= geometry.top && row <= geometry.bottom) return "composer";
  if (row === geometry.status) return "status";
  if (row === geometry.help) return "help";
  return "transcript";
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function escapeAttribute(value) {
  return escapeHtml(value).replaceAll("'", "&#39;");
}

function domForFrame(frame, cols, rows, geometry) {
  const cells = normalizeCells(frame, cols);
  const rowMarkup = cells
    .map((row, rowIndex) => {
      const cellMarkup = row
        .map(
          (character, column) =>
            `<span class="cell" data-col="${column}" data-char="${escapeAttribute(character)}">${escapeHtml(character)}</span>`,
        )
        .join("");
      return `<div class="row" data-row="${rowIndex}" data-region="${regionFor(rowIndex, geometry)}">${cellMarkup}</div>`;
    })
    .join("");
  return `<!doctype html>
    <html>
      <head>
        <style>
          :root { color-scheme: dark; }
          html, body { margin: 0; background: #0c0c0c; color: #e4e4e4; }
          body { font: 16px/1 monospace; }
          #terminal { width: ${cols}ch; height: ${rows}em; overflow: hidden; }
          .row { display: block; height: 1em; white-space: pre; }
          .cell { display: inline-block; width: 1ch; height: 1em; }
        </style>
      </head>
      <body><main id="terminal" data-cols="${cols}" data-rows="${rows}">${rowMarkup}</main></body>
    </html>`;
}

async function assertDom(page, frame, cols, rows, geometry) {
  await page.setContent(domForFrame(frame, cols, rows, geometry));
  const rowLocator = page.locator("#terminal > .row");
  assert.equal(await rowLocator.count(), rows, `${cols}x${rows}: DOM row count`);

  const measurements = await rowLocator.evaluateAll((rowNodes) =>
    rowNodes.map((row) => ({
      cells: row.querySelectorAll(".cell").length,
      top: row.getBoundingClientRect().top,
      bottom: row.getBoundingClientRect().bottom,
      region: row.dataset.region,
    })),
  );
  assert(measurements.every(({ cells }) => cells === cols), `${cols}x${rows}: every terminal row must have ${cols} cells`);
  assert(measurements.every(({ bottom, top }) => bottom > top), `${cols}x${rows}: every row must have visible height`);
  assert.equal(measurements[geometry.top].region, "composer", `${cols}x${rows}: composer region marker`);
  assert.equal(measurements[geometry.help].region, "help", `${cols}x${rows}: help region marker`);

  const geometryInBrowser = await page.evaluate(() => {
    const terminal = document.querySelector("#terminal");
    const firstCell = document.querySelector(".cell");
    const firstRow = document.querySelector(".row");
    return {
      terminalWidth: terminal.getBoundingClientRect().width,
      cellWidth: firstCell.getBoundingClientRect().width,
      rowHeight: firstRow.getBoundingClientRect().height,
    };
  });
  assert(geometryInBrowser.cellWidth > 0, `${cols}x${rows}: DOM cell width`);
  assert(geometryInBrowser.rowHeight > 0, `${cols}x${rows}: DOM row height`);
  assert(
    Math.abs(geometryInBrowser.terminalWidth - geometryInBrowser.cellWidth * cols) < geometryInBrowser.cellWidth * 2,
    `${cols}x${rows}: DOM terminal width must be a cell grid`,
  );
}

function assertFrame(frame, cols, rows, geometry, label) {
  const [context] = frame;
  const joined = frame.join(" ").replace(/\s+/g, " ");
  assert(geometry.left >= 2, `${label}: workbench needs a left breathing gutter`);
  assert(geometry.right <= cols - 3, `${label}: composer must leave a right breathing gutter`);
  assert(geometry.right - geometry.left >= 20, `${label}: composer is too cramped to read`);
  assert.equal(geometry.status, rows - 2, `${label}: status rail must stay anchored above the help rail`);
  assert.equal(geometry.help, rows - 1, `${label}: help rail must stay on the last row`);
  assert(context.includes("auto"), `${label}: provider must remain visible in the context rail`);
  assert(context.includes("  auto"), `${label}: context and provider need a readable separator`);
  assert(joined.includes("What should Optimus do?"), `${label}: greeting title was clipped\n${renderFrame(frame)}`);
  assert(
    joined.includes("Describe a task and press Enter. Ctrl-C stops a run; Esc clears a draft."),
    `${label}: greeting help was clipped instead of wrapping\n${renderFrame(frame)}`,
  );

  const help = frame[geometry.help];
  if (cols >= 52) {
    assert(help.includes("Enter:send") && help.includes("Tab:inspect") && help.includes("Esc:clear"), `${label}: full help rail`);
  } else if (cols >= 36) {
    assert(help.includes("↵:send") && help.includes("Tab:inspect") && help.includes("Esc:clear"), `${label}: compact help rail`);
  } else {
    assert(help.includes("Esc:clear"), `${label}: smallest help rail must keep the exit affordance`);
  }
}

function assertActiveFrame(frame, rows, geometry, label) {
  const joined = frame.join(" ").replace(/\s+/g, " ");
  assert(joined.includes("› layout ping"), `${label}: user turn is missing\n${renderFrame(frame)}`);
  assert(joined.includes("offline echo: layout ping"), `${label}: assistant turn is missing\n${renderFrame(frame)}`);
  assert(frame[geometry.status].includes("ready"), `${label}: settled status is missing`);
  assert.equal(geometry.status, rows - 2, `${label}: status rail drifted after a turn`);
  assert.equal(geometry.help, rows - 1, `${label}: help rail drifted after a turn`);
}

function launch(binary, home, session, cols, rows) {
  const command = `${shellQuote(binary)} --home ${shellQuote(home)}`;
  const result = tmux(
    "new-session",
    "-d",
    "-s",
    session,
    "-x",
    String(cols),
    "-y",
    String(rows),
    "--",
    command,
  );
  if (result.status !== 0) throw new Error(result.stderr || "tmux could not start the TUI");
}

async function checkViewport(browser, binary, cols, rows) {
  const session = `optimus-tui-layout-${process.pid}-${cols}-${rows}`;
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "optimus-tui-layout-"));
  const page = await browser.newPage({ viewport: { width: cols * 10, height: rows * 18 } });
  try {
    launch(binary, home, session, cols, rows);
    const idle = await waitFor(session, rows, (frame) => frame.join("\n").includes("ready"), `${cols}x${rows}: launch never reached ready`);
    const idleGeometry = composerGeometry(idle, cols);
    assertFrame(idle, cols, rows, idleGeometry, `${cols}x${rows} idle`);
    await assertDom(page, idle, cols, rows, idleGeometry);

    const draft = `layout${"x".repeat(Math.max(10, cols))}`;
    const buffered = tmux("set-buffer", draft);
    if (buffered.status !== 0) throw new Error(buffered.stderr || "tmux could not prepare the draft");
    const typed = tmux("paste-buffer", "-t", session, "-p");
    if (typed.status !== 0) throw new Error(typed.stderr || "tmux could not paste the draft");
    const drafted = await waitFor(session, rows, (frame) => frame.join("\n").includes(draft.slice(-8)), `${cols}x${rows}: draft never painted`);
    const draftGeometry = composerGeometry(drafted, cols);
    assert(
      draftGeometry.top < idleGeometry.top,
      `${cols}x${rows}: wrapped draft must grow upward (idle top ${idleGeometry.top}, draft top ${draftGeometry.top})\n${renderFrame(drafted)}`,
    );
    assert.equal(draftGeometry.status, rows - 2, `${cols}x${rows}: draft must not move the status rail`);
    await assertDom(page, drafted, cols, rows, draftGeometry);

    const cleared = tmux("send-keys", "-t", session, "Escape");
    if (cleared.status !== 0) throw new Error(cleared.stderr || "tmux could not clear the draft");
    await waitFor(session, rows, (frame) => !frame.join("\n").includes(draft.slice(-8)), `${cols}x${rows}: Escape did not clear the draft`);
    const prompt = tmux("send-keys", "-t", session, "-l", "layout ping");
    if (prompt.status !== 0) throw new Error(prompt.stderr || "tmux could not type a prompt");
    const submitted = tmux("send-keys", "-t", session, "Enter");
    if (submitted.status !== 0) throw new Error(submitted.stderr || "tmux could not submit a prompt");
    const answered = await waitFor(
      session,
      rows,
      (frame) => frame.join("\n").includes("offline echo: layout ping") && frame.join("\n").includes("ready"),
      `${cols}x${rows}: offline turn never settled`,
    );
    const activeGeometry = composerGeometry(answered, cols);
    assertActiveFrame(answered, rows, activeGeometry, `${cols}x${rows} active`);
    await assertDom(page, answered, cols, rows, activeGeometry);

    console.log(`TUI_LAYOUT_PLAYWRIGHT_OK ${cols}x${rows} idle+wrapped-draft+active-turn`);
  } finally {
    tmux("kill-session", "-t", session);
    await page.close();
    fs.rmSync(home, { recursive: true, force: true });
  }
}

async function main() {
  const binaryIndex = process.argv.indexOf("--binary");
  const binary = path.resolve(binaryIndex >= 0 ? process.argv[binaryIndex + 1] : DEFAULT_BINARY);
  assert(fs.existsSync(binary), `${binary} does not exist — build optimus-cli first`);
  assert(tmux("-V").status === 0, "tmux is required for the terminal layout oracle");

  let browser;
  try {
    browser = await chromium.launch({ headless: true });
  } catch (error) {
    // The repository's other Playwright suites usually install their own
    // browser payload. The terminal oracle is also useful on a developer
    // machine with only the distro Chromium available, so keep that fallback
    // explicit and still let Playwright own the browser session.
    const systemChromium = process.env.OPTIMUS_TUI_CHROMIUM || "/usr/bin/chromium";
    if (!fs.existsSync(systemChromium)) throw error;
    browser = await chromium.launch({ headless: true, executablePath: systemChromium });
  }
  try {
    for (const [cols, rows] of VIEWPORTS) await checkViewport(browser, binary, cols, rows);
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(`TUI_LAYOUT_PLAYWRIGHT_FAIL: ${error.message}`);
  process.exitCode = 1;
});
