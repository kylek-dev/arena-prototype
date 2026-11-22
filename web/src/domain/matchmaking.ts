// Queues and parties (ADR 0046, ADR 0047): the messages the front door's
// `/api/queue` WebSocket exchanges, the queue's state and how the interface
// words it. Pure, so it is unit tested without sockets.

import { parseTicket, type MatchTicket } from "./network";

export type QueueMode = "1v1" | "2v2" | "3v3";
export const QUEUE_MODES: readonly {
  id: QueueMode;
  label: string;
  /** Largest party this queue takes. */
  maxParty: number;
  hint: string;
}[] = [
  { id: "1v1", label: "1v1", maxParty: 1, hint: "One against one." },
  { id: "2v2", label: "2v2", maxParty: 2, hint: "Alone or with a friend." },
  { id: "3v3", label: "3v3", maxParty: 3, hint: "Alone or with up to two friends." },
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
  | {
      type: "queued";
      mode: QueueMode;
      waiting: number;
      waitedMs: number;
      expectedMs: number | null;
    }
  | {
      type: "ready_check";
      mode: QueueMode;
      accepted: number;
      total: number;
      you: boolean;
      remainingMs: number;
    }
  | { type: "requeued"; mode: QueueMode }
  | { type: "stopped"; reason: string }
  | { type: "refused"; code: string }
  | { type: "match"; mode: QueueMode; matchId: string; seat: number; ticket: MatchTicket }
  | { type: "error"; code: string };

const count = (value: unknown): value is number =>
  Number.isInteger(value) && (value as number) >= 0 && (value as number) < 1_000_000;
/** Milliseconds up to about four days. */
const millis = (value: unknown): value is number =>
  Number.isInteger(value) && (value as number) >= 0 && (value as number) < 4e8;
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
      const code = typeof v.code === "string" ? normalizeCode(v.code) : null;
      if ((v.code !== null && code === null) || !count(v.members)) return null;
      if (v.members < 1 || v.members > MAX_PARTY || typeof v.leader !== "boolean") return null;
      if (v.queue !== null && !isQueueMode(v.queue)) return null;
      return {
        type: "party",
        party: { code, members: v.members, leader: v.leader },
        queue: v.queue as QueueMode | null,
      };
    }
    case "queued":
      if (!isQueueMode(v.mode) || !count(v.waiting) || !millis(v.waited_ms)) return null;
      if (v.expected_ms !== null && !millis(v.expected_ms)) return null;
      return {
        type: "queued",
        mode: v.mode,
        waiting: v.waiting,
        waitedMs: v.waited_ms,
        expectedMs: v.expected_ms as number | null,
      };
    case "ready_check":
      if (!isQueueMode(v.mode) || !count(v.accepted) || !count(v.total)) return null;
      if (v.total < 2 || v.accepted > v.total || typeof v.you !== "boolean") return null;
      if (!millis(v.remaining_ms)) return null;
      return {
        type: "ready_check",
        mode: v.mode,
        accepted: v.accepted,
        total: v.total,
        you: v.you,
        remainingMs: v.remaining_ms,
      };
    case "requeued":
      return isQueueMode(v.mode) ? { type: "requeued", mode: v.mode } : null;
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
  accept: () => JSON.stringify({ type: "accept" }),
  decline: () => JSON.stringify({ type: "decline" }),
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

export type ReadyCheck = {
  accepted: number;
  total: number;
  /** Whether this player accepted. */
  you: boolean;
  /** When it ends, on the queue's clock (ms). */
  deadline: number;
};

