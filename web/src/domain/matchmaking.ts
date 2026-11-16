// Solo queues (ADR 0046): the messages the front door's `/api/queue`
// WebSocket exchanges, the queue's state and what the banner says about it.
// Pure, so it is unit tested without sockets.

import { parseTicket, type MatchTicket } from "./network";

export type QueueMode = "1v1" | "solo-2v2" | "solo-3v3";
export const QUEUE_MODES: readonly { id: QueueMode; label: string; players: number }[] = [
  { id: "1v1", label: "1v1", players: 2 },
  { id: "solo-2v2", label: "Solo 2v2", players: 4 },
  { id: "solo-3v3", label: "Solo 3v3", players: 6 },
];
export function isQueueMode(value: unknown): value is QueueMode {
  return QUEUE_MODES.some((mode) => mode.id === value);
}
export function modeLabel(mode: QueueMode) {
  return QUEUE_MODES.find((entry) => entry.id === mode)?.label ?? mode;
}

/** What the door sends. A `match` carries this player's own seat ticket. */
export type QueueMessage =
  | { type: "queued"; mode: QueueMode; waiting: number }
  | { type: "found"; mode: QueueMode }
  | { type: "requeued"; mode: QueueMode }
  | { type: "match"; mode: QueueMode; matchId: string; seat: number; ticket: MatchTicket }
  | { type: "error"; code: string };

const count = (value: unknown): value is number =>
  Number.isInteger(value) && (value as number) >= 0 && (value as number) < 1_000_000;

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
    case "queued":
      return isQueueMode(v.mode) && count(v.waiting)
        ? { type: "queued", mode: v.mode, waiting: v.waiting }
        : null;
    case "found":
    case "requeued":
      return isQueueMode(v.mode) ? { type: v.type, mode: v.mode } : null;
    case "match": {
      const ticket = parseTicket(v);
      if (!ticket || !isQueueMode(v.mode) || !count(v.seat)) return null;
      if (typeof v.match_id !== "string" || !/^[0-9a-f]{16}$/.test(v.match_id)) return null;
      return { type: "match", mode: v.mode, matchId: v.match_id, seat: v.seat, ticket };
    }
    case "error":
      return typeof v.code === "string" && v.code.length <= 64
        ? { type: "error", code: v.code }
        : null;
    default:
      return null;
  }
}

export function joinMessage(mode: QueueMode, guest: string) {
  return JSON.stringify({ type: "join", mode, guest });
}
export const READY_MESSAGE = JSON.stringify({ type: "ready" });
export const LEAVE_MESSAGE = JSON.stringify({ type: "leave" });

/** The door's queue endpoint on the page's own origin. */
export function queueUrl(location: Pick<Location, "protocol" | "host">) {
  return `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/api/queue`;
}

/** A browser's guest id: 32 hex digits from 16 random bytes. */
export function guestId(bytes: Uint8Array) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
export function isGuestId(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{32}$/.test(value);
}

export type QueueState = {
  phase: "idle" | "connecting" | "queued" | "found" | "ended";
  mode: QueueMode | null;
  /** When this search started (ms), for the elapsed time shown. */
  since: number | null;
  /** Players waiting for this mode, including this one. */
  waiting: number | null;
  /** Why the search ended: a door error code, `unreachable` or `lost`. */
  ended: string | null;
};
export const IDLE_QUEUE: QueueState = {
  phase: "idle",
  mode: null,
  since: null,
  waiting: null,
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
  replaced: "You joined the queue from another tab or window.",
  not_ready: "A match was found, but this game did not confirm in time. Queue again when ready.",
  shutdown: "The matchmaker restarted. Queue again in a moment.",
  rate_limited: "Too many queue requests. Wait a moment and try again.",
  bad_request: "The matchmaker did not accept this request. Reload the page and try again.",
  unreachable: "Could not reach the matchmaker. Online play may not be available here.",
  lost: "Lost the connection to the matchmaker. Queue again.",
};

/** What the queue banner says, or `null` when not searching. */
export function queueMessage(state: QueueState, now: number) {
  const mode = state.mode ? modeLabel(state.mode) : "";
  switch (state.phase) {
    case "idle":
      return null;
    case "connecting":
      return { text: `Joining the ${mode} queue…`, tone: "info" as const, cancel: true };
    case "queued": {
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
    case "found":
      return { text: "Match found · getting ready…", tone: "info" as const, cancel: false };
    case "ended":
      return {
        text: ENDED[state.ended ?? ""] ?? "The search ended.",
        tone: "warning" as const,
        cancel: false,
      };
  }
}
