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
