#!/usr/bin/env bun
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
const { chromium } = require("playwright");

const ROOT = path.resolve(__dirname, "..");
const DEFAULT_BINARY = path.join(ROOT, "target", "debug", "optimus");
const VIEWPORTS = [
  [110, 32],
  [80, 24],
  [60, 20],
  [52, 16],
  [40, 20],
  [32, 16],
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

function captureAnsi(session) {
  const result = tmux("capture-pane", "-e", "-t", session, "-p");
  if (result.status !== 0) {
    throw new Error(result.stderr || `tmux ANSI capture failed for ${session}`);
  }
  return result.stdout.replaceAll("\r", "");
}

async function waitFor(session, rows, predicate, description) {
  // The full gate runs browser, Electron, and PTY suites concurrently. Keep
  // polling responsive, but allow slow CI hosts to schedule the real TUI.
  const deadline = Date.now() + 30_000;
  let frame = capture(session, rows);
  while (Date.now() < deadline) {
    if (predicate(frame)) return frame;
    await sleep(80);
    frame = capture(session, rows);
  }
  throw new Error(`${description}\n${renderFrame(frame)}`);
}

// Like `waitFor`, but the matching frame must also be stable: two captures
// in a row return the same cells. A capture that lands mid-repaint — the
// reopen of the sidebar repaints the whole screen — returns a torn frame
// whose geometry fails assertions a settled frame passes. At idle nothing
// repaints, so stability is reached within one extra capture.
async function waitForSettled(session, rows, predicate, description) {
  const deadline = Date.now() + 30_000;
  let frame = capture(session, rows);
  while (Date.now() < deadline) {
    if (predicate(frame)) {
      await sleep(80);
      const settled = capture(session, rows);
      if (settled.join("\n") === frame.join("\n")) return frame;
      frame = settled;
      continue;
    }
    await sleep(80);
    frame = capture(session, rows);
  }
  throw new Error(`${description}\n${renderFrame(frame)}`);
}

function renderFrame(lines) {
  return lines.map((line, index) => `${String(index).padStart(2, "0")} ${line}`).join("\n");
}

function composerGeometry(lines, cols) {
  const top = lines.findIndex((line) => line.includes("╭"));
  const bottom = lines.findIndex((line, index) => index > top && line.includes("╰"));
  assert(top >= 0, `composer top border is missing at ${cols} columns\n${renderFrame(lines)}`);
  assert(bottom > top, `composer bottom border is missing at ${cols} columns\n${renderFrame(lines)}`);
  const left = lines[top].indexOf("╭");
  const right = lines[top].lastIndexOf("╮");
  assert(right > left, `composer right border is missing at ${cols} columns\n${renderFrame(lines)}`);
  return { top, bottom, left, right, status: bottom + 1, help: bottom + 2 };
}

// The busy state is identified by the status rail's marker, never by a phase
// label: the kernel replaces the initial "working" label with the first
// "model step N" status milliseconds after submit, so a predicate that waits
// for the label races a sub-millisecond window and only passes on slow hosts.
// The ◌ marker and the absence of "ready" hold for the whole turn.
function isBusy(lines) {
  const top = lines.findIndex((line) => line.includes("╭"));
  if (top < 0) return false;
  const bottom = lines.findIndex((line, index) => index > top && line.includes("╰"));
  if (bottom < 0 || bottom + 1 >= lines.length) return false;
  const rail = lines[bottom + 1];
  return rail.includes("◌") && !rail.includes("ready");
}

function sidebarGeometry(lines) {
  const header = lines.findIndex((line) => line.includes("WORKSPACE"));
  if (header < 0) return { open: false, divider: -1 };
  const dividerRow = lines.findIndex((line) => line.includes("┊"));
  return {
    open: true,
    divider: dividerRow < 0 ? -1 : lines[dividerRow].indexOf("┊"),
  };
}

function normalizeCells(lines, cols) {
  return lines.map((line) => {
    const cells = terminalCells(line);
    assert(cells.length <= cols, `captured terminal row exceeded ${cols} cells: ${line}`);
    while (cells.length < cols) cells.push(" ");
    return cells.slice(0, cols);
  });
}

// tmux capture-pane gives us glyphs, while the TUI reasons in terminal cells.
// Keep the projection honest for the same families that the Rust oracle tests:
// combining marks stay attached to the previous cell, and wide CJK/emoji/full-
// width glyphs consume two cells.
function codePointWidth(character) {
  const codePoint = character.codePointAt(0);
  if (
    codePoint === 0 ||
    (codePoint >= 0x300 && codePoint <= 0x36f) ||
    (codePoint >= 0x1ab0 && codePoint <= 0x1aff) ||
    (codePoint >= 0x1dc0 && codePoint <= 0x1dff) ||
    (codePoint >= 0x20d0 && codePoint <= 0x20ff) ||
    (codePoint >= 0xfe00 && codePoint <= 0xfe0f) ||
    (codePoint >= 0x200d && codePoint <= 0x200d)
  ) {
    return 0;
  }
  if (
    (codePoint >= 0x1100 && codePoint <= 0x115f) ||
    (codePoint >= 0x2329 && codePoint <= 0x232a) ||
    (codePoint >= 0x2e80 && codePoint <= 0xa4cf) ||
    (codePoint >= 0xac00 && codePoint <= 0xd7a3) ||
    (codePoint >= 0xf900 && codePoint <= 0xfaff) ||
    (codePoint >= 0xfe10 && codePoint <= 0xfe6f) ||
    (codePoint >= 0xff01 && codePoint <= 0xff60) ||
    (codePoint >= 0xffe0 && codePoint <= 0xffe6) ||
    (codePoint >= 0x1f300 && codePoint <= 0x1faff)
  ) {
    return 2;
  }
  return 1;
}

function terminalCells(line) {
  const cells = [];
  for (const character of Array.from(line)) {
    const width = codePointWidth(character);
    if (width === 0) {
      if (cells.length > 0) cells[cells.length - 1] += character;
      continue;
    }
    cells.push(character);
    if (width === 2) cells.push("");
  }
  return cells;
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
  const sidebar = sidebarGeometry(frame);
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
      <body><main id="terminal" data-cols="${cols}" data-rows="${rows}" data-sidebar-open="${sidebar.open}" data-sidebar-divider="${sidebar.divider}">${rowMarkup}</main></body>
    </html>`;
}

async function assertDom(page, frame, cols, rows, geometry, expectedSidebar = cols >= 67) {
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

  const sidebarState = await page.locator("#terminal").evaluate((terminal) => ({
    open: terminal.dataset.sidebarOpen === "true",
    divider: Number(terminal.dataset.sidebarDivider),
  }));
  assert.equal(sidebarState.open, expectedSidebar, `${cols}x${rows}: DOM sidebar state`);
  if (expectedSidebar) {
    assert.equal(sidebarState.divider, geometry.left - 1, `${cols}x${rows}: DOM divider must touch the composer rail`);
  }

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

async function assertNoDomOverflow(page, frame, cols, rows) {
  const geometry = { top: -2, bottom: -2, left: 0, right: 0, status: rows - 2, help: rows - 1 };
  normalizeCells(frame, cols);
  await page.setContent(domForFrame(frame, cols, rows, geometry));
  const overflow = await page.locator("#terminal").evaluate((terminal) => ({
    horizontal: terminal.scrollWidth - terminal.clientWidth,
    vertical: Math.max(
      0,
      ...Array.from(terminal.querySelectorAll(".row"), (row) =>
        row.getBoundingClientRect().bottom - terminal.getBoundingClientRect().bottom,
      ),
    ),
    widestRow: Math.max(
      0,
      ...Array.from(terminal.querySelectorAll(".row"), (row) => row.scrollWidth - terminal.clientWidth),
    ),
  }));
  assert(overflow.horizontal <= 1, `${cols}x${rows}: DOM grid overflowed horizontally by ${overflow.horizontal}px`);
  assert(overflow.vertical <= 1, `${cols}x${rows}: DOM grid overflowed vertically by ${overflow.vertical}px`);
  assert(overflow.widestRow <= 1, `${cols}x${rows}: a DOM row overflowed by ${overflow.widestRow}px`);
}

async function assertOverlay(page, session, frame, cols, rows, title, label) {
  assert(frame.some((line) => line.includes(title)), `${label}: overlay title is missing\n${renderFrame(frame)}`);
  assert(frame.some((line) => line.includes("›")), `${label}: overlay selection marker is missing\n${renderFrame(frame)}`);
  await assertNoDomOverflow(page, frame, cols, rows);
  const ansi = captureAnsi(session);
  assert(!ansi.includes("\u001b[7m"), `${label}: overlay still uses terminal reverse-video selection`);
}

function assertFrame(frame, cols, rows, geometry, label) {
  const [context] = frame;
  const joined = frame.join(" ").replace(/\s+/g, " ");
  const readable = frame.map((line) => line.slice(geometry.left)).join(" ").replace(/\s+/g, " ");
  const sidebar = sidebarGeometry(frame);
  assert(geometry.left >= 2, `${label}: workbench needs a left breathing gutter`);
  assert(geometry.right <= cols - 3, `${label}: composer must leave a right breathing gutter`);
  assert(geometry.right - geometry.left >= 20, `${label}: composer is too cramped to read`);
  assert.equal(geometry.status, rows - 2, `${label}: status rail must stay anchored above the help rail`);
  assert.equal(geometry.help, rows - 1, `${label}: help rail must stay on the last row`);
  if (cols >= 67) {
    assert(sidebar.open, `${label}: workspace sidebar should be visible at this width`);
    for (const item of ["WORKSPACE", "New session", "SESSIONS", "PROJECTS", "PINNED"]) {
      assert(joined.includes(item), `${label}: sidebar item ${item} is missing\n${renderFrame(frame)}`);
    }
    assert.equal(sidebar.divider, geometry.left - 1, `${label}: sidebar divider must meet the main workbench`);
  } else {
    assert(!sidebar.open, `${label}: sidebar should collapse before the main workbench becomes cramped`);
    assert(frame[0].startsWith("›"), `${label}: collapsed sidebar needs its reopen tab`);
  }
  assert(context.includes("auto"), `${label}: provider must remain visible in the context rail`);
  assert(context.includes("  auto"), `${label}: context and provider need a readable separator`);
  assert(readable.includes("What should Optimus do?"), `${label}: greeting title was clipped\n${renderFrame(frame)}`);
  assert(
    readable.includes("Describe a task and press Enter.") &&
      readable.includes("Ctrl-C stops a run; Esc clears a draft."),
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

function assertBusyFrame(frame, cols, rows, geometry, label) {
  const joined = frame.join(" ").replace(/\s+/g, " ");
  assert(joined.includes("layout ping"), `${label}: busy turn prompt is missing\n${renderFrame(frame)}`);
  assert(joined.includes("Ctrl-C"), `${label}: busy interrupt affordance is missing\n${renderFrame(frame)}`);
  assert(frame[geometry.status].includes("◌"), `${label}: busy status marker is missing\n${renderFrame(frame)}`);
  assert(!frame[geometry.status].includes("ready"), `${label}: busy status rail still shows ready\n${renderFrame(frame)}`);
  if (cols >= 47) {
    assert(frame[geometry.help].includes("Ctrl-C:stop"), `${label}: full busy help rail`);
  } else if (cols >= 39) {
    assert(frame[geometry.help].includes("^C:stop"), `${label}: compact busy help rail`);
    assert(!frame[geometry.help].includes("Ctrl-C:stop"), `${label}: compact rail must not clip Ctrl-C`);
  } else {
    assert(frame[geometry.help].includes("Esc:clear"), `${label}: smallest busy help rail`);
  }
  assert.equal(geometry.status, rows - 2, `${label}: busy status rail drifted`);
  assert.equal(geometry.help, rows - 1, `${label}: busy help rail drifted`);
}

function assertActiveFrame(frame, rows, geometry, label, prompt = "layout ping") {
  const joined = textProjection(frame);
  assert(joined.includes(prompt), `${label}: user turn is missing\n${renderFrame(frame)}`);
  assert(joined.includes(`offline echo: ${prompt}`), `${label}: assistant turn is missing\n${renderFrame(frame)}`);
  assert(frame[geometry.status].includes("ready"), `${label}: settled status is missing\n${renderFrame(frame)}`);
  assert.equal(geometry.status, rows - 2, `${label}: status rail drifted after a turn`);
  assert.equal(geometry.help, rows - 1, `${label}: help rail drifted after a turn`);
}

function textProjection(frame) {
  return frame
    .join(" ")
    .replace(/[╭╮╰╯│─]/g, " ")
    .replace(/\s+/g, " ");
}

function settled(frame) {
  return frame.some((line) => line.includes("turn · ready") || line.includes("· ready"));
}

function launch(binary, home, session, cols, rows, environment = {}) {
  const prefix = Object.entries(environment)
    .map(([key, value]) => `${key}=${shellQuote(value)}`)
    .join(" ");
  const command = `${prefix}${prefix ? " " : ""}${shellQuote(binary)} --home ${shellQuote(home)}`;
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

function sendMouse(session, kind, column, row) {
  const code = kind === "drag" ? 32 : 0;
  const suffix = kind === "up" ? "m" : "M";
  const sequence = `\u001b[<${code};${column + 1};${row + 1}${suffix}`;
  const result = tmux("send-keys", "-t", session, "-l", sequence);
  if (result.status !== 0) throw new Error(result.stderr || `tmux could not send ${kind} mouse event`);
}

async function checkViewport(browser, binary, cols, rows) {
  const session = `optimus-tui-layout-${process.pid}-${cols}-${rows}`;
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "optimus-tui-layout-"));
  const page = await browser.newPage({ viewport: { width: cols * 10, height: rows * 18 } });
  try {
    launch(binary, home, session, cols, rows, { OPTIMUS_OFFLINE_LATENCY_MS: "250" });
    let idle = await waitFor(session, rows, (frame) => frame.join("\n").includes("ready"), `${cols}x${rows}: launch never reached ready`);
    let idleGeometry = composerGeometry(idle, cols);
    assertFrame(idle, cols, rows, idleGeometry, `${cols}x${rows} idle`);
    await assertDom(page, idle, cols, rows, idleGeometry);

    const suggestionInput = tmux("send-keys", "-t", session, "-l", "/pro");
    if (suggestionInput.status !== 0) throw new Error(suggestionInput.stderr || `${cols}x${rows}: could not type suggestion prefix`);
    const suggestions = await waitFor(
      session,
      rows,
      (frame) => frame.join("\n").includes("Tab to complete"),
      `${cols}x${rows}: slash suggestions never appeared`,
    );
    await assertOverlay(page, session, suggestions, cols, rows, "Tab to complete", `${cols}x${rows} suggestions`);
    const clearSuggestions = tmux("send-keys", "-t", session, "Escape");
    if (clearSuggestions.status !== 0) throw new Error(clearSuggestions.stderr || `${cols}x${rows}: could not clear suggestions`);
    await waitFor(session, rows, (frame) => !frame.join("\n").includes("Tab to complete"), `${cols}x${rows}: suggestions did not close`);

    const pickerInput = tmux("send-keys", "-t", session, "-l", "/providers");
    if (pickerInput.status !== 0) throw new Error(pickerInput.stderr || `${cols}x${rows}: could not type provider command`);
    const pickerSubmit = tmux("send-keys", "-t", session, "Enter");
    if (pickerSubmit.status !== 0) throw new Error(pickerSubmit.stderr || `${cols}x${rows}: could not open provider picker`);
    const pickerFrame = await waitFor(
      session,
      rows,
      (frame) => frame.join("\n").includes("Select a provider"),
      `${cols}x${rows}: provider picker never appeared`,
    );
    await assertOverlay(page, session, pickerFrame, cols, rows, "Select a provider", `${cols}x${rows} picker`);
    const closePicker = tmux("send-keys", "-t", session, "Escape");
    if (closePicker.status !== 0) throw new Error(closePicker.stderr || `${cols}x${rows}: could not close provider picker`);
    await waitFor(session, rows, (frame) => !frame.join("\n").includes("Select a provider"), `${cols}x${rows}: provider picker did not close`);

    if (cols >= 80) {
      const initialSidebar = sidebarGeometry(idle);
      assert(initialSidebar.open, `${cols}x${rows}: sidebar interaction needs an open rail`);
      sendMouse(session, "down", initialSidebar.divider, Math.floor(rows / 2));
      sendMouse(session, "drag", initialSidebar.divider + 6, Math.floor(rows / 2));
      const resized = await waitForSettled(
        session,
        rows,
        (frame) => sidebarGeometry(frame).divider === initialSidebar.divider + 6,
        `${cols}x${rows}: divider drag never resized the sidebar`,
      );
      const resizedGeometry = composerGeometry(resized, cols);
      assert.equal(
        resizedGeometry.left,
        initialSidebar.divider + 7,
        `${cols}x${rows}: main workbench must move with the divider`,
      );
      await assertDom(page, resized, cols, rows, resizedGeometry);
      sendMouse(session, "up", initialSidebar.divider + 6, Math.floor(rows / 2));

      sendMouse(session, "down", initialSidebar.divider + 6, Math.floor(rows / 2));
      sendMouse(session, "drag", 6, Math.floor(rows / 2));
      const closed = await waitFor(
        session,
        rows,
        (frame) => !sidebarGeometry(frame).open,
        `${cols}x${rows}: far-left drag never closed the sidebar`,
      );
      const closedGeometry = composerGeometry(closed, cols);
      assert(closed[0].startsWith("›"), `${cols}x${rows}: closed sidebar tab is missing`);
      await assertDom(page, closed, cols, rows, closedGeometry, false);
      sendMouse(session, "up", 6, Math.floor(rows / 2));

      sendMouse(session, "down", 0, 0);
      idle = await waitForSettled(
        session,
        rows,
        (frame) => sidebarGeometry(frame).open,
        `${cols}x${rows}: collapsed tab never reopened the sidebar`,
      );
      idleGeometry = composerGeometry(idle, cols);
      assertFrame(idle, cols, rows, idleGeometry, `${cols}x${rows} reopened`);
      await assertDom(page, idle, cols, rows, idleGeometry);

      sendMouse(session, "down", 4, 3);
      const freshSession = await waitFor(
        session,
        rows,
        (frame) => frame.join("\n").includes("new session ready"),
        `${cols}x${rows}: New session click did not reset the workbench`,
      );
      assert(
        freshSession.join(" ").includes("New session"),
        `${cols}x${rows}: New session affordance disappeared after activation`,
      );
      sendMouse(session, "up", 4, 3);
    }

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
    const busy = await waitFor(
      session,
      rows,
      // Wait for the busy state itself, not a phase label: the initial
      // "working" is replaced by "model step N" milliseconds after submit, so
      // waiting for that literal only succeeds on slow hosts.
      (frame) => frame.join("\n").includes("Ctrl-C to interrupt") && isBusy(frame),
      `${cols}x${rows}: busy turn never became visible`,
    );
    const busyGeometry = composerGeometry(busy, cols);
    assertBusyFrame(busy, cols, rows, busyGeometry, `${cols}x${rows} busy`);
    await assertDom(page, busy, cols, rows, busyGeometry);

    const answered = await waitFor(
      session,
      rows,
      (frame) => textProjection(frame).includes("offline echo: layout ping") && settled(frame),
      `${cols}x${rows}: offline turn never settled`,
    );
    const activeGeometry = composerGeometry(answered, cols);
    assertActiveFrame(answered, rows, activeGeometry, `${cols}x${rows} active`);
    await assertDom(page, answered, cols, rows, activeGeometry);

    const unicodePrompt = "界👍e\u0301ｶ";
    const unicodeBuffer = tmux("set-buffer", unicodePrompt);
    if (unicodeBuffer.status !== 0) throw new Error(unicodeBuffer.stderr || "tmux could not prepare the Unicode prompt");
    const unicodeTyped = tmux("paste-buffer", "-t", session, "-p");
    if (unicodeTyped.status !== 0) throw new Error(unicodeTyped.stderr || "tmux could not paste the Unicode prompt");
    await waitFor(
      session,
      rows,
      (frame) => frame.join("\n").includes("界") && frame.join("\n").includes("ｶ"),
      `${cols}x${rows}: Unicode draft never painted`,
    );
    const unicodeEnter = tmux("send-keys", "-t", session, "Enter");
    if (unicodeEnter.status !== 0) throw new Error(unicodeEnter.stderr || "tmux could not submit the Unicode prompt");
    const unicodeAnswered = await waitFor(
      session,
      rows,
      (frame) => textProjection(frame).includes(`offline echo: ${unicodePrompt}`) && settled(frame),
      `${cols}x${rows}: Unicode offline turn never settled`,
    );
    const unicodeGeometry = composerGeometry(unicodeAnswered, cols);
    assertActiveFrame(unicodeAnswered, rows, unicodeGeometry, `${cols}x${rows} Unicode`, unicodePrompt);
    assert(unicodeAnswered.join(" ").includes(`offline echo: ${unicodePrompt}`), `${cols}x${rows}: Unicode response was clipped\n${renderFrame(unicodeAnswered)}`);
    await assertDom(page, unicodeAnswered, cols, rows, unicodeGeometry);

    console.log(`TUI_LAYOUT_PLAYWRIGHT_OK ${cols}x${rows} idle+wrapped-draft+busy+active+unicode`);
  } finally {
    tmux("kill-session", "-t", session);
    await page.close();
    fs.rmSync(home, { recursive: true, force: true });
  }
}

async function checkResizeSweep(browser, binary) {
  const session = `optimus-tui-layout-sweep-${process.pid}`;
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "optimus-tui-layout-sweep-"));
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  const widths = Array.from({ length: 121 }, (_, index) => index + 20);
  const heights = Array.from({ length: 31 }, (_, index) => index + 10);
  try {
    launch(binary, home, session, 110, 32);
    await waitFor(session, 32, (frame) => frame.join("\n").includes("ready"), "resize sweep launch never reached ready");

    for (const cols of widths) {
      const resized = tmux("resize-window", "-t", session, "-x", String(cols), "-y", "24");
      if (resized.status !== 0) throw new Error(resized.stderr || `${cols}x24 resize failed`);
      await sleep(45);
      const frame = await waitFor(
        session,
        24,
        (lines) => cols < 32 || (lines.some((line) => line.includes("╭")) && lines.some((line) => line.includes("╰"))),
        `${cols}x24 did not repaint after resize`,
      );
      await assertNoDomOverflow(page, frame, cols, 24);
      assert(tmux("has-session", "-t", session).status === 0, `${cols}x24: TUI exited during width sweep`);
    }

    for (const rows of heights) {
      const resized = tmux("resize-window", "-t", session, "-x", "80", "-y", String(rows));
      if (resized.status !== 0) throw new Error(resized.stderr || `80x${rows} resize failed`);
      await sleep(45);
      const frame = await waitFor(
        session,
        rows,
        (lines) => rows < 16 || (lines.some((line) => line.includes("╭")) && lines.some((line) => line.includes("╰"))),
        `80x${rows} did not repaint after resize`,
      );
      await assertNoDomOverflow(page, frame, 80, rows);
      assert(tmux("has-session", "-t", session).status === 0, `80x${rows}: TUI exited during height sweep`);
    }
    console.log(`TUI_LAYOUT_SWEEP_OK widths=${widths[0]}..${widths.at(-1)} heights=${heights[0]}..${heights.at(-1)}`);
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
    await checkResizeSweep(browser, binary);
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(`TUI_LAYOUT_PLAYWRIGHT_FAIL: ${error.message}`);
  process.exitCode = 1;
});
