// Queues and parties (ADR 0046): the messages the front door's `/api/queue`
// WebSocket exchanges, the queue's state and how the interface words it.
// Pure, so it is unit tested without sockets.

import { parseTicket, type MatchTicket } from "./network";

export type QueueMode = "1v1" | "2v2" | "3v3" | "solo-2v2" | "solo-3v3";
export const QUEUE_MODES: readonly {
  id: QueueMode;
  label: string;
  /** Largest party this queue takes. */
  maxParty: number;
  hint: string;
}[] = [
  { id: "1v1", label: "1v1", maxParty: 1, hint: "One against one." },
  { id: "2v2", label: "2v2", maxParty: 2, hint: "Queue with a friend, or alone." },
  { id: "3v3", label: "3v3", maxParty: 3, hint: "Queue with up to two friends, or alone." },
  { id: "solo-2v2", label: "Solo 2v2", maxParty: 1, hint: "Everyone queues alone." },
  { id: "solo-3v3", label: "Solo 3v3", maxParty: 1, hint: "Everyone queues alone." },
];
export function isQueueMode(value: unknown): value is QueueMode {
  return QUEUE_MODES.some((mode) => mode.id === value);
}
export function modeLabel(mode: QueueMode) {
  return QUEUE_MODES.find((entry) => entry.id === mode)?.label ?? mode;
}
/** Players per party at most: one full team. */
export const MAX_PARTY = 3;

export type Party = {
  /** Invite code; `null` for a player on their own. */
  code: string | null;
  members: number;
  leader: boolean;
};

/** What the door sends. A `match` carries this player's own seat ticket. */
export type QueueMessage =
  | { type: "party"; party: Party; queue: QueueMode | null }
  | { type: "queued"; mode: QueueMode; waiting: number }
  | { type: "found"; mode: QueueMode }
  | { type: "requeued"; mode: QueueMode }
  | { type: "stopped"; reason: string }
  | { type: "refused"; code: string }
  | { type: "match"; mode: QueueMode; matchId: string; seat: number; ticket: MatchTicket }
  | { type: "error"; code: string };

const count = (value: unknown): value is number =>
  Number.isInteger(value) && (value as number) >= 0 && (value as number) < 1_000_000;
const word = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 64;

const CODE = /^[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{6}$/;
/** A typed or linked invite code, normalized; `null` when it cannot be one. */
export function normalizeCode(text: string) {
  const code = text.trim().toUpperCase();
  return CODE.test(code) ? code : null;
}

/** Parses one text frame from the door; `null` when it is not understood. */
export function parseQueueMessage(text: unknown): QueueMessage | null {
  if (typeof text !== "string" || text.length > 4096) return null;
  let v: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(text);
    if (!parsed || typeof parsed !== "object") return null;
    v = parsed as Record<string, unknown>;
  } catch {
    return null;
  }
  switch (v.type) {
    case "party": {
      // No code (on one's own), or a well-formed one; anything else is refused.
      const code =
        v.code === null ? null : typeof v.code === "string" ? normalizeCode(v.code) : null;
      if (
        (v.code !== null && code === null) ||
        !count(v.members) ||
        v.members < 1 ||
        v.members > MAX_PARTY
      )
        return null;
      if (typeof v.leader !== "boolean") return null;
      if (v.queue !== null && !isQueueMode(v.queue)) return null;
      return {
        type: "party",
        party: { code, members: v.members, leader: v.leader },
        queue: v.queue as QueueMode | null,
      };
    }
    case "queued":
      return isQueueMode(v.mode) && count(v.waiting)
        ? { type: "queued", mode: v.mode, waiting: v.waiting }
        : null;
    case "found":
    case "requeued":
      return isQueueMode(v.mode) ? { type: v.type, mode: v.mode } : null;
    case "stopped":
      return word(v.reason) ? { type: "stopped", reason: v.reason } : null;
    case "refused":
      return word(v.code) ? { type: "refused", code: v.code } : null;
    case "match": {
      const ticket = parseTicket(v);
      if (!ticket || !isQueueMode(v.mode) || !count(v.seat)) return null;
      if (typeof v.match_id !== "string" || !/^[0-9a-f]{16}$/.test(v.match_id)) return null;
      return { type: "match", mode: v.mode, matchId: v.match_id, seat: v.seat, ticket };
    }
    case "error":
      return word(v.code) ? { type: "error", code: v.code } : null;
    default:
      return null;
  }
}

/** Requests to the door. */
export const requests = {
  hello: (guest: string) => JSON.stringify({ type: "hello", guest }),
  queue: (mode: QueueMode) => JSON.stringify({ type: "queue", mode }),
  cancel: () => JSON.stringify({ type: "cancel" }),
  ready: () => JSON.stringify({ type: "ready" }),
  createParty: () => JSON.stringify({ type: "create_party" }),
  joinParty: (code: string) => JSON.stringify({ type: "join_party", code }),
  leaveParty: () => JSON.stringify({ type: "leave_party" }),
};

