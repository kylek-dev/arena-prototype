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
/** System lines: written by the page itself (match found, a player left,
 * command output), never received: the door refuses `system` and the parser
 * above never yields it, so no player can fake one. */
export type SystemLine = { channel: "system"; text: string };
export type ChatMessage = (ChatLine | (SystemLine & { from: null; to: null; admin: false })) & {
  id: number;
};

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

/** A chat box tab: everything (`all`), one channel, or one whisper
 * conversation (`w:` and the other player's handle). */
export type ChatTab = "all" | Exclude<ChatChannel, "whisper"> | `w:${string}`;
export const conversationTab = (handle: string): ChatTab => `w:${handle}`;
/** The other player in a whisper tab, or `null` for a channel tab. */
export function tabPartner(tab: ChatTab) {
  return tab.startsWith("w:") ? tab.slice(2) : null;
}

/** The chat box's tabs: All, the channels this player can use (a
 * spectator's first), then one per whisper conversation. */
export function chatTabs(spectating: boolean, conversations: readonly string[] = []): ChatTab[] {
  return [
    "all",
    ...(spectating ? (["spectator", "public"] as const) : (["public", "team", "match"] as const)),
    ...conversations.map(conversationTab),
  ];
}
const TAB_LABELS: Record<ChatChannel, string> = {
  public: "Public",
  team: "Team",
  match: "Match",
  spectator: "Spectators",
  whisper: "Whisper",
};
export function chatTabLabel(tab: ChatTab) {
  if (tab === "all") return "All";
  const partner = tabPartner(tab);
  return partner ?? TAB_LABELS[tab as ChatChannel];
}

/** Where text typed in the All tab goes: the team when seated in a match,
 * the spectators when watching, otherwise the public channel. */
export function defaultChannel(place: { seated: boolean; spectating: boolean }) {
  return place.spectating ? "spectator" : place.seated ? "team" : "public";
}

/** A draft that whispers: `/w Name#0000 ` (or `/whisper`) and the message
 * so far. `null` until the handle is complete and followed by a space. */
