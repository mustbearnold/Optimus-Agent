#!/usr/bin/env node
/**
 * Geometry invariants for the React workbench.
 *
 * The existing UI suites assert text and named measurements: "the rail is
 * 240px", "this control is labelled X". They pass while the shell looks wrong,
 * because nothing asks the questions a person asks on sight — is that text cut
 * in half, is that chat listed twice, is that panel mostly empty, is that label
 * squeezed to three characters. Those are relationships between boxes, not
 * values a test author thought to pin, so they have to be measured rather than
 * enumerated.
 *
 * Every rule here is a defect that shipped. Add the rule when you fix the bug.
 *
 * Usage: node scripts/tests/ui_layout_audit.cjs [--url http://127.0.0.1:4174/]
 */

const path = require("node:path");
const { chromium } = require("playwright");

const URL = process.env.OPTIMUS_UI_URL || "http://127.0.0.1:4174/";

// Width, height, and what to open first. The workspace cases matter because a
// panel steals width from the work column without changing the viewport.
const VIEWPORTS = [
  { name: "wide", width: 1600, height: 1000, workspace: false },
  { name: "wide+workspace", width: 1600, height: 1000, workspace: true },
  { name: "installed", width: 1280, height: 833, workspace: false },
  { name: "installed+workspace", width: 1280, height: 833, workspace: true },
  { name: "medium", width: 1000, height: 800, workspace: false },
  { name: "narrow", width: 860, height: 760, workspace: false },
  // States, not just sizes: each of these has shipped its own defect class.
  { name: "installed+menu", width: 1280, height: 833, workspace: false, openScopeMenu: true },
  { name: "installed+light", width: 1280, height: 833, workspace: false, theme: "light" },
  { name: "installed+compact", width: 1280, height: 833, workspace: false, density: "compact" },
  { name: "installed+collapsed-rail", width: 1280, height: 833, workspace: false, collapseRail: true },
  { name: "installed+long-data", width: 1280, height: 833, workspace: false, longData: true },
  { name: "narrow+long-data", width: 900, height: 700, workspace: false, longData: true },
];

