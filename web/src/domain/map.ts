// Map files (assets/maps/*.json) as the browser editor sees them. Metres
// throughout, matching the file. Rust (`arena-map`) is the authority on
// validity; this module only checks the shape it needs to draw and edit, and
// provides pure edits plus undo history.

export const MAP_FORMAT = 1;
export const DEFAULT_MAP_ID = "colosseum";
/// Editor snapping step (metres); the rules work in millimetres.
export const SNAP = 0.5;

export type Team = "friendly" | "opposing";
export type Edge = "north" | "south" | "west" | "east";
export const EDGES: readonly Edge[] = ["north", "south", "west", "east"];
export const TEAMS: readonly Team[] = ["friendly", "opposing"];

export interface Pillar {
  x: number;
  z: number;
  width: number;
  depth: number;
}
export interface SpawnPoint {
  /** From the room's centre line, toward +X (north/south rooms) or +Z (west/east). */
  across: number;
  /** From the floor edge into the room. */
  back: number;
}
export interface Room {
  team: Team;
  edge: Edge;
  /** Centre of the opening along its edge (X for north/south, Z for west/east). */
  offset: number;
  width: number;
  depth: number;
  ceiling: number;
  spawns: SpawnPoint[];
}
export interface Venue {
  pitWallHeight: number;
  pitWallThickness: number;
  tiers: number;
  tierRise: number;
  tierTread: number;
  concourseDepth: number;
  outerWallThickness: number;
  outerWallHeight: number;
  pillarHeight: number;
}
export interface ArenaMap {
  format: number;
  name: string;
  floor: { width: number; depth: number };
  pillars: Pillar[];
  rooms: Room[];
  venue: Venue;
}

export type Selection =
  | { kind: "floor" }
  | { kind: "pillar"; index: number }
  | { kind: "room"; index: number }
  | { kind: "spawn"; room: number; index: number }
  | null;

export interface Point {
  x: number;
  z: number;
}
export interface Rect {
  minX: number;
  minZ: number;
  maxX: number;
  maxZ: number;
}

export const venueFields: readonly {
  key: keyof Venue;
  label: string;
  step: number;
  integer?: boolean;
}[] = [
  { key: "pitWallHeight", label: "Pit wall height", step: 0.5 },
  { key: "pitWallThickness", label: "Pit wall thickness", step: 0.25 },
  { key: "tiers", label: "Tiers", step: 1, integer: true },
  { key: "tierRise", label: "Tier rise", step: 0.05 },
  { key: "tierTread", label: "Tier tread", step: 0.1 },
  { key: "concourseDepth", label: "Concourse depth", step: 0.5 },
  { key: "outerWallThickness", label: "Outer wall thickness", step: 0.5 },
  { key: "outerWallHeight", label: "Outer wall height", step: 1 },
  { key: "pillarHeight", label: "Pillar height", step: 1 },
];

const isNumber = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

/** Parses a map file's text, checking only the shape the editor relies on. */
export function parseMap(text: string): ArenaMap {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    throw new Error("The file is not valid JSON.");
  }
  if (!isObject(raw)) throw new Error("A map file must be a JSON object.");
  if (raw.format !== MAP_FORMAT)
    throw new Error(`Map format ${String(raw.format)} is not supported (expected ${MAP_FORMAT}).`);
  const fail = (what: string) => {
    throw new Error(`The map's ${what} is missing or not a number.`);
  };
  const numbers = (value: unknown, keys: readonly string[], what: string) => {
    if (!isObject(value)) fail(what);
    for (const key of keys)
      if (!isNumber((value as Record<string, unknown>)[key])) fail(`${what} ${key}`);
  };
  if (typeof raw.name !== "string") throw new Error("The map needs a name.");
  numbers(raw.floor, ["width", "depth"], "floor");
  if (!Array.isArray(raw.pillars)) fail("pillar list");
  (raw.pillars as unknown[]).forEach((p) => numbers(p, ["x", "z", "width", "depth"], "pillar"));
  if (!Array.isArray(raw.rooms)) fail("room list");
  for (const room of raw.rooms as unknown[]) {
    numbers(room, ["offset", "width", "depth", "ceiling"], "room");
    const r = room as Record<string, unknown>;
    if (!TEAMS.includes(r.team as Team))
      throw new Error("Each room's team must be friendly or opposing.");
    if (!EDGES.includes(r.edge as Edge))
      throw new Error("Each room's edge must be north, south, west or east.");
    if (!Array.isArray(r.spawns)) fail("room spawn list");
    (r.spawns as unknown[]).forEach((s) => numbers(s, ["across", "back"], "spawn"));
  }
  numbers(
    raw.venue,
    venueFields.map((f) => f.key),
    "venue",
  );
  return structuredClone(raw) as unknown as ArenaMap;
}

