import { describe, expect, it } from "vitest";
import {
  IDLE_QUEUE,
  QUEUE_MODES,
  customInviteLink,
  formatExpected,
  formatRoundLimit,
  formatWait,
  guestId,
  inviteLink,
  isGuestId,
  isQueueBarVisible,
  lobbyReadinessText,
  noticeText,
  normalizeCode,
  parseCustomFragment,
  parsePartyFragment,
  parseQueueMessage,
  queueBlocked,
  queueStatus,
  queueUrl,
  readyStatus,
  requests,
  type CustomLobby,
  type QueueState,
} from "../../web/src/domain/matchmaking";

const MATCH = {
  type: "match",
  mode: "2v2",
  match_id: "00ff00ff00ff00ff",
  seat: 3,
  server: "wss://arena.example.com/play",
  ticket: "ab01",
  expires: 2_000_000_000,
};
const QUEUED = { type: "queued", mode: "3v3", waiting: 4, waited_ms: 61_000, expected_ms: 90_000 };
const CHECK = {
  type: "ready_check",
  mode: "2v2",
  accepted: 1,
  total: 4,
  you: false,
  remaining_ms: 12_000,
};

describe("queue messages", () => {
  it("parses what the door sends and nothing else", () => {
    expect(
      parseQueueMessage('{"type":"party","code":"K7P2QX","members":2,"leader":true,"queue":"3v3"}'),
    ).toEqual({
      type: "party",
      party: { code: "K7P2QX", members: 2, leader: true },
      queue: "3v3",
    });
    expect(parseQueueMessage(JSON.stringify(QUEUED))).toEqual({
      type: "queued",
      mode: "3v3",
      waiting: 4,
      waitedMs: 61_000,
      expectedMs: 90_000,
    });
    expect(parseQueueMessage(JSON.stringify({ ...QUEUED, expected_ms: null }))).toMatchObject({
      expectedMs: null,
    });
    expect(parseQueueMessage(JSON.stringify(CHECK))).toEqual({
      type: "ready_check",
      mode: "2v2",
      accepted: 1,
      total: 4,
      you: false,
      remainingMs: 12_000,
    });
    expect(parseQueueMessage(JSON.stringify(MATCH))).toMatchObject({ type: "match", seat: 3 });
    expect(parseQueueMessage('{"type":"stopped","reason":"declined"}')).toEqual({
      type: "stopped",
      reason: "declined",
    });
    for (const bad of [
      "not json",
      "null",
      JSON.stringify({ ...QUEUED, mode: "solo-2v2" }),
      JSON.stringify({ ...QUEUED, waited_ms: -1 }),
      JSON.stringify({ ...QUEUED, expected_ms: "soon" }),
      JSON.stringify({ ...CHECK, accepted: 5 }),
      JSON.stringify({ ...CHECK, total: 1 }),
      JSON.stringify({ ...CHECK, you: "yes" }),
      '{"type":"found","mode":"2v2"}',
      '{"type":"party","code":"0OIL11","members":2,"leader":true,"queue":null}',
      JSON.stringify({ ...MATCH, server: "https://arena.example.com/play" }),
      new Uint8Array([1, 2]),
      "x".repeat(5000),
    ])
      expect(parseQueueMessage(bad), String(bad).slice(0, 40)).toBeNull();
  });

  it("writes the requests the door reads", () => {
    expect(JSON.parse(requests.hello("0".repeat(32)))).toEqual({
      type: "hello",
      guest: "0".repeat(32),
    });
    expect(JSON.parse(requests.accept())).toEqual({ type: "accept" });
    expect(JSON.parse(requests.decline())).toEqual({ type: "decline" });
    expect(QUEUE_MODES.map((mode) => mode.label)).toEqual(["1v1", "2v2", "3v3"]);
  });

  it("finds the queue on the page's origin and invites in the fragment", () => {
    expect(queueUrl({ protocol: "https:", host: "arena.example.com" })).toBe(
      "wss://arena.example.com/api/queue",
    );
    const link = inviteLink({ origin: "https://arena.example.com", pathname: "/" }, "K7P2QX");
    expect(link).toBe("https://arena.example.com/#party=K7P2QX");
    expect(parsePartyFragment("#party=k7p2qx")).toBe("K7P2QX");
    expect(parsePartyFragment("#party=nope")).toBeNull();
    expect(normalizeCode(" k7p2qx ")).toBe("K7P2QX");
  });

  it("makes 32-hex-digit guest ids", () => {
    const id = guestId(new Uint8Array(16).fill(0xab));
    expect(isGuestId(id)).toBe(true);
    expect(isGuestId("AB".repeat(16))).toBe(false);
  });
});

