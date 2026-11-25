import { describe, expect, it } from "vitest";
import { adaptSnapshot } from "../../web/src/bridge/snapshot";
import {
  handleName,
  isHandle,
  parseHandles,
  parseTags,
  withHandles,
} from "../../web/src/domain/handles";
import { buildMatchScoreboard } from "../../web/src/domain/scoreboard";
import { shortName } from "../../web/src/domain/units/selection";
import { teamSnapshot } from "../ui/fixtures";
import { isHandles, isTags } from "../../web/src/domain/network";

it("accepts only server-shaped handles", () => {
  for (const good of ["Rook#0042", "abc#9999", "Gladiator7#0001", "abcdefghijklmnop#1234"])
    expect(isHandle(good), good).toBe(true);
  for (const bad of [
    "Rook",
    "Rook#42",
    "Rook#0000",
    "ab#0001",
    "abcdefghijklmnopq#0001",
    "7Rook#0001",
    "Ro ok#0001",
    "<b>x</b>#0001",
    42,
    null,
  ])
    expect(isHandle(bad), String(bad)).toBe(false);
  expect(handleName("Rook#0042")).toBe("Rook");
});

it("parses the relay strictly: even seat counts, handles or null", () => {
  expect(parseHandles(["Rook#0042", null])).toEqual(["Rook#0042", null]);
  expect(parseHandles([null, null, "Guest#0007", "Ash#1000"])).toHaveLength(4);
  expect(parseHandles(null)).toBeNull();
  expect(parseHandles(["Rook#0042"])).toBeNull();
  expect(parseHandles(Array(8).fill(null))).toBeNull();
  expect(parseHandles(["Rook#0042", "nope"])).toBeNull();
});

it("recognizes the Handles frame (variant 5)", () => {
  expect(isHandles(new Uint8Array([5, 2, 0, 0]))).toBe(true);
  expect(isHandles(new Uint8Array([4, 2, 0, 0]))).toBe(false);
  expect(isHandles(new Uint8Array([]))).toBe(false);
});

it("parses the tags relay strictly", () => {
  expect(parseTags([{ admin: true }, { admin: false }])).toEqual([
    { admin: true },
    { admin: false },
  ]);
  expect(parseTags(null)).toBeNull();
  expect(parseTags([{ admin: true }])).toBeNull();
  expect(parseTags([{ admin: "yes" }, { admin: false }])).toBeNull();
  expect(parseTags([{ admin: true, owner: true }, { admin: false }])).toBeNull();
  expect(parseTags([null, { admin: false }])).toBeNull();
  expect(isTags(new Uint8Array([6, 2, 1, 0]))).toBe(true);
  expect(isTags(new Uint8Array([5, 2]))).toBe(false);
});

describe("naming fighters by handles", () => {
  const network = () => {
    const base = teamSnapshot(2);
    return adaptSnapshot({
      ...base,
      maxHealth: 100,
      roster: base.roster?.map((unit) => ({
        actor: unit.actor,
        tag: unit.tag,
        health: unit.health,
        connected: true,
      })),
    });
  };

  it("names a network roster by handle; seats without one keep their role name", () => {
    const named = withHandles(network(), ["Rook#0042", "Ash#1000", null, "Guest#0007"]);
    expect(named.roster?.map((unit) => [unit.name, unit.handle])).toEqual([
      ["Rook#0042", "Rook#0042"],
      ["Ash#1000", "Ash#1000"],
      ["Teammate 1", undefined],
      ["Guest#0007", "Guest#0007"],
    ]);
    expect(named.player.name).toBe("Rook#0042");
    expect(named.target.name).toBe("Ash#1000");
    expect(shortName(named.target)).toBe("Ash");
    expect(shortName(named.roster![2])).toBe("Teammate 1");
  });

  it("names a spectator's view the same way", () => {
    const base = teamSnapshot(1);
    const watching = adaptSnapshot({
      ...base,
      maxHealth: 100,
      spectating: true,
      roster: base.roster?.map((unit) => ({
        actor: unit.actor,
        tag: unit.tag,
        health: unit.health,
        connected: true,
      })),
    });
    const named = withHandles(watching, [null, "Ash#1000"]);
    expect(named.roster?.map((unit) => unit.name)).toEqual(["Team A 1", "Ash#1000"]);
  });

  it("leaves practice, a missing relay or a mismatched one alone", () => {
    const practice = teamSnapshot(2);
    expect(withHandles(practice, ["Rook#0042", null, null, null])).toBe(practice);
    const live = network();
    expect(withHandles(live, null)).toBe(live);
    expect(withHandles(live, ["Rook#0042", null])).toBe(live);
  });

  it("carries handles onto the post-match scoreboard", () => {
    const named = withHandles(network(), ["Rook#0042", "Ash#1000", null, "Guest#0007"]);
    const board = buildMatchScoreboard({
      tick: 300,
      tickRate: 30,
      roundResult: "won",
      roster: named.roster,
      scoreboard: named.roster!.map((unit) => ({
        actor: unit.actor,
        tag: unit.tag,
        damageDone: 0,
        damageTaken: 0,
        healingDone: 0,
        healingReceived: 0,
        killingBlows: 0,
      })),
    });
    expect(board?.allies.map((row) => row.name)).toEqual(["Rook#0042", "Teammate 1"]);
    expect(board?.opponents.map((row) => row.name)).toEqual(["Ash#1000", "Guest#0007"]);
  });
});
