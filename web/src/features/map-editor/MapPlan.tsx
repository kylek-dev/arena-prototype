import { useRef, type PointerEvent as ReactPointerEvent } from "react";
import {
  deepenRoom,
  edgePoint,
  movePillar,
  moveRoom,
  moveSpawn,
  pillarRect,
  planBounds,
  resizePillar,
  roomRect,
  SNAP,
  spawnPoint,
  wallInner,
  type ArenaMap,
  type Point,
  type Rect,
  type Selection,
} from "../../domain/map";
import { commitMap, mapPreview, mapSelection } from "../../state/map-editor";

type Grab =
  | { kind: "pillar"; index: number; mode: "move" | "resize" }
  | { kind: "room"; index: number; mode: "move" | "depth" }
  | { kind: "spawn"; room: number; index: number };

/** Body radius (m) the rules use for spawn spacing. */
const BODY = 0.45;
const GRID = 6;
/** Pointer travel (px) before a press becomes a drag. */
const DRAG_SLOP = 3;

/** Screen point to plan metres for an SVG drawn with `xMidYMid meet`. */
export function toPlan(
  svg: Element,
  client: { clientX: number; clientY: number },
  box: Rect,
): Point {
  const r = svg.getBoundingClientRect();
  const w = box.maxX - box.minX;
  const h = box.maxZ - box.minZ;
  const scale = Math.min(r.width / w, r.height / h) || 1;
  const left = r.left + (r.width - w * scale) / 2;
  const top = r.top + (r.height - h * scale) / 2;
  return {
    x: box.minX + (client.clientX - left) / scale,
    z: box.minZ + (client.clientY - top) / scale,
  };
}

const grabPoint = (map: ArenaMap, grab: Grab): Point => {
  if (grab.kind === "pillar") return map.pillars[grab.index];
  if (grab.kind === "room") {
    const room = map.rooms[grab.index];
    return edgePoint(map, room.edge, room.offset, 0);
  }
  const room = map.rooms[grab.room];
  return spawnPoint(map, room, room.spawns[grab.index]);
};

function applyGrab(base: ArenaMap, grab: Grab, at: Point, offset: Point, step: number): ArenaMap {
  const moved = { x: at.x + offset.x, z: at.z + offset.z };
  if (grab.kind === "pillar")
    return grab.mode === "move"
      ? movePillar(base, grab.index, moved, step)
      : resizePillar(base, grab.index, at, step);
  if (grab.kind === "room")
    return grab.mode === "move"
      ? moveRoom(base, grab.index, moved, step)
      : deepenRoom(base, grab.index, at, step);
  return moveSpawn(base, grab.room, grab.index, moved, step);
}

const selectionOf = (grab: Grab): Selection =>
  grab.kind === "spawn"
    ? { kind: "spawn", room: grab.room, index: grab.index }
    : { kind: grab.kind, index: grab.index };

const same = (a: Selection, b: Selection) => JSON.stringify(a) === JSON.stringify(b);

function rectProps(r: Rect) {
  return { x: r.minX, y: r.minZ, width: r.maxX - r.minX, height: r.maxZ - r.minZ };
}
function grow(r: Rect, by: number): Rect {
  return { minX: r.minX - by, minZ: r.minZ - by, maxX: r.maxX + by, maxZ: r.maxZ + by };
}

