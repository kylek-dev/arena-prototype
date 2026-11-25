// Chat (planning/issues/chat.md): the channels the front door's queue
// connection carries, how the chat box groups them into tabs, and how it
// words the door's refusals. The door names every sender and decides who
// hears what; the page sends only text and the channel it wants. Pure, so
// it is unit tested without sockets.

import { isHandle } from "./handles";

export type ChatChannel = "public" | "match" | "team" | "spectator" | "whisper";
const CHANNELS = new Set<ChatChannel>(["public", "match", "team", "spectator", "whisper"]);
export function isChatChannel(value: unknown): value is ChatChannel {
  return CHANNELS.has(value as ChatChannel);
}

/** One message as the door sent it. `to` is set on the sender's own copy of
 * a whisper. */
export type ChatLine = {
  channel: ChatChannel;
  from: string;
  to: string | null;
  admin: boolean;
  text: string;
};
export type ChatMessage = ChatLine & { id: number };

/** The door keeps 1–200 characters; a little room for any script. */
export const MAX_CHAT_CHARS = 200;
const MAX_WIRE_TEXT = 1_000;

/** Parses a `chat` message's fields; `null` when they are not understood. */
export function parseChatLine(v: Record<string, unknown>): ChatLine | null {
  if (!isChatChannel(v.channel) || !isHandle(v.from) || typeof v.admin !== "boolean") return null;
  if (v.to !== undefined && v.to !== null && !isHandle(v.to)) return null;
  if (typeof v.text !== "string" || !v.text || v.text.length > MAX_WIRE_TEXT) return null;
  if (v.to && v.channel !== "whisper") return null;
  return {
    channel: v.channel,
    from: v.from,
    to: (v.to as string | undefined) ?? null,
    admin: v.admin,
    text: v.text,
  };
}

/** The chat box's tabs: a player's channels, or a spectator's (who can post
 * only to the spectators). Whispers share one tab. */
export function chatTabs(spectating: boolean): readonly ChatChannel[] {
  return spectating ? ["spectator", "public", "whisper"] : ["public", "team", "match", "whisper"];
}
const TAB_LABELS: Record<ChatChannel, string> = {
  public: "Public",
  team: "Team",
  match: "Match",
  spectator: "Spectators",
  whisper: "Whispers",
};
export function chatTabLabel(tab: ChatChannel) {
  return TAB_LABELS[tab];
}

/** Text the player typed, as the door will count it: trimmed, and too long
 * past `MAX_CHAT_CHARS` characters (not UTF-16 units). */
export function chatText(raw: string) {
  const text = raw.trim();
  return { text, tooLong: [...text].length > MAX_CHAT_CHARS };
}

/** Codes chat shares with other requests: the chat box's only when the
 * last request was a chat one. Any other code not listed below is not. */
export const SHARED_CHAT_REFUSALS: ReadonlySet<string> = new Set([
  "rate_limited",
  "not_logged_in",
  "no_such_friend",
  "offline",
  "unavailable",
]);
/** Codes an ignore is refused with. */
export const IGNORE_REFUSALS: ReadonlySet<string> = new Set(["no_such_player", "too_many"]);
/** Ignores sent again on a new connection: the newest few, inside the
 * door's 10 messages a second with room for waiting requests. */