/**
 * The file text, formatted like the tracked map files: flat objects that fit
 * stay on one line (`{ "x": 1, "z": 2 }`), everything else is indented, so
 * saved maps produce small, readable diffs.
 */
export function serializeMap(map: ArenaMap): string {
  return `${format(map, "")}\n`;
}
const LINE = 100;
function format(value: unknown, indent: string): string {
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    const inner = indent + "  ";
    return `[\n${value.map((v) => inner + format(v, inner)).join(",\n")}\n${indent}]`;
  }
  if (isObject(value)) {
    const entries = Object.entries(value);
    if (entries.length === 0) return "{}";
    if (entries.every(([, v]) => !isObject(v) && !Array.isArray(v))) {
      const flat = `{ ${entries.map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(", ")} }`;
      if (indent.length + flat.length + 24 <= LINE) return flat;
    }
    const inner = indent + "  ";
    return `{\n${entries
      .map(([k, v]) => `${inner}${JSON.stringify(k)}: ${format(v, inner)}`)
      .join(",\n")}\n${indent}}`;
  }
  return JSON.stringify(value);
}

// `|| 0` turns −0 into 0 so files never record it.
export const snap = (value: number, step = SNAP) => round(Math.round(value / step) * step);
const round = (value: number) => Math.round(value * 1000) / 1000 || 0;

// ---------------------------------------------------------------- geometry

const alongHalf = (map: ArenaMap, edge: Edge) =>
  edge === "north" || edge === "south" ? map.floor.width / 2 : map.floor.depth / 2;
const edgeDistance = (map: ArenaMap, edge: Edge) =>
  edge === "north" || edge === "south" ? map.floor.depth / 2 : map.floor.width / 2;

/** World point `along` an edge's axis and `out` beyond it (negative is on the floor). */
export function edgePoint(map: ArenaMap, edge: Edge, along: number, out: number): Point {
  const beyond = edgeDistance(map, edge) + out;
  switch (edge) {
    case "north":
      return { x: along, z: -beyond };
    case "south":
      return { x: along, z: beyond };
    case "west":
      return { x: -beyond, z: along };
    case "east":
      return { x: beyond, z: along };
  }
}

/** Inverse of `edgePoint`. */
export function edgeFrame(map: ArenaMap, edge: Edge, p: Point): { along: number; out: number } {
  const d = edgeDistance(map, edge);
  switch (edge) {
    case "north":
      return { along: p.x, out: -p.z - d };
    case "south":
      return { along: p.x, out: p.z - d };
    case "west":
      return { along: p.z, out: -p.x - d };
    case "east":
      return { along: p.z, out: p.x - d };
  }
}

function spanRect(a: Point, b: Point): Rect {
  return {
    minX: Math.min(a.x, b.x),
    minZ: Math.min(a.z, b.z),
    maxX: Math.max(a.x, b.x),
    maxZ: Math.max(a.z, b.z),
  };
}

export function pillarRect(p: Pillar): Rect {
  return {
    minX: p.x - p.width / 2,
    minZ: p.z - p.depth / 2,
    maxX: p.x + p.width / 2,
    maxZ: p.z + p.depth / 2,
  };
}

export function roomRect(map: ArenaMap, room: Room): Rect {
  const half = room.width / 2;
  return spanRect(
    edgePoint(map, room.edge, room.offset - half, 0),
    edgePoint(map, room.edge, room.offset + half, room.depth),
  );
}

export function spawnPoint(map: ArenaMap, room: Room, spawn: SpawnPoint): Point {
  return edgePoint(map, room.edge, room.offset + spawn.across, spawn.back);
}

/** Distance from the floor edge to the inner face of the outer wall. */
export function wallInner(v: Venue): number {
  return v.pitWallThickness + v.tiers * v.tierTread + v.concourseDepth;
}

/** The plan's drawing bounds: floor, stands and building. */
export function planBounds(map: ArenaMap): Rect {
  const reach = wallInner(map.venue) + map.venue.outerWallThickness;
  return {
    minX: -map.floor.width / 2 - reach,
    minZ: -map.floor.depth / 2 - reach,
    maxX: map.floor.width / 2 + reach,
    maxZ: map.floor.depth / 2 + reach,
  };
}

// ---------------------------------------------------------------- edits

const clamp = (value: number, low: number, high: number) =>
  Math.min(Math.max(value, low), Math.max(low, high));

export function setName(map: ArenaMap, name: string): ArenaMap {
  return { ...map, name };
}

export function setFloor(map: ArenaMap, key: "width" | "depth", value: number): ArenaMap {
  return { ...map, floor: { ...map.floor, [key]: round(value) } };
}