export function whisperDraft(draft: string) {
  const match = /^\/(?:w|whisper)\s+(\S+)\s([\s\S]*)$/i.exec(draft);
  if (!match || !isHandle(match[1])) return null;
  return { to: match[1], text: match[2] };
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
export function chatPrefix(message: ChatLine | SystemLine) {
  if (message.channel === "system") return "";
  if (message.channel === "whisper")
    return message.to ? `To ${message.to}` : `${message.from} whispers`;
  return `[${TAB_LABELS[message.channel]}] ${message.from}`;
}
/** The other player in a whisper, from either side. */
export function whisperPartner(message: ChatMessage) {
  return message.channel === "whisper" ? (message.to ?? message.from) : null;
}

/** The chat log and the chat box's place in it. */
export type ChatState = {
  /** Newest last, `MAX_CHAT_LOG` at most. */
  messages: ChatMessage[];
  /** The tab shown. */
  active: ChatTab;
  /** Messages that arrived in each tab while neither it nor All was shown. */
  unread: Partial<Record<ChatTab, number>>;
  /** Why the last request was refused (`chatErrorText`). */
  error: string | null;
  /** Players this connection ignores, as typed (the door matches by who
   * they are; this list is for showing and unignoring). */
  ignored: string[];
  /** Whisper conversations with a tab, in the order they began. */
  conversations: string[];
  /** Who whispered last, for `/r`. */
  whisperTo: string | null;
  nextId: number;
};
export const MAX_CHAT_LOG = 200;
/** Conversation tabs kept at once; the oldest closes first. */
export const MAX_CONVERSATIONS = 6;
export const EMPTY_CHAT: ChatState = {
  messages: [],
  active: "all",
  unread: {},
  error: null,
  ignored: [],
  conversations: [],
  whisperTo: null,
  nextId: 1,
};
const sameHandle = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** The tab a message belongs to besides All. */
function tabOf(message: ChatLine): ChatTab {
  return message.channel === "whisper"
    ? conversationTab(message.to ?? message.from)
    : (message.channel as ChatTab);
}

/** Adds a message: a whisper opens (or reuses) its conversation tab; it is
 * unread in All and in its own tab unless one of them is shown (one's own
 * whispers never are); `/r` answers whoever whispered last. */
export function receiveChat(state: ChatState, line: ChatLine): ChatState {
  const message: ChatMessage = { ...line, id: state.nextId };
  const own = line.channel === "whisper" && line.to !== null;
  let conversations = state.conversations;
  let tab = tabOf(line);
  if (line.channel === "whisper") {
    const partner = line.to ?? line.from;
    const known = conversations.find((other) => sameHandle(other, partner));
    if (known) tab = conversationTab(known);
    else conversations = withConversation(conversations, partner, state.active);
  }
  const seen = state.active === "all" || state.active === tab;
  let unread =
    own || seen
      ? state.unread
      : {
          ...state.unread,
          all: (state.unread.all ?? 0) + 1,
          [tab]: (state.unread[tab] ?? 0) + 1,
        };
  // A conversation closed to make room takes its unread count with it.
  for (const gone of state.conversations)
    if (!conversations.includes(gone)) unread = readTab(unread, conversationTab(gone));
  return {
    ...state,
    messages: [...state.messages, message].slice(-MAX_CHAT_LOG),
    unread,
    conversations,
    whisperTo: line.channel === "whisper" && !own ? line.from : state.whisperTo,
    nextId: state.nextId + 1,
  };
}

/** Adds a system line, shown in every tab and never unread. */
export function systemChat(state: ChatState, text: string): ChatState {
  const message: ChatMessage = {
    channel: "system",
    from: null,
    to: null,
    admin: false,
    text,
    id: state.nextId,
  };
  return {
    ...state,
    messages: [...state.messages, message].slice(-MAX_CHAT_LOG),
    nextId: state.nextId + 1,
  };
}

/** Adds a conversation, closing the oldest when full, never the tab shown. */
function withConversation(conversations: string[], handle: string, active: ChatTab) {
  const next = [...conversations, handle];
  while (next.length > MAX_CONVERSATIONS) {
    const oldest = next.findIndex((other) => conversationTab(other) !== active);
    next.splice(oldest, 1);
  }
  return next;
}

/** Clears `tab`'s unread count, and takes it off All's as well. */
function readTab(unread: ChatState["unread"], tab: ChatTab): ChatState["unread"] {
  const next: ChatState["unread"] = { ...unread };
  const count = next[tab] ?? 0;
  delete next[tab];
  const all = (next.all ?? 0) - count;
  if (all > 0) next.all = all;
  else delete next.all;
  return next;
}

/** Shows `tab`, clearing its unread count (All clears every count). */
export function showChatTab(state: ChatState, tab: ChatTab): ChatState {
  if (tab === "all") return { ...state, active: tab, unread: {} };
  return { ...state, active: tab, unread: readTab(state.unread, tab) };
}

/** Opens (or shows) the conversation tab with `handle`. */
export function openConversation(state: ChatState, handle: string): ChatState {
  const known = state.conversations.find((other) => sameHandle(other, handle));
  const conversations = known
    ? state.conversations
    : withConversation(state.conversations, handle, state.active);
  return showChatTab({ ...state, conversations }, conversationTab(known ?? handle));
}

/** Closes a conversation tab (its whispers stay in All). */
export function closeConversation(state: ChatState, handle: string): ChatState {
  const tab = conversationTab(
    state.conversations.find((other) => sameHandle(other, handle)) ?? handle,
  );
  return {
    ...state,
    conversations: state.conversations.filter((other) => !sameHandle(other, handle)),
    unread: readTab(state.unread, tab),
    active: state.active === tab ? "all" : state.active,
    whisperTo: state.whisperTo,
  };
}

/** Whether `handle` is ignored here (names compare without case). */
export function isIgnored(state: Pick<ChatState, "ignored">, handle: string) {
  return state.ignored.some((other) => sameHandle(other, handle));
}
/** The tab's messages without ignored players' earlier lines: All has
 * everything; a channel tab its channel and system lines; a conversation
 * tab the whispers with that player. */
export function tabMessages(state: ChatState, tab: ChatTab) {
  const partner = tabPartner(tab);
  return state.messages.filter((message) => {
    if (message.channel !== "system" && message.to === null && isIgnored(state, message.from))
      return false;
    if (tab === "all") return true;
    if (partner) {
      const other = whisperPartner(message);
      return !!other && sameHandle(other, partner);
    }
    return message.channel === "system" || message.channel === tab;
  });
}
export function withIgnored(state: ChatState, handle: string, on: boolean): ChatState {
  const rest = state.ignored.filter((other) => !sameHandle(other, handle));
  return { ...state, ignored: on ? [...rest, handle] : rest };
}
