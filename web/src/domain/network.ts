// Network match identity and connection policy (ADR 0044, Phase 5).
// Pure functions only: the socket lives in runtime/network.ts.
import type { RoundResult } from "../bridge/snapshot";
import type { Appearance } from "./appearance";
import { roundResultLabel } from "./round-clock";

/** A seat or spectator pass in one match: where to connect and the bearer ticket for it. */
export type MatchTicket = {
  /** `ws://` or `wss://` URL of the server's `/play` endpoint. */
  server: string;
  /** Hex-encoded join ticket. A credential for one seat or spectator pass only. */
  ticket: string;
  /** Unix seconds after which the server refuses the ticket. */
  expires: number;
  /** True when this ticket is a watch-only spectator pass. */
  spectator?: boolean;
};

/** What Rust reports about the match through `onNetwork`. */
export type NetworkStatus = {
  phase: "connecting" | "waiting" | "playing" | "finished" | "closed";
  error: string | null;
  seat: number | null;
  /** True when attached as a watch-only spectator. */
  spectator?: boolean;
  mapDiffers: boolean;
  rttMs: number | null;
  inputMs: number | null;
  delayMs: number;
  snapshots: number;
  underruns: number;
  hashMismatches: number;
  /** Inputs the player's own fighter is predicted ahead of the server. */
  predictedTicks: number;
  /** Times reconciling with the server moved the predicted fighter. */
  corrections: number;
  lastCorrectionMm: number | null;
  /** This player's inputs waiting on the server, averaged. */
  queued: number | null;
  /** The lowest that queue was over the last second (pacing keeps it at 1). */
  queuedMin: number | null;
  /** Input interval multiplier steering that queue (1 = the rules tick). */
  pace: number;
  /** Catch-up inputs not sent after a stall. */
  skippedInputs: number;
  /** Fixed steps run and inputs sent in the last second (browser clock). */
  stepsPerSecond: number;
  sentPerSecond: number;
  /** Server ticks per second measured on this browser's clock. */
  tickRate: number | null;
};

export type LinkPhase = "idle" | "connecting" | "open" | "reconnecting" | "ended";
export type LinkState = {
  phase: LinkPhase;
  /** Reconnect attempts since the last open socket. */
  attempt: number;
  /** Why the link ended: a server error code or `expired`. */
  ended: string | null;
  /** True when the active or just-ended link was opened with a spectator ticket. */
  spectator?: boolean;
  /** The latest status Rust reported, if any. */
  status: NetworkStatus | null;
};

/** What the match banner says and offers. `null` when nothing needs saying. */
export type MatchMessage = {
  text: string;
  tone: "info" | "warning";
  action: { kind: "rejoin" | "dismiss"; label: string } | null;
};
/** How many seats have a connected player, while waiting for the start. */
export type Attendance = { joined: number; seats: number; seat: number | null };
const ENDED: Record<string, string> = {
  MatchOver: "The match is over.",
  BadTicket: "This match link is not valid.",
  VersionMismatch: "This game version cannot join that server.",
  MatchFull: "That seat is taken.",
  expired: "The match link has expired.",
  Replaced: "This seat is now playing in another tab or window.",
};
export function matchMessage(
  state: LinkState,
  rememberedMatch: boolean,
  attendance: Attendance | null = null,
  lastResult: RoundResult | null = null,
): MatchMessage | null {
  switch (state.phase) {
    case "idle":
      return rememberedMatch
        ? {
            text: "A match you joined is still open.",
            tone: "info",
            action: { kind: "rejoin", label: "Rejoin match" },
          }
        : null;
    case "connecting":
      return { text: "Connecting to the match…", tone: "info", action: null };
    case "reconnecting":
      return {
        text: `Connection lost · reconnecting${state.attempt > 1 ? ` (attempt ${state.attempt})` : ""}…`,
        tone: "warning",
        action: null,
      };
    case "ended": {
      const spectating = Boolean(state.spectator || state.status?.spectator);
      const outcome = state.ended === "MatchOver" ? roundResultLabel(lastResult, spectating) : null;
      const endedText =
        spectating && state.ended === "Replaced"
          ? "This spectator view is now open in another tab or window."
          : spectating && state.ended === "MatchFull"
            ? "Spectator slots are full."
            : (ENDED[state.ended ?? ""] ?? "You left the match.");
      return {
        text: outcome ? `${outcome} · ${ENDED.MatchOver}` : endedText,
        tone: state.ended === "MatchOver" && lastResult === "time_limit_draw" ? "warning" : "info",
        action:
          state.ended === "Replaced" && rememberedMatch
            ? { kind: "rejoin", label: spectating ? "Watch here" : "Play here" }
            : { kind: "dismiss", label: "Dismiss" },
      };
    }
    case "open":
      if (!state.status || state.status.phase === "connecting")
        return { text: "Joining the match…", tone: "info", action: null };
      if (state.status.phase === "waiting") {
        const count = attendance ? ` (${attendance.joined} of ${attendance.seats} joined)` : "";
        if (state.status.spectator) {
          return {
            text: `Spectating · waiting for every player to join${count}…`,
            tone: "info",
            action: null,
          };
        }
        const seat = state.status.seat === null ? "" : ` · you are seat ${state.status.seat + 1}`;
        return {
          text: `Waiting for every player to join${count}${seat}…`,
          tone: "info",
          action: null,
        };
      }
      if (state.status.mapDiffers)
        return {
          text: "This build's map differs from the server's.",
          tone: "warning",
          action: null,
        };
      return null;
  }
}