export function setVenue(map: ArenaMap, key: keyof Venue, value: number): ArenaMap {
  const integer = venueFields.find((f) => f.key === key)?.integer;
  return { ...map, venue: { ...map.venue, [key]: integer ? Math.round(value) : round(value) } };
}

export function updatePillar(map: ArenaMap, index: number, patch: Partial<Pillar>): ArenaMap {
  const pillars = map.pillars.map((p, i) => {
    if (i !== index) return p;
    const next = { ...p, ...patch };
    return {
      x: round(next.x),
      z: round(next.z),
      width: round(next.width),
      depth: round(next.depth),
    };
  });
  return { ...map, pillars };
}

/** Moves a pillar so its centre is at `point`, snapped and kept on the floor. */
export function movePillar(map: ArenaMap, index: number, point: Point, step = SNAP): ArenaMap {
  const p = map.pillars[index];
  const hx = map.floor.width / 2 - p.width / 2;
  const hz = map.floor.depth / 2 - p.depth / 2;
  return updatePillar(map, index, {
    x: clamp(snap(point.x, step), -hx, hx),
    z: clamp(snap(point.z, step), -hz, hz),
  });
}

/** Resizes a pillar symmetrically so one corner follows `point`. */
export function resizePillar(map: ArenaMap, index: number, point: Point, step = SNAP): ArenaMap {
  const p = map.pillars[index];
  const width = Math.max(1, snap(Math.abs(point.x - p.x) * 2, step * 2));
  const depth = Math.max(1, snap(Math.abs(point.z - p.z) * 2, step * 2));
  return updatePillar(map, index, { width, depth });
}

export function addPillar(map: ArenaMap, at: Point = { x: 0, z: 0 }): ArenaMap {
  return {
    ...map,
    pillars: [...map.pillars, { x: snap(at.x), z: snap(at.z), width: 4, depth: 4 }],
  };
}

export function updateRoom(
  map: ArenaMap,
  index: number,
  patch: Partial<Omit<Room, "spawns">>,
): ArenaMap {
  const rooms = map.rooms.map((room, i) => {
    if (i !== index) return room;
    const next = { ...room, ...patch };
    for (const key of ["offset", "width", "depth", "ceiling"] as const)
      next[key] = round(next[key]);
    return next;
  });
  return { ...map, rooms };
}

/** Slides a room along its edge so its opening is centred nearest `point`. */
export function moveRoom(map: ArenaMap, index: number, point: Point, step = SNAP): ArenaMap {
  const room = map.rooms[index];
  const limit = alongHalf(map, room.edge) - room.width / 2 - 1;
  const { along } = edgeFrame(map, room.edge, point);
  return updateRoom(map, index, { offset: clamp(snap(along, step), -limit, limit) });
}

/** Sets a room's depth from a point dragged out from its edge. */
export function deepenRoom(map: ArenaMap, index: number, point: Point, step = SNAP): ArenaMap {
  const room = map.rooms[index];
  const { out } = edgeFrame(map, room.edge, point);
  const deepest = Math.max(...room.spawns.map((s) => s.back + 0.5), 3);
  return updateRoom(map, index, { depth: Math.max(deepest, snap(out, step)) });
}

/** A new room on the first edge with space, copying the team's existing room shape. */
export function addRoom(map: ArenaMap, team: Team): ArenaMap {
  const template = map.rooms.find((r) => r.team === team) ?? map.rooms[0];
  const width = template?.width ?? 6;
  const depth = template?.depth ?? 11;
  const ceiling = template?.ceiling ?? 6;
  const order: Edge[] =
    team === "friendly" ? ["south", "west", "east", "north"] : ["north", "east", "west", "south"];
  let placement: { edge: Edge; offset: number } | null = null;
  for (const edge of order) {
    const half = alongHalf(map, edge) - width / 2 - 1;
    const taken = map.rooms.filter((r) => r.edge === edge);
    for (let offset = 0; offset <= half && !placement; offset += SNAP) {
      for (const candidate of offset === 0 ? [0] : [offset, -offset]) {
        const clear = taken.every(
          (r) => Math.abs(r.offset - candidate) >= (r.width + width) / 2 + 1,
        );
        if (clear) {
          placement = { edge, offset: candidate };
          break;
        }
      }
    }
    if (placement) break;
  }
  const { edge, offset } = placement ?? { edge: order[0], offset: 0 };
  const back = Math.min(5, depth - 1);
  const room: Room = {
    team,
    edge,
    offset,
    width,
    depth,
    ceiling,
    spawns: [
      { across: 0, back },
      { across: -2, back },
      { across: 2, back },
    ].filter((s) => Math.abs(s.across) <= width / 2 - 0.5),
  };
  return { ...map, rooms: [...map.rooms, room] };
}

