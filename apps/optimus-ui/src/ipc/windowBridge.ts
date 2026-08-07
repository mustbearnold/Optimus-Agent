import { invoke } from '@tauri-apps/api/core';

/**
 * Window-chrome bridge (spec-001 R5): OS window affordances are owned by
 * the Tauri shell, NOT the surface protocol. The renderer may run inside
 * the packaged webview while its surface transport is the WS carrier
 * (spec-015 A3) — so window controls must reach the shell through the
 * Tauri bridge directly, independent of which transport the workbench
 * chose. Outside the packaged webview (dev browser, fixtures) every
 * affordance degrades to a resolved no-op — the same graceful
 * degradation the wire transports always had.
 */

export type WindowAction = 'minimize' | 'maximize' | 'close';

export type ResizeDirection =
  | 'north'
  | 'south'
  | 'east'
  | 'west'
  | 'northEast'
  | 'northWest'
  | 'southEast'
  | 'southWest';

/**
 * The packaged-vs-dev discriminator (spec-001 R8's existing predicate,
 * pinned by spec-015 A3): the Tauri bridge exists ONLY in the packaged
 * webview. Dev-mode tests that fake it must set `__TAURI_INTERNALS__`.
 */
export function isPackaged(): boolean {
  return Boolean(window.__TAURI_INTERNALS__ || window.__TAURI__);
}

/** Route a chrome-button action to the shell (`window_action`). */
export function windowAction(action: WindowAction): Promise<unknown> {
  if (!isPackaged()) return Promise.resolve({ ok: false });
  return invoke('window_action', { action }).catch(() => ({ ok: false }));
}

/**
 * Hand the pointer drag for an edge/corner hotspot over to the native
 * compositor (`window_resize_start`). Outside the packaged webview the
 * hotspot is inert.
 */
export function startResize(direction: ResizeDirection): Promise<unknown> {
  if (!isPackaged()) return Promise.resolve({ ok: false });
  return invoke('window_resize_start', { direction }).catch(() => ({ ok: false }));
}
