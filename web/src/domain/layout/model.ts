export type Layout = { x: number; y: number; scale: number; visible: boolean; locked: boolean };
export const clamp = (n: number, min: number, max: number) => Math.min(max, Math.max(min, n));
export function layout(x = 0.04, y = 0.18): Layout {
  return { x, y, scale: 1, visible: true, locked: false };
}
// x/y are normalized fractions of the available top-left travel, not the viewport.
export function position(
  l: Layout,
  viewport: { width: number; height: number },
  size: { width: number; height: number },
) {
  return {
    left: clamp(l.x, 0, 1) * Math.max(0, viewport.width - size.width * l.scale),
    top: clamp(l.y, 0, 1) * Math.max(0, viewport.height - size.height * l.scale),
  };
}
export function move(l: Layout, dx: number, dy: number, width: number, height: number): Layout {
  return {
    ...l,
    x: clamp(l.x + dx / Math.max(1, width), 0, 1),
    y: clamp(l.y + dy / Math.max(1, height), 0, 1),
  };
}

/** The layout fractions that put a frame's top-left corner at `left`/`top`
 * (px from the usable area's origin) for its current size: a frame resized
 * from its bottom-right corner keeps its top-left where it was. */
export function anchorAt(
  l: Layout,
  left: number,
  top: number,
  viewport: { width: number; height: number },
  size: { width: number; height: number },
): Layout {
  const travelX = viewport.width - size.width * l.scale;
  const travelY = viewport.height - size.height * l.scale;
  return {
    ...l,
    x: travelX > 0 ? clamp(left / travelX, 0, 1) : 0,
    y: travelY > 0 ? clamp(top / travelY, 0, 1) : 0,
  };
}

/** Where an attached frame (the chat box) goes beside the launcher: flush
 * to its right with their bottoms aligned, and as wide as the space up to
 * the action bar (`width`, unscaled; `null` when no bar is level with it,
 * so the saved width applies). `null` when even `minWidth` does not fit:
 * the frame then keeps its own place. Coordinates are usable-area px (the
 * viewport minus 12 px each side), as `position` and `anchorAt` use. */
export function besideLauncher(
  launcher: { right: number; bottom: number },
  barLeft: number | null,
  height: number,
  scale: number,
  viewportWidth: number,
  minWidth: number,
  gap = 8,
) {
  const left = launcher.right + gap;
  const right = barLeft !== null ? barLeft - gap : viewportWidth - 12;
  if (right - left < minWidth * scale) return null;
  return {
    left: left - 12,
    top: launcher.bottom - height * scale - 12,
    width: barLeft !== null ? (right - left) / scale : null,
  };
}