export function updateSpawn(
  map: ArenaMap,
  roomIndex: number,
  index: number,
  patch: Partial<SpawnPoint>,
): ArenaMap {
  return {
    ...map,
    rooms: map.rooms.map((room, i) =>
      i !== roomIndex
        ? room
        : {
            ...room,
            spawns: room.spawns.map((s, j) =>
              j !== index
                ? s
                : { across: round(patch.across ?? s.across), back: round(patch.back ?? s.back) },
            ),
          },
    ),
  };
}

/** Moves a spawn to `point`, kept inside its room. */
export function moveSpawn(
  map: ArenaMap,
  roomIndex: number,
  index: number,
  point: Point,
  step = SNAP,
): ArenaMap {
  const room = map.rooms[roomIndex];
  const { along, out } = edgeFrame(map, room.edge, point);
  const side = Math.max(0, room.width / 2 - 0.5);
  return updateSpawn(map, roomIndex, index, {
    across: clamp(snap(along - room.offset, step), -side, side),
    back: clamp(snap(out, step), 0.5, room.depth - 0.5),
  });
}

export function addSpawn(map: ArenaMap, roomIndex: number): ArenaMap {
  const room = map.rooms[roomIndex];
  const back = Math.max(0.5, Math.min(room.depth - 0.5, (room.spawns.at(-1)?.back ?? 5) - 1.5));
  return {
    ...map,
    rooms: map.rooms.map((r, i) =>
      i === roomIndex ? { ...r, spawns: [...r.spawns, { across: 0, back }] } : r,
    ),
  };
}

export function duplicate(
  map: ArenaMap,
  selection: Selection,
): { map: ArenaMap; selection: Selection } {
  if (selection?.kind !== "pillar") return { map, selection };
  const p = map.pillars[selection.index];
  const next = { ...map, pillars: [...map.pillars, { ...p, x: p.x + p.width + 2 }] };
  return { map: next, selection: { kind: "pillar", index: next.pillars.length - 1 } };
}

/** Removes the selected item; the floor cannot be removed. */
export function removeSelected(map: ArenaMap, selection: Selection): ArenaMap {
  switch (selection?.kind) {
    case "pillar":
      return { ...map, pillars: map.pillars.filter((_, i) => i !== selection.index) };
    case "room":
      return { ...map, rooms: map.rooms.filter((_, i) => i !== selection.index) };
    case "spawn":
      return {
        ...map,
        rooms: map.rooms.map((r, i) =>
          i === selection.room
            ? { ...r, spawns: r.spawns.filter((_, j) => j !== selection.index) }
            : r,
        ),
      };
    default:
      return map;
  }
}

/** Moves the selected item by a whole offset (keyboard nudges). */
export function nudge(map: ArenaMap, selection: Selection, dx: number, dz: number): ArenaMap {
  switch (selection?.kind) {
    case "pillar": {
      const p = map.pillars[selection.index];
      return movePillar(map, selection.index, { x: p.x + dx, z: p.z + dz }, 0.001);
    }
    case "room": {
      const room = map.rooms[selection.index];
      const at = edgePoint(map, room.edge, room.offset, 0);
      return moveRoom(map, selection.index, { x: at.x + dx, z: at.z + dz }, 0.001);
    }
    case "spawn": {
      const room = map.rooms[selection.room];
      const at = spawnPoint(map, room, room.spawns[selection.index]);
      return moveSpawn(map, selection.room, selection.index, { x: at.x + dx, z: at.z + dz }, 0.001);
    }
    default:
      return map;
  }
}

/** Keeps a selection pointing at something that still exists. */
export function reconcileSelection(map: ArenaMap, selection: Selection): Selection {
  switch (selection?.kind) {
    case "pillar":
      return selection.index < map.pillars.length ? selection : null;
    case "room":
      return selection.index < map.rooms.length ? selection : null;
    case "spawn":
      return selection.index < (map.rooms[selection.room]?.spawns.length ?? 0) ? selection : null;
    default:
      return selection;
  }
}

// ---------------------------------------------------------------- history

export const HISTORY_LIMIT = 100;
export interface History {
  past: ArenaMap[];
  present: ArenaMap;
  future: ArenaMap[];
}
export const historyOf = (present: ArenaMap): History => ({ past: [], present, future: [] });
export function commit(history: History, next: ArenaMap): History {
  if (JSON.stringify(next) === JSON.stringify(history.present)) return history;
  return {
    past: [...history.past, history.present].slice(-HISTORY_LIMIT),
    present: next,
    future: [],
  };
}
export function undo(history: History): History {
  const previous = history.past.at(-1);
  if (!previous) return history;
  return {
    past: history.past.slice(0, -1),
    present: previous,
    future: [history.present, ...history.future],
  };
}
export function redo(history: History): History {
  const [next, ...future] = history.future;
  if (!next) return history;
  return { past: [...history.past, history.present], present: next, future };
}
