import { describe, expect, it } from "vitest";
import { defaultAppearance } from "../../web/src/domain/appearance";
import {
  OUT_OF_HUB,
  appearanceFromWire,
  applyHubMessage,
  clampIntent,
  hubNoticeText,
  hubRequests,
  parseHubMessage,
  wireAppearance,
  type HubState,
} from "../../web/src/domain/hub";

const LOOK = {
  ancestry: "human",
  body: "male",
  skin_tone: "s1",
  hair_color: "h1",
  facial_hair_color: "h1",
  eyes: "e1",
  build: 50,
  groups: [["hair", "bob"]],
  variants: [],
};
const state = (patch: object) =>
  JSON.stringify({
    type: "hub_state",
    tick: 1,
    ack: 0,
    x: 0,
    z: 0,
    y: 0,
    vy: 0,
    mx: 0,
    mz: 0,
    players: [],
    ...patch,
  });

describe("hub messages", () => {
  it("parses the door's messages strictly", () => {
    expect(parseHubMessage('{"type":"hub","instance":2,"you":7,"x":-5250,"z":24000}')).toEqual({
      type: "hub",
      instance: 2,
      you: 7,
      x: -5250,
      z: 24000,
    });
    expect(parseHubMessage('{"type":"hub_left","reason":"match"}')).toEqual({
      type: "hub_left",
      reason: "match",
    });
    const entered = parseHubMessage(
      state({
        players: [[3, 10, 20, 900, 0]],
        enter: [{ id: 3, handle: "Ash#1000", admin: true, appearance: LOOK }],
        exit: [4],
      }),
    );
    expect(entered?.type === "hub_state" && entered.enter[0].handle).toBe("Ash#1000");
    for (const bad of [
      '{"type":"hub","instance":0,"you":1,"x":0,"z":0}',
      '{"type":"hub_left","reason":"kicked"}',
      state({ players: [[3, 1.5, 0, 0]] }),
      state({ players: [[3, 0, 0, 70000, 0]] }),
      state({ players: Array.from({ length: 40 }, (_, i) => [i + 1, 0, 0, 0]) }),
      state({ enter: [{ id: 3, handle: "nobody", admin: false, appearance: LOOK }] }),
      state({
        enter: [{ id: 3, handle: "Ash#1000", admin: false, appearance: { ...LOOK, body: "../x" } }],
      }),
      state({
        enter: [{ id: 3, handle: "Ash#1000", admin: false, appearance: { ...LOOK, build: 101 } }],
      }),
      state({ exit: [0] }),
      state({
        players: [
          [3, 0, 0, 0, 0],
          [3, 1, 1, 0, 0],
        ],
      }),
      state({ enter: 5 }),
      "not json",
      '{"type":"party"}',
    ])
      expect(parseHubMessage(bad), bad).toBeNull();
  });

  it("keeps who is in view, with names and looks from their arrival", () => {
    let hub: HubState = { ...OUT_OF_HUB, status: "joining" };
    const apply = (raw: string) => {
      hub = applyHubMessage(hub, parseHubMessage(raw)!);
    };
    apply('{"type":"hub","instance":1,"you":1,"x":0,"z":24000}');
    expect(hub.status).toBe("in");
    apply(
      state({
        players: [[2, 1000, 24000, 0, 0]],
        enter: [{ id: 2, handle: "Ash#1000", admin: false, appearance: LOOK }],
      }),
    );
    expect(hub.players.get(2)?.handle).toBe("Ash#1000");
    apply(state({ tick: 2, ack: 3, x: 720, players: [[2, 1720, 24000, 16384, 0]] }));
    expect(hub.players.get(2)).toMatchObject({ x: 1720, facing: 16384 });
    expect([hub.x, hub.ack]).toEqual([720, 3]);
    // Someone never introduced waits; an exit forgets.
    apply(
      state({
        tick: 3,
        players: [
          [2, 1720, 24000, 0, 0],
          [9, 0, 0, 0, 0],
        ],
      }),
    );
    expect([...hub.players.keys()]).toEqual([2]);
    // A rename comes again under the same id.
    apply(
      state({
        tick: 4,
        players: [[2, 1720, 24000, 0, 0]],
        enter: [{ id: 2, handle: "Ash#2000", admin: true, appearance: LOOK }],
      }),
    );
    expect(hub.players.get(2)).toMatchObject({ handle: "Ash#2000", admin: true });
    // Back under a new id with a new look: the old one goes.
    apply(
      state({
        tick: 5,
        players: [[5, 0, 0, 0, 0]],
        enter: [
          { id: 5, handle: "Ash#2000", admin: true, appearance: { ...LOOK, body: "female" } },
        ],
        exit: [2],
      }),
    );
    expect([...hub.players.keys()]).toEqual([5]);
    expect(hub.players.get(5)?.appearance.body).toBe("female");
    apply(state({ tick: 6, exit: [5] }));
    expect(hub.players.size).toBe(0);
    apply('{"type":"hub_left","reason":"match"}');
    expect(hub).toEqual({ ...OUT_OF_HUB, notice: "match" });
    // States after leaving change nothing.
    expect(applyHubMessage(hub, parseHubMessage(state({ tick: 9 }))!)).toBe(hub);
  });
});

describe("hub looks and intents", () => {
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
    expect(JSON.parse(hubRequests.join(wire, "Ash#1000"))).toMatchObject({
      type: "hub_join",
      friend: "Ash#1000",
    });
  });

  it("carries jumps: heights in view, this player's body, and the jump flag", () => {
    let hub: HubState = applyHubMessage(
      { ...OUT_OF_HUB, status: "joining" },
      parseHubMessage('{"type":"hub","instance":1,"you":1,"x":0,"z":0}')!,
    );
    hub = applyHubMessage(
      hub,
      parseHubMessage(
        state({
          y: 220,
          vy: 160,
          mx: 720,
          mz: 0,
          players: [[2, 0, 0, 0, 640]],
          enter: [{ id: 2, handle: "Ash#1000", admin: false, appearance: LOOK }],
        }),
      )!,
    );
    expect(hub.body).toEqual({ y: 220, vy: 160, mx: 720, mz: 0 });
    expect(hub.players.get(2)?.height).toBe(640);
    // Below the plaza, or a player entry without a height, is malformed.
    expect(parseHubMessage(state({ y: -1 }))).toBeNull();
    expect(parseHubMessage(state({ players: [[2, 0, 0, 0]] }))).toBeNull();
    expect(JSON.parse(hubRequests.move(3, 0, 0, 0, true))).toEqual({
      type: "hub_move",
      seq: 3,
      x: 0,
      z: 0,
      facing: 0,
      jump: true,
    });
    expect(JSON.parse(hubRequests.move(4, 0, 0, 0))).not.toHaveProperty("jump");
  });

  it("clamps intents into what the door accepts", () => {
    expect(clampIntent(1500, -2000.4, -1)).toEqual({ x: 1000, z: -1000, facing: 65535 });
    expect(clampIntent(Number.NaN, 3.6, 65536 + 5)).toEqual({ x: 0, z: 4, facing: 5 });
    expect(hubNoticeText("hub_full")).toMatch(/full/);
    expect(hubNoticeText("left")).toBeNull();
    expect(hubNoticeText("lost")).toBe("The hub cannot be reached.");
    expect(hubNoticeText("no_name")).toMatch(/Choose your name/);
  });
});