/** Runs in the page. Returns violations, not raw geometry. */
function collect() {
  const violations = [];
  const box = (el) => el.getBoundingClientRect();
  const label = (el) =>
    (el.getAttribute("aria-label") ||
      el.getAttribute("title") ||
      el.textContent ||
      "")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 60);

  // 1. Text painted outside whatever actually clips it.
  //
  //    Checking `scrollHeight > clientHeight` on the row is useless: the row is
  //    `overflow: visible`, so its content spills instead of scrolling and the
  //    two are always equal. The glyphs are cut by an *ancestor* — the rail —
  //    which is why a fixed-height card sliced its own title in the shipped
  //    build while every suite stayed green. Measure against the real clipper.
  // Being past the fold of a *scrollable* ancestor is reachable, not clipped —
  // sixty chats in a scrolling band are fine. Only a hidden/clip ancestor makes
  // content unreachable, and a scroll container that cannot actually scroll
  // behaves like plain overflow for its content.
  const clipperOf = (start) => {
    let node = start.parentElement;
    while (node && node !== document.body) {
      const style = getComputedStyle(node);
      const overflowY = style.overflowY;
      if (overflowY === "auto" || overflowY === "scroll") {
        if (node.scrollHeight > node.clientHeight + 2) return null; // reachable by scrolling
      } else if (overflowY === "hidden" || overflowY === "clip" || style.overflow === "hidden") {
        return node;
      }
      node = node.parentElement;
    }
    return null;
  };
  for (const el of document.querySelectorAll(
    ".session-title, .session-card-meta, .session-worktree, .session-state, .rail-empty, " +
      ".rail-section-heading, .workbench-status-segment, .composer-access-trigger, " +
      ".composer-settings-trigger, .project-heading"
  )) {
    const rect = box(el);
    if (rect.height === 0 || getComputedStyle(el).visibility === "hidden") continue;
    const clipper = clipperOf(el);
    if (!clipper) continue;
    const bounds = box(clipper);
    const cutBottom = rect.bottom - bounds.bottom;
    const cutTop = bounds.top - rect.top;
    // A horizontal cut is normal (ellipsis). A vertical cut slices glyphs.
    if (cutBottom > 1 || cutTop > 1) {
      violations.push({
        rule: "clipped-text",
        el: el.className,
        label: label(el),
        detail: `cut ${Math.round(Math.max(cutBottom, cutTop))}px by .${String(clipper.className).split(" ")[0]}`,
      });
    }
  }

  // 1b. Content painted outside the box that is supposed to hold it.
  //
  //     The ancestor check above only fires once something downstream actually
  //     cuts the glyphs, which depends on how tall the rail happens to be and
  //     how long the title happens to be. This one is unconditional: if a card
  //     is given a fixed height and its own children do not fit inside it, that
  //     is a defect now, whether or not today's content makes it visible.
  for (const container of document.querySelectorAll(
    ".session-select, .composer-controls, .rail-section-heading, .workbench-status-segment"
  )) {
    const bounds = box(container);
    if (bounds.height === 0) continue;
    const style = getComputedStyle(container);
    const padTop = parseFloat(style.paddingTop) || 0;
    const padBottom = parseFloat(style.paddingBottom) || 0;
    for (const child of container.children) {
      const rect = box(child);
      if (rect.height === 0) continue;
      const spillBottom = rect.bottom - (bounds.bottom - padBottom);
      const spillTop = bounds.top + padTop - rect.top;
      if (spillBottom > 1 || spillTop > 1) {
        violations.push({
          rule: "content-overflows-container",
          el: child.className || child.tagName.toLowerCase(),
          label: label(child),
          detail: `spills ${Math.round(Math.max(spillBottom, spillTop))}px out of .${String(container.className).split(" ")[0]} (${Math.round(bounds.height)}px tall)`,
        });
      }
    }
  }

  // 1c. A text box shorter than one line of its own text.
  //
  //     This is what a too-short card actually does. The children are flex
  //     items, so they do not spill — they are *compressed*, and the title ends
  //     up in a 5px box rendering 12px glyphs, sliced through the middle. The
  //     box is inside its parent and inside the viewport, so every containment
  //     check passes; only comparing the box to its own leading finds it.
  for (const el of document.querySelectorAll(
    ".session-title, .session-card-meta, .session-state, .session-worktree, " +
      ".rail-empty, .workbench-status-segment span, .composer-access-trigger span"
  )) {
    const text = (el.textContent || "").trim();
    if (!text) continue;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden") continue;
    const rect = box(el);
    if (rect.height === 0) continue;
    const fontSize = parseFloat(style.fontSize) || 0;
    const lineHeight = parseFloat(style.lineHeight);
    const needed = Number.isFinite(lineHeight) ? lineHeight : fontSize * 1.2;
    if (needed - rect.height > 1) {
      violations.push({
        rule: "text-box-shorter-than-line",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `${Math.round(rect.height)}px box for ${Math.round(needed)}px of line`,
      });
    }
  }

  // 2. The same session listed in more than one rail band.
  const bands = new Map();
  for (const row of document.querySelectorAll(".project-rail .session-row")) {
    const id = row.getAttribute("data-session-id");
    if (!id) continue;
    const band =
      row.closest(".rail-section")?.getAttribute("data-testid") || "unknown";
    if (!bands.has(id)) bands.set(id, new Set());
    bands.get(id).add(band);
  }
  for (const [id, set] of bands) {
    if (set.size > 1) {
      violations.push({
        rule: "duplicate-session",
        el: id,
        label: [...set].join(" + "),
        detail: `listed in ${set.size} bands`,
      });
    }
  }

  // 3. A visible label squeezed to a fraction of its own text.
  for (const el of document.querySelectorAll(
    ".workbench-status-segment span, .composer-access-trigger span, .composer-settings-trigger span, .rail-section-heading > span"
  )) {
    const style = getComputedStyle(el);
    // `width === 0` is not "absent" — a label crushed to nothing by a greedy
    // flex row measures zero and is the worst case of this defect, not an
    // exemption from it. Only genuinely hidden or empty elements are skipped.
    if (style.display === "none" || style.visibility === "hidden") continue;
    if (!(el.textContent || "").trim()) continue;
    if (el.scrollWidth > el.clientWidth + 1 && el.clientWidth < el.scrollWidth * 0.6) {
      violations.push({
        rule: "squeezed-label",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `${el.clientWidth}px of ${el.scrollWidth}px shown`,
      });
    }
  }

  // 4. A rail band far taller than its content reads as a rendering fault.
  for (const section of document.querySelectorAll(".rail-scroll > .rail-section")) {
    const body = section.querySelector(".session-stack, .project-stack");
    if (!body) continue;
    const bandHeight = box(section).height;
    const contentHeight = [...body.children].reduce(
      (total, child) => total + box(child).height,
      0
    );
    const heading = section.querySelector(".rail-section-heading");
    const headingHeight = heading ? box(heading).height : 0;
    const dead = bandHeight - contentHeight - headingHeight;
    if (dead > 90) {
      violations.push({
        rule: "dead-space",
        el: section.getAttribute("data-testid"),
        label: label(heading || section),
        detail: `${Math.round(dead)}px empty in a ${Math.round(bandHeight)}px band`,
      });
    }
  }

  // 5. Overlapping siblings in a stack: two rows drawn on top of each other.
  for (const stack of document.querySelectorAll(
    ".session-stack, .project-stack, .rail-scroll, .session-select, .rail-primary"
  )) {
    const kids = [...stack.children]
      .map((child) => ({ child, rect: box(child), style: getComputedStyle(child) }))
      // Absolutely positioned children are lifted out of the flow on purpose —
      // the run-state chip is meant to sit over the card's first line.
      .filter(({ rect, style }) => rect.height > 0 && style.position !== "absolute")
      .sort((a, b) => a.rect.top - b.rect.top);
    for (let i = 1; i < kids.length; i += 1) {
      const previous = kids[i - 1];
      const current = kids[i];
      const overlap = previous.rect.bottom - current.rect.top;
      if (overlap > 1) {
        violations.push({
          rule: "overlapping-siblings",
          el: current.child.className,
          label: label(current.child),
          detail: `${Math.round(overlap)}px over ${label(previous.child)}`,
        });
      }
    }
  }

  // 6. Anything painted outside the window.
  const view = { w: document.documentElement.clientWidth, h: document.documentElement.clientHeight };
  for (const el of document.querySelectorAll(
    ".project-rail, .workbench-statusbar, .composer-card, .topbar"
  )) {
    const rect = box(el);
    if (rect.width === 0) continue;
    if (rect.left < -1 || rect.right > view.w + 1) {
      violations.push({
        rule: "outside-viewport",
        el: el.className,
        label: label(el),
        detail: `x ${Math.round(rect.left)}..${Math.round(rect.right)} vs ${view.w}`,
      });
    }
  }

  // 7. Interactive controls below the comfortable hit target.
  for (const el of document.querySelectorAll(
    ".project-rail button, .composer-controls button, .workbench-statusbar button"
  )) {
    const rect = box(el);
    if (rect.width === 0 || rect.height === 0) continue;
    if (rect.height < 18 || rect.width < 18) {
      violations.push({
        rule: "tiny-hit-target",
        el: el.className,
        label: label(el),
        detail: `${Math.round(rect.width)}x${Math.round(rect.height)}`,
      });
    }
  }

  // 8. Typography. A shell reads as one product only if its type comes from one
  //    scale; stray sizes and cramped leading are what make a dense UI feel
  //    unfinished. 10px is the smallest size this design uses (rail headings).
  const TYPE_SCALE = [10, 11, 12, 13, 14, 15, 16, 18, 20, 24, 28, 32];
  const seenSizes = new Map();
  for (const el of document.querySelectorAll(
    ".project-rail *, .workbench-statusbar *, .composer-card *, .topbar *"
  )) {
    if (!el.textContent || !el.textContent.trim()) continue;
    if (el.children.length > 0) continue; // leaf text only
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden") continue;
    const size = Math.round(parseFloat(style.fontSize) * 10) / 10;
    if (!size) continue;
    seenSizes.set(size, (seenSizes.get(size) || 0) + 1);

    if (size < 10) {
      violations.push({
        rule: "type-too-small",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `${size}px`,
      });
    }
    if (!TYPE_SCALE.includes(Math.round(size))) {
      violations.push({
        rule: "type-off-scale",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `${size}px is not on the ${TYPE_SCALE.join("/")} scale`,
      });
    }
    const lineHeight = parseFloat(style.lineHeight);
    if (Number.isFinite(lineHeight) && lineHeight < size * 1.15 && box(el).height > 0) {
      violations.push({
        rule: "leading-too-tight",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `line-height ${lineHeight}px on ${size}px text`,
      });
    }
  }

  // 9. Symmetry. Horizontal padding on a container should match unless the
  //    design means it to differ; a lopsided card is the classic "why does this
  //    look off" defect that no text assertion can see.
  for (const el of document.querySelectorAll(
    ".rail-section-heading, .session-select, .composer-controls, .workbench-statusbar, .rail-search"
  )) {
    const style = getComputedStyle(el);
    const left = parseFloat(style.paddingLeft) || 0;
    const right = parseFloat(style.paddingRight) || 0;
    if (Math.abs(left - right) > 6) {
      violations.push({
        rule: "asymmetric-padding",
        el: el.className,
        label: label(el),
        detail: `padding-left ${left}px vs padding-right ${right}px`,
      });
    }
  }

  // Sibling rows in one stack should share a height; a single odd row out is a
  // layout accident, not a design.
  for (const stack of document.querySelectorAll(".session-stack")) {
    const heights = [...stack.querySelectorAll(":scope > .session-row")].map((row) =>
      Math.round(box(row).height)
    );
    if (heights.length < 2) continue;
    const min = Math.min(...heights);
    const max = Math.max(...heights);
    if (max - min > 1) {
      violations.push({
        rule: "uneven-sibling-rows",
        el: stack.className,
        label: stack.closest(".rail-section")?.getAttribute("data-testid") || "",
        detail: `row heights ${min}px..${max}px`,
      });
    }
  }


  // 10. Text that renders as nothing. A zero-height box, a 0px font, opacity 0
  //     anywhere up the chain, or a fully transparent colour all pass every
  //     geometry check while showing the user a blank rail.
  const parseColor = (value) => {
    const match = String(value).match(/rgba?\(([\d.]+)[, ]+([\d.]+)[, ]+([\d.]+)(?:[,/ ]+([\d.]+))?\)/);
    return match
      ? { r: +match[1], g: +match[2], b: +match[3], a: match[4] === undefined ? 1 : +match[4] }
      : null;
  };
  const luminance = ({ r, g, b }) => {
    const channel = (v) => {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
  };
  const effectiveBackground = (el) => {
    const chain = [];
    for (let node = el; node && node !== document.documentElement; node = node.parentElement) chain.push(node);
    let base = { r: 0, g: 0, b: 0 };
    for (const node of chain.reverse()) {
      const bg = parseColor(getComputedStyle(node).backgroundColor);
      if (bg && bg.a > 0) {
        base = {
          r: bg.r * bg.a + base.r * (1 - bg.a),
          g: bg.g * bg.a + base.g * (1 - bg.a),
          b: bg.b * bg.a + base.b * (1 - bg.a),
        };
      }
    }
    return base;
  };
  const effectiveOpacity = (el) => {
    let value = 1;
    for (let node = el; node && node !== document.documentElement; node = node.parentElement) {
      value *= parseFloat(getComputedStyle(node).opacity);
    }
    return value;
  };
  const TEXT_TARGETS =
    ".session-title, .session-card-meta > span:last-child, .rail-section-heading > span, " +
    ".workbench-status-segment span, .composer-access-trigger span, .composer-settings-trigger span, " +
    ".message";
  for (const el of document.querySelectorAll(TEXT_TARGETS)) {
    const text = (el.textContent || "").trim();
    if (!text) continue;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden") continue;
    const rect = box(el);
    // A 0x0 rect is an element an ancestor removed from rendering entirely
    // (display: none up the tree reports the child's own display unchanged).
    // Invisible-TEXT means the element occupies space and shows nothing.
    if (rect.width === 0 && rect.height === 0) continue;
    const fontSize = parseFloat(style.fontSize) || 0;
    const color = parseColor(style.color) || { r: 255, g: 255, b: 255, a: 1 };
    // Between fully hidden and legible sits the ghost band: an ancestor
    //  opacity of 0.3 leaves text that "has contrast" arithmetically but reads
    //  as a watermark. The design dims text with colour tokens, never with
    //  ancestor opacity, so anything below 0.6 is a defect.
    const ghostOpacity = effectiveOpacity(el);
    if (ghostOpacity >= 0.05 && ghostOpacity < 0.6) {
      violations.push({
        rule: "text-ghosted",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `effective opacity ${ghostOpacity.toFixed(2)}`,
      });
      continue;
    }
    if (rect.height < 1 || fontSize < 1 || ghostOpacity < 0.05 || color.a < 0.05) {
      violations.push({
        rule: "invisible-text",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `height ${Math.round(rect.height)}px, font ${fontSize}px, opacity ${effectiveOpacity(el).toFixed(2)}, alpha ${color.a}`,
      });
      continue;
    }
    // 11. Contrast: the text must be readable against what is actually behind
    //     it. A half-transparent ancestor dims the glyphs exactly like a faint
    //     colour would, so effective opacity multiplies into the alpha.
    const bg = effectiveBackground(el);
    const alpha = color.a * effectiveOpacity(el);
    const fg = {
      r: color.r * alpha + bg.r * (1 - alpha),
      g: color.g * alpha + bg.g * (1 - alpha),
      b: color.b * alpha + bg.b * (1 - alpha),
    };
    const l1 = luminance(fg);
    const l2 = luminance(bg);
    const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    if (ratio < 1.6) {
      violations.push({
        rule: "contrast-too-low",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `${ratio.toFixed(2)}:1 against its effective background`,
      });
    }
    // 12. Typography in context: rail text has a ceiling, tracking has a range,
    //     and the family must come from the app's stack.
    if (el.closest(".project-rail") && fontSize > 16.5) {
      violations.push({
        rule: "type-over-context-ceiling",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `${fontSize}px inside the rail (ceiling 16px)`,
      });
    }
    const wordSpacing = parseFloat(style.wordSpacing);
    if (Number.isFinite(wordSpacing) && Math.abs(wordSpacing) > 6) {
      violations.push({
        rule: "word-spacing-out-of-range",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `word-spacing ${wordSpacing}px`,
      });
    }
    const tracking = parseFloat(style.letterSpacing);
    if (Number.isFinite(tracking) && Math.abs(tracking) > 3) {
      violations.push({
        rule: "tracking-out-of-range",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `letter-spacing ${tracking}px`,
      });
    }
    if (
      !/dm sans|inter|ubuntu|atmosphere|system-ui|-apple-system|segoe|sf pro|roboto|helvetica|arial|noto|menlo|monaco|jetbrains|source code|consolas|monospace|sans-serif/i.test(
        style.fontFamily
      )
    ) {
      violations.push({
        rule: "off-family-font",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: style.fontFamily.slice(0, 60),
      });
    }
    // 13. A title should ellipsize, not wrap into a tower of broken words.
    if (el.matches(".session-title") && rect.height > (parseFloat(style.lineHeight) || fontSize * 1.2) * 2.6) {
      violations.push({
        rule: "text-wrapped-excessively",
        el: el.className,
        label: label(el),
        detail: `${Math.round(rect.height)}px tall for a single title`,
      });
    }
  }

  // 14. Major regions must tile, not overlay. A negative margin or a stray
  //     absolute position slides one surface under another; every within-region
  //     check stays green while the shell visibly double-exposes.
  const regions = [".topbar", ".project-rail", ".composer-card", ".workbench-statusbar"]
    .map((selector) => ({ selector, el: document.querySelector(selector) }))
    .filter(({ el }) => el && box(el).width > 0 && box(el).height > 0);
  for (let i = 0; i < regions.length; i += 1) {
    for (let j = i + 1; j < regions.length; j += 1) {
      const a = box(regions[i].el);
      const b = box(regions[j].el);
      const xOverlap = Math.min(a.right, b.right) - Math.max(a.left, b.left);
      const yOverlap = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
      if (xOverlap > 4 && yOverlap > 4) {
        violations.push({
          rule: "regions-overlap",
          el: `${regions[i].selector} × ${regions[j].selector}`,
          label: "",
          detail: `${Math.round(xOverlap)}x${Math.round(yOverlap)}px double-exposed`,
        });
      }
    }
  }

  // 15. Visual order must match document order: a *-reverse or order override
  //     silently rearranges the rail while every content assertion still passes.
  for (const parent of document.querySelectorAll(".rail-scroll, .session-stack")) {
    const kids = [...parent.children].filter((child) => box(child).height > 0);
    const sorted = [...kids].sort((a, b) => box(a).top - box(b).top);
    if (kids.some((child, index) => sorted[index] !== child)) {
      violations.push({
        rule: "visual-order-mismatch",
        el: parent.className,
        label: "",
        detail: "children render in a different order than the document",
      });
    }
    // Siblings in one stack share a left edge.
    const lefts = kids.filter((k) => k.classList.contains("session-row")).map((k) => Math.round(box(k).left));
    if (lefts.length > 1 && Math.max(...lefts) - Math.min(...lefts) > 3) {
      violations.push({
        rule: "misaligned-siblings",
        el: parent.className,
        label: "",
        detail: `left edges span ${Math.min(...lefts)}..${Math.max(...lefts)}px`,
      });
    }
  }

  // 16. The document never scrolls horizontally.
  if (document.documentElement.scrollWidth > document.documentElement.clientWidth + 2) {
    violations.push({
      rule: "document-h-scroll",
      el: "html",
      label: "",
      detail: `scrollWidth ${document.documentElement.scrollWidth} > viewport ${document.documentElement.clientWidth}`,
    });
  }

  // 17. Corner radii come from the token set; an off-token radius is a paste
  //     from somewhere else.
  const RADIUS_TOKENS = new Set([0, 4, 5, 6, 7, 8, 10, 12, 16, 20]);
  for (const el of document.querySelectorAll(".session-row, .session-select, .composer-card")) {
    const radius = Math.round(parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0);
    if (radius < 100 && !RADIUS_TOKENS.has(radius)) {
      violations.push({
        rule: "radius-off-scale",
        el: el.className,
        label: label(el),
        detail: `${radius}px is not a design token`,
      });
    }
  }

  // 18. The composer control row is one line; wrapping stacks the chips under
  //     the send button.
  for (const rowEl of document.querySelectorAll(".composer-controls")) {
    const tops = [...rowEl.querySelectorAll("button, .composer-selects > *")]
      .filter((child) => box(child).height > 0)
      .map((child) => Math.round(box(child).top));
    if (tops.length > 1 && Math.max(...tops) - Math.min(...tops) > 14) {
      violations.push({
        rule: "controls-wrapped",
        el: rowEl.className,
        label: "",
        detail: `control tops span ${Math.max(...tops) - Math.min(...tops)}px`,
      });
    }
  }

  // 19. An open popup must be on top of the stacking order — a menu that exists
  //     but cannot be clicked reads as "the app ignored me".
  for (const popup of document.querySelectorAll('[role="menu"], [role="listbox"], .project-scope-menu, .row-menu')) {
    const rect = box(popup);
    if (rect.width < 2 || rect.height < 2) continue;
    const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    if (hit && !(popup === hit || popup.contains(hit))) {
      violations.push({
        rule: "popup-not-hittable",
        el: popup.className || popup.getAttribute("role"),
        label: label(popup),
        detail: `covered by .${String(hit.className).split(" ")[0] || hit.tagName.toLowerCase()}`,
      });
    }
  }


  // 20. A title crushed to a sliver of its card. `word-break` plus a rogue
  //     width leaves a 30px column of letters beside 200px of empty card; no
  //     overflow fires because the element is exactly as wide as it was told.
  for (const el of document.querySelectorAll(".session-title")) {
    const card = el.closest(".session-select");
    if (!card) continue;
    const cardWidth = box(card).width;
    if (cardWidth < 80) continue;
    if (el.clientWidth > 0 && el.scrollWidth > el.clientWidth * 1.5 && el.clientWidth < cardWidth * 0.45) {
      violations.push({
        rule: "title-crushed",
        el: el.className,
        label: label(el),
        detail: `${el.clientWidth}px of title in a ${Math.round(cardWidth)}px card`,
      });
    }
  }

  // 21. Conversation content painted beyond the viewport. A rogue min-width
  //     does not scroll the document (an ancestor clips), so the messages are
  //     simply unreachable off the right edge.
  const viewWidth = document.documentElement.clientWidth;
  for (const el of document.querySelectorAll(".message, .composer-card")) {
    const rect = box(el);
    if (rect.width === 0) continue;
    if (rect.right > viewWidth + 2 || rect.left < -2) {
      violations.push({
        rule: "content-outside-viewport",
        el: el.className,
        label: label(el),
        detail: `x ${Math.round(rect.left)}..${Math.round(rect.right)} vs viewport ${viewWidth}`,
      });
    }
  }


  // 22. An essential surface collapsed to a sliver. Ripping one region out of
  //     the grid does not overlap anything — the tracks re-solve and the
  //     composer ends up 2px wide. Present-but-unusable is the failure mode.
  //     The redesign moved the composer inside .surface-row, which is the
  //     track that re-solves to a 1px sliver when the rail leaves the grid;
  //     the composer itself collapses to 0 (skipped by the width > 0 guard),
  //     so the row is the collapse target that stays measurable.
  if (document.documentElement.clientWidth >= 700) {
    for (const [selector, minWidth] of [
      [".composer-card", 240],
      [".surface-row", 240],
      [".workbench-statusbar", 240],
      [".topbar", 400],
    ]) {
      const el = document.querySelector(selector);
      if (!el) continue;
      const rect = box(el);
      if (rect.width > 0 && rect.width < minWidth) {
        violations.push({
          rule: "surface-collapsed",
          el: selector,
          label: label(el),
          detail: `${Math.round(rect.width)}px wide (needs ${minWidth}px to be usable)`,
        });
      }
    }
  }


  // 23. The document itself never scrolls in a desktop shell.
  if (document.documentElement.scrollHeight > document.documentElement.clientHeight + 2) {
    violations.push({
      rule: "document-v-scroll",
      el: "html",
      label: "",
      detail: `scrollHeight ${document.documentElement.scrollHeight} > viewport ${document.documentElement.clientHeight}`,
    });
  }

  // 24. A band so short its own scrolling is a joke: two rows of content
  //     behind a sliver you can technically scroll is unusable, not "fine
  //     because reachable".
  for (const stack of document.querySelectorAll(".session-stack, .project-stack")) {
    if (stack.clientHeight > 0 && stack.clientHeight < 44 && stack.scrollHeight > stack.clientHeight * 2) {
      violations.push({
        rule: "band-unusably-short",
        el: stack.className,
        label: stack.closest(".rail-section")?.getAttribute("data-testid") || "",
        detail: `${stack.clientHeight}px window over ${stack.scrollHeight}px of rows`,
      });
    }
  }

  // 25. Blur/ghost filters on text, and transforms on whole regions. This
  //     shell animates nothing structural; a matrix on the rail is a paste.
  for (const el of document.querySelectorAll(TEXT_TARGETS)) {
    for (let node = el; node && node !== document.body; node = node.parentElement) {
      const filter = getComputedStyle(node).filter;
      if (filter && filter !== "none" && /blur\((?!0px)/.test(filter)) {
        violations.push({
          rule: "text-blurred",
          el: el.className || el.tagName.toLowerCase(),
          label: label(el),
          detail: `${filter} on .${String(node.className).split(" ")[0] || node.tagName.toLowerCase()}`,
        });
        break;
      }
    }
  }
  for (const selector of [".topbar", ".project-rail", ".composer-card", ".workbench-statusbar"]) {
    const el = document.querySelector(selector);
    if (!el) continue;
    const transform = getComputedStyle(el).transform;
    if (transform && transform !== "none" && transform !== "matrix(1, 0, 0, 1, 0, 0)") {
      violations.push({
        rule: "region-transformed",
        el: selector,
        label: "",
        detail: transform.slice(0, 60),
      });
    }
  }

  // 26. Interaction that feels broken: a second-long transition on a click
  //     target reads as the app ignoring the click.
  for (const el of document.querySelectorAll(
    ".project-rail button, .composer-controls button, .session-select, .toolbar-button"
  )) {
    const durations = getComputedStyle(el)
      .transitionDuration.split(",")
      .map((value) => parseFloat(value) || 0);
    if (Math.max(...durations) > 0.8) {
      violations.push({
        rule: "sluggish-transition",
        el: el.className,
        label: label(el),
        detail: `transition ${Math.max(...durations)}s`,
      });
    }
  }

  // 27. Overflowing text must ellipsize, not hard-cut mid-glyph.
  for (const el of document.querySelectorAll(TEXT_TARGETS)) {
    const style = getComputedStyle(el);
    if (style.whiteSpace !== "nowrap") continue;
    if (el.scrollWidth > el.clientWidth + 2 && el.clientWidth > 0 && style.textOverflow === "clip" && style.overflowX !== "visible") {
      violations.push({
        rule: "clipped-without-ellipsis",
        el: el.className || el.tagName.toLowerCase(),
        label: label(el),
        detail: `overflows ${el.scrollWidth - el.clientWidth}px with text-overflow: clip`,
      });
    }
  }

  // 28. Every visible control must actually receive its own click. A stray
  //     overlay, a drag-region, or pointer-events: none silently eats input.
  const insideScrollView = (el) => {
    const rect = box(el);
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    if (cx < 0 || cy < 0 || cx > document.documentElement.clientWidth || cy > document.documentElement.clientHeight) return false;
    for (let node = el.parentElement; node && node !== document.body; node = node.parentElement) {
      const style = getComputedStyle(node);
      if (style.overflowY !== "visible" || style.overflowX !== "visible") {
        const bounds = box(node);
        if (cy < bounds.top || cy > bounds.bottom || cx < bounds.left || cx > bounds.right) return false;
      }
    }
    return true;
  };
  for (const el of document.querySelectorAll(
    ".project-rail button, .composer-controls button, .topbar button, .workbench-statusbar button"
  )) {
    const rect = box(el);
    if (rect.width < 4 || rect.height < 4) continue;
    if (el.disabled) continue;
    // Hover-revealed controls are hidden until their row is hovered; a hidden
    // control is not expected to win a hit-test from a cold pointer.
    const controlStyle = getComputedStyle(el);
    if (controlStyle.visibility === "hidden" || parseFloat(controlStyle.opacity) < 0.05) continue;
    if (!insideScrollView(el)) continue;
    const hit = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    // An open menu covering the controls beneath it is the menu working.
    const openPopup = hit
      ? hit.closest('[role="menu"], [role="listbox"], .project-scope-menu, .row-menu')
      : null;
    if (openPopup) continue;
    // An ancestor answering for its child is never innocent: a child paints
    // above its ancestor's background, so the ancestor only wins the hit-test
    // when a pseudo-overlay sits on top or the child lost pointer-events.
    if (hit && !(el === hit || el.contains(hit))) {
      violations.push({
        rule: "control-not-hittable",
        el: el.className,
        label: label(el),
        detail: `click lands on .${String(hit.className).split(" ")[0] || hit.tagName.toLowerCase()}`,
      });
    }
  }

  // 29. Menus are opaque: transcript text shining through a popup makes both
  //     unreadable.
  for (const popup of document.querySelectorAll('[role="menu"], [role="listbox"], .project-scope-menu, .row-menu')) {
    const rect = box(popup);
    if (rect.width < 2 || rect.height < 2) continue;
    const bg = parseColor(getComputedStyle(popup).backgroundColor);
    if (!bg || bg.a < 0.85) {
      violations.push({
        rule: "popup-translucent",
        el: popup.className || popup.getAttribute("role"),
        label: label(popup),
        detail: `background alpha ${bg ? bg.a : 0}`,
      });
    }
  }

  // 30. Horizontal rows render left-to-right in DOM order (rtl/order pastes).
  for (const row of document.querySelectorAll(".composer-controls, .topbar-actions")) {
    const kids = [...row.querySelectorAll(":scope > *")]
      .filter((child) => box(child).width > 0)
      .filter((child) => getComputedStyle(child).position !== "absolute");
    const sorted = [...kids].sort((a, b) => box(a).left - box(b).left);
    if (kids.some((child, index) => sorted[index] !== child)) {
      violations.push({
        rule: "horizontal-order-mismatch",
        el: row.className,
        label: "",
        detail: "row children render in a different order than the document",
      });
    }
  }

  // 31. Border weight: this shell draws hairlines; a fat border is a paste.
  for (const el of document.querySelectorAll(".session-row, .composer-card, .workbench-status-segment, .rail-search")) {
    const width = parseFloat(getComputedStyle(el).borderTopWidth) || 0;
    if (width > 2) {
      violations.push({
        rule: "border-overweight",
        el: el.className,
        label: label(el),
        detail: `${width}px border`,
      });
    }
  }

  // 32. The composer placeholder must be legible: it is the empty state's only
  //     guidance.
  const composerInput = document.querySelector(".composer-card textarea");
  if (composerInput) {
    const ph = getComputedStyle(composerInput, "::placeholder");
    const phColor = parseColor(ph.color);
    if (phColor) {
      const bg = effectiveBackground(composerInput);
      const alpha = phColor.a;
      const fg = {
        r: phColor.r * alpha + bg.r * (1 - alpha),
        g: phColor.g * alpha + bg.g * (1 - alpha),
        b: phColor.b * alpha + bg.b * (1 - alpha),
      };
      const l1 = luminance(fg);
      const l2 = luminance(bg);
      const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
      if (ratio < 1.5) {
        violations.push({
          rule: "placeholder-invisible",
          el: "textarea::placeholder",
          label: "",
          detail: `${ratio.toFixed(2)}:1 against the composer background`,
        });
      }
    }
  }

  return violations;
}

async function auditViewport(browser, viewport, injectCss) {
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
  });
  const page = await context.newPage();
  const consoleErrors = [];
  page.on("pageerror", (error) => consoleErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  try {
    await page.goto(URL, { waitUntil: "domcontentloaded" });
    // A project-assigned session is the densest rail card and lives in
    // localStorage, not the transport, so a cleared profile renders a rail the
    // user never actually has. Seed one before measuring; long-data states seed
    // sixty chats across eight projects so volume-only defects can appear.
    await page.evaluate((longData) => {
      localStorage.clear();
      const projects = longData
        ? Array.from({ length: 8 }, (_, index) => ({
            id: `project-${index}`,
            name: index === 0 ? "Optimus Agent" : `Project ${index} with a name`,
            rootPaths: [`/projects/p${index}`],
          }))
        : [{ id: "optimus-agent", name: "Optimus Agent", rootPaths: ["/projects/optimus-agent"] }];
      localStorage.setItem("optimus.ui.projects", JSON.stringify({ projects }));
      const assignments = longData
        ? Object.fromEntries(
            Array.from({ length: 30 }, (_, index) => [
              `fixture-bulk-${index * 2}`,
              `project-${index % 8}`,
            ])
          )
        : { "fixture-assess": "optimus-agent" };
      if (longData) assignments["fixture-assess"] = "project-0";
      localStorage.setItem("optimus.ui.sessionProjects", JSON.stringify(assignments));
      localStorage.setItem(
        "optimus.ui.projectExpanded",
        JSON.stringify(Object.fromEntries(projects.map((project) => [project.id, true])))
      );
      if (longData) localStorage.setItem("optimus.fixture.bulkSessions", "60");
    }, Boolean(viewport.longData));
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForTimeout(700);

    if (viewport.theme || viewport.density) {
      await page.evaluate(({ theme, density }) => {
        if (theme) document.documentElement.dataset.theme = theme;
        if (density) document.documentElement.dataset.density = density;
      }, { theme: viewport.theme || null, density: viewport.density || null });
      await page.waitForTimeout(250);
    }

    if (viewport.collapseRail) {
      await page
        .getByRole("button", { name: "Close project rail", exact: true })
        .click()
        .catch(() => undefined);
      await page.waitForTimeout(300);
    }

    if (viewport.workspace) {
      const toggle = page.getByRole("button", { name: "Workspace", exact: true });
      if ((await toggle.count()) && (await toggle.getAttribute("aria-pressed")) !== "true") {
        await toggle.click();
        await page.waitForTimeout(500);
      }
    }

    if (viewport.openScopeMenu) {
      await page.getByRole("button", { name: "All projects" }).click().catch(() => undefined);
      await page.waitForTimeout(300);
    }

    if (injectCss) {
      await page.addStyleTag({ content: injectCss });
      await page.waitForTimeout(250);
    }

    const violations = await page.evaluate(collect);

    // Keyboard users need a visible focus ring. Tab to the first control and
    // measure what focus *adds* to its paint.
    //
    // Three races broke earlier versions of this probe, all observed in real
    // runs:
    //
    // 1. Tab sent before React finished mounting: focus stayed on <body> and
    //    the probe silently reported nothing. Focusability is established
    //    first now, and a control that never becomes focusable is itself a
    //    violation.
    // 2. The ring paints through a 90ms box-shadow transition; an immediate
    //    read caught the pre-transition value. Both readings settle here:
    //    poll until two consecutive reads agree.
    // 3. The probe compared focused and blurred paint for identity. The
    //    ring-strip defect injects `box-shadow: none !important` on
    //    :focus-visible, which on a control carrying a resting shadow
    //    *removes* paint on focus — focused differs from blurred, and the
    //    probe concluded a ring existed. The question is now whether focus
    //    adds a ring (an outline or box-shadow absent at rest), which the
    //    strip can never fake.
    const focusable = page.locator("button, a[href], input, select, textarea, [tabindex]").first();
    await focusable.waitFor({ timeout: 10_000 });
    let focusMeasurement = null;
    for (let attempt = 0; attempt < 3 && !focusMeasurement; attempt += 1) {
      let focusEstablished = false;
      for (let press = 0; press < 5 && !focusEstablished; press += 1) {
        await page.keyboard.press("Tab");
        focusEstablished = await page.evaluate(() => {
          const el = document.activeElement;
          return Boolean(el && el !== document.body);
        });
        if (!focusEstablished) await page.waitForTimeout(120);
      }
      if (!focusEstablished) break;
      focusMeasurement = await page.evaluate(async () => {
        const el = document.activeElement;
        if (!el || el === document.body) return null;
        const ringOf = (node) => {
          const style = getComputedStyle(node);
          return {
            outlineStyle: style.outlineStyle,
            outlineWidth: style.outlineWidth,
            shadow: style.boxShadow,
          };
        };
        const same = (a, b) =>
          a.outlineStyle === b.outlineStyle &&
          a.outlineWidth === b.outlineWidth &&
          a.shadow === b.shadow;
        // Two consecutive identical reads: the transition has settled.
        const settle = async (node) => {
          let previous = ringOf(node);
          for (let i = 0; i < 8; i += 1) {
            await new Promise((resolve) => setTimeout(resolve, 60));
            const current = ringOf(node);
            if (same(current, previous)) return current;
            previous = current;
          }
          return previous;
        };
        const focused = await settle(el);
        if (document.activeElement !== el) return null; // focus moved mid-measure
        el.blur();
        const resting = await settle(el);
        return {
          el: el.className || el.tagName.toLowerCase(),
          label: (el.getAttribute("aria-label") || el.textContent || "").trim().slice(0, 60),
          focused,
          resting,
        };
      });
      if (!focusMeasurement) await page.waitForTimeout(120);
    }
    if (!focusMeasurement) {
      violations.push({
        rule: "focus-invisible",
        el: "document",
        label: "",
        detail: "Tab never moved focus to a control: the keyboard path is unusable",
      });
    } else {
      const { focused, resting } = focusMeasurement;
      const outlineAdded =
        focused.outlineStyle !== "none" && focused.outlineStyle !== resting.outlineStyle;
      const shadowAdded = focused.shadow !== "none" && focused.shadow !== resting.shadow;
      if (!outlineAdded && !shadowAdded) {
        violations.push({
          rule: "focus-invisible",
          el: focusMeasurement.el,
          label: focusMeasurement.label,
          detail: `focus adds no ring (focused shadow: ${focused.shadow}; resting: ${resting.shadow})`,
        });
      }
    }

    return { violations, consoleErrors };
  } finally {
    await context.close();
  }
}


