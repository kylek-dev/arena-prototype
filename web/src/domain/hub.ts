// The hub (ADR 0057): a world on the world server, played like a match. The
// door only places the player and hands over a `world` ticket on the queue
// WebSocket; the page keeps whether it is in a world and why it left. Also
// the look as it goes on the wire. Pure, so it is unit tested without sockets.

import { parseAppearance, type Appearance } from "./appearance";

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

export type HubState = {
  /** Not in the hub, asked to join, or placed in a world. */
  status: "out" | "joining" | "in";
  /** The world's instance (16 hex digits) while in one. */
  instance: string | null;
  /** Why the player left the hub last (`left`, `match`, `renamed`) or a
   * join refusal. */
  notice: string | null;
};

export const OUT_OF_HUB: HubState = { status: "out", instance: null, notice: null };

/** Refusals that answer a hub request. */
export const HUB_REFUSALS = new Set([
  "in_hub",
  "not_in_hub",
  "hub_full",
  "not_friends",
  "hub_unavailable",
]);

export const hubRequests = {
  join: (friend?: string) =>
    JSON.stringify(friend ? { type: "world_join", friend } : { type: "world_join" }),
  leave: () => JSON.stringify({ type: "world_leave" }),
};

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
    case "hub_unavailable":
      return "The hub is not answering. Try again in a moment.";
    case "no_name":
      return "Choose your name on the launcher page before entering the hub.";
    case "rate_limited":
      return "Too many requests. Wait a moment and try again.";
    case "lost":
      return "The hub cannot be reached.";
    default:
      return null;
  }
}
