// Queues and parties (ADR 0046, ADR 0047): the messages the front door's
// `/api/queue` WebSocket exchanges, the queue's state and how the interface
// words it. Pure, so it is unit tested without sockets.

import { parseChatLine, type ChatChannel, type ChatLine } from "./chat";
import { isHandle } from "./handles";
import { parseTicket, type MatchTicket } from "./network";

export type QueueMode = "1v1" | "2v2" | "3v3";
export const QUEUE_MODES: readonly {
  id: QueueMode;
  label: string;
  /** Largest party this queue takes. */
  maxParty: number;
  hint: string;
}[] = [
  { id: "1v1", label: "1v1", maxParty: 1, hint: "Queue for a one-versus-one match." },
  {
    id: "2v2",
    label: "2v2",
    maxParty: 2,
    hint: "Queue for a two-versus-two match, alone or with one party member.",
  },
  {
    id: "3v3",
    label: "3v3",
    maxParty: 3,
    hint: "Queue for a three-versus-three match, alone or with up to two party members.",
  },
];
export function isQueueMode(value: unknown): value is QueueMode {
  return QUEUE_MODES.some((mode) => mode.id === value);
}
export function modeLabel(mode: QueueMode) {
  return QUEUE_MODES.find((entry) => entry.id === mode)?.label ?? mode;
}
/** Players per party at most: one full team. */
export const MAX_PARTY = 3;
/** Spectator slots reserved in a private custom lobby. */
export const MAX_LOBBY_SPECTATORS = 4;
/** Current schema version for server-enforced custom match settings. */
export const MATCH_SETTINGS_VERSION = 1;
/** Allowed custom match round time limits (seconds). */
export const ALLOWED_ROUND_SECONDS = [300, 600, 900, 1200] as const;
export type AllowedRoundSeconds = (typeof ALLOWED_ROUND_SECONDS)[number];

export type CustomMapId = "colosseum";
export const CUSTOM_MAPS: readonly { id: CustomMapId; label: string }[] = [
  { id: "colosseum", label: "Colosseum" },
];
export function isCustomMapId(value: unknown): value is CustomMapId {
  return CUSTOM_MAPS.some((map) => map.id === value);
}
export function mapLabel(map: CustomMapId) {
  return CUSTOM_MAPS.find((entry) => entry.id === map)?.label ?? map;
}
export function isAllowedRoundSeconds(value: unknown): value is AllowedRoundSeconds {
  return typeof value === "number" && ALLOWED_ROUND_SECONDS.includes(value as AllowedRoundSeconds);
}
export function formatRoundLimit(seconds: number) {
  return `${Math.round(seconds / 60)} min`;
}

export type LobbySlot = "team_a" | "team_b" | "spectator";
export function isLobbySlot(value: unknown): value is LobbySlot {
  return value === "team_a" || value === "team_b" || value === "spectator";
}

export type MatchSettings = {
  version: typeof MATCH_SETTINGS_VERSION;
  map: CustomMapId;
  roundSeconds: AllowedRoundSeconds;
  allowSpectators: boolean;
};
export type LobbyMember = {
  id: number;
  slot: LobbySlot;
  host: boolean;
  you: boolean;
  /** The member's `Name#number`, set by the door; `null` from older doors. */
  handle: string | null;
};
/** How a lobby member is named: their handle, else by their lobby number. */
export function memberName(member: Pick<LobbyMember, "id" | "handle">) {
  return member.handle ?? `Player ${member.id}`;
}
/** A member's name for buttons that act on them: as `memberName`, with the
 * lobby number added when another member shows the same name (guest
 * handles can repeat). */
export function memberLabel(
  member: Pick<LobbyMember, "id" | "handle">,
  members: readonly Pick<LobbyMember, "id" | "handle">[],
) {
  const name = memberName(member);
  const repeated = members.some((other) => other.id !== member.id && memberName(other) === name);
  return repeated ? `${name} (Player ${member.id})` : name;
}
export type CustomLobby = {
  code: string;
  mode: QueueMode;
  settings: MatchSettings;
  spectatorSlots: number;
  you: number;
  host: boolean;
  canStart: boolean;
  members: LobbyMember[];
};

