import { describe, expect, it } from "vitest";
import {
  IDLE_QUEUE,
  QUEUE_MODES,
  formatWait,
  guestId,
  inviteLink,
  isGuestId,
  noticeText,
  normalizeCode,
  parsePartyFragment,
  parseQueueMessage,
  queueBlocked,
  queueMessage,
  queueUrl,
  requests,
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

describe("queue messages", () => {
  it("parses what the door sends and nothing else", () => {
    expect(
      parseQueueMessage('{"type":"party","code":"K7P2QX","members":2,"leader":true,"queue":"3v3"}'),
    ).toEqual({
      type: "party",
      party: { code: "K7P2QX", members: 2, leader: true },
      queue: "3v3",
    });
    expect(
      parseQueueMessage('{"type":"party","code":null,"members":1,"leader":true,"queue":null}'),
    ).toMatchObject({ party: { code: null }, queue: null });
    expect(parseQueueMessage('{"type":"queued","mode":"solo-2v2","waiting":2}')).toEqual({
      type: "queued",
      mode: "solo-2v2",
      waiting: 2,
    });
    expect(parseQueueMessage(JSON.stringify(MATCH))).toEqual({
      type: "match",
      mode: "2v2",
      matchId: "00ff00ff00ff00ff",
      seat: 3,
      ticket: { server: "wss://arena.example.com/play", ticket: "ab01", expires: 2_000_000_000 },
    });
    expect(parseQueueMessage('{"type":"stopped","reason":"member_left"}')).toEqual({
      type: "stopped",
      reason: "member_left",
    });
    expect(parseQueueMessage('{"type":"refused","code":"party_full"}')).toEqual({
      type: "refused",
      code: "party_full",
    });
    for (const bad of [
      "not json",
      "null",
      '{"type":"queued","mode":"4v4","waiting":1}',
      '{"type":"party","code":"0OIL11","members":2,"leader":true,"queue":null}',
      '{"type":"party","code":null,"members":4,"leader":true,"queue":null}',
      '{"type":"party","code":null,"members":1,"leader":"yes","queue":null}',
      '{"type":"teleport"}',
      JSON.stringify({ ...MATCH, server: "https://arena.example.com/play" }),
      JSON.stringify({ ...MATCH, ticket: "not hex" }),
      JSON.stringify({ ...MATCH, match_id: "12" }),
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
    expect(JSON.parse(requests.queue("3v3"))).toEqual({ type: "queue", mode: "3v3" });
    expect(JSON.parse(requests.joinParty("K7P2QX"))).toEqual({
      type: "join_party",
      code: "K7P2QX",
    });
    expect(QUEUE_MODES.map((mode) => mode.label)).toEqual([
      "1v1",
      "2v2",
      "3v3",
      "Solo 2v2",
      "Solo 3v3",
    ]);
  });

  it("finds the queue on the page's origin and invites in the fragment", () => {
    expect(queueUrl({ protocol: "https:", host: "arena.example.com" })).toBe(
      "wss://arena.example.com/api/queue",
    );
    expect(queueUrl({ protocol: "http:", host: "127.0.0.1:8000" })).toBe(
      "ws://127.0.0.1:8000/api/queue",
    );
    const link = inviteLink({ origin: "https://arena.example.com", pathname: "/" }, "K7P2QX");
    expect(link).toBe("https://arena.example.com/#party=K7P2QX");
    expect(parsePartyFragment("#party=k7p2qx")).toBe("K7P2QX");
    expect(parsePartyFragment("#party=nope")).toBeNull();
    expect(parsePartyFragment("#server=x&ticket=ab")).toBeNull();
    expect(normalizeCode(" k7p2qx ")).toBe("K7P2QX");
    expect(normalizeCode("K7P2Q")).toBeNull();
  });

  it("makes 32-hex-digit guest ids", () => {
    const id = guestId(new Uint8Array(16).fill(0xab));
    expect(id).toBe("ab".repeat(16));
    expect(isGuestId(id)).toBe(true);
    expect(isGuestId("AB".repeat(16))).toBe(false);
    expect(isGuestId(null)).toBe(false);
  });
});

describe("queue wording", () => {
  const at = (patch: Partial<QueueState>): QueueState => ({ ...IDLE_QUEUE, ...patch });

  it("formats waits as minutes and seconds", () => {
    expect(formatWait(0)).toBe("0:00");
    expect(formatWait(65_400)).toBe("1:05");
    expect(formatWait(3_725_000)).toBe("1:02:05");
    expect(formatWait(-5)).toBe("0:00");
  });

  it("says what the search is doing in the banner", () => {
    expect(queueMessage(IDLE_QUEUE, 0)).toBeNull();
    expect(queueMessage(at({ connection: "online" }), 0)).toBeNull();
    expect(queueMessage(at({ connection: "connecting", mode: "1v1" }), 0)).toMatchObject({
      text: "Joining the 1v1 queue…",
      cancel: true,
    });
    expect(
      queueMessage(
        at({ connection: "online", phase: "queued", mode: "2v2", since: 1000, waiting: 3 }),
        43_000,
      ),
    ).toEqual({
      text: "Searching for a 2v2 match · 0:42 · 3 players queued",
      tone: "info",
      cancel: true,
    });
    expect(queueMessage(at({ phase: "found", mode: "1v1" }), 0)).toMatchObject({ cancel: false });
    expect(queueMessage(at({ connection: "ended", ended: "unreachable" }), 0)).toMatchObject({
      tone: "warning",
      text: expect.stringContaining("Could not reach"),
    });
  });

  it("explains why a queue is closed to a party", () => {
    const party = (members: number, leader: boolean) =>
      at({ party: { code: "K7P2QX", members, leader } });
    expect(queueBlocked(IDLE_QUEUE, "solo-3v3")).toBeNull();
    expect(queueBlocked(party(2, true), "2v2")).toBeNull();
    expect(queueBlocked(party(2, true), "solo-2v2")).toBe("Everyone queues alone here.");
    expect(queueBlocked(party(3, true), "2v2")).toBe("Parties of up to 2 only.");
    expect(queueBlocked(party(2, false), "2v2")).toContain("leader");
    expect(noticeText("member_left")).toContain("left");
    expect(noticeText("something_new")).toBeNull();
  });
});
