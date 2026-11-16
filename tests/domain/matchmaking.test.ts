import { describe, expect, it } from "vitest";
import {
  IDLE_QUEUE,
  QUEUE_MODES,
  formatWait,
  guestId,
  isGuestId,
  joinMessage,
  parseQueueMessage,
  queueMessage,
  queueUrl,
  type QueueState,
} from "../../web/src/domain/matchmaking";

const MATCH = {
  type: "match",
  mode: "solo-2v2",
  match_id: "00ff00ff00ff00ff",
  seat: 3,
  server: "wss://arena.example.com/play",
  ticket: "ab01",
  expires: 2_000_000_000,
};

describe("queue messages", () => {
  it("parses what the door sends and nothing else", () => {
    expect(parseQueueMessage('{"type":"queued","mode":"1v1","waiting":2}')).toEqual({
      type: "queued",
      mode: "1v1",
      waiting: 2,
    });
    expect(parseQueueMessage('{"type":"found","mode":"solo-3v3"}')).toEqual({
      type: "found",
      mode: "solo-3v3",
    });
    expect(parseQueueMessage(JSON.stringify(MATCH))).toEqual({
      type: "match",
      mode: "solo-2v2",
      matchId: "00ff00ff00ff00ff",
      seat: 3,
      ticket: { server: "wss://arena.example.com/play", ticket: "ab01", expires: 2_000_000_000 },
    });
    expect(parseQueueMessage('{"type":"error","code":"not_ready","message":"…"}')).toEqual({
      type: "error",
      code: "not_ready",
    });
    for (const bad of [
      "not json",
      "null",
      '{"type":"queued","mode":"2v2","waiting":1}',
      '{"type":"queued","mode":"1v1","waiting":-1}',
      '{"type":"teleport"}',
      JSON.stringify({ ...MATCH, server: "https://arena.example.com/play" }),
      JSON.stringify({ ...MATCH, ticket: "not hex" }),
      JSON.stringify({ ...MATCH, match_id: "12" }),
      JSON.stringify({ ...MATCH, seat: 1.5 }),
      new Uint8Array([1, 2]),
      "x".repeat(5000),
    ])
      expect(parseQueueMessage(bad), String(bad).slice(0, 40)).toBeNull();
  });

  it("joins with the mode and guest id", () => {
    expect(JSON.parse(joinMessage("solo-3v3", "0".repeat(32)))).toEqual({
      type: "join",
      mode: "solo-3v3",
      guest: "0".repeat(32),
    });
    expect(QUEUE_MODES.map((mode) => mode.label)).toEqual(["1v1", "Solo 2v2", "Solo 3v3"]);
  });

  it("finds the queue on the page's own origin", () => {
    expect(queueUrl({ protocol: "https:", host: "arena.example.com" })).toBe(
      "wss://arena.example.com/api/queue",
    );
    expect(queueUrl({ protocol: "http:", host: "127.0.0.1:8000" })).toBe(
      "ws://127.0.0.1:8000/api/queue",
    );
  });

  it("makes 32-hex-digit guest ids", () => {
    const id = guestId(new Uint8Array(16).fill(0xab));
    expect(id).toBe("ab".repeat(16));
    expect(isGuestId(id)).toBe(true);
    expect(isGuestId("AB".repeat(16))).toBe(false);
    expect(isGuestId("ab".repeat(15))).toBe(false);
    expect(isGuestId(null)).toBe(false);
  });
});

describe("queue banner", () => {
  const at = (patch: Partial<QueueState>): QueueState => ({ ...IDLE_QUEUE, ...patch });

  it("formats waits as minutes and seconds", () => {
    expect(formatWait(0)).toBe("0:00");
    expect(formatWait(65_400)).toBe("1:05");
    expect(formatWait(3_725_000)).toBe("1:02:05");
    expect(formatWait(-5)).toBe("0:00");
  });

  it("says what the search is doing and offers Cancel while it can be cancelled", () => {
    expect(queueMessage(IDLE_QUEUE, 0)).toBeNull();
    expect(queueMessage(at({ phase: "connecting", mode: "1v1" }), 0)).toMatchObject({
      text: "Joining the 1v1 queue…",
      cancel: true,
    });
    expect(
      queueMessage(at({ phase: "queued", mode: "solo-2v2", since: 1000, waiting: 3 }), 43_000),
    ).toEqual({
      text: "Searching for a Solo 2v2 match · 0:42 · 3 players queued",
      tone: "info",
      cancel: true,
    });
    expect(queueMessage(at({ phase: "queued", mode: "1v1", since: 0, waiting: 1 }), 0)?.text).toBe(
      "Searching for a 1v1 match · 0:00 · 1 player queued",
    );
    expect(queueMessage(at({ phase: "found", mode: "1v1" }), 0)).toMatchObject({ cancel: false });
    expect(queueMessage(at({ phase: "ended", ended: "not_ready" }), 0)).toMatchObject({
      tone: "warning",
      text: expect.stringContaining("did not confirm"),
    });
    expect(queueMessage(at({ phase: "ended", ended: "unreachable" }), 0)?.text).toContain(
      "Could not reach",
    );
    expect(queueMessage(at({ phase: "ended", ended: "something_new" }), 0)?.text).toBe(
      "The search ended.",
    );
  });
});