/** Errors after which retrying cannot help: the seat is gone for good. */
const FINAL_ERRORS = new Set(["MatchOver", "BadTicket", "VersionMismatch", "MatchFull"]);
export function isFinalError(error: string | null) {
  return !!error && FINAL_ERRORS.has(error);
}
/** Errors that end this tab's link: final ones, and losing the seat to a
 * newer connection (another tab), which must not be taken back silently. */
export function endsLink(error: string | null) {
  return isFinalError(error) || error === "Replaced";
}

// The runtime speaks two tiny, version-stable corners of the protocol
// itself, so a background tab (whose engine the browser pauses) still joins
// and still notices a final error. arena-protocol's golden tests pin the
// same bytes; everything else is decoded by the engine.

/** Must match `arena_protocol::PROTOCOL_VERSION`. */
export const PROTOCOL_VERSION = 9;
/** `ServerMessage::Error` codes in declaration order (`[3, index]`). */
export const ERROR_CODES = [
  "VersionMismatch",
  "BadTicket",
  "MatchFull",
  "MatchOver",
  "RateLimited",
  "Malformed",
  "HelloTimeout",
  "ServerShutdown",
  "Replaced",
] as const;

function varint(value: number): number[] {
  const bytes: number[] = [];
  let rest = value;
  while (rest >= 0x80) {
    bytes.push((rest & 0x7f) | 0x80);
    rest = Math.floor(rest / 0x80);
  }
  bytes.push(rest);
  return bytes;
}

const WIRE_ID_RE = /^[A-Za-z0-9_-]{1,32}$/;
const MAX_CLIENT_FRAME = 1024;
const MAX_APPEARANCE_GROUPS = 24;
const MAX_APPEARANCE_VARIANTS = 8;

function encodeAsciiString(value: string): number[] {
  const bytes = varint(value.length);
  for (let i = 0; i < value.length; i += 1) bytes.push(value.charCodeAt(i));
  return bytes;
}