/** What to do about each rule, printed beside every violation. */
const RULE_FIXES = {
  "clipped-text": "Let the container grow (height: auto with a min-height) or give the clipping ancestor overflow-y: auto.",
  "content-overflows-container": "Raise the container's min-height, or set flex-shrink: 0 on the rows and height: auto on the container.",
  "text-box-shorter-than-line": "Remove the fixed height pinning this text; use min-height and flex-shrink: 0 so the box fits its line.",
  "duplicate-session": "Filter the session out of one band (Recent must exclude pinned and project-assigned chats).",
  "squeezed-label": "Set flex: 0 0 auto on the segment and shed lower-priority segments with a container query instead of shrinking all.",
  "dead-space": "Use max-height as a ceiling instead of flex-grow so the band hugs its content.",
  "overlapping-siblings": "Remove the negative margin or stray absolute offset; let the stack flow.",
  "outside-viewport": "Check margins/position on this region; it must stay inside the window.",
  "tiny-hit-target": "Give the control at least 18x18px via min-width/min-height or padding.",
  "type-too-small": "Raise the size to at least 10px or drop the label entirely.",
  "type-off-scale": "Use a size from the type scale (10/11/12/13/14/15/16/18/20/24/28/32).",
  "leading-too-tight": "Set line-height to at least 1.15x the font size.",
  "asymmetric-padding": "Match the horizontal padding, or name the offset as a deliberate token.",
  "uneven-sibling-rows": "Sibling rows share a min-height; remove the one-off height override.",
  "invisible-text": "The text renders as nothing — restore its height, font-size, opacity, or colour alpha.",
  "contrast-too-low": "Use a token with contrast (--text, --text-2, --muted) instead of a colour near the background.",
  "regions-overlap": "Regions must tile, not overlay: remove the negative margin or absolute position pulling one over the other.",
  "visual-order-mismatch": "Visual order must match DOM order: remove *-reverse or CSS order overrides.",
  "misaligned-siblings": "Rows in one stack share a left edge; remove the stray margin or indent.",
  "document-h-scroll": "Something forces a min-width past the viewport: use minmax(0, 1fr) and min-width: 0 in the grid.",
  "type-over-context-ceiling": "Rail text caps at 16px; larger sizes belong on the work surface.",
  "tracking-out-of-range": "Keep letter-spacing within +/-3px; the design tokens use hundredths of an em.",
  "off-family-font": "Use the app font stack (DM Sans / system-ui / the mono stack).",
  "controls-wrapped": "Keep the control row on one line: flex-wrap: nowrap and a min-width on the composer card.",
  "text-wrapped-excessively": "Titles ellipsize, they do not wrap: white-space: nowrap + text-overflow: ellipsis.",
  "popup-not-hittable": "Raise the popup's z-index above the surface that is covering it.",
  "title-crushed": "Remove the width cap on the title; let it fill the card and ellipsize.",
  "content-outside-viewport": "A rogue width pushes content off-screen: drop the min-width or make the pane scroll.",
  "surface-collapsed": "A region left the layout flow and the grid re-solved without it: restore its grid placement.",
  "document-v-scroll": "The shell must own scrolling: something pushed the body taller than the viewport.",
  "band-unusably-short": "Give the band a workable min-height or let it flex; a 40px scroll window is not a UI.",
  "text-blurred": "Remove the blur filter from the text's ancestor chain.",
  "region-transformed": "Remove the transform; shell regions are never skewed or scaled.",
  "sluggish-transition": "Keep interactive transitions under 200ms.",
  "clipped-without-ellipsis": "Add text-overflow: ellipsis wherever nowrap text can overflow.",
  "control-not-hittable": "Something overlays this control: check z-index, pointer-events, and fixed overlays.",
  "popup-translucent": "Menus use the solid --elevated surface; raise the background alpha to 1.",
  "horizontal-order-mismatch": "Remove row-reverse/order overrides; rows read left to right in DOM order.",
  "border-overweight": "This shell draws hairlines: keep borders at 1px.",
  "placeholder-invisible": "Raise the placeholder colour to a legible token (--muted).",
  "focus-invisible": "Restore the :focus-visible ring (box-shadow tokens); keyboard users cannot see focus.",
  "word-spacing-out-of-range": "Keep word-spacing within +/-6px; reset the stray override.",
  "text-ghosted": "Text is dimmed by ancestor opacity: dim with colour tokens (--muted/--faint), never with opacity.",
  "console-error": "Open the devtools trace for this page state; a rendering error usually explains the visual defect.",
};

