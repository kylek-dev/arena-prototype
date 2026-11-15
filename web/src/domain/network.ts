// Network match identity and connection policy (ADR 0044, Phase 5).
// Pure functions only: the socket lives in runtime/network.ts.

/** A seat in one match: where to connect and the bearer ticket for it. */
export type MatchTicket = {
  /** `ws://` or `wss://` URL of the server's `/play` endpoint. */
  server: string;
  /** Hex-encoded join ticket. A credential for one seat only. */
  ticket: string;
  /** Unix seconds after which the server refuses the ticket. */
  expires: number;
};

/** What Rust reports about the match through `onNetwork`. */
export type NetworkStatus = {
  phase: "connecting" | "waiting" | "playing" | "finished" | "closed";
  error: string | null;
  seat: number | null;
  mapDiffers: boolean;
  rttMs: number | null;
  inputMs: number | null;
  delayMs: number;
  snapshots: number;
  underruns: number;
  hashMismatches: number;
};

/** Errors after which retrying cannot help: the seat is gone for good. */
const FINAL_ERRORS = new Set(["MatchOver", "BadTicket", "VersionMismatch", "MatchFull"]);
export function isFinalError(error: string | null) {
  return !!error && FINAL_ERRORS.has(error);
}

/** Delay before reconnect attempt `attempt` (0-based): 0.5 s doubling to 8 s. */
export function backoffMs(attempt: number) {
  return Math.min(500 * 2 ** Math.max(0, Math.floor(attempt)), 8000);
}

const MAX_TICKET_HEX = 1024;
export function parseTicket(value: unknown): MatchTicket | null {
  if (!value || typeof value !== "object") return null;
  const v = value as Record<string, unknown>;
  if (typeof v.server !== "string" || typeof v.ticket !== "string") return null;
  if (!Number.isInteger(v.expires) || (v.expires as number) <= 0) return null;
  let url: URL;
  try {
    url = new URL(v.server);
  } catch {
    return null;
  }
  if (url.protocol !== "ws:" && url.protocol !== "wss:") return null;
  if (
    v.ticket.length === 0 ||
    v.ticket.length > MAX_TICKET_HEX ||
    v.ticket.length % 2 !== 0 ||
    !/^[0-9a-f]+$/i.test(v.ticket)
  )
    return null;
  return { server: url.href, ticket: v.ticket.toLowerCase(), expires: v.expires as number };
}

/**
 * A development join link carries the ticket in the URL fragment
 * (`#server=…&ticket=…&expires=…`), which browsers never send to a server.
 * The runtime removes it from the address bar as soon as it is read.
 */
export function parseJoinFragment(hash: string): MatchTicket | null {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  if (!params.has("ticket")) return null;
  return parseTicket({
    server: params.get("server"),
    ticket: params.get("ticket"),
    expires: Number(params.get("expires")),
  });
}

export function isExpired(ticket: MatchTicket, nowMs: number) {
  return nowMs / 1000 >= ticket.expires;
}

const PHASES = new Set(["connecting", "waiting", "playing", "finished", "closed"]);
export function parseStatus(value: unknown): NetworkStatus | null {
  if (!value || typeof value !== "object") return null;
  const v = value as NetworkStatus;
  const optionalNumber = (n: unknown) =>
    n === null || (typeof n === "number" && Number.isFinite(n));
  const count = (n: unknown) => Number.isInteger(n) && (n as number) >= 0;
  if (
    !PHASES.has(v.phase) ||
    (v.error !== null && typeof v.error !== "string") ||
    (v.seat !== null && !(Number.isInteger(v.seat) && v.seat >= 0 && v.seat < 6)) ||
    typeof v.mapDiffers !== "boolean" ||
    !optionalNumber(v.rttMs) ||
    !optionalNumber(v.inputMs) ||
    typeof v.delayMs !== "number" ||
    !Number.isFinite(v.delayMs) ||
    !count(v.snapshots) ||
    !count(v.underruns) ||
    !count(v.hashMismatches)
  )
    return null;
  return v;
}
