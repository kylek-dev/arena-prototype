// The social hub (ADR 0056): the door's hub messages on the queue WebSocket,
// the hub as the page keeps it, and the intents it sends. Pure, so it is
// unit tested without sockets. The door integrates movement from intents;
// the page only predicts its own and shows everyone else.

import { parseAppearance, type Appearance } from "./appearance";
import { isHandle } from "./handles";

/** Most players in one instance, as the door holds them. */
export const MAX_HUB_PLAYERS = 40;
/** Movement axes run −1000..1000, normalized as in the arena. */
export const AXIS_LIMIT = 1000;
/** The page sends at most this many intents a second (the door allows 20). */
export const HUB_MOVES_PER_SECOND = 10;

const WIRE_ID = /^[A-Za-z0-9_-]{1,32}$/;
const MAX_GROUPS = 24;
const MAX_VARIANTS = 8;

/** `WireAppearance` as JSON: identifiers only, groups and variants as pairs. */
export type WireAppearanceJson = {
  ancestry: string;
  body: string;
  skin_tone: string;
  hair_color: string;
  facial_hair_color: string;
  eyes: string;
  build: number;
  groups: [string, string][];
  variants: [string, string][];
};

/** The wire form of a look (as the match protocol's `Hello` sends it):
 * identifiers only, sorted, bounded; `null` when a required one is not an
 * identifier. */
export function wireAppearance(look: Appearance): WireAppearanceJson | null {
  const required = [
    look.ancestry,
    look.body,
    look.skinTone,
    look.hairColor,
    look.facialHairColor,
    look.eyes,
  ];
  if (!required.every((id) => typeof id === "string" && WIRE_ID.test(id))) return null;
  const pairs = (entries: Record<string, unknown>, limit: number) =>
    Object.entries(entries)
      .filter(
        (pair): pair is [string, string] =>
          typeof pair[1] === "string" && WIRE_ID.test(pair[0]) && WIRE_ID.test(pair[1]),
      )
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .slice(0, limit);
  return {
    ancestry: look.ancestry,
    body: look.body,
    skin_tone: look.skinTone,
    hair_color: look.hairColor,
    facial_hair_color: look.facialHairColor,
    eyes: look.eyes,
    build: Number.isFinite(look.build)
      ? Math.max(0, Math.min(100, Math.round(look.build * 100)))
      : 0,
    groups: pairs(look.groups, MAX_GROUPS),
    variants: pairs(look.variants, MAX_VARIANTS),
  };
}

/** A relayed wire look checked strictly; `null` when malformed. */
export function parseWireAppearance(value: unknown): WireAppearanceJson | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  const ids = ["ancestry", "body", "skin_tone", "hair_color", "facial_hair_color", "eyes"];
  if (!ids.every((key) => typeof v[key] === "string" && WIRE_ID.test(v[key] as string)))
    return null;
  if (!Number.isInteger(v.build) || (v.build as number) < 0 || (v.build as number) > 100)
    return null;
  const pairs = (raw: unknown, limit: number) =>
    Array.isArray(raw) &&
    raw.length <= limit &&
    raw.every(
      (pair) =>
        Array.isArray(pair) &&
        pair.length === 2 &&
        pair.every((id) => typeof id === "string" && WIRE_ID.test(id)),
    );
  if (!pairs(v.groups, MAX_GROUPS) || !pairs(v.variants, MAX_VARIANTS)) return null;
  return v as unknown as WireAppearanceJson;
}

/** The page's look from a wire look (as the client's `appearances_json`
 * builds it), checked as a saved look is; `null` when it is not one. */
export function appearanceFromWire(wire: WireAppearanceJson): Appearance | null {
  return parseAppearance({
    version: 1,
    ancestry: wire.ancestry,
    body: wire.body,
    skinTone: wire.skin_tone,
    hairColor: wire.hair_color,
    facialHairColor: wire.facial_hair_color,
    eyes: wire.eyes,
    build: wire.build / 100,
    groups: Object.fromEntries(wire.groups),
    variants: Object.fromEntries(wire.variants),
  });
}

export type HubPlayer = {
  id: number;
  handle: string;
  admin: boolean;
  appearance: WireAppearanceJson;
  /** Place in millimetres and facing (a binary angle, 65536 per turn). */
  x: number;
  z: number;
  facing: number;
};

export type HubState = {
  /** Not in the hub, asked to join, or in an instance. */
  status: "out" | "joining" | "in";
  instance: number | null;
  /** This player's hub id. */
  you: number | null;
  /** This player's place, as the door last sent it (millimetres). */
  x: number;
  z: number;
  /** The last intent the door applied, and its hub tick. */
  ack: number;
  tick: number;
  /** Everyone in view, by hub id; only players whose name and look arrived. */
  players: ReadonlyMap<number, HubPlayer>;
  /** Why the player left the hub last (`left`, `match`) or a join refusal. */
  notice: string | null;
};

export const OUT_OF_HUB: HubState = {
  status: "out",
  instance: null,
  you: null,
  x: 0,
  z: 0,
  ack: 0,
  tick: 0,
  players: new Map(),
  notice: null,
};

type Entered = { id: number; handle: string; admin: boolean; appearance: WireAppearanceJson };
export type HubMessage =
  | { type: "hub"; instance: number; you: number; x: number; z: number }
  | {
      type: "hub_state";
      tick: number;
      ack: number;
      x: number;
      z: number;
      players: [number, number, number, number][];
      enter: Entered[];
      exit: number[];
    }
  | { type: "hub_left"; reason: "left" | "match" };

/** Refusals that answer a hub request. */
export const HUB_REFUSALS = new Set(["in_hub", "not_in_hub", "hub_full", "not_friends"]);