async function reachable(url) {
  try {
    const response = await fetch(url, { method: "GET" });
    return response.ok;
  } catch {
    return false;
  }
}

/** Serve the built UI when nothing is already listening, so the gate stands alone. */
async function ensureServer() {
  if (await reachable(URL)) return null;
  const { spawn } = require("node:child_process");
  const uiDir = path.join(__dirname, "../../apps/optimus-ui");
  // `URL` here is this module's target-address constant, which shadows the
  // global constructor, so read the port off the string.
  const port = (URL.match(/:(\d+)/) || [])[1] || "4174";
  const child = spawn(
    "bunx",
    ["vite", "preview", "--host", "127.0.0.1", "--port", port, "--strictPort"],
    { cwd: uiDir, stdio: "ignore", detached: false }
  );
  for (let attempt = 0; attempt < 60; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    if (await reachable(URL)) return child;
  }
  child.kill();
  throw new Error(`could not serve the built UI at ${URL} (run: bun run --cwd apps/optimus-ui build)`);
}

/**
 * Every defect this audit has ever caught, as the CSS that caused it.
 *
 * A checker that only ever passes is unfalsifiable — it cannot tell "the shell
 * is correct" from "my rule stopped working". The first version of this file
 * looked healthy while three shipped defects walked straight past it, because
 * the rules measured the wrong property. So each rule owns a reproduction here,
 * and `--self-test` re-breaks the shell and demands the rule fire. Weakening a
 * rule now fails the gate; it cannot rot quietly.
 *
 * Add an entry whenever you add a rule. Never delete one.
 */
