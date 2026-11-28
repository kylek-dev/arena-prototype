import { describe, expect, it } from "vitest";
import { defaultAppearance } from "../../web/src/domain/appearance";
import {
  HUB_REFUSALS,
  OUT_OF_HUB,
  appearanceFromWire,
  hubNoticeText,
  hubRequests,
  wireAppearance,
} from "../../web/src/domain/hub";
import { parseQueueMessage } from "../../web/src/domain/matchmaking";
import { isMemberRelay } from "../../web/src/domain/network";

const TICKET = "ab".repeat(40);

describe("hub worlds through the door (ADR 0057)", () => {
  it("asks for a place and leaves with the door's messages", () => {
    expect(JSON.parse(hubRequests.join())).toEqual({ type: "world_join" });
    expect(JSON.parse(hubRequests.join("Ash#1000"))).toEqual({
      type: "world_join",
      friend: "Ash#1000",
    });
    expect(JSON.parse(hubRequests.leave())).toEqual({ type: "world_leave" });
    expect(OUT_OF_HUB).toEqual({ status: "out", instance: null, notice: null });
  });

  it("reads a world ticket and departures strictly", () => {
    const world = (patch: object) =>
      parseQueueMessage(
        JSON.stringify({
          type: "world",
          instance: "00000000000000ab",
          server: "wss://example.test/play",
          ticket: TICKET,
          expires: 2_000_000_000,
          ...patch,
        }),
      );
    expect(world({})).toEqual({
      type: "world",
      instance: "00000000000000ab",
      ticket: {
        server: "wss://example.test/play",
        ticket: TICKET,
        expires: 2_000_000_000,
        world: true,
      },
    });
    expect(world({ instance: "AB" })).toBeNull();
    expect(world({ instance: "00000000000000AB" })).toBeNull();
    expect(world({ server: "http://example.test/play" })).toBeNull();
    expect(world({ ticket: "xyz" })).toBeNull();
    for (const reason of ["left", "match", "renamed"]) {
      expect(parseQueueMessage(JSON.stringify({ type: "hub_left", reason }))).toEqual({
        type: "hub_left",
        reason,
      });
    }
    expect(parseQueueMessage(JSON.stringify({ type: "hub_left", reason: "x" }))).toBeNull();
    // The door's own hub messages are gone.
    expect(parseQueueMessage(JSON.stringify({ type: "hub_state", tick: 1 }))).toBeNull();
  });

  it("keeps member relays from being dropped while the engine is paused", () => {
    expect(isMemberRelay(new Uint8Array([7, 1]))).toBe(true);
    expect(isMemberRelay(new Uint8Array([8, 1]))).toBe(true);
    expect(isMemberRelay(new Uint8Array([1, 1]))).toBe(false);
    expect(isMemberRelay(new Uint8Array([]))).toBe(false);
  });

  it("explains refusals, including the world server not answering", () => {
    for (const code of HUB_REFUSALS) expect(hubNoticeText(code)).toBeTruthy();
    expect(hubNoticeText("hub_unavailable")).toMatch(/not answering/);
    expect(hubNoticeText("left")).toBeNull();
  });

  it("sends looks as the match protocol does, and reads them back", () => {
    const wire = wireAppearance({ ...defaultAppearance, build: 0.42 })!;
    expect(wire.build).toBe(42);
    expect(wire.groups.map(([group]) => group)).toEqual(
      [...wire.groups.map(([group]) => group)].sort(),
    );
    expect(wireAppearance({ ...defaultAppearance, body: "../male" })).toBeNull();
    const back = appearanceFromWire(wire)!;
    expect(back.body).toBe(defaultAppearance.body);
    expect(back.build).toBeCloseTo(0.42);
  });
});
