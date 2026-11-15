import { describe, expect, it } from "vitest";
import colosseumText from "../../assets/maps/colosseum.json?raw";
import {
  addPillar,
  addRoom,
  addSpawn,
  commit,
  deepenRoom,
  edgeFrame,
  edgePoint,
  EDGES,
  historyOf,
  HISTORY_LIMIT,
  movePillar,
  moveRoom,
  moveSpawn,
  nudge,
  parseMap,
  reconcileSelection,
  redo,
  removeSelected,
  resizePillar,
  roomRect,
  serializeMap,
  setVenue,
  spawnPoint,
  undo,
} from "../../web/src/domain/map";

const colosseum = () => parseMap(colosseumText);

describe("map files", () => {
  it("round-trips the tracked colosseum byte for byte", () => {
    expect(serializeMap(colosseum())).toBe(colosseumText);
  });

  it("rejects files the editor cannot draw, with plain messages", () => {
    const edit = (change: (v: ReturnType<typeof JSON.parse>) => void) => {
      const value = JSON.parse(colosseumText);
      change(value);
      return () => parseMap(JSON.stringify(value));
    };
    expect(() => parseMap("{")).toThrow("not valid JSON");
    expect(edit((v) => (v.format = 2))).toThrow("format 2");
    expect(edit((v) => (v.pillars[0].x = "1"))).toThrow("pillar x");
    expect(edit((v) => (v.rooms[0].edge = "up"))).toThrow("north, south, west or east");
    expect(edit((v) => delete v.venue.tiers)).toThrow("venue tiers");
  });
});

describe("plan geometry", () => {
  it("mirrors the rules' edge frames for every edge", () => {
    const map = colosseum();
    for (const edge of EDGES) {
      const p = edgePoint(map, edge, 3, 2);
      expect(edgeFrame(map, edge, p)).toEqual({ along: 3, out: 2 });
    }
    // North is −Z; a south room runs from the floor edge toward +Z.
    const south = map.rooms[0];
    expect(roomRect(map, south)).toEqual({ minX: -3, minZ: 42, maxX: 3, maxZ: 53 });
    expect(spawnPoint(map, south, south.spawns[1])).toEqual({ x: -2, z: 47 });
    const north = map.rooms[1];
    expect(spawnPoint(map, north, north.spawns[0])).toEqual({ x: 0, z: -47 });
  });
});

describe("edits", () => {
  it("snaps and clamps dragged pillars, rooms and spawns", () => {
    const map = colosseum();
    expect(movePillar(map, 0, { x: -10.26, z: 3.74 }).pillars[0]).toMatchObject({
      x: -10.5,
      z: 3.5,
    });
    // Kept on the floor (108 × 84 m with a 9 m pillar).
    expect(movePillar(map, 0, { x: -500, z: 500 }).pillars[0]).toMatchObject({ x: -49.5, z: 37.5 });
    expect(resizePillar(map, 0, { x: -18, z: -15 }).pillars[0]).toMatchObject({
      width: 6,
      depth: 1,
    });
    // Rooms slide along their edge and keep 1 m from the corners.
    expect(moveRoom(map, 0, { x: 12.2, z: 60 }).rooms[0].offset).toBe(12);
    expect(moveRoom(map, 0, { x: 999, z: 0 }).rooms[0].offset).toBe(50);
    // Depth never drops below the deepest spawn.
    expect(deepenRoom(map, 0, { x: 0, z: 43 }).rooms[0].depth).toBe(5.5);
    expect(deepenRoom(map, 1, { x: 0, z: -58 }).rooms[1].depth).toBe(16);
    const spawn = moveSpawn(map, 0, 0, { x: 10, z: 100 }).rooms[0].spawns[0];
    expect(spawn).toEqual({ across: 2.5, back: 10.5 });
  });

  it("adds rooms clear of existing openings and spawns inside them", () => {
    let map = addRoom(colosseum(), "friendly");
    const added = map.rooms[2];
    expect(added).toMatchObject({ team: "friendly", edge: "south", width: 6 });
    expect(Math.abs(added.offset)).toBeGreaterThanOrEqual(7);
    expect(added.spawns).toHaveLength(3);
    map = addSpawn(map, 2);
    expect(map.rooms[2].spawns.at(-1)).toEqual({ across: 0, back: 3.5 });
    expect(addPillar(map, { x: 1.2, z: -0.2 }).pillars.at(-1)).toEqual({
      x: 1,
      z: 0,
      width: 4,
      depth: 4,
    });
  });

  it("removes and nudges the selection and drops stale selections", () => {
    const map = colosseum();
    const fewer = removeSelected(map, { kind: "spawn", room: 1, index: 2 });
    expect(fewer.rooms[1].spawns).toHaveLength(2);
    expect(reconcileSelection(fewer, { kind: "spawn", room: 1, index: 2 })).toBeNull();
    expect(removeSelected(map, { kind: "floor" })).toBe(map);
    expect(nudge(map, { kind: "pillar", index: 0 }, 0.5, 0).pillars[0].x).toBe(-20.5);
    expect(nudge(map, { kind: "room", index: 1 }, -2, 0).rooms[1].offset).toBe(-2);
    expect(setVenue(map, "tiers", 7.6).venue.tiers).toBe(8);
  });
});

describe("history", () => {
  it("undoes and redoes, ignores no-op commits and caps its length", () => {
    const start = colosseum();
    let history = historyOf(start);
    expect(commit(history, structuredClone(start))).toBe(history);
    const moved = movePillar(start, 0, { x: 0, z: 0 });
    history = commit(history, moved);
    expect(undo(history).present).toBe(start);
    expect(redo(undo(history)).present).toBe(moved);
    expect(commit(undo(history), setVenue(start, "tiers", 9)).future).toEqual([]);
    for (let i = 0; i < HISTORY_LIMIT + 20; i++)
      history = commit(history, setVenue(history.present, "pillarHeight", 40 + i));
    expect(history.past).toHaveLength(HISTORY_LIMIT);
  });
});