export const MAX_IGNORE_REPLAY = 5;
/** Codes only chat requests are refused with: always the chat box's. */
export const CHAT_ONLY_REFUSALS: ReadonlySet<string> = new Set([
  "too_long",
  "empty",
  "no_such_channel",
  "log_in",
  "muted",
  "no_name",
  "not_in_match",
  "not_spectating",
  "spectators_only",
  "in_match",
  "no_such_player",
  "too_many",
]);
const CHAT_ERRORS: Record<string, string> = {
  rate_limited: "You are sending messages too fast. Wait a moment.",
  too_long: `Messages hold up to ${MAX_CHAT_CHARS} characters.`,
  empty: "Type a message first.",
  no_such_channel: "That channel is not available.",
  log_in: "Log in to chat in Public.",
  muted: "An admin has muted your chat for now.",
  no_name: "Choose your name before chatting.",
  not_in_match: "You are not in a match, so there is no match or team chat.",
  not_spectating: "You are not spectating a match.",
  spectators_only: "While spectating you can only talk to the other spectators.",
  not_logged_in: "Log in to whisper.",
  no_such_friend: "You can only whisper to friends.",
  offline: "That friend is offline.",
  in_match: "That friend is playing a match and cannot hear whispers now.",
  no_such_player: "Nobody by that name is connected.",
  unavailable: "The server could not check that right now. Try again in a minute.",
  too_many: "You are ignoring too many players. Unignore some first.",
};
/** Words for a refused chat request. */
export function chatErrorText(code: string) {
  return CHAT_ERRORS[code] ?? "The message was not sent.";
}

/** The line as shown: `[Team] Name#number: hi`, `To Name#number: hi`. */
export function chatPrefix(message: ChatLine) {
  if (message.channel === "whisper")
    return message.to ? `To ${message.to}` : `${message.from} whispers`;
  return `[${TAB_LABELS[message.channel]}] ${message.from}`;
}

/** The chat log and the chat box's place in it. */
export type ChatState = {
  /** Newest last, `MAX_CHAT_LOG` at most. */
  messages: ChatMessage[];
  /** The tab shown. */
  active: ChatChannel;
  /** Messages that arrived in each tab while another was shown. */
  unread: Partial<Record<ChatChannel, number>>;
  /** Why the last request was refused (`chatErrorText`). */
  error: string | null;
  /** Players this connection ignores, as typed (the door matches by who
   * they are; this list is for showing and unignoring). */
  ignored: string[];
  /** Who the whisper tab writes to. */
  whisperTo: string | null;
  nextId: number;
};
export const MAX_CHAT_LOG = 200;
export const EMPTY_CHAT: ChatState = {
  messages: [],
  active: "public",
  unread: {},
  error: null,
  ignored: [],
  whisperTo: null,
  nextId: 1,
};

/** Adds a message: unread in its tab unless shown (or one's own whisper),
 * and the whisper tab answers whoever whispered last if nobody was picked. */
export function receiveChat(state: ChatState, line: ChatLine): ChatState {
  const message = { ...line, id: state.nextId };
  const own = line.channel === "whisper" && line.to !== null;
  const unread =
    line.channel === state.active || own
      ? state.unread
      : { ...state.unread, [line.channel]: (state.unread[line.channel] ?? 0) + 1 };
  return {
    ...state,
    messages: [...state.messages, message].slice(-MAX_CHAT_LOG),
    unread,
    whisperTo: line.channel === "whisper" && !own && !state.whisperTo ? line.from : state.whisperTo,
    nextId: state.nextId + 1,
  };
}

/** Shows `tab`, clearing its unread count. */
export function showChatTab(state: ChatState, tab: ChatChannel): ChatState {
  const { [tab]: _, ...unread } = state.unread;
  return { ...state, active: tab, unread };
}

const sameHandle = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
/** Whether `handle` is ignored here (names compare without case). */
export function isIgnored(state: Pick<ChatState, "ignored">, handle: string) {
  return state.ignored.some((other) => sameHandle(other, handle));
}
/** The tab's messages, without ignored players' earlier lines. */
export function tabMessages(state: ChatState, tab: ChatChannel) {
  return state.messages.filter(
    (message) =>
      message.channel === tab && !(message.to === null && isIgnored(state, message.from)),
  );
}
export function withIgnored(state: ChatState, handle: string, on: boolean): ChatState {
  const rest = state.ignored.filter((other) => !sameHandle(other, handle));
  return { ...state, ignored: on ? [...rest, handle] : rest };
}