const int = (value: unknown, min: number, max: number): value is number =>
  Number.isInteger(value) && (value as number) >= min && (value as number) <= max;
const COORD = 2 ** 31 - 1;
const U32 = 2 ** 32 - 1;

/** A hub message from the door, checked strictly; `null` for anything else. */
export function parseHubMessage(raw: unknown): HubMessage | null {
  let v: Record<string, unknown>;
  try {
    v = typeof raw === "string" ? JSON.parse(raw) : (raw as Record<string, unknown>);
  } catch {
    return null;
  }
  if (!v || typeof v !== "object") return null;
  switch (v.type) {
    case "hub":
      return int(v.instance, 1, U32) &&
        int(v.you, 1, U32) &&
        int(v.x, -COORD, COORD) &&
        int(v.z, -COORD, COORD)
        ? { type: "hub", instance: v.instance, you: v.you, x: v.x, z: v.z }
        : null;
    case "hub_state": {
      if (
        !int(v.tick, 0, Number.MAX_SAFE_INTEGER) ||
        !int(v.ack, 0, U32) ||
        !int(v.x, -COORD, COORD) ||
        !int(v.z, -COORD, COORD) ||
        !Array.isArray(v.players) ||
        v.players.length >= MAX_HUB_PLAYERS
      )
        return null;
      const players = v.players as unknown[];
      if (
        !players.every(
          (p) =>
            Array.isArray(p) &&
            p.length === 4 &&
            int(p[0], 1, U32) &&
            int(p[1], -COORD, COORD) &&
            int(p[2], -COORD, COORD) &&
            int(p[3], 0, 65535),
        )
      )
        return null;
      const ids = players.map((p) => (p as number[])[0]);
      if (new Set(ids).size !== ids.length) return null;
      if (!Array.isArray(v.enter ?? []) || !Array.isArray(v.exit ?? [])) return null;
      const enter: Entered[] = [];
      for (const e of (v.enter ?? []) as unknown[]) {
        const entry = e as Record<string, unknown>;
        const appearance = parseWireAppearance(entry?.appearance);
        if (
          !int(entry?.id, 1, U32) ||
          !isHandle(entry.handle) ||
          typeof entry.admin !== "boolean" ||
          !appearance
        )
          return null;
        enter.push({ id: entry.id, handle: entry.handle, admin: entry.admin, appearance });
      }
      const exit = (v.exit ?? []) as unknown[];
      if (!exit.every((id) => int(id, 1, U32))) return null;
      return {
        type: "hub_state",
        tick: v.tick,
        ack: v.ack,
        x: v.x,
        z: v.z,
        players: players as [number, number, number, number][],
        enter,
        exit: exit as number[],
      };
    }
    case "hub_left":
      return v.reason === "left" || v.reason === "match"
        ? { type: "hub_left", reason: v.reason }
        : null;
    default:
      return null;
  }
}

/** The hub after a door message. */
export function applyHubMessage(state: HubState, message: HubMessage): HubState {
  switch (message.type) {
    case "hub":
      return {
        ...OUT_OF_HUB,
        status: "in",
        instance: message.instance,
        you: message.you,
        x: message.x,
        z: message.z,
      };
    case "hub_left":
      return { ...OUT_OF_HUB, notice: message.reason };
    case "hub_state": {
      if (state.status !== "in") return state;
      const known = new Map(state.players);
      for (const id of message.exit) known.delete(id);
      const entered = new Map(message.enter.map((entry) => [entry.id, entry]));
      const players = new Map<number, HubPlayer>();
      for (const [id, x, z, facing] of message.players) {
        const who = entered.get(id) ?? known.get(id);
        // A player whose name and look never arrived waits for them.
        if (who)
          players.set(id, {
            id,
            handle: who.handle,
            admin: who.admin,
            appearance: who.appearance,
            x,
            z,
            facing,
          });
      }
      return {
        ...state,
        tick: message.tick,
        ack: message.ack,
        x: message.x,
        z: message.z,
        players,
      };
    }
  }
}

export const hubRequests = {
  join: (appearance: WireAppearanceJson, friend?: string) =>
    JSON.stringify(
      friend ? { type: "hub_join", appearance, friend } : { type: "hub_join", appearance },
    ),
  move: (seq: number, x: number, z: number, facing: number) =>
    JSON.stringify({ type: "hub_move", seq, x, z, facing }),
  leave: () => JSON.stringify({ type: "hub_leave" }),
};

/** A movement intent clamped into what the door accepts. */
export function clampIntent(x: number, z: number, facing: number) {
  const axis = (value: number) =>
    Number.isFinite(value) ? Math.max(-AXIS_LIMIT, Math.min(AXIS_LIMIT, Math.round(value))) : 0;
  const turn = Number.isFinite(facing) ? ((Math.round(facing) % 65536) + 65536) % 65536 : 0;
  return { x: axis(x), z: axis(z), facing: turn };
}

/** Words for a hub refusal or departure. */
export function hubNoticeText(code: string | null): string | null {
  switch (code) {
    case "hub_full":
      return "That hub is full. Try again in a moment.";
    case "not_friends":
      return "You can join only a friend or a party member.";
    case "not_in_hub":
      return "That player is not in the hub.";
    case "in_hub":
      return "You are already in the hub.";
    case "no_name":
      return "Choose your name on the launcher page before entering the hub.";
    case "rate_limited":
      return "Too many requests. Wait a moment and try again.";
    case "bad_request":
      return "Your character could not be sent. Check it in the customizer.";
    case "lost":
      return "The hub cannot be reached.";
    default:
      return null;
  }
}