export type Party = {
  /** Invite code; `null` for a player on their own. */
  code: string | null;
  members: number;
  leader: boolean;
  /** The other members' handles, leader first (`null` when unnamed); empty
   * from older doors. Shown as teammates. */
  mates: (string | null)[];
};

/** A friend's invite: accepting joins their party or custom lobby by code. */
export type Invite = { from: string; kind: "party" | "lobby"; code: string };
/** Invites kept waiting for an answer at once; older ones give way. */
export const MAX_INVITES = 3;

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
  | {
      type: "match";
      mode: QueueMode;
      matchId: string;
      seat: number;
      spectator: boolean;
      ticket: MatchTicket;
    }
  | { type: "lobby"; lobby: CustomLobby }
  | { type: "lobby_left"; reason: "left" | "kicked" }
  | { type: "invite"; invite: Invite }
  | { type: "invite_sent"; to: string }
  | { type: "friends_changed" }
  | { type: "chat"; line: ChatLine }
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
      const mates = v.mates ?? [];
      if (
        !Array.isArray(mates) ||
        mates.length >= MAX_PARTY ||
        !mates.every((mate) => mate === null || isHandle(mate))
      )
        return null;
      return {
        type: "party",
        party: { code, members: v.members, leader: v.leader, mates: mates as (string | null)[] },
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
      if (v.spectator !== undefined && typeof v.spectator !== "boolean") return null;
      if (typeof v.match_id !== "string" || !/^[0-9a-f]{16}$/.test(v.match_id)) return null;
      const spectator = v.spectator === true;
      return {
        type: "match",
        mode: v.mode,
        matchId: v.match_id,
        seat: v.seat,
        spectator,
        ticket: spectator ? { ...ticket, spectator: true } : ticket,
      };
    }
    case "lobby": {
      const code = typeof v.code === "string" ? normalizeCode(v.code) : null;
      if (!code || !isQueueMode(v.mode)) return null;
      const rawSettings = v.settings;
      if (!rawSettings || typeof rawSettings !== "object") return null;
      const s = rawSettings as Record<string, unknown>;
      if (
        s.version !== MATCH_SETTINGS_VERSION ||
        !isCustomMapId(s.map) ||
        !isAllowedRoundSeconds(s.round_seconds) ||
        (s.allow_spectators !== undefined && typeof s.allow_spectators !== "boolean")
      )
        return null;
      const allowSpectators = s.allow_spectators !== false;
      if (!count(v.spectator_slots) || v.spectator_slots > MAX_LOBBY_SPECTATORS) return null;
      if (!count(v.you) || v.you < 1 || typeof v.host !== "boolean") return null;
      if (typeof v.can_start !== "boolean" || !Array.isArray(v.members)) return null;
      const maxMembers = 2 * MAX_PARTY + MAX_LOBBY_SPECTATORS;
      if (v.members.length < 1 || v.members.length > maxMembers) return null;
      const members: LobbyMember[] = [];
      for (const entry of v.members) {
        if (!entry || typeof entry !== "object") return null;
        const m = entry as Record<string, unknown>;
        if (
          !count(m.id) ||
          m.id < 1 ||
          !isLobbySlot(m.slot) ||
          typeof m.host !== "boolean" ||
          typeof m.you !== "boolean" ||
          (m.handle !== undefined && m.handle !== null && !isHandle(m.handle))
        )
          return null;
        members.push({
          id: m.id,
          slot: m.slot,
          host: m.host,
          you: m.you,
          handle: (m.handle as string | null | undefined) ?? null,
        });
      }
      return {
        type: "lobby",
        lobby: {
          code,
          mode: v.mode,
          settings: {
            version: MATCH_SETTINGS_VERSION,
            map: s.map,
            roundSeconds: s.round_seconds,
            allowSpectators,
          },
          spectatorSlots: v.spectator_slots,
          you: v.you,
          host: v.host,
          canStart: v.can_start,
          members,
        },
      };
    }
    case "lobby_left":
      return v.reason === "left" || v.reason === "kicked"
        ? { type: "lobby_left", reason: v.reason }
        : null;
    case "invite": {
      const code = typeof v.code === "string" ? normalizeCode(v.code) : null;
      if (!isHandle(v.from) || (v.kind !== "party" && v.kind !== "lobby") || !code) return null;
      return { type: "invite", invite: { from: v.from, kind: v.kind, code } };
    }
    case "invite_sent":
      return isHandle(v.to) ? { type: "invite_sent", to: v.to } : null;
    case "friends_changed":
      return { type: "friends_changed" };
    case "chat": {
      const line = parseChatLine(v);
      return line ? { type: "chat", line } : null;
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
  createLobby: (mode: QueueMode = "1v1") => JSON.stringify({ type: "create_lobby", mode }),
  joinLobby: (code: string) => JSON.stringify({ type: "join_lobby", code }),
  leaveLobby: () => JSON.stringify({ type: "leave_lobby" }),
  setLobbySlot: (slot: LobbySlot) => JSON.stringify({ type: "set_lobby_slot", slot }),
  moveLobbyMember: (member: number, slot: LobbySlot) =>
    JSON.stringify({ type: "move_lobby_member", member, slot }),
  kickLobbyMember: (member: number) => JSON.stringify({ type: "kick_lobby_member", member }),
  transferLobbyHost: (member: number) => JSON.stringify({ type: "transfer_lobby_host", member }),
  setLobbyMode: (mode: QueueMode) => JSON.stringify({ type: "set_lobby_mode", mode }),
  setLobbySettings: (settings: MatchSettings) =>
    JSON.stringify({
      type: "set_lobby_settings",
      settings: {
        version: settings.version,
        map: settings.map,
        round_seconds: settings.roundSeconds,
        allow_spectators: settings.allowSpectators,
      },
    }),
  startLobby: () => JSON.stringify({ type: "start_lobby" }),
  invite: (handle: string) => JSON.stringify({ type: "invite", handle }),
  chat: (channel: Exclude<ChatChannel, "whisper">, text: string) =>
    JSON.stringify({ type: "chat", channel, text }),
  whisper: (to: string, text: string) => JSON.stringify({ type: "whisper", to, text }),
  ignore: (handle: string, on: boolean) =>
    JSON.stringify({ type: on ? "ignore" : "unignore", handle }),
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
/** The game page with a custom lobby invite in the fragment, which no server sees. */
export function customInviteLink(location: Pick<Location, "origin" | "pathname">, code: string) {
  return `${location.origin}${location.pathname}#custom=${code}`;
}
/** The custom lobby code in a page fragment such as `#custom=K7P2QX`. */
export function parseCustomFragment(hash: string) {
  const code = new URLSearchParams(hash.replace(/^#/, "")).get("custom");
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
  /** This player's private custom lobby, or `null` when not in one. */
  lobby: CustomLobby | null;
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
  /** Friends' invites waiting for an answer, newest last. */
  invites: Invite[];
  /** Who the last invite reached (the door's `invite_sent`). */
  inviteSent: string | null;
  /** An invite on its way to the door, until it is sent or refused. */
  inviting: string | null;
  /** An accepted invite waiting to be joined. */
  joining: Invite | null;
  /** Why an accepted invite could not be joined (a refusal code). */
  inviteError: { from: string; code: string } | null;
  /** Counts the door's `friends_changed` pushes: the list is stale. */
  friendsChanged: number;
};
export const IDLE_QUEUE: QueueState = {
  connection: "offline",
  party: null,
  lobby: null,
  phase: "idle",
  mode: null,
  since: null,
  waiting: null,
  expectedMs: null,
  readyCheck: null,
  notice: null,
  ended: null,
  invites: [],
  inviteSent: null,
  inviting: null,
  joining: null,
  inviteError: null,
  friendsChanged: 0,
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
  replaced: "The game is open in another tab or window. Online play continues there.",
  shutdown: "The matchmaker restarted. Please try again in a moment.",
  rate_limited: "Too many requests. Please wait a moment and try again.",
  bad_request: "The matchmaker rejected a request. Reload the page and try again.",
  unreachable: "The matchmaker could not be reached. Online play may be unavailable.",
  lost: "The connection to the matchmaker was lost.",
};
const NOTICES: Record<string, string> = {
  requeued: "Another player did not accept the match. You are back at the front of the queue.",
  cancelled: "The search was cancelled.",
  member_left: "A party member left, so your party has left the queue.",
  declined: "You declined the match and have left the queue.",
  not_accepted: "The match was not accepted in time, so you have left the queue.",
  member_declined: "A party member declined the match, so your party has left the queue.",
  member_not_accepted:
    "A party member did not accept the match in time, so your party has left the queue.",
  not_leader: "Only the party leader can start a search.",
  party_too_big: "Your party has too many members for this queue.",
  party_full: "This party is full.",
  party_busy: "This party is currently in a queue. Please try again when the search ends.",
  no_such_party: "This party no longer exists.",
  in_lobby: "Leave your custom lobby before joining a queue or a party.",
  no_such_lobby: "No custom lobby exists with this code.",
  lobby_full: "This custom lobby is full.",
  not_host: "Only the lobby host can perform this action.",
  slot_full: "This team or spectator slot is full.",
  lobby_not_full: "Both teams must be full before the match can start.",
  lobby_too_big: "The lobby has too many players for this team size.",
  no_such_member: "This player is no longer in the lobby.",
  invalid_settings: "The server does not support these match settings.",
  kicked: "The host removed you from the custom lobby.",
  rate_limited: "Too many requests. Please wait a moment and try again.",
  not_logged_in: "Log in to invite friends.",
  no_such_friend: "Only friends can be invited.",
  offline: "This friend is offline.",
  unavailable: "The server could not process the invitation. Please try again later.",
  spectators_disabled: "Spectators are disabled in this custom lobby.",
  spectators_occupied: "Move or remove all spectators before disabling spectators.",
};
export function endedText(code: string | null) {
  return ENDED[code ?? ""] ?? "Online play stopped.";
}
export function noticeText(code: string | null) {
  return code ? (NOTICES[code] ?? null) : null;
}

/** Why the party left the queue or lobby: worth telling wherever the player is. */
export const LEFT_QUEUE_NOTICES: ReadonlySet<string> = new Set([
  "member_left",
  "declined",
  "not_accepted",
  "member_declined",
  "member_not_accepted",
  "kicked",
]);

/** Team size for a queue or custom match mode. */
export function teamSizeForMode(mode: QueueMode) {
  return QUEUE_MODES.find((entry) => entry.id === mode)?.maxParty ?? 1;
}

/** Plain-language summary of why a custom lobby can or cannot start. */
export function lobbyReadinessText(lobby: CustomLobby) {
  const size = teamSizeForMode(lobby.mode);
  const teamA = lobby.members.filter((m) => m.slot === "team_a").length;
  const teamB = lobby.members.filter((m) => m.slot === "team_b").length;
  if (teamA === size && teamB === size) {
    return lobby.host
      ? "Both teams are full. Ready to start."
      : "Both teams are full. Waiting for the host to start.";
  }
  const needA = Math.max(0, size - teamA);
  const needB = Math.max(0, size - teamB);
  if (needA > 0 && needB > 0) {
    return `Need ${needA} more on Team A and ${needB} more on Team B to start.`;
  }
  if (needA > 0) {
    return `Need ${needA} more on Team A to start.`;
  }
  return `Need ${needB} more on Team B to start.`;
}

export type QueueBarContext = {
  lobbyOpen?: boolean;
  inMatch?: boolean;
  scoreboardOpen?: boolean;
};

/** Whether the floating queue/custom-lobby status bar occupies the top-center slot. */
export function isQueueBarVisible(state: QueueState, ctx?: QueueBarContext): boolean {
  if (
    state.connection === "ended" ||
    (state.phase === "queued" && !!state.mode) ||
    (state.phase === "idle" && !!state.notice && LEFT_QUEUE_NOTICES.has(state.notice))
  ) {
    return true;
  }
  return Boolean(state.lobby && !ctx?.lobbyOpen && !ctx?.inMatch && !ctx?.scoreboardOpen);
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
  if (state.lobby) return "Leave your custom lobby before joining a public queue.";
  const party = state.party;
  if (party && !party.leader) return "Only the party leader can start a search.";
  const limit = QUEUE_MODES.find((entry) => entry.id === mode)?.maxParty ?? 1;
  if (party && party.members > limit)
    return limit === 1
      ? "The 1v1 queue does not accept parties."
      : `This queue accepts parties of up to ${limit} players.`;
  return null;
}