describe("queue wording", () => {
  const at = (patch: Partial<QueueState>): QueueState => ({ ...IDLE_QUEUE, ...patch });

  it("formats waits and expected waits", () => {
    expect(formatWait(0)).toBe("0:00");
    expect(formatWait(65_400)).toBe("1:05");
    expect(formatWait(3_725_000)).toBe("1:02:05");
    expect(formatExpected(20_000)).toBe("under a minute");
    expect(formatExpected(150_000)).toBe("about 3 min");
    expect(formatExpected(4_200_000)).toBe("about 1 h 10 min");
    expect(formatExpected(3_600_000)).toBe("about 1 h");
  });

  it("shows the queue, the time waited, the expected wait and players queued", () => {
    expect(queueStatus(IDLE_QUEUE, 0)).toBeNull();
    const queued = at({
      phase: "queued",
      mode: "2v2",
      since: 1000,
      waiting: 3,
      expectedMs: 90_000,
    });
    expect(queueStatus(queued, 84_000)).toEqual({
      title: "In queue · 2v2",
      waited: "1:23",
      expected: "Expected wait about 2 min",
      players: "3 players in queue",
    });
    expect(queueStatus({ ...queued, expectedMs: null, waiting: 1 }, 1000)).toMatchObject({
      expected: "Not enough recent matches to estimate",
      players: "1 player in queue",
    });
  });

  it("counts a ready check down and shows how many accepted", () => {
    const ready = at({
      phase: "ready",
      mode: "3v3",
      readyCheck: { accepted: 2, total: 6, you: false, deadline: 15_000 },
    });
    expect(readyStatus(ready, 2_500)).toEqual({
      title: "Your 3v3 match is ready",
      seconds: 13,
      fraction: 12_500 / 15_000,
      accepted: 2,
      total: 6,
      you: false,
      progress: "2 of 6 accepted",
    });
    expect(readyStatus(ready, 20_000)).toMatchObject({ seconds: 0, fraction: 0 });
    expect(readyStatus(IDLE_QUEUE, 0)).toBeNull();
  });

  it("explains why a queue is closed to a party, and what happened", () => {
    const party = (members: number, leader: boolean) =>
      at({ party: { code: "K7P2QX", members, leader } });
    expect(queueBlocked(IDLE_QUEUE, "1v1")).toBeNull();
    expect(queueBlocked(party(2, true), "2v2")).toBeNull();
    expect(queueBlocked(party(2, true), "1v1")).toBe("1v1 is for one player.");
    expect(queueBlocked(party(3, true), "2v2")).toBe("Parties of up to 2 only.");
    expect(queueBlocked(party(2, false), "3v3")).toContain("leader");
    expect(noticeText("not_accepted")).toContain("in time");
    expect(noticeText("member_declined")).toContain("party member declined");
    expect(noticeText("requeued")).toContain("front of the queue");
    expect(noticeText("something_new")).toBeNull();
  });

  it("reports when the floating queue bar occupies the top-center slot", () => {
    expect(isQueueBarVisible(IDLE_QUEUE)).toBe(false);
    expect(isQueueBarVisible(at({ connection: "online", phase: "queued", mode: "2v2" }))).toBe(
      true,
    );
    expect(isQueueBarVisible(at({ connection: "ended", ended: "lost" }))).toBe(true);
    expect(isQueueBarVisible(at({ phase: "idle", notice: "declined" }))).toBe(true);
    expect(isQueueBarVisible(at({ phase: "idle", notice: "cancelled" }))).toBe(false);
    expect(isQueueBarVisible(at({ phase: "ready", mode: "2v2" }))).toBe(false);
    expect(isQueueBarVisible(at({ phase: "idle", notice: "kicked" }))).toBe(true);
  });
});

