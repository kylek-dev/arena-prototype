// `Name#number` handles relayed by the match server (social-identity step 3):
// one per local actor, set by the front door and signed into tickets, never
// a client's claim. Parsing is strict so a malformed relay shows nothing.

import type { Snapshot } from "../bridge/snapshot";

/** A handle as the server writes it: 3–16 ASCII letters and digits starting
 * with a letter, then `#` and four digits (0001–9999). */
const HANDLE = /^[A-Za-z][A-Za-z0-9]{2,15}#(?!0000)\d{4}$/;
/** Most fighters a match has (3v3). */
const MAX_FIGHTERS = 6;

export function isHandle(value: unknown): value is string {
  return typeof value === "string" && HANDLE.test(value);
}

/** The relay from `onHandles`: `null` outside a match or when malformed;
 * otherwise one handle per actor, or `null` for a seat not joined yet or
 * joined by a lobby link. Guest handles (`Guest#…`) may repeat. */
export function parseHandles(raw: unknown): (string | null)[] | null {
  if (!Array.isArray(raw) || raw.length < 2 || raw.length > MAX_FIGHTERS || raw.length % 2)
    return null;
  return raw.every((entry) => entry === null || isHandle(entry))
    ? (raw as (string | null)[])
    : null;
}

export type SeatTags = { admin: boolean };

/** The tags relay from `onTags`: `null` outside a match, when no seat has a
 * tag, or when malformed; otherwise `{admin}` per actor. Set by the server
 * from deploy configuration, never by a client. */
export function parseTags(raw: unknown): SeatTags[] | null {
  if (!Array.isArray(raw) || raw.length < 2 || raw.length > MAX_FIGHTERS || raw.length % 2)
    return null;
  const valid = raw.every(
    (entry) =>
      entry !== null &&
      typeof entry === "object" &&
      Object.keys(entry).length === 1 &&
      typeof (entry as SeatTags).admin === "boolean",
  );
  return valid ? (raw as SeatTags[]).map((entry) => ({ admin: entry.admin })) : null;
}

/** The name part of a handle, for tight spaces such as nameplates. */
export function handleName(handle: string) {
  return handle.slice(0, handle.indexOf("#"));
}

/** Names a network match's fighters by the relayed handles: the handle in
 * full (`name`) and kept as `handle` for tight spaces. A seat without one
 * (not joined yet, or joined by a lobby link) keeps its role name, such as
 * "Opponent 2" or "Team A 1", which stays the same for this viewer, unlike a
 * number per local actor. Practice, a missing relay, or one that does not
 * fit the roster changes nothing. */
export function withHandles(
  snapshot: Snapshot,
  handles: readonly (string | null)[] | null,
): Snapshot {
  const roster = snapshot.roster;
  if (!snapshot.networked || !roster || !handles || handles.length !== roster.length)
    return snapshot;
  const named = roster.map((unit, actor) => {
    const handle = handles[actor];
    return handle ? { ...unit, name: handle, handle } : unit;
  });
  return {
    ...snapshot,
    roster: named,
    player: { ...snapshot.player, name: named[0].name, handle: named[0].handle },
    target: { ...snapshot.target, name: named[1].name, handle: named[1].handle },
  };
}