const DEFECTS = [
  {
    name: "fixed-height-card-slices-its-own-text",
    // The shipped bug: the densest rail card had a fixed height, so its flex
    // children were compressed and the title rendered inside a 9px box.
    // Project-folder rows are single-line since the rail dedup (folder rows
    // no longer repeat worktree/state), so the forced height must undershoot
    // the line directly to recreate the same clipping.
    css: `.project-sessions .session-row .session-select { height: 9px !important; min-height: 0 !important; }
          .session-select > * { flex: 0 0 auto !important; height: 8px !important; min-height: 0 !important; }`,
    expect: ["text-box-shorter-than-line", "content-overflows-container"],
  },
  {
    name: "rail-band-forced-to-a-third-leaves-dead-space",
    css: `.rail-scroll > .rail-section { flex: 1 1 0 !important; max-height: none !important; }`,
    // Dead space is proportional to rail height, so reproduce it on a tall one.
    viewport: { name: "self-test:tall", width: 1600, height: 1000, workspace: false },
    expect: ["dead-space"],
  },
  {
    name: "status-segments-squeezed-to-ellipses",
    css: `.workbench-status-segment { flex: 0 1 auto !important; min-width: 0 !important; }
          .workbench-statusbar { container-type: normal !important; width: 420px !important; }`,
    // The bar only runs out of room once the evidence workspace takes width
    // (or, post-redesign, when the app-wide footer is forced narrow).
    viewport: { name: "self-test:narrow-bar", width: 1280, height: 833, workspace: true },
    expect: ["squeezed-label"],
  },
  {
    name: "text-cut-by-an-ancestor",
    css: `.rail-scroll > .rail-section { max-height: 40px !important; overflow: hidden !important; }`,
    expect: ["clipped-text", "band-unusably-short"],
  },
  {
    name: "rows-overlap-each-other",
    css: `.session-stack > .session-row { margin-bottom: -24px !important; }`,
    expect: ["overlapping-siblings"],
  },
  {
    name: "type-below-the-scale",
    css: `.session-title { font-size: 7px !important; }`,
    expect: ["type-too-small", "type-off-scale"],
  },
  {
    name: "leading-collapsed-onto-the-glyphs",
    css: `.workbench-statusbar { line-height: 1 !important; }`,
    expect: ["leading-too-tight"],
  },
  {
    name: "lopsided-container-padding",
    css: `.rail-section-heading { padding-left: 2px !important; padding-right: 40px !important; }`,
    expect: ["asymmetric-padding"],
  },
  {
    name: "uneven-sibling-row-heights",
    css: `.session-stack > .session-row:nth-child(2) .session-select { min-height: 96px !important; }`,
    expect: ["uneven-sibling-rows"],
  },
  {
    name: "control-too-small-to-hit",
    css: `.rail-section-heading button { width: 10px !important; height: 10px !important; min-width: 0 !important; }`,
    expect: ["tiny-hit-target"],
  },
  {
    name: "text-rendered-invisible",
    css: `.session-title { opacity: 0 !important; }`,
    expect: ["invisible-text"],
  },
  {
    name: "text-without-contrast",
    css: `.session-title { color: #0f0f0f !important; }`,
    expect: ["contrast-too-low"],
  },
  {
    name: "statusbar-pulled-over-the-composer",
    css: `.workbench-statusbar { margin-top: -40px !important; }`,
    expect: ["regions-overlap"],
  },
  {
    name: "rail-sections-visually-reversed",
    css: `.rail-scroll { flex-direction: column-reverse !important; }`,
    expect: ["visual-order-mismatch"],
  },
  {
    name: "one-row-indented-out-of-line",
    css: `.session-stack > .session-row:nth-child(2) { margin-left: 40px !important; }`,
    expect: ["misaligned-siblings"],
  },
  {
    name: "display-type-inside-the-rail",
    css: `.session-title { font-size: 28px !important; }`,
    expect: ["type-over-context-ceiling"],
  },
  {
    name: "letter-spacing-blown-out",
    css: `.rail-section-heading { letter-spacing: 8px !important; }`,
    expect: ["tracking-out-of-range"],
  },
  {
    name: "foreign-font-pasted-in",
    css: `.session-title { font-family: "Comic Sans MS", cursive !important; }`,
    expect: ["off-family-font"],
  },
  {
    name: "title-crushed-to-a-sliver",
    css: `.session-title { width: 30px !important; }`,
    expect: ["title-crushed"],
  },
  {
    name: "conversation-pushed-off-screen",
    css: `.workbench-conversation-row { min-width: 2400px !important; }`,
    expect: ["content-outside-viewport"],
  },
  {
    name: "region-ripped-from-the-grid",
    css: `.project-rail { position: absolute !important; width: 800px !important; z-index: 99 !important; }`,
    expect: ["surface-collapsed"],
  },
  {
    name: "open-menu-buried-in-the-stack",
    css: `.project-scope-menu { z-index: -1 !important; }`,
    viewport: { name: "self-test:menu", width: 1280, height: 833, workspace: false, openScopeMenu: true },
    expect: ["popup-not-hittable"],
  },
  {
    name: "rail-window-shrunk-to-a-slit",
    css: `.rail-scroll > .rail-section { max-height: 40px !important; }`,
    expect: ["band-unusably-short", "clipped-text"],
  },
  {
    name: "document-grows-a-scrollbar",
    // The shell pins html/body; a real regression breaks that pin too.
    css: `html, body { overflow: auto !important; height: auto !important; } body { padding-bottom: 400px !important; }`,
    expect: ["document-v-scroll"],
  },
  {
    name: "text-behind-a-blur",
    css: `.project-rail { filter: blur(2px) !important; }`,
    expect: ["text-blurred"],
  },
  {
    name: "region-skewed-by-a-transform",
    css: `.project-rail { transform: rotate(1.5deg) !important; }`,
    expect: ["region-transformed"],
  },
  {
    name: "click-feels-ignored",
    css: `.session-select { transition: all 2s ease !important; }`,
    expect: ["sluggish-transition"],
  },
  {
    name: "overflow-cut-without-ellipsis",
    css: `.session-title { text-overflow: clip !important; }`,
    viewport: { name: "self-test:narrow-title", width: 900, height: 700, workspace: false },
    expect: ["clipped-without-ellipsis"],
  },
  {
    name: "invisible-overlay-eats-clicks",
    css: `.rail-primary::after { content: "" !important; position: fixed !important; inset: 0 !important; z-index: 999 !important; }`,
    expect: ["control-not-hittable"],
  },
  {
    name: "menu-goes-translucent",
    // The app itself defends menu opacity with a layered !important, which
    // outranks unlayered injection — so the reproduction speaks from the
    // earlier `theme` layer, which outranks it right back.
    css: `@layer theme { .project-scope-menu, [role="menu"] { background: rgba(12, 12, 12, 0.3) !important; } }`,
    viewport: { name: "self-test:menu2", width: 1280, height: 833, workspace: false, openScopeMenu: true },
    expect: ["popup-translucent"],
  },
  {
    name: "control-row-runs-right-to-left",
    css: `.composer-controls { flex-direction: row-reverse !important; }`,
    expect: ["horizontal-order-mismatch"],
  },
  {
    name: "fat-border-pasted-in",
    css: `.session-row { border: 5px solid red !important; }`,
    expect: ["border-overweight"],
  },
  {
    name: "placeholder-fades-to-nothing",
    css: `.composer-card textarea::placeholder { color: rgba(10, 10, 10, 0.9) !important; }`,
    expect: ["placeholder-invisible"],
  },
  {
    name: "ghost-opacity-dims-the-rail",
    css: `.project-rail { opacity: 0.25 !important; }`,
    expect: ["text-ghosted", "contrast-too-low"],
  },
  {
    name: "transcript-ghosted-to-a-quarter",
    css: `.workbench-conversation-row { opacity: 0.3 !important; }`,
    expect: ["text-ghosted"],
  },
  {
    name: "full-screen-overlay-eats-every-click",
    css: `.optimus-app::after { content: "" !important; position: fixed !important; inset: 0 !important; z-index: 9999 !important; }`,
    expect: ["control-not-hittable"],
  },
  {
    name: "buttons-lose-pointer-events",
    css: `.rail-section-heading button { pointer-events: none !important; }`,
    expect: ["control-not-hittable"],
  },
  {
    name: "focus-ring-stripped-globally",
    css: `@layer theme { *:focus-visible { outline: none !important; box-shadow: none !important; } }`,
    expect: ["focus-invisible"],
  },
  {
    name: "words-drift-apart",
    css: `.rail-section-heading { word-spacing: 30px !important; }`,
    expect: ["word-spacing-out-of-range"],
  },
];