export type QueueState = {
  /** The door connection: not opened, opening, open, or ended (see `ended`). */
  connection: "offline" | "connecting" | "online" | "ended";
  /** This player's party once connected; on their own it has no code. */
  party: Party | null;
  /** The party's search: none, waiting in a queue, or a match to accept. */
  phase: "idle" | "queued" | "ready";
  mode: QueueMode | null;
  /** When this search started (ms, queue clock), as the door measures it. */
  since: number | null;
  /** Players waiting for this queue, including this party. */
  waiting: number | null;
  /** How long recent matches took to find; `null` when unknown. */
  expectedMs: number | null;
  readyCheck: ReadyCheck | null;
  /** The last refusal, stopped search or requeue, to tell the player. */
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
  expectedMs: null,
  readyCheck: null,
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

/** A rough expected wait: "under a minute", "about 2 min", "about 1 h 10 min". */
export function formatExpected(ms: number) {
  const minutes = Math.round(ms / 60_000);
  if (minutes < 1) return "under a minute";
  if (minutes < 60) return `about ${minutes} min`;
  const rest = minutes % 60;
  return `about ${Math.floor(minutes / 60)} h${rest ? ` ${rest} min` : ""}`;
}

const ENDED: Record<string, string> = {
  replaced: "The game is open in another tab or window; online play continues there.",
  shutdown: "The matchmaker restarted. Try again in a moment.",
  rate_limited: "Too many requests. Wait a moment and try again.",
  bad_request: "The matchmaker did not accept a request. Reload the page and try again.",
  unreachable: "Could not reach the matchmaker. Online play may not be available here.",
  lost: "Lost the connection to the matchmaker.",
};
const NOTICES: Record<string, string> = {
  requeued: "Someone did not accept the match. You are back at the front of the queue.",
  cancelled: "The search was cancelled.",
  member_left: "A party member left, so your party left the queue.",
  declined: "You declined the match and left the queue.",
  not_accepted: "You did not accept the match in time, so you left the queue.",
  member_declined: "A party member declined the match, so your party left the queue.",
  member_not_accepted: "A party member did not accept in time, so your party left the queue.",
  not_leader: "Only the party leader can start a search.",
  party_too_big: "Your party is too big for that queue.",
  party_full: "That party is full (3 players).",
  party_busy: "That party is in a queue right now. Try again when it stops.",
  no_such_party: "No party has that code. Check it, or ask for a new invite.",
};
export function endedText(code: string | null) {
  return ENDED[code ?? ""] ?? "Online play stopped.";
}
export function noticeText(code: string | null) {
  return code ? (NOTICES[code] ?? null) : null;
}

/** Why the party left the queue: worth telling wherever the player is. */
export const LEFT_QUEUE_NOTICES: ReadonlySet<string> = new Set([
  "member_left",
  "declined",
  "not_accepted",
  "member_declined",
  "member_not_accepted",
]);

/** Whether the floating queue status bar occupies the top-center slot. */
export function isQueueBarVisible(state: QueueState): boolean {
  return (
    state.connection === "ended" ||
    (state.phase === "queued" && !!state.mode) ||
    (state.phase === "idle" && !!state.notice && LEFT_QUEUE_NOTICES.has(state.notice))
  );
}

/** What the queue bar shows while searching, or `null` when not. */
export function queueStatus(state: QueueState, now: number) {
  if (state.phase !== "queued" || !state.mode) return null;
  const players = state.waiting ?? 0;
  return {
    title: `In queue · ${modeLabel(state.mode)}`,
    waited: formatWait(state.since === null ? 0 : now - state.since),
    expected:
      state.expectedMs === null
        ? "Not enough recent matches to estimate"
        : `Expected wait ${formatExpected(state.expectedMs)}`,
    players:
      state.waiting === null ? "" : `${players} ${players === 1 ? "player" : "players"} in queue`,
  };
}

/** What the match-ready window shows, or `null` without a ready check. */
export function readyStatus(state: QueueState, now: number) {
  const check = state.readyCheck;
  if (state.phase !== "ready" || !check || !state.mode) return null;
  const remainingMs = Math.max(0, check.deadline - now);
  return {
    title: `Your ${modeLabel(state.mode)} match is ready`,
    seconds: Math.ceil(remainingMs / 1000),
    /** 1 → 0 as time runs out. */
    fraction: Math.min(1, remainingMs / READY_MS),
    accepted: check.accepted,
    total: check.total,
    you: check.you,
    progress: `${check.accepted} of ${check.total} accepted`,
  };
}
/** The ready check's length, for the countdown bar. */
export const READY_MS = 15_000;

/** Why `mode` cannot be queued for now, or `null` when it can. */
export function queueBlocked(state: QueueState, mode: QueueMode) {
  const party = state.party;
  if (party && !party.leader) return "Only the party leader can start a search.";
  const limit = QUEUE_MODES.find((entry) => entry.id === mode)?.maxParty ?? 1;
  if (party && party.members > limit)
    return limit === 1 ? "1v1 is for one player." : `Parties of up to ${limit} only.`;
  return null;
}
