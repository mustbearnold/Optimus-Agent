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

// Node >= 26 defines a configurable global `localStorage` accessor that
// evaluates to undefined unless the process was started with
// --localstorage-file, and its presence makes the test environment skip
// installing jsdom's real Storage — so neither `localStorage` nor
// `window.localStorage` works. An in-memory Storage keeps the suite
// independent of the host Node version and of that flag, whose unquoted-path
// workaround once wrote a stray storage file outside the workspace.
// Guarded: node-environment suites have no window.
class TestStorage implements Storage {
  private store = new Map<string, string>();
  get length() {
    return this.store.size;
  }
  clear() {
    this.store.clear();
  }
  getItem(key: string) {
    return this.store.has(key) ? this.store.get(key)! : null;
  }
  key(index: number) {
    return [...this.store.keys()][index] ?? null;
  }
  removeItem(key: string) {
    this.store.delete(key);
  }
  setItem(key: string, value: string) {
    this.store.set(key, String(value));
  }
}

if (typeof window !== 'undefined') {
  for (const storage of ['localStorage', 'sessionStorage'] as const) {
    if (!globalThis[storage]) {
      Object.defineProperty(globalThis, storage, {
        value: new TestStorage(),
        configurable: true,
      });
    }
  }
}
