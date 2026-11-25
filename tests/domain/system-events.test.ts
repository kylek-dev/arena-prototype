import { expect, it } from "vitest";
import {
  friendEvents,
  lobbyEvents,
  partyEvents,
  rosterEvents,
  roundEvents,
} from "../../web/src/domain/system-events";
import type { CustomLobby } from "../../web/src/domain/matchmaking";

const lobby = (members: [number, string | null][], host = false): CustomLobby => ({
  code: "K7P2QX",
  mode: "1v1",
  settings: { version: 1, map: "colosseum", roundSeconds: 600, allowSpectators: true },
  spectatorSlots: 4,
  you: 1,
  host,
  canStart: false,
  members: members.map(([id, handle]) => ({
    id,
    handle,
    slot: "team_a",
    host: false,
    you: id === 1,
  })),
});

it("tells lobby joins, leaves, hosting and kicks", () => {
  expect(lobbyEvents(null, lobby([[1, "Rook#0042"]]), false)).toEqual([
    "You joined custom lobby K7P2QX.",
  ]);
  expect(
    lobbyEvents(
      lobby([
        [1, "Rook#0042"],
        [2, "Ash#1000"],
      ]),
      lobby(
        [
          [1, "Rook#0042"],
          [3, null],
        ],
        true,
      ),
      false,
    ),
  ).toEqual([
    "Player 3 joined the lobby.",
    "Ash#1000 left the lobby.",
    "You are now the lobby host.",
  ]);
  expect(lobbyEvents(lobby([[1, null]]), null, true)).toEqual([
    "You were removed from the custom lobby.",
  ]);
  expect(lobbyEvents(lobby([[1, null]]), null, false)).toEqual(["You left the custom lobby."]);
  expect(lobbyEvents(null, null, false)).toEqual([]);
});

it("tells party changes", () => {
  const party = (members: number, leader = false) => ({ code: "M9Q3RT", members, leader });
  expect(partyEvents(null, party(2))).toEqual(["You joined party M9Q3RT."]);
  expect(partyEvents(null, party(1, true))).toEqual([]);
  expect(partyEvents(party(1, true), party(2, true))).toEqual(["A player joined your party."]);
  expect(partyEvents(party(3), party(2))).toEqual(["A player left your party."]);
  expect(partyEvents(party(2), party(2, true))).toEqual(["You now lead the party."]);
  expect(partyEvents(party(2), { code: null, members: 1, leader: true })).toEqual([
    "You left the party.",
  ]);
  expect(partyEvents(null, { code: null, members: 1, leader: true })).toEqual([]);
});

it("tells friends coming online, requests and acceptances, but not the first list", () => {
  const before = {
    friends: [
      { handle: "Ash#1000", relation: "friend" as const, presence: "offline" as const },
      { handle: "Bea#0002", relation: "outgoing" as const, presence: null },
    ],
    blocked: [],
  };
  const after = {
    friends: [
      { handle: "Ash#1000", relation: "friend" as const, presence: "online" as const },
      { handle: "Bea#0002", relation: "friend" as const, presence: "online" as const },
      { handle: "Cyd#0003", relation: "incoming" as const, presence: null },
    ],
    blocked: [],
  };
  expect(friendEvents(before, after)).toEqual([
    "Ash#1000 is online.",
    "Bea#0002 accepted your friend request.",
    "Cyd#0003 wants to be friends.",
  ]);
  expect(friendEvents(null, after)).toEqual([]);
});

it("tells disconnects, reconnects, round starts and results", () => {
  const unit = (name: string, disconnected = false) => ({
    id: name,
    name,
    health: 1,
    maxHealth: 1,
    disconnected,
  });
  expect(
    rosterEvents(
      [unit("Rook#0042"), unit("Ash#1000", true)],
      [unit("Rook#0042", true), unit("Ash#1000")],
    ),
  ).toEqual(["Rook#0042 disconnected.", "Ash#1000 reconnected."]);
  expect(rosterEvents(undefined, [unit("Rook#0042")])).toEqual([]);
  expect(
    roundEvents({ playing: false, result: null }, { playing: true, result: null }, false),
  ).toEqual(["The round has started."]);
  expect(
    roundEvents({ playing: true, result: null }, { playing: false, result: "won" }, false),
  ).toEqual(["Round over: Victory."]);
  expect(
    roundEvents({ playing: true, result: null }, { playing: false, result: "won" }, true),
  ).toEqual(["Round over: Team A wins."]);
});
