import { expect, it } from "vitest";
import {
  friendDoneText,
  friendErrorText,
  groupFriends,
  invitable,
  normalizeHandle,
  parseFriends,
} from "../../web/src/domain/friends";
import { noticeText, parseQueueMessage, requests } from "../../web/src/domain/matchmaking";

const LIST = {
  friends: [
    { handle: "Zed#0001", relation: "friend", presence: "offline" },
    { handle: "ash#1000", relation: "friend", presence: "in_match" },
    { handle: "Bea#0002", relation: "friend", presence: "online" },
    { handle: "Cyd#0003", relation: "incoming" },
    { handle: "Dot#0004", relation: "outgoing" },
  ],
  blocked: ["Eve#0005"],
};

it("parses the friends list strictly", () => {
  const list = parseFriends(LIST);
  expect(list?.friends).toHaveLength(5);
  expect(list?.friends[3]).toEqual({ handle: "Cyd#0003", relation: "incoming", presence: null });
  expect(list?.blocked).toEqual(["Eve#0005"]);
  for (const bad of [
    null,
    { friends: [] },
    { friends: {}, blocked: [] },
    { friends: [{ handle: "nope", relation: "friend", presence: "online" }], blocked: [] },
    { friends: [{ handle: "Cyd#0003", relation: "enemy" }], blocked: [] },
    { friends: [{ handle: "Cyd#0003", relation: "friend", presence: "away" }], blocked: [] },
    { friends: [], blocked: ["<b>"] },
    { friends: Array(401).fill({ handle: "Cyd#0003", relation: "incoming" }), blocked: [] },
  ])
    expect(parseFriends(bad), JSON.stringify(bad).slice(0, 80)).toBeNull();
});

it("groups requests first, friends online first and by name, then sent requests", () => {
  const groups = groupFriends(parseFriends(LIST)!);
  expect(groups.incoming.map((f) => f.handle)).toEqual(["Cyd#0003"]);
  expect(groups.friends.map((f) => f.handle)).toEqual(["Bea#0002", "ash#1000", "Zed#0001"]);
  expect(groups.outgoing.map((f) => f.handle)).toEqual(["Dot#0004"]);
  expect(groups.friends.map(invitable)).toEqual([true, true, false]);
  expect(invitable(groups.incoming[0])).toBe(false);
});

it("reads typed handles and words answers", () => {
  expect(normalizeHandle("  Rook#0042 ")).toBe("Rook#0042");
  expect(normalizeHandle("Rook")).toBeNull();
  expect(normalizeHandle("Rook#42")).toBeNull();
  expect(friendErrorText("pick_a_name_first")).toMatch(/choose a player name/i);
  expect(friendErrorText("weird")).toBe(friendErrorText("failed"));
  expect(friendDoneText("request", "Rook#0042", "outgoing")).toMatch(/request sent/i);
  expect(friendDoneText("request", "Rook#0042", "friend")).toMatch(/now friends/i);
  expect(friendDoneText("block", "Rook#0042")).toMatch(/can no longer send you requests/i);
});

it("parses invites, sent invites and friend pushes from the queue", () => {
  expect(
    parseQueueMessage('{"type":"invite","from":"Rook#0042","kind":"lobby","code":"k7p2qx"}'),
  ).toEqual({ type: "invite", invite: { from: "Rook#0042", kind: "lobby", code: "K7P2QX" } });
  expect(parseQueueMessage('{"type":"invite_sent","to":"Rook#0042"}')).toEqual({
    type: "invite_sent",
    to: "Rook#0042",
  });
  expect(parseQueueMessage('{"type":"friends_changed"}')).toEqual({ type: "friends_changed" });
  for (const bad of [
    '{"type":"invite","from":"nope","kind":"party","code":"K7P2QX"}',
    '{"type":"invite","from":"Rook#0042","kind":"raid","code":"K7P2QX"}',
    '{"type":"invite","from":"Rook#0042","kind":"party","code":"000000"}',
    '{"type":"invite_sent","to":42}',
  ])
    expect(parseQueueMessage(bad), bad).toBeNull();
  expect(JSON.parse(requests.invite("Rook#0042"))).toEqual({ type: "invite", handle: "Rook#0042" });
});

it("words every invite refusal", () => {
  for (const code of ["not_logged_in", "no_such_friend", "offline", "unavailable", "rate_limited"])
    expect(noticeText(code), code).toBeTruthy();
});