export function MapPlan({
  map,
  preview,
  selection,
}: {
  map: ArenaMap;
  preview: ArenaMap | null;
  selection: Selection;
}) {
  const shown = preview ?? map;
  // Bounds follow the committed map so the view stays still while dragging.
  const box = planBounds(map);
  const svg = useRef<SVGSVGElement>(null);
  const drag = useRef<{
    grab: Grab;
    start: { x: number; y: number };
    offset: Point;
    moved: boolean;
  } | null>(null);

  const begin = (grab: Grab) => (e: ReactPointerEvent) => {
    if (e.button !== 0 || !svg.current) return;
    e.stopPropagation();
    svg.current.focus({ preventScroll: true });
    svg.current.setPointerCapture?.(e.pointerId);
    const at = toPlan(svg.current, e, box);
    const anchor = grabPoint(map, grab);
    drag.current = {
      grab,
      start: { x: e.clientX, y: e.clientY },
      offset: { x: anchor.x - at.x, z: anchor.z - at.z },
      moved: false,
    };
    mapSelection.set(selectionOf(grab));
  };
  const move = (e: ReactPointerEvent) => {
    const d = drag.current;
    if (!d || !svg.current) return;
    if (!d.moved && Math.hypot(e.clientX - d.start.x, e.clientY - d.start.y) < DRAG_SLOP) return;
    d.moved = true;
    const step = e.altKey ? 0.1 : SNAP;
    mapPreview.set(applyGrab(map, d.grab, toPlan(svg.current, e, box), d.offset, step));
  };
  const end = () => {
    const d = drag.current;
    drag.current = null;
    const next = mapPreview.get();
    if (d?.moved && next) commitMap(next);
    else mapPreview.set(null);
  };

  const floor: Rect = {
    minX: -shown.floor.width / 2,
    minZ: -shown.floor.depth / 2,
    maxX: shown.floor.width / 2,
    maxZ: shown.floor.depth / 2,
  };
  const grid: number[][] = [];
  for (let x = Math.ceil(floor.minX / GRID) * GRID; x <= floor.maxX; x += GRID)
    grid.push([x, floor.minZ, x, floor.maxZ]);
  for (let z = Math.ceil(floor.minZ / GRID) * GRID; z <= floor.maxZ; z += GRID)
    grid.push([floor.minX, z, floor.maxX, z]);
  const label = Math.max(2, (box.maxX - box.minX) / 45);
  const selected = (s: Selection) => same(s, selection);

  return (
    <svg
      ref={svg}
      className="map-plan"
      viewBox={`${box.minX} ${box.minZ} ${box.maxX - box.minX} ${box.maxZ - box.minZ}`}
      preserveAspectRatio="xMidYMid meet"
      role="group"
      aria-label="Map plan (north is up)"
      tabIndex={0}
      onPointerDown={(e) => {
        if (e.button === 0) mapSelection.set(null);
      }}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={() => {
        drag.current = null;
        mapPreview.set(null);
      }}
    >
      <rect className="plan-building" {...rectProps(box)} />
      <rect className="plan-stands" {...rectProps(grow(floor, wallInner(shown.venue)))} />
      <rect className="plan-pit-wall" {...rectProps(grow(floor, shown.venue.pitWallThickness))} />
      <rect className="plan-floor" {...rectProps(floor)} />
      {grid.map(([x1, z1, x2, z2]) => (
        <line
          key={`${x1},${z1},${x2},${z2}`}
          className={
            x1 === 0 && x2 === 0 ? "plan-axis" : z1 === 0 && z2 === 0 ? "plan-axis" : "plan-grid"
          }
          x1={x1}
          y1={z1}
          x2={x2}
          y2={z2}
        />
      ))}
      {shown.rooms.map((room, i) => {
        const r = roomRect(shown, room);
        const back = edgePoint(shown, room.edge, room.offset, room.depth);
        const sel = selected({ kind: "room", index: i });
        return (
          <g key={`room-${i}`} className={`plan-room team-${room.team}${sel ? " selected" : ""}`}>
            <rect
              {...rectProps(r)}
              role="button"
              aria-label={`Room ${i + 1} (${room.team}, ${room.edge})`}
              onPointerDown={begin({ kind: "room", index: i, mode: "move" })}
            />
            {sel && (
              <rect
                className="plan-handle"
                x={back.x - 0.9}
                y={back.z - 0.9}
                width={1.8}
                height={1.8}
                role="button"
                aria-label={`Room ${i + 1} depth handle`}
                onPointerDown={begin({ kind: "room", index: i, mode: "depth" })}
              />
            )}
          </g>
        );
      })}
      {shown.pillars.map((pillar, i) => {
        const r = pillarRect(pillar);
        const sel = selected({ kind: "pillar", index: i });
        return (
          <g key={`pillar-${i}`} className={`plan-pillar${sel ? " selected" : ""}`}>
            <rect
              {...rectProps(r)}
              role="button"
              aria-label={`Pillar ${i + 1}`}
              onPointerDown={begin({ kind: "pillar", index: i, mode: "move" })}
            />
            {sel && (
              <rect
                className="plan-handle"
                x={r.maxX - 0.9}
                y={r.maxZ - 0.9}
                width={1.8}
                height={1.8}
                role="button"
                aria-label={`Pillar ${i + 1} size handle`}
                onPointerDown={begin({ kind: "pillar", index: i, mode: "resize" })}
              />
            )}
          </g>
        );
      })}
      {shown.rooms.flatMap((room, i) =>
        room.spawns.map((spawn, j) => {
          const p = spawnPoint(shown, room, spawn);
          const sel = selected({ kind: "spawn", room: i, index: j });
          return (
            <circle
              key={`spawn-${i}-${j}`}
              className={`plan-spawn team-${room.team}${sel ? " selected" : ""}`}
              cx={p.x}
              cy={p.z}
              r={BODY}
              role="button"
              aria-label={`Room ${i + 1} spawn ${j + 1}`}
              onPointerDown={begin({ kind: "spawn", room: i, index: j })}
            />
          );
        }),
      )}
      <g className="plan-compass" fontSize={label} aria-hidden="true">
        <text x={0} y={box.minZ + label * 1.3} textAnchor="middle">
          N
        </text>
        <text x={0} y={box.maxZ - label * 0.4} textAnchor="middle">
          S
        </text>
        <text x={box.minX + label * 0.6} y={label * 0.35}>
          W
        </text>
        <text x={box.maxX - label * 0.6} y={label * 0.35} textAnchor="end">
          E
        </text>
      </g>
    </svg>
  );
}
