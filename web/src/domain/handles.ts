// `Name#number` handles relayed by the match server (social-identity step 3):
// one per local actor, set by the front door and signed into tickets, never
// a client's claim. Parsing is strict so a malformed relay shows nothing.

import type { Snapshot } from "../bridge/snapshot";

/** A handle as the server writes it: 3–16 ASCII letters and digits starting
 * with a letter, then `#` and four digits (0001–9999). */
const HANDLE = /^[A-Za-z][A-Za-z0-9]{2,15}#(?!0000)\d{4}$/;
/** Most fighters a match has (3v3). */
const MAX_FIGHTERS = 6;
/** Most fighters a hub world has (ADR 0057): relays are per engine actor. */
const MAX_WORLD_FIGHTERS = 16;
/** A relay's length: even up to 3v3 in a match, anything up to a world's. */
const relayLength = (length: number) =>
  (length >= 2 && length <= MAX_FIGHTERS && length % 2 === 0) ||
  (length >= 1 && length <= MAX_WORLD_FIGHTERS);

export function isHandle(value: unknown): value is string {
  return typeof value === "string" && HANDLE.test(value);
}

/** The relay from `onHandles`: `null` outside a match or when malformed;
 * otherwise one handle per actor, or `null` for a seat not joined yet or
 * joined by a lobby link. Guest handles (`Guest#…`) may repeat. */
export function parseHandles(raw: unknown): (string | null)[] | null {
  if (!Array.isArray(raw) || !relayLength(raw.length)) return null;
  return raw.every((entry) => entry === null || isHandle(entry))
    ? (raw as (string | null)[])
    : null;
}

export type SeatTags = { admin: boolean };

/** The tags relay from `onTags`: `null` outside a match, when no seat has a
 * tag, or when malformed; otherwise `{admin}` per actor. Set by the server
 * from deploy configuration, never by a client. */
export function parseTags(raw: unknown): SeatTags[] | null {
  if (!Array.isArray(raw) || !relayLength(raw.length)) return null;
  const valid = raw.every(
    (entry) =>
      entry !== null &&
      typeof entry === "object" &&
      Object.keys(entry).length === 1 &&
      typeof (entry as SeatTags).admin === "boolean",
  );
  return valid ? (raw as SeatTags[]).map((entry) => ({ admin: entry.admin })) : null;
}

/** Whether a per-actor relay of `length` entries fits `roster`: one entry
 * per unit in a match, and in a hub world at least up to its last actor. */
function fits(roster: readonly { actor: number }[], length: number) {
  return roster.length === length || roster.every((unit) => unit.actor < length);
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
  if (!snapshot.networked || !roster || !handles || !fits(roster, handles.length)) return snapshot;
  // Relays are per engine actor; a hub world's roster leaves vacant slots
  // out, so each unit takes its own actor's entry.
  const named = roster.map((unit) => {
    const handle = handles[unit.actor];
    return handle ? { ...unit, name: handle, handle } : unit;
  });
  const of = (actor: number) => named.find((unit) => unit.actor === actor);
  return {
    ...snapshot,
    roster: named,
    player: {
      ...snapshot.player,
      name: of(0)?.name ?? snapshot.player.name,
      handle: of(0)?.handle,
    },
    target: {
      ...snapshot.target,
      name: of(1)?.name ?? snapshot.target.name,
      handle: of(1)?.handle,
    },
  };
}

/** Marks a network match's fighters whose players carry the server's admin
 * tag (deploy configuration, never a client's claim). Like `withHandles`,
 * changes nothing outside a network match or for a relay that does not fit. */
export function withTags(snapshot: Snapshot, tags: readonly SeatTags[] | null): Snapshot {
  const roster = snapshot.roster;
  if (!snapshot.networked || !roster || !tags || !fits(roster, tags.length)) return snapshot;
  if (!tags.some((tag) => tag.admin)) return snapshot;
  const tagged = roster.map((unit) => (tags[unit.actor].admin ? { ...unit, admin: true } : unit));
  const of = (actor: number) => tagged.find((unit) => unit.actor === actor);
  return {
    ...snapshot,
    roster: tagged,
    player: { ...snapshot.player, admin: of(0)?.admin },
    target: { ...snapshot.target, admin: of(1)?.admin },
  };
}