/** The door's queue endpoint on the page's own origin. */
export function queueUrl(location: Pick<Location, "protocol" | "host">) {
  return `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/api/queue`;
}

/** The game page with a party invite in the fragment, which no server sees. */
export function inviteLink(location: Pick<Location, "origin" | "pathname">, code: string) {
  return `${location.origin}${location.pathname}#party=${code}`;
}
/** The invite code in a page fragment such as `#party=K7P2QX`. */
export function parsePartyFragment(hash: string) {
  const code = new URLSearchParams(hash.replace(/^#/, "")).get("party");
  return code ? normalizeCode(code) : null;
}

/** A browser's guest id: 32 hex digits from 16 random bytes. */
export function guestId(bytes: Uint8Array) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
export function isGuestId(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{32}$/.test(value);
}

export type QueueState = {
  /** The door connection: not opened, opening, open, or ended (see `ended`). */
  connection: "offline" | "connecting" | "online" | "ended";
  /** This player's party once connected; on their own it has no code. */
  party: Party | null;
  /** The party's search. */
  phase: "idle" | "queued" | "found";
  mode: QueueMode | null;
  /** When this search started (ms), for the elapsed time shown. */
  since: number | null;
  /** Players waiting for this mode, including this party. */
  waiting: number | null;
  /** The last refusal or stopped search, to tell the player. */
  notice: string | null;
  /** Why the connection ended: a door error code, `unreachable` or `lost`. */
  ended: string | null;
};
export const IDLE_QUEUE: QueueState = {
  connection: "offline",
  party: null,
  phase: "idle",
  mode: null,
  since: null,
  waiting: null,
  notice: null,
  ended: null,
};

/** `m:ss`, or `h:mm:ss` past an hour. */
export function formatWait(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const s = String(seconds % 60).padStart(2, "0");
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}:${s}`;
  return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}:${s}`;
}

const ENDED: Record<string, string> = {
  replaced: "The game is open in another tab or window; online play continues there.",
  not_ready: "A match was found, but this game did not confirm in time.",
  shutdown: "The matchmaker restarted. Try again in a moment.",
  rate_limited: "Too many requests. Wait a moment and try again.",
  bad_request: "The matchmaker did not accept a request. Reload the page and try again.",
  unreachable: "Could not reach the matchmaker. Online play may not be available here.",
  lost: "Lost the connection to the matchmaker.",
};
const NOTICES: Record<string, string> = {
  cancelled: "The search was cancelled.",
  member_left: "A party member left, so the search stopped.",
  member_not_ready: "A party member did not confirm the match, so the search stopped.",
  not_leader: "Only the party leader can start a search.",
  party_too_big: "Your party is too big for that queue.",
  party_full: "That party is full (3 players).",
  party_busy: "That party is searching for a match. Try again when it stops.",
  no_such_party: "No party has that code. Check it, or ask for a new invite.",
};
export function endedText(code: string | null) {
  return ENDED[code ?? ""] ?? "Online play stopped.";
}
export function noticeText(code: string | null) {
  return code ? (NOTICES[code] ?? null) : null;
}

/** What the in-game banner says about a search, or `null`. */
export function queueMessage(state: QueueState, now: number) {
  if (state.connection === "ended")
    return { text: endedText(state.ended), tone: "warning" as const, cancel: false };
  const mode = state.mode ? modeLabel(state.mode) : "";
  if (state.phase === "found")
    return { text: "Match found · getting ready…", tone: "info" as const, cancel: false };
  if (state.phase === "queued") {
    const waited = state.since === null ? "" : ` · ${formatWait(now - state.since)}`;
    const waiting =
      state.waiting === null
        ? ""
        : ` · ${state.waiting} ${state.waiting === 1 ? "player" : "players"} queued`;
    return {
      text: `Searching for a ${mode} match${waited}${waiting}`,
      tone: "info" as const,
      cancel: true,
    };
  }
  if (state.connection === "connecting" && state.mode)
    return { text: `Joining the ${mode} queue…`, tone: "info" as const, cancel: true };
  return null;
}

/** Why `mode` cannot be queued for now, or `null` when it can. */
export function queueBlocked(state: QueueState, mode: QueueMode) {
  const party = state.party;
  if (party && !party.leader) return "Only the party leader can start a search.";
  const limit = QUEUE_MODES.find((entry) => entry.id === mode)?.maxParty ?? 1;
  if (party && party.members > limit)
    return limit === 1 ? "Everyone queues alone here." : `Parties of up to ${limit} only.`;
  return null;
}