describe("custom lobbies", () => {
  const LOBBY_WIRE = {
    type: "lobby",
    code: "M9Q3RT",
    mode: "2v2",
    settings: { version: 1, map: "colosseum", round_seconds: 600, allow_spectators: true },
    spectator_slots: 4,
    you: 1,
    host: true,
    can_start: false,
    members: [
      { id: 1, slot: "team_a", host: true, you: true },
      { id: 2, slot: "team_b", host: false, you: false },
    ],
  };

  it("parses valid custom lobby messages and rejects malformed settings or slots", () => {
    const parsed = parseQueueMessage(JSON.stringify(LOBBY_WIRE));
    expect(parsed).toEqual({
      type: "lobby",
      lobby: {
        code: "M9Q3RT",
        mode: "2v2",
        settings: { version: 1, map: "colosseum", roundSeconds: 600, allowSpectators: true },
        spectatorSlots: 4,
        you: 1,
        host: true,
        canStart: false,
        members: [
          { id: 1, slot: "team_a", host: true, you: true },
          { id: 2, slot: "team_b", host: false, you: false },
        ],
      },
    });
    expect(parseQueueMessage('{"type":"lobby_left","reason":"kicked"}')).toEqual({
      type: "lobby_left",
      reason: "kicked",
    });
    expect(parseQueueMessage(JSON.stringify({ ...MATCH, seat: 0, spectator: true }))).toMatchObject(
      {
        type: "match",
        spectator: true,
        ticket: { spectator: true },
      },
    );
    for (const bad of [
      { ...LOBBY_WIRE, settings: { version: 2, map: "colosseum", round_seconds: 600 } },
      { ...LOBBY_WIRE, settings: { version: 1, map: "../hack", round_seconds: 600 } },
      { ...LOBBY_WIRE, settings: { version: 1, map: "colosseum", round_seconds: 123 } },
      {
        ...LOBBY_WIRE,
        settings: { version: 1, map: "colosseum", round_seconds: 600, allow_spectators: "yes" },
      },
      { ...LOBBY_WIRE, members: [] },
      { ...LOBBY_WIRE, members: [{ id: 1, slot: "team_c", host: true, you: true }] },
      { type: "lobby_left", reason: "other" },
    ]) {
      expect(parseQueueMessage(JSON.stringify(bad))).toBeNull();
    }
  });

  it("formats custom invite links, requests, readiness, and blocks public queues while in a lobby", () => {
    const link = customInviteLink({ origin: "https://arena.example.com", pathname: "/" }, "M9Q3RT");
    expect(link).toBe("https://arena.example.com/#custom=M9Q3RT");
    expect(parseCustomFragment("#custom=m9q3rt")).toBe("M9Q3RT");
    expect(parseCustomFragment("#custom=000000")).toBeNull();
    expect(formatRoundLimit(300)).toBe("5 min");
    expect(
      JSON.parse(
        requests.setLobbySettings({
          version: 1,
          map: "colosseum",
          roundSeconds: 300,
          allowSpectators: false,
        }),
      ),
    ).toEqual({
      type: "set_lobby_settings",
      settings: { version: 1, map: "colosseum", round_seconds: 300, allow_spectators: false },
    });

    const lobby: CustomLobby = {
      code: "M9Q3RT",
      mode: "1v1",
      settings: { version: 1, map: "colosseum", roundSeconds: 600, allowSpectators: true },
      spectatorSlots: 4,
      you: 1,
      host: true,
      canStart: false,
      members: [{ id: 1, slot: "team_a", host: true, you: true }],
    };
    expect(lobbyReadinessText(lobby)).toBe("Need 1 more on Team B to start.");
    expect(
      lobbyReadinessText({
        ...lobby,
        canStart: true,
        members: [...lobby.members, { id: 2, slot: "team_b", host: false, you: false }],
      }),
    ).toBe("Both teams are full. Ready to start.");
    expect(queueBlocked({ ...IDLE_QUEUE, lobby }, "1v1")).toContain("custom lobby");
  });
});
