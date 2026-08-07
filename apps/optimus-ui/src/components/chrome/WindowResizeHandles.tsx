import type { PointerEvent as ReactPointerEvent } from 'react';
import { isPackaged, startResize, type ResizeDirection } from '../../ipc/windowBridge';

/**
 * Native resize hotspots (spec-001 R5 chrome): a borderless window has
 * no WM-provided resize edges on every desktop (KDE/XWayland
 * undecorated windows in particular), so the renderer draws thin
 * edge/corner hotspots and hands the pointer drag to the compositor via
 * `window_resize_start`. Renders nothing outside the packaged webview —
 * in a dev browser there is no window to resize, and honest capability
 * boundaries (spec-001 fixture contract) mean no inert chrome either.
 */

const DIRECTIONS: ResizeDirection[] = [
  'north',
  'south',
  'east',
  'west',
  'northEast',
  'northWest',
  'southEast',
  'southWest',
];

const CURSORS: Record<ResizeDirection, string> = {
  north: 'ns-resize',
  south: 'ns-resize',
  east: 'ew-resize',
  west: 'ew-resize',
  northEast: 'nesw-resize',
  northWest: 'nwse-resize',
  southEast: 'nwse-resize',
  southWest: 'nesw-resize',
};

export function WindowResizeHandles() {
  if (!isPackaged()) return null;
  const begin = (event: ReactPointerEvent<HTMLDivElement>, direction: ResizeDirection) => {
    // The hotspot sits above the topbar drag region and the app content;
    // the drag must not start, and the browser must not treat the press
    // as a scroll/selection gesture (touch-action handles touch).
    event.preventDefault();
    event.stopPropagation();
    void startResize(direction);
  };
  return (
    <div className="window-resize-handles" aria-hidden="true">
      {DIRECTIONS.map((direction) => (
        <div
          key={direction}
          className={`resize-handle resize-handle-${direction}`}
          data-resize-handle={direction}
          style={{ cursor: CURSORS[direction] }}
          onPointerDown={(event) => begin(event, direction)}
        />
      ))}
    </div>
  );
}