async function selfTest(browser) {
  // One representative viewport is enough: this proves the rule fires, not that
  // the shell is correct at every size.
  const fallback = { name: "self-test", width: 1280, height: 833, workspace: false };
  const broken = [];
  for (const defect of DEFECTS) {
    const { violations } = await auditViewport(browser, defect.viewport || fallback, defect.css);
    const rules = new Set(violations.map((violation) => violation.rule));
    const caught = defect.expect.filter((rule) => rules.has(rule));
    const ok = caught.length > 0;
    console.log(
      JSON.stringify({
        event: "self-test",
        defect: defect.name,
        expected: defect.expect,
        caught,
        ok,
      })
    );
    if (!ok) broken.push(defect);
  }
  if (broken.length) {
    console.error("UI_LAYOUT_AUDIT_SELFTEST_FAIL");
    for (const defect of broken) {
      console.error(
        `  ${defect.name}: expected one of [${defect.expect.join(", ")}] and the audit stayed silent`
      );
    }
    return false;
  }
  console.log(`UI_LAYOUT_AUDIT_SELFTEST_OK defects=${DEFECTS.length}`);
  return true;
}

async function main() {
  const server = await ensureServer();
  const browser = await chromium.launch({ headless: true });
  const failures = [];
  try {
    if (!(await selfTest(browser))) {
      process.exitCode = 1;
      return;
    }
    for (const viewport of VIEWPORTS) {
      const { violations, consoleErrors } = await auditViewport(browser, viewport);
      for (const violation of violations) {
        failures.push({ viewport: viewport.name, ...violation });
      }
      console.log(
        JSON.stringify({
          event: "viewport",
          name: viewport.name,
          size: `${viewport.width}x${viewport.height}`,
          workspace: Boolean(viewport.workspace),
          violations: violations.length,
          consoleErrors: consoleErrors.length,
        })
      );
      for (const error of consoleErrors) {
        failures.push({
          viewport: viewport.name,
          rule: "console-error",
          el: "",
          label: "",
          detail: error.slice(0, 160),
        });
      }
    }
  } finally {
    await browser.close();
    if (server) server.kill();
  }

  if (failures.length) {
    console.error("UI_LAYOUT_AUDIT_FAIL");
    for (const failure of failures) {
      console.error(
        `  ${failure.viewport}: ${failure.rule}: ${failure.label || failure.el} — ${failure.detail}`
      );
      const fix = RULE_FIXES[failure.rule];
      if (fix) console.error(`      fix: ${fix}`);
    }
    process.exitCode = 1;
    return;
  }
  console.log(`UI_LAYOUT_AUDIT_OK viewports=${VIEWPORTS.length}`);
}

main().catch((error) => {
  console.error(`UI_LAYOUT_AUDIT_FAIL fatal=${error.stack}`);
  process.exitCode = 1;
});