function encodeWireAppearance(look: Appearance): number[] | null {
  if (
    !WIRE_ID_RE.test(look.ancestry) ||
    !WIRE_ID_RE.test(look.body) ||
    !WIRE_ID_RE.test(look.skinTone) ||
    !WIRE_ID_RE.test(look.hairColor) ||
    !WIRE_ID_RE.test(look.facialHairColor) ||
    !WIRE_ID_RE.test(look.eyes)
  ) {
    return null;
  }
  const build = Number.isFinite(look.build)
    ? Math.max(0, Math.min(100, Math.round(look.build * 100)))
    : 0;
  const groups = Object.entries(look.groups)
    .filter(([k, v]) => typeof v === "string" && WIRE_ID_RE.test(k) && WIRE_ID_RE.test(v))
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .slice(0, MAX_APPEARANCE_GROUPS) as [string, string][];
  const variants = Object.entries(look.variants)
    .filter(([k, v]) => typeof v === "string" && WIRE_ID_RE.test(k) && WIRE_ID_RE.test(v))
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .slice(0, MAX_APPEARANCE_VARIANTS);
  const out: number[] = [
    ...encodeAsciiString(look.ancestry),
    ...encodeAsciiString(look.body),
    ...encodeAsciiString(look.skinTone),
    ...encodeAsciiString(look.hairColor),
    ...encodeAsciiString(look.facialHairColor),
    ...encodeAsciiString(look.eyes),
    build,
    ...varint(groups.length),
  ];
  for (const [k, v] of groups) {
    out.push(...encodeAsciiString(k), ...encodeAsciiString(v));
  }
  out.push(...varint(variants.length));
  for (const [k, v] of variants) {
    out.push(...encodeAsciiString(k), ...encodeAsciiString(v));
  }
  return out;
}

/** `ClientMessage::Hello { protocol, ticket, appearance }`: variant 0, then varints + Option<WireAppearance>. */
export function encodeHello(
  ticketHex: string,
  protocolOrAppearance: number | Appearance | null = PROTOCOL_VERSION,
  protocolOverride?: number,
): Uint8Array {
  const appearance = typeof protocolOrAppearance === "object" ? protocolOrAppearance : null;
  const protocol =
    typeof protocolOrAppearance === "number"
      ? protocolOrAppearance
      : (protocolOverride ?? PROTOCOL_VERSION);
  const ticket = (ticketHex.match(/../g) ?? []).map((pair) => Number.parseInt(pair, 16));
  const prefix = [0, ...varint(protocol), ...varint(ticket.length), ...ticket];
  const encodedLook = appearance ? encodeWireAppearance(appearance) : null;
  if (encodedLook && prefix.length + 1 + encodedLook.length <= MAX_CLIENT_FRAME) {
    return new Uint8Array([...prefix, 1, ...encodedLook]);
  }
  return new Uint8Array([...prefix, 0]);
}

/** The code of a `ServerMessage::Error` frame, or null for any other frame. */
export function serverError(frame: Uint8Array): string | null {
  return frame.length === 2 && frame[0] === 3 ? (ERROR_CODES[frame[1]] ?? null) : null;
}

/** Whether a frame is the server's `Welcome` (variant 0). */
export function isWelcome(frame: Uint8Array) {
  return frame.length > 0 && frame[0] === 0;
}

/** Whether a frame is the server's `Appearances` relay (variant 4). */
export function isAppearances(frame: Uint8Array) {
  return frame.length > 0 && frame[0] === 4;
}

/** Whether a frame is the server's `Handles` relay (variant 5). */
export function isHandles(frame: Uint8Array) {
  return frame.length > 0 && frame[0] === 5;
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
  return {
    server: url.href,
    ticket: v.ticket.toLowerCase(),
    expires: v.expires as number,
    ...(v.spectator === true ? { spectator: true } : {}),
  };
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
    (v.spectator !== undefined && typeof v.spectator !== "boolean") ||
    typeof v.mapDiffers !== "boolean" ||
    !optionalNumber(v.rttMs) ||
    !optionalNumber(v.inputMs) ||
    typeof v.delayMs !== "number" ||
    !Number.isFinite(v.delayMs) ||
    !count(v.snapshots) ||
    !count(v.underruns) ||
    !count(v.hashMismatches) ||
    !count(v.predictedTicks) ||
    !count(v.corrections) ||
    !(v.lastCorrectionMm === null || count(v.lastCorrectionMm)) ||
    !optionalNumber(v.queued) ||
    !(v.queuedMin === null || count(v.queuedMin)) ||
    typeof v.pace !== "number" ||
    !(v.pace > 0.5 && v.pace < 1.5) ||
    !count(v.skippedInputs) ||
    !count(v.stepsPerSecond) ||
    !count(v.sentPerSecond) ||
    !optionalNumber(v.tickRate)
  )
    return null;
  return v;
}
