import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';

afterEach(cleanup);

class TestResizeObserver implements ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

Object.defineProperty(globalThis, 'ResizeObserver', {
  value: TestResizeObserver,
  configurable: true,
});

// jsdom implements the layout-free half of the DOM, so `scrollIntoView` exists
// nowhere. cmdk calls it every time the active command changes, which turns a
// keyboard-navigation test into a TypeError inside a layout effect. Scrolling
// is not the behaviour under test; being able to reach the third item is.
// Guarded: this setup file also runs for the node-environment suites, where
// there is no DOM at all.
if (typeof Element !== 'undefined' && !Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = function scrollIntoView() {};
}

if (!globalThis.requestAnimationFrame) {
  globalThis.requestAnimationFrame = (callback) =>
    window.setTimeout(() => callback(performance.now()), 0);
  globalThis.cancelAnimationFrame = (id) => window.clearTimeout(id);
}
